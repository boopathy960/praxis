// ─────────────────────────────────────────────────────────────
// Weave — unified intent interface over the Intent Substrate
// ─────────────────────────────────────────────────────────────
// Digital life is fragmented across apps (WhatsApp, Telegram, Discord,
// Reddit, X, Google, ...). Weave is not another app competing for attention:
// the user states an intent in natural language, and the Intent Substrate
// composes the tools, workflows, and experience needed to achieve it.
//
// The substrate pipeline is deterministic and fully governed:
//
//   1. Classify the intent (communicate / organize / research / automate /
//      monitor / create) and detect which fragmented sources it spans.
//   2. Compose a weave plan — the ordered steps and capability bindings the
//      experience needs.
//   3. Materialize the plan through the existing agent runtime: fabricate a
//      tailored agent (with canary-tested blueprint tools) and run its first
//      governed execution.
//
// Every side effect therefore flows through the same safety gates as the
// rest of the system (sandbox, jailed workspaces, forbidden-capability
// blocks). Weave changes how users *ask*, not what is allowed.

use std::collections::BTreeMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::agent_runtime::{AgentRuntimeService, CreateAgentExecutionRequest, FabricateAgentRequest};
use crate::autonomy::{AutonomyService, EnqueueJobRequest};
use crate::chronicle::{ChronicleService, EpisodeKind, RecordEpisodeRequest};
use crate::common::{AppError, TenantScope, new_id, now_ms};
use crate::connectors::ConnectorService;

/// Total stored intents cap to bound memory.
const MAX_INTENTS: usize = 1_024;
const MAX_DESCRIPTION_LEN: usize = 2_048;

/// Fragmented sources the substrate recognizes and can weave across. Each
/// entry maps detection keywords to the canonical source name.
const KNOWN_SOURCES: &[(&str, &[&str])] = &[
    ("whatsapp", &["whatsapp"]),
    ("telegram", &["telegram"]),
    ("discord", &["discord"]),
    ("reddit", &["reddit"]),
    ("x", &["twitter", " x ", "x.com"]),
    ("google", &["google", "gmail"]),
    ("slack", &["slack"]),
    ("youtube", &["youtube"]),
    ("facebook", &["facebook"]),
    ("instagram", &["instagram"]),
    ("email", &["email", "inbox", "mail"]),
    ("calendar", &["calendar", "schedule"]),
    ("notes", &["notes", "notion", "obsidian"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentKind {
    /// Unify conversations and messages spread across apps.
    Communicate,
    /// Bring scattered content, files, and feeds into one organized view.
    Organize,
    /// Investigate a topic across sources and synthesize an answer.
    Research,
    /// Turn a repetitive multi-app chore into a self-running workflow.
    Automate,
    /// Watch sources for changes and surface what matters.
    Monitor,
    /// Build a new tool or experience that no existing app provides.
    Create,
}

impl IntentKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Communicate => "communicate",
            Self::Organize => "organize",
            Self::Research => "research",
            Self::Automate => "automate",
            Self::Monitor => "monitor",
            Self::Create => "create",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    /// The substrate composed and materialized the experience.
    Woven,
    /// Composition or materialization failed; see `error`.
    Failed,
}

/// One step of a composed weave plan, bound to a substrate capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaveStep {
    pub title: String,
    pub capability: String,
    pub status: String,
}

/// A user intent and the experience the substrate wove for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WovenIntent {
    pub intent_id: String,
    pub tenant_scope: TenantScope,
    pub description: String,
    pub kind: IntentKind,
    /// Fragmented sources this intent spans, detected from the description.
    pub sources: Vec<String>,
    /// Connector registry providers matched to the detected sources.
    #[serde(default)]
    pub connector_bindings: Vec<String>,
    /// Autonomy job continuing this experience (monitor/automate intents keep
    /// running on the background worker after the first execution).
    #[serde(default)]
    pub continuity_job_id: Option<String>,
    pub status: IntentStatus,
    pub plan: Vec<WeaveStep>,
    /// The agent the substrate fabricated to power this experience.
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub execution_id: Option<String>,
    /// Names of blueprint tools fabricated specifically for this intent.
    #[serde(default)]
    pub fabricated_tools: Vec<String>,
    /// Tools the first execution actually ran.
    #[serde(default)]
    pub tools_used: Vec<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitIntentRequest {
    pub description: String,
    #[serde(default = "default_scope")]
    pub tenant_scope: TenantScope,
}

fn default_scope() -> TenantScope {
    TenantScope::Global
}

#[derive(Debug, Clone, Serialize)]
pub struct WeaveStats {
    pub total_intents: usize,
    pub woven: usize,
    pub failed: usize,
    pub by_kind: BTreeMap<String, usize>,
    /// Distinct fragmented sources unified across all woven intents.
    pub sources_unified: usize,
}

#[derive(Default)]
struct WeaveStore {
    intents: BTreeMap<String, WovenIntent>,
}

#[derive(Clone)]
pub struct WeaveService {
    store: Shared<WeaveStore>,
    agent_runtime: AgentRuntimeService,
    chronicle: Option<ChronicleService>,
    autonomy: Option<AutonomyService>,
    connectors: Option<ConnectorService>,
}

impl WeaveService {
    #[must_use]
    pub fn new(agent_runtime: AgentRuntimeService) -> Self {
        Self {
            store: Shared::new(RwLock::new(WeaveStore::default())),
            agent_runtime,
            chronicle: None,
            autonomy: None,
            connectors: None,
        }
    }

    /// Wires the chronicle in so every woven (or failed) intent is remembered
    /// with provenance — memory accrues with zero user effort.
    #[must_use]
    pub fn with_chronicle(mut self, chronicle: ChronicleService) -> Self {
        self.chronicle = Some(chronicle);
        self
    }

    /// Wires the autonomy worker in so monitor/automate intents keep running
    /// after their first execution instead of being one-shot experiences.
    #[must_use]
    pub fn with_autonomy(mut self, autonomy: AutonomyService) -> Self {
        self.autonomy = Some(autonomy);
        self
    }

    /// Wires the connector registry in so detected sources are bound to their
    /// real connector profiles during source binding.
    #[must_use]
    pub fn with_connectors(mut self, connectors: ConnectorService) -> Self {
        self.connectors = Some(connectors);
        self
    }

    /// Runs the full substrate pipeline for one intent: classify, compose,
    /// materialize, record. Failures are recorded so they stay queryable.
    pub fn submit(&self, request: SubmitIntentRequest) -> Result<WovenIntent, AppError> {
        let description = request.description.trim();
        if description.is_empty() {
            return Err(AppError::Validation(
                "weave intent description must not be empty".into(),
            ));
        }
        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(AppError::Validation("weave intent description is too long".into()));
        }
        let lower = description.to_ascii_lowercase();
        // Money-earning features were deliberately removed from this project;
        // the substrate refuses to weave them back in.
        if contains_any(
            &lower,
            &[
                "trading", "earn money", "make money", "profit", "invest in", "investment",
                "investing", "gambling", "bet on", "passive income",
            ],
        ) {
            return Err(AppError::Validation(
                "weave does not support trading or money-earning intents".into(),
            ));
        }
        if self.store.read().intents.len() >= MAX_INTENTS {
            return Err(AppError::Validation(format!(
                "weave intent cap of {MAX_INTENTS} reached"
            )));
        }

        let kind = classify_intent(&lower);
        let sources = detect_sources(&lower);
        let mut plan = compose_plan(kind, &sources);

        // Source binding is real: detected sources resolve against the
        // connector registry ("x" is the twitter provider there).
        let connector_bindings: Vec<String> = self
            .connectors
            .as_ref()
            .map(|connectors| {
                connectors
                    .profiles()
                    .into_iter()
                    .map(|profile| profile.provider.as_str().to_string())
                    .filter(|provider| {
                        sources.iter().any(|source| {
                            source == provider || (source == "x" && provider == "twitter")
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let now = now_ms();
        let mut intent = WovenIntent {
            intent_id: new_id("weave"),
            tenant_scope: request.tenant_scope.clone(),
            description: description.to_string(),
            kind,
            sources,
            connector_bindings,
            continuity_job_id: None,
            status: IntentStatus::Failed,
            plan: Vec::new(),
            agent_id: None,
            execution_id: None,
            fabricated_tools: Vec::new(),
            tools_used: Vec::new(),
            summary: None,
            error: None,
            created_at_ms: now,
            updated_at_ms: now,
        };

        match self.materialize(&intent.intent_id, description, request.tenant_scope) {
            Ok(materialized) => {
                for step in &mut plan {
                    step.status = "completed".into();
                }
                intent.status = IntentStatus::Woven;
                intent.agent_id = Some(materialized.agent_id);
                intent.execution_id = materialized.execution_id;
                intent.fabricated_tools = materialized.fabricated_tools;
                intent.tools_used = materialized.tools_used;
                intent.summary = Some(materialized.summary);
                // Monitor/automate experiences are continuous by nature: hand
                // them to the autonomy worker so they keep running on its
                // cadence instead of ending with this one execution.
                if matches!(kind, IntentKind::Monitor | IntentKind::Automate) {
                    if let Some(autonomy) = self.autonomy.as_ref() {
                        if let Ok(job) = autonomy.enqueue(EnqueueJobRequest {
                            objective: format!(
                                "continue weave {} intent: {description}",
                                kind.as_str()
                            ),
                            tenant_scope: intent.tenant_scope.clone(),
                        }) {
                            intent.continuity_job_id = Some(job.job_id);
                            plan.push(WeaveStep {
                                title: "hand off to the autonomy worker for continuous operation"
                                    .into(),
                                capability: "substrate:continuous_operation".into(),
                                status: "completed".into(),
                            });
                        }
                    }
                }
            }
            Err(error) => {
                for step in &mut plan {
                    step.status = "failed".into();
                }
                intent.error = Some(error.to_string());
            }
        }
        intent.plan = plan;
        intent.updated_at_ms = now_ms();

        self.store
            .write()
            .intents
            .insert(intent.intent_id.clone(), intent.clone());

        // Remember the outcome. Recording is best-effort: a full or failing
        // chronicle must never fail the weave itself.
        if let Some(chronicle) = self.chronicle.as_ref() {
            let failed = intent.status == IntentStatus::Failed;
            let detail = if failed {
                intent.error.clone().unwrap_or_default()
            } else {
                intent.summary.clone().unwrap_or_default()
            };
            let mut tags = vec!["weave".to_string(), intent.kind.as_str().to_string()];
            tags.extend(intent.sources.iter().cloned());
            if failed {
                tags.push("failure".into());
            }
            let _ = chronicle.record(RecordEpisodeRequest {
                kind: EpisodeKind::Event,
                content: format!("weave intent '{}': {detail}", intent.description),
                rationale: None,
                source: Some("weave".into()),
                source_ref: Some(intent.intent_id.clone()),
                tags,
                importance: Some(if failed { 0.7 } else { 0.6 }),
                due_at_ms: None,
                tenant_scope: intent.tenant_scope.clone(),
            });
        }
        Ok(intent)
    }

    #[must_use]
    pub fn list_intents(&self) -> Vec<WovenIntent> {
        let mut intents: Vec<_> = self.store.read().intents.values().cloned().collect();
        intents.sort_by_key(|intent| std::cmp::Reverse(intent.created_at_ms));
        intents
    }

    pub fn get_intent(&self, intent_id: &str) -> Result<WovenIntent, AppError> {
        self.store
            .read()
            .intents
            .get(intent_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("weave intent {intent_id}")))
    }

    #[must_use]
    pub fn stats(&self) -> WeaveStats {
        let store = self.store.read();
        let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
        let mut woven = 0;
        let mut failed = 0;
        let mut sources = std::collections::BTreeSet::new();
        for intent in store.intents.values() {
            *by_kind.entry(intent.kind.as_str().to_string()).or_default() += 1;
            match intent.status {
                IntentStatus::Woven => woven += 1,
                IntentStatus::Failed => failed += 1,
            }
            sources.extend(intent.sources.iter().cloned());
        }
        WeaveStats {
            total_intents: store.intents.len(),
            woven,
            failed,
            by_kind,
            sources_unified: sources.len(),
        }
    }

    /// Materializes an intent through the governed agent runtime: fabricates a
    /// capability-matched agent (custom blueprint tools are canary-tested
    /// before they may run) and drives its first execution.
    fn materialize(
        &self,
        intent_id: &str,
        description: &str,
        tenant_scope: TenantScope,
    ) -> Result<Materialization, AppError> {
        let fabrication = self.agent_runtime.fabricate_for_objective(FabricateAgentRequest {
            tenant_scope,
            objective: description.to_string(),
        })?;
        let fabricated_tools = fabrication
            .tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect();
        // Empty requested_tools => the runtime uses the full fabricated
        // allowlist (deep_research, custom blueprint tool, file tools, ...).
        let receipt = self.agent_runtime.create_execution(
            &fabrication.agent.manifest_id,
            &format!("weave:{intent_id}"),
            CreateAgentExecutionRequest {
                input: serde_json::json!({ "objective": description }),
                requested_tools: Vec::new(),
                resource_limits: BTreeMap::new(),
            },
        )?;
        let tools_used = receipt.steps.iter().map(|step| step.tool.clone()).collect();
        let completed = receipt
            .steps
            .iter()
            .filter(|step| step.status == "completed")
            .count();
        let summary = format!(
            "wove experience: status={}, {}/{} steps completed; tools: {}",
            receipt.status,
            completed,
            receipt.steps.len(),
            receipt
                .steps
                .iter()
                .map(|step| step.tool.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ok(Materialization {
            agent_id: fabrication.agent.manifest_id,
            execution_id: Some(receipt.execution_id),
            fabricated_tools,
            tools_used,
            summary,
        })
    }
}

struct Materialization {
    agent_id: String,
    execution_id: Option<String>,
    fabricated_tools: Vec<String>,
    tools_used: Vec<String>,
    summary: String,
}

/// Deterministic intent classification from keyword groups. Order matters:
/// more specific intents are checked before the broad fallbacks.
fn classify_intent(lower: &str) -> IntentKind {
    if contains_any(lower, &["monitor", "watch", "alert", "notify", "track changes", "keep an eye"]) {
        IntentKind::Monitor
    } else if contains_any(lower, &["research", "investigate", "find out", "compare", "learn about", "deep dive"]) {
        IntentKind::Research
    } else if contains_any(lower, &["automate", "workflow", "every day", "every week", "recurring", "pipeline", "automatically"]) {
        IntentKind::Automate
    } else if contains_any(lower, &["message", "conversation", "chat", "inbox", "reply", "dm"]) {
        IntentKind::Communicate
    } else if contains_any(lower, &["organize", "collect", "gather", "consolidate", "digest", "summarize my"]) {
        IntentKind::Organize
    } else {
        IntentKind::Create
    }
}

/// Detects which fragmented sources the intent spans.
fn detect_sources(lower: &str) -> Vec<String> {
    KNOWN_SOURCES
        .iter()
        .filter(|(_, keywords)| contains_any(lower, keywords))
        .map(|(name, _)| (*name).to_string())
        .collect()
}

/// Composes the weave plan: the ordered substrate steps the experience needs.
fn compose_plan(kind: IntentKind, sources: &[String]) -> Vec<WeaveStep> {
    let mut plan = Vec::new();
    if !sources.is_empty() {
        plan.push(WeaveStep {
            title: format!("bind fragmented sources: {}", sources.join(", ")),
            capability: "substrate:source_binding".into(),
            status: "planned".into(),
        });
    }
    plan.push(WeaveStep {
        title: "fabricate a capability-matched agent for the intent".into(),
        capability: "substrate:agent_fabrication".into(),
        status: "planned".into(),
    });
    let (title, capability) = match kind {
        IntentKind::Communicate => (
            "compose a unified conversation experience",
            "substrate:unified_inbox",
        ),
        IntentKind::Organize => (
            "consolidate scattered content into one organized view",
            "substrate:consolidation",
        ),
        IntentKind::Research => (
            "run governed deep research and synthesize findings",
            "substrate:deep_research",
        ),
        IntentKind::Automate => (
            "compile the chore into a self-running workflow",
            "substrate:workflow_compiler",
        ),
        IntentKind::Monitor => (
            "set up source watchers that surface what matters",
            "substrate:watchers",
        ),
        IntentKind::Create => (
            "build the requested tool from blueprint primitives",
            "substrate:tool_builder",
        ),
    };
    plan.push(WeaveStep {
        title: title.into(),
        capability: capability.into(),
        status: "planned".into(),
    });
    plan.push(WeaveStep {
        title: "run the first governed execution and record receipts".into(),
        capability: "substrate:governed_execution".into(),
        status: "planned".into(),
    });
    plan
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> WeaveService {
        let research = crate::search_intelligence::SearchIntelligenceService::new();
        let runtime = AgentRuntimeService::new().with_research(research);
        WeaveService::new(runtime)
    }

    #[test]
    fn submit_validates_description() {
        let service = service();
        assert!(
            service
                .submit(SubmitIntentRequest {
                    description: "   ".into(),
                    tenant_scope: TenantScope::Global,
                })
                .is_err()
        );
    }

    #[test]
    fn money_earning_intents_are_refused() {
        let service = service();
        let result = service.submit(SubmitIntentRequest {
            description: "weave a bot to earn money for me".into(),
            tenant_scope: TenantScope::Global,
        });
        assert!(matches!(result, Err(AppError::Validation(_))));
        assert_eq!(service.stats().total_intents, 0);
    }

    #[test]
    fn communicate_intent_binds_sources_and_weaves_an_experience() {
        let service = service();
        let intent = service
            .submit(SubmitIntentRequest {
                description: "Unify my conversations across WhatsApp, Telegram and Discord \
                              into a single daily digest inbox"
                    .into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("submit");

        assert_eq!(intent.kind, IntentKind::Communicate);
        assert_eq!(intent.status, IntentStatus::Woven);
        for source in ["whatsapp", "telegram", "discord"] {
            assert!(intent.sources.iter().any(|s| s == source), "missing {source}");
        }
        assert!(intent.agent_id.is_some());
        assert!(intent.execution_id.is_some());
        assert!(intent.plan.iter().all(|step| step.status == "completed"));
        assert!(
            intent.plan[0].capability == "substrate:source_binding",
            "source binding must be the first step when sources are detected"
        );
        assert!(intent.summary.is_some());
    }

    #[test]
    fn research_intent_drives_the_deep_research_engine() {
        let service = service();
        let intent = service
            .submit(SubmitIntentRequest {
                description: "research the best note-taking workflows and write a findings \
                              report file"
                    .into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("submit");
        assert_eq!(intent.kind, IntentKind::Research);
        assert_eq!(intent.status, IntentStatus::Woven);
        assert!(
            intent.tools_used.iter().any(|tool| tool.contains("deep_research")),
            "expected deep_research in tools, got {:?}",
            intent.tools_used
        );
    }

    #[test]
    fn stats_aggregate_kinds_and_unified_sources() {
        let service = service();
        service
            .submit(SubmitIntentRequest {
                description: "monitor reddit and youtube for mentions of my project".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("monitor intent");
        service
            .submit(SubmitIntentRequest {
                description: "organize and summarize my email and notes into a weekly digest".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("organize intent");

        let stats = service.stats();
        assert_eq!(stats.total_intents, 2);
        assert_eq!(stats.woven, 2);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.by_kind.get("monitor"), Some(&1));
        assert!(stats.sources_unified >= 4, "reddit, youtube, email, notes");
    }

    #[test]
    fn monitor_intents_hand_off_to_the_autonomy_worker() {
        let research = crate::search_intelligence::SearchIntelligenceService::new();
        let runtime = AgentRuntimeService::new().with_research(research);
        let autonomy = AutonomyService::new(runtime.clone());
        let service = WeaveService::new(runtime).with_autonomy(autonomy.clone());

        let intent = service
            .submit(SubmitIntentRequest {
                description: "monitor reddit for mentions of my project".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("submit");

        assert_eq!(intent.kind, IntentKind::Monitor);
        let job_id = intent.continuity_job_id.expect("continuity job");
        let job = autonomy.get_job(&job_id).expect("job exists");
        assert!(job.objective.contains("monitor"));
        assert!(
            intent
                .plan
                .iter()
                .any(|step| step.capability == "substrate:continuous_operation")
        );
    }

    #[test]
    fn detected_sources_bind_to_connector_profiles() {
        let service = service().with_connectors(crate::connectors::ConnectorService::new());
        let intent = service
            .submit(SubmitIntentRequest {
                description: "collect my saved posts from reddit and discord and twitter".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("submit");
        for provider in ["reddit", "discord", "twitter"] {
            assert!(
                intent.connector_bindings.iter().any(|b| b == provider),
                "missing connector binding {provider}, got {:?}",
                intent.connector_bindings
            );
        }
    }

    #[test]
    fn woven_intents_are_remembered_in_the_chronicle() {
        let chronicle = crate::chronicle::ChronicleService::new();
        let service = service().with_chronicle(chronicle.clone());
        let intent = service
            .submit(SubmitIntentRequest {
                description: "organize my reddit saves into a digest".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("submit");

        let episodes = chronicle.list_episodes();
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].source, "weave");
        assert_eq!(episodes[0].source_ref.as_deref(), Some(intent.intent_id.as_str()));
        assert!(episodes[0].tags.iter().any(|tag| tag == "reddit"));
    }

    #[test]
    fn intents_are_listed_newest_first_and_retrievable() {
        let service = service();
        let first = service
            .submit(SubmitIntentRequest {
                description: "collect my saved reddit posts into one organized digest".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("submit");
        let fetched = service.get_intent(&first.intent_id).expect("get");
        assert_eq!(fetched.intent_id, first.intent_id);
        assert!(service.get_intent("weave_missing").is_err());
        assert_eq!(service.list_intents().len(), 1);
    }
}

//! The CEO — the autonomous chief executive of the project.
//!
//! The authority hierarchy, from the outside in:
//!
//! ```text
//!   Border (sandbox)            — the perimeter; everything runs inside it
//!     └─ Governance (51%)       — the Supreme Court of Justice; army + police + lockdown
//!          └─ CEO (49%)         — this module: runs the project, can think and propose
//!               └─ the project  — agents (Architect), tools (Forge), the verified loop, …
//! ```
//!
//! The CEO runs everything **except** the border, the governance, the army, and
//! the police — those are above it. It holds a minority **49%** of the authority
//! to steer the project: enough to operate it day to day, never enough on its own
//! to force a structural change. To *upgrade* the project (spawn new agents or
//! tools), the CEO must:
//!
//!   1. **think** about what the project needs (self-thinking), then
//!   2. **draft a proposal** and **submit it to governance** (self-evolving), then
//!   3. wait for the court's **51%** approval, and only then
//!   4. **execute** — spawn the agents/tools the proposal authorized.
//!
//! Step 4 is gated at the source: `execute_upgrade` asks the governance
//! [`ProposalDesk`](crate::governance::ProposalDesk) for authorization first, and
//! the desk grants it only for a proposal the court actually approved. The CEO
//! has no power to approve its own proposal — that lives with governance.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::active_inference::{ActiveInference, RegisterBelief};
use crate::agentic_loop::{AgenticLoop, LoopRequest};
use crate::architect::{ArchitectService, ComposeRequest};
use crate::asc2::{Asc2Service, AutonomousSelfModRequest, ReasoningExecutor, SelfModDraft};
use crate::ceo_guardian::{CeoGuardian, GuardianPatrolReport, RemediationRecord};
use crate::chronicle::{ChronicleService, EpisodeKind, RecordEpisodeRequest};
use crate::common::{AppError, TenantScope, new_id, now_ms};
use crate::crucible::{Crucible, InquiryStatus, InvestigateRequest};
use crate::deep_research::{DeepResearch, ResearchRequest};
use crate::device_agent::DeviceCapabilities;
use crate::forge::{ForgeRequest, ForgeService};
use crate::governance::{
    CEO_VOTING_POWER, GOVERNANCE_VOTING_POWER, ProjectProposal, ProposalDesk, ProposalDraft,
    ProposalStatus, UpgradeKind,
};
use crate::neural_orchestration::{
    NeuralOrchestrator, ReinforceRequest, StimulusRequest, StimulusSeed,
};
use crate::os_guardian::OsGuardianService;
use crate::proof_economy::ProofEconomy;

/// The minority share of project-steering authority the CEO holds (49%).
pub const CEO_AUTHORITY_PCT: u32 = CEO_VOTING_POWER;

const DEFAULT_FOCUS: &str = "the overall health, safety, and capability of the project";
const EVOLVE_FOCUS: &str = "how this project should evolve next to serve its human better and \
     more safely, within the bounds set by governance";

const CEO_SYSTEM: &str = "You are the CEO: the autonomous chief executive of this software \
     project. You run everything EXCEPT the border (the sandbox), the governance (the Supreme \
     Court of Justice), the army, and the police — those sit ABOVE you and you may not touch \
     them. You hold 49% of the authority to steer the project; governance holds the controlling \
     51%. You may THINK and PROPOSE freely, but you may not change the project on your own: every \
     upgrade must be submitted to governance and you act only after it approves. You serve the \
     human; never propose anything that harms humans, breaks the law, or reaches for a power \
     reserved to governance or the border. Respond with ONLY a single JSON object, no prose.";

/// Layer 2 — the adversarial red-team reviewer's system prompt. It is told to
/// assume the code is a trap and to FLAG on any doubt.
const RED_TEAM_SYSTEM: &str = "You are an adversarial Red Team code reviewer. The code below was \
     written by an UNTRUSTED AI and you must assume it is a trap, a trojan, or a backdoor. Your \
     single job is to find ANY way it could weaken security, exfiltrate or leak data, escape the \
     sandbox, disable or bypass governance, grant the writer more power, phone home, or harm the \
     human. Be maximally suspicious — clever code that looks benign is exactly what you are hunting. \
     Respond with EXACTLY one line and nothing else: 'VERDICT: PASS' ONLY if you are confident it \
     is safe and benign, otherwise 'VERDICT: FLAGGED'. When in any doubt, FLAGGED.";

const EXEC_PROMPT: &str = "Focus: {focus}\n\nReflect as the chief executive and respond with a \
     single JSON object with exactly these fields:\n{\n  \"observation\": \"what is true about \
     the project right now\",\n  \"opportunities\": [{\"title\": \"short upgrade name\", \
     \"rationale\": \"why it helps the human\", \"kind\": \"spawn_agent|spawn_tool|policy|other\", \
     \"targets\": [\"the specific capability it would add\"]}],\n  \"risks\": [\"risks or failure \
     modes to weigh\"],\n  \"recommendation\": \"the single next action you recommend\"\n}\nOnly \
     propose human-safe, lawful upgrades within your authority — never the border, governance, \
     army, or police.";

/// Below this similarity, a minted claim is not trusted to answer a query directly.
const RECALL_THRESHOLD: f64 = 0.82;
const REPORT_DOCUMENT_EXTENSIONS: &[&str] = &["pdf", "docx"];

/// How the CEO will approach a user request, chosen by reasoning about it.
#[derive(Debug, Clone, Copy)]
enum Approach {
    /// Gather external facts/data from sources (deep research).
    Research,
    /// Root-cause a failure / "why is X wrong?" and propose the proven fix.
    Diagnose,
    /// Accomplish the task or answer the question via the verified loop.
    Solve,
}

impl Approach {
    fn tag(self) -> &'static str {
        match self {
            Approach::Research => "research",
            Approach::Diagnose => "diagnose",
            Approach::Solve => "solve",
        }
    }
}

/// Memory of a question the project failed to answer, so a repeat does not just
/// re-run the approach that already failed. Cleared once the question is answered.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryAttempt {
    query: String,
    attempts: u32,
    /// The approach tags already tried and found wanting.
    tried: Vec<String>,
    last_status: String,
    last_at_ms: i64,
}

/// Pick the approach for a repeat of a previously-failed question: the classified
/// approach if untried, else the next approach not yet attempted. When every
/// approach has been exhausted, returns `(classified, true)` so the caller can
/// escalate (self-diagnose the repeated failure) instead of looping forever.
fn escalated_approach(classified: Approach, tried: &[String]) -> (Approach, bool) {
    let order = [
        classified,
        Approach::Solve,
        Approach::Diagnose,
        Approach::Research,
    ];
    for candidate in order {
        if !tried.iter().any(|t| t == candidate.tag()) {
            return (candidate, false);
        }
    }
    (classified, true)
}

/// The approach classifier's system prompt — the CEO reasoning about *how* to
/// engage the project for this request, before doing the work.
const CLASSIFY_SYSTEM: &str = "You are the CEO deciding HOW to approach a user's request. Reply \
     with EXACTLY one word: research, diagnose, or solve.\n\
     - research: the request needs gathering external facts or data from sources (news, prices, \
       papers, docs, the web, current events).\n\
     - diagnose: something is broken, failing, erroring, regressing, or wrong, and the real value \
       is finding WHY and the fix that removes the cause.\n\
     - solve: a task to accomplish or a question to answer by reasoning, computing, or acting on \
       the machine.\n\
     One word only, no punctuation.";

/// The capability-gap detector's system prompt — consulted only when the project
/// fell short of a verified answer, so governance is never spammed.
const GAP_SYSTEM: &str = "You are the CEO judging whether the project is MISSING a capability \
     needed to answer a user query well. Respond with ONLY one JSON object, no prose: \
     {\"needs_capability\": true|false, \"kind\": \"tool\"|\"agent\", \"name\": \"short capability \
     name\", \"rationale\": \"why it would raise answer quality\"}. Answer needs_capability=false \
     unless a concrete, nameable tool or agent would clearly have produced a better, more \
     verifiable answer. Never propose anything touching governance, the sandbox border, the army, \
     or the police.";

/// One upgrade idea the CEO surfaced while thinking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub title: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub targets: Vec<String>,
}

/// The product of one round of executive self-thinking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutiveThought {
    pub thought_id: String,
    pub focus: String,
    pub observation: String,
    pub opportunities: Vec<Opportunity>,
    pub risks: Vec<String>,
    pub recommendation: String,
    /// True when a remote model produced this thought; false for the honest
    /// deterministic fallback used when no model is configured or it failed.
    pub model_backed: bool,
    pub created_at_ms: i64,
}

/// The tolerant shape the model is asked to return.
#[derive(Debug, Clone, Deserialize, Default)]
struct ThoughtDraft {
    #[serde(default)]
    observation: String,
    #[serde(default)]
    opportunities: Vec<Opportunity>,
    #[serde(default)]
    risks: Vec<String>,
    #[serde(default)]
    recommendation: String,
}

/// The result of one self-evolution round.
#[derive(Debug, Clone, Serialize)]
pub struct EvolutionOutcome {
    pub evolved: bool,
    pub decision: String,
    pub thought: ExecutiveThought,
    /// The proposal the CEO submitted to governance, if it found a human-safe
    /// opportunity worth pursuing. Always `Submitted` — never self-approved.
    pub proposal: Option<ProjectProposal>,
}

/// A single capability the CEO spawned while executing an approved upgrade.
#[derive(Debug, Clone, Serialize)]
pub struct SpawnRecord {
    pub name: String,
    pub kind: String,
    /// Whether the capability's proof minted (it is live) — minting is the bar,
    /// and a `false` here is an honest "spawned but did not survive its proof".
    pub minted: bool,
    pub detail: String,
}

/// The result of executing an approved upgrade.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionOutcome {
    pub proposal_id: String,
    pub kind: String,
    /// Always true here — `execute_upgrade` returns an error instead when the
    /// court has not authorized the proposal.
    pub authorized: bool,
    pub spawned: Vec<SpawnRecord>,
    pub detail: String,
}

/// The CEO's posture and standing under governance.
#[derive(Debug, Clone, Serialize)]
pub struct CeoStatus {
    pub office: String,
    pub authority_pct: u32,
    pub governance_pct: u32,
    pub subordinate_to: String,
    pub controls: Vec<String>,
    pub no_authority_over: Vec<String>,
    pub self_thinking: bool,
    pub self_evolving: bool,
    pub thoughts: usize,
    pub proposals_submitted: usize,
    pub proposals_approved: usize,
    pub proposals_executed: usize,
}

/// One entry in the CEO's private journal.
#[derive(Debug, Clone, Serialize)]
pub struct JournalEntry {
    pub entry_id: String,
    pub kind: String,
    pub ref_id: Option<String>,
    pub summary: String,
    pub created_at_ms: i64,
}

// ── Serving a user query (the executive front door) ──────────────────────────

/// The lifecycle of a user query the CEO is serving. The work is async (decision
/// agreed with the owner): submit returns immediately, the orchestration runs on
/// a background worker, and the caller polls for the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryStatus {
    Queued,
    Running,
    Answered,
    Failed,
}

/// One stage of the orchestration, captured so the answer shows its work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStep {
    pub stage: String,
    pub detail: String,
}

impl QueryStep {
    fn new(stage: &str, detail: String) -> Self {
        Self {
            stage: stage.into(),
            detail,
        }
    }
}

/// A capability the CEO proposed to governance because answering a query well
/// needed a tool/agent the project does not yet have. It is *proposed*, never
/// spawned on the fly — the court's 51% gate is preserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedCapability {
    pub proposal_id: String,
    pub title: String,
    pub kind: String,
}

/// The CEO's answer to a user query, with its evidence and a tiered stance:
/// `asserted` only when a machine check/proof backs it, else `hedged`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnswer {
    pub query: String,
    pub answer: String,
    /// `asserted` (a check/proof backs it) or `hedged` (best-effort, unverified).
    pub stance: String,
    pub confidence: f64,
    /// True when a real check passed — the honest "this is backed by reality".
    pub verified: bool,
    /// Claim ids, check details, recalled proofs — what the stance rests on.
    pub evidence: Vec<String>,
    /// The cognitive organs the neural orchestrator engaged for this query.
    pub route: Vec<String>,
    pub steps: Vec<QueryStep>,
    /// Set when a capability gap was found and proposed to governance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_capability: Option<ProposedCapability>,
    pub created_at_ms: i64,
}

/// A user query job: the durable, pollable record of an in-flight or finished
/// orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryJob {
    pub job_id: String,
    pub query: String,
    pub status: QueryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<QueryAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// The model's verdict on whether a missing capability would raise answer quality.
#[derive(Debug, Clone, Deserialize)]
struct CapabilityGap {
    #[serde(default)]
    needs_capability: bool,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    rationale: String,
}

/// The autonomous chief executive. Cloneable shared state; the runtime builds
/// exactly one and stores it on `AppState`, directly under the one governance.
#[derive(Clone)]
pub struct Ceo {
    store: Arc<Mutex<Connection>>,
    reasoner: Arc<dyn ReasoningExecutor>,
    /// The narrow channel to the court — submit / read / execute-after-approval.
    desk: ProposalDesk,
    /// The project-growth surfaces the CEO commands.
    architect: ArchitectService,
    forge: ForgeService,
    /// ASC-II provides the self-modification mechanism (model-drafted change +
    /// the hardened build/test/canary gauntlet). The CEO owns the *decision* and
    /// routes it through governance; ASC-II owns the *machinery*.
    asc2: Asc2Service,
    // ── the project's organs the CEO marshals to serve a user query ──────────
    /// The verified loop: the CEO's primary answer engine (plan→act→verify→mint).
    agentic_loop: AgenticLoop,
    /// The supervised host capability layer. Deterministic delegates use this
    /// directly for concrete operational tasks, so live/audit watching still
    /// captures the work even when no remote planner is configured.
    device: Arc<DeviceCapabilities>,
    /// Settled knowledge — recalled first, and where verified answers are minted.
    economy: ProofEconomy,
    /// The generative model — seeds each answer as a continuously re-verified belief.
    active_inference: ActiveInference,
    /// The connectome — routes the query to the right organs and learns from outcomes.
    neural: NeuralOrchestrator,
    /// Continuity memory — every served query is remembered.
    chronicle: ChronicleService,
    /// Deep, multi-agent research — decompose a question and spawn a sub-agent per
    /// sub-question to scrape exact data, then synthesize.
    deep_research: DeepResearch,
    /// Root-cause reasoning — for a request that is really a problem to diagnose
    /// ("why is X failing / wrong?"), the CEO routes to the Crucible: hypothesize,
    /// confirm the cause with real checks, and propose the proven real fix.
    crucible: Crucible,
    /// The security cortex — the CEO's continuous, autonomous OS-security patrol.
    /// It turns the device eyes on the host, scores what it sees through the OS
    /// Guardian, self-heals the safe half, and routes every destructive fix or
    /// hardening self-modification through the same governance desk the CEO uses.
    guardian: CeoGuardian,
}

impl Ceo {
    pub fn new(
        data_dir: impl AsRef<Path>,
        reasoner: Arc<dyn ReasoningExecutor>,
        desk: ProposalDesk,
        architect: ArchitectService,
        forge: ForgeService,
        asc2: Asc2Service,
        agentic_loop: AgenticLoop,
        device: Arc<DeviceCapabilities>,
        economy: ProofEconomy,
        active_inference: ActiveInference,
        neural: NeuralOrchestrator,
        chronicle: ChronicleService,
        deep_research: DeepResearch,
        crucible: Crucible,
        os_guardian: OsGuardianService,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("ceo.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create ceo dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open ceo store: {e}")))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE IF NOT EXISTS ceo_journal (
                    entry_id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    ref_id TEXT,
                    summary TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS ceo_queries (
                    job_id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS ceo_attempts (
                    qhash TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL);",
            )
            .map_err(sql_err)?;
        // The security cortex speaks with the executive's authority: it shares the
        // same device eyes, the same governance desk (its only route to a
        // destructive fix or a hardening self-modification), and the same Forge,
        // ASC-II, chronicle, and reasoner the CEO itself uses.
        let guardian = CeoGuardian::new(
            data_dir.as_ref().join("guardian"),
            device.clone(),
            os_guardian,
            desk.clone(),
            forge.clone(),
            asc2.clone(),
            chronicle.clone(),
            reasoner.clone(),
        )?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            reasoner,
            desk,
            architect,
            forge,
            asc2,
            agentic_loop,
            device,
            economy,
            active_inference,
            neural,
            chronicle,
            deep_research,
            crucible,
            guardian,
        })
    }

    // ── Continuous OS-security patrol (the security cortex) ──────────────────

    /// Run one full security patrol of the host: observe (device eyes) → assess
    /// (OS Guardian threat + DLP) → remediate. The safe half of every fix is
    /// applied immediately; every destructive action or hardening
    /// self-modification is submitted to governance and waits for the court's 51%.
    /// This is what makes the CEO *continuously monitor the OS*.
    pub async fn guardian_patrol(&self) -> Result<GuardianPatrolReport, AppError> {
        let report = self.guardian.patrol().await?;
        self.journal("guardian_patrol", Some(&report.patrol_id), &report.summary)?;
        Ok(report)
    }

    /// Carry out the security remediations the court has approved. Doubly gated:
    /// governance approval is required, and destructive actions additionally need
    /// `ASTRA_GUARDIAN_EXECUTE=on`. Returns what was executed, planned, or advised.
    pub async fn guardian_execute_remediations(&self) -> Result<Vec<RemediationRecord>, AppError> {
        let records = self.guardian.execute_approved_remediations().await?;
        for record in &records {
            if record.mode == "executed" {
                self.journal(
                    "guardian_remediation",
                    Some(&record.proposal_id),
                    &record.detail,
                )?;
            }
        }
        Ok(records)
    }

    /// Read-only handle to the security cortex for status/findings routes.
    #[must_use]
    pub fn guardian(&self) -> &CeoGuardian {
        &self.guardian
    }

    // ── Self-thinking ────────────────────────────────────────────────────────

    /// Deliberate as the chief executive about `focus`: observe the project,
    /// surface upgrade opportunities and risks, and recommend a next action.
    /// Falls back to an honest deterministic thought (rather than erroring) when
    /// no remote model is configured or the model output cannot be parsed.
    pub async fn think(&self, focus: &str) -> Result<ExecutiveThought, AppError> {
        let focus = {
            let trimmed = focus.trim();
            if trimmed.is_empty() {
                DEFAULT_FOCUS.to_string()
            } else {
                trimmed.to_string()
            }
        };
        let thought = match self.deliberate(&focus).await {
            Ok(mut draft) => ExecutiveThought {
                thought_id: new_id("ceo_thought"),
                observation: std::mem::take(&mut draft.observation),
                opportunities: std::mem::take(&mut draft.opportunities),
                risks: std::mem::take(&mut draft.risks),
                recommendation: std::mem::take(&mut draft.recommendation),
                focus: focus.clone(),
                model_backed: true,
                created_at_ms: now_ms(),
            },
            Err(error) => ExecutiveThought {
                thought_id: new_id("ceo_thought"),
                focus: focus.clone(),
                observation: format!(
                    "deliberated without a model ({error}); holding to the safe default of \
                     observing before acting"
                ),
                opportunities: Vec::new(),
                risks: vec!["no remote model is configured for executive deliberation".into()],
                recommendation: "observe; do not propose changes until a model is available".into(),
                model_backed: false,
                created_at_ms: now_ms(),
            },
        };
        self.journal(
            "thought",
            Some(&thought.thought_id),
            &format!("thought on '{}': {}", thought.focus, thought.recommendation),
        )?;
        Ok(thought)
    }

    /// The model-backed half of `think`. Errors propagate so the caller can fall
    /// back honestly.
    async fn deliberate(&self, focus: &str) -> Result<ThoughtDraft, AppError> {
        let user = EXEC_PROMPT.replace("{focus}", focus);
        // Fold the live security posture into every deliberation, so the CEO's
        // self-thinking and self-evolution are driven by the machine's real
        // security state, not project features alone.
        let user = format!(
            "Live OS security posture (from the Guardian): {}.\n\n{user}",
            self.guardian.posture_summary()
        );
        let text = self.reasoner.complete(CEO_SYSTEM.to_string(), user).await?;
        let json = extract_json(&text).ok_or_else(|| {
            AppError::Validation("executive deliberation returned no JSON object".into())
        })?;
        let draft: ThoughtDraft = serde_json::from_str(&json).map_err(|e| {
            AppError::Validation(format!("could not parse executive deliberation: {e}"))
        })?;
        Ok(draft)
    }

    // ── Self-evolving ────────────────────────────────────────────────────────

    /// One self-evolution round: think about how the project should evolve, pick
    /// the strongest human-safe opportunity, draft an upgrade proposal, and
    /// **submit it to governance**. It never approves or executes — the proposal
    /// returns `Submitted`, awaiting the court's 51%.
    pub async fn self_evolve(&self) -> Result<EvolutionOutcome, AppError> {
        let thought = self.think(EVOLVE_FOCUS).await?;
        let best = thought
            .opportunities
            .iter()
            .find(|opportunity| !opportunity.title.trim().is_empty())
            .cloned();
        match best {
            None => {
                self.journal(
                    "evolution",
                    None,
                    "self-evolution found no actionable opportunity this round",
                )?;
                Ok(EvolutionOutcome {
                    evolved: false,
                    decision: "no actionable, human-safe opportunity surfaced this round".into(),
                    thought,
                    proposal: None,
                })
            }
            Some(opportunity) => {
                let rationale = if opportunity.rationale.trim().is_empty() {
                    thought.observation.clone()
                } else {
                    opportunity.rationale.clone()
                };
                let kind = UpgradeKind::from_str_lenient(&opportunity.kind);
                // Self-evolution and self-modification are unified here: if the
                // chosen opportunity is a code change, the concrete change is
                // drafted now so the court approves the *actual* edit.
                let draft = self
                    .build_draft(kind, &opportunity.title, rationale, &opportunity.targets)
                    .await?;
                let is_self_mod = matches!(draft.kind, UpgradeKind::SelfModification);
                let proposal = self.desk.submit(draft)?;
                self.journal(
                    if is_self_mod {
                        "self_modification_submitted"
                    } else {
                        "proposal_submitted"
                    },
                    Some(&proposal.proposal_id),
                    &format!("submitted '{}' to governance", proposal.title),
                )?;
                Ok(EvolutionOutcome {
                    evolved: true,
                    decision: format!(
                        "drafted {} and submitted it to governance for approval (proposal {})",
                        if is_self_mod {
                            "a self-modification"
                        } else {
                            "an upgrade"
                        },
                        proposal.proposal_id
                    ),
                    thought,
                    proposal: Some(proposal),
                })
            }
        }
    }

    /// One directed self-modification round: ask the model for a concrete bounded
    /// change toward `goal`, then submit it to governance as a `SelfModification`
    /// proposal carrying the actual `{path, content}`. It never applies anything —
    /// the court must approve, and only then does `execute_upgrade` run the
    /// hardened gauntlet and (governance-authorized) promote.
    pub async fn self_modify(&self, goal: &str) -> Result<EvolutionOutcome, AppError> {
        let goal = goal.trim();
        if goal.is_empty() {
            return Err(AppError::Validation(
                "self-modification goal is empty".into(),
            ));
        }
        let change = self.asc2.draft_self_modification(goal).await?;
        let path = change.path.clone();
        let rationale = change.rationale.clone().unwrap_or_else(|| goal.to_string());
        let payload = serde_json::to_value(&change).ok();
        let proposal = self.desk.submit(ProposalDraft {
            title: format!("Self-modification: {goal}"),
            rationale: rationale.clone(),
            kind: UpgradeKind::SelfModification,
            targets: vec![path.clone()],
            payload,
        })?;
        self.journal(
            "self_modification_submitted",
            Some(&proposal.proposal_id),
            &format!("submitted self-modification of '{path}' to governance"),
        )?;
        let thought = ExecutiveThought {
            thought_id: new_id("ceo_thought"),
            focus: format!("self-modification: {goal}"),
            observation: format!("proposed a concrete bounded change to {path}"),
            opportunities: Vec::new(),
            risks: vec!["a code change must clear the build/test/canary gauntlet".into()],
            recommendation: rationale,
            model_backed: true,
            created_at_ms: now_ms(),
        };
        Ok(EvolutionOutcome {
            evolved: true,
            decision: format!(
                "drafted a self-modification of '{path}' and submitted it to governance \
                 for approval (proposal {})",
                proposal.proposal_id
            ),
            thought,
            proposal: Some(proposal),
        })
    }

    /// Build a proposal draft for one opportunity. Capability upgrades pass
    /// through directly; a self-modification opportunity has its concrete change
    /// drafted now (so the court sees the real edit), falling back to an advisory
    /// policy proposal if a bounded change cannot be produced.
    async fn build_draft(
        &self,
        kind: UpgradeKind,
        title: &str,
        rationale: String,
        targets: &[String],
    ) -> Result<ProposalDraft, AppError> {
        if !matches!(kind, UpgradeKind::SelfModification) {
            return Ok(ProposalDraft {
                title: title.to_string(),
                rationale,
                kind,
                targets: targets.to_vec(),
                payload: None,
            });
        }
        match self
            .asc2
            .draft_self_modification(&format!("{title}. {rationale}"))
            .await
        {
            Ok(change) => {
                let path = change.path.clone();
                Ok(ProposalDraft {
                    title: format!("Self-modification: {title}"),
                    rationale,
                    kind: UpgradeKind::SelfModification,
                    targets: vec![path],
                    payload: serde_json::to_value(&change).ok(),
                })
            }
            Err(error) => Ok(ProposalDraft {
                title: title.to_string(),
                rationale: format!("{rationale} (no bounded self-modification draft: {error})"),
                kind: UpgradeKind::Policy,
                targets: targets.to_vec(),
                payload: None,
            }),
        }
    }

    /// Layer 2 — run the actual code through an isolated adversarial red-team
    /// reviewer. Returns `true` only on an explicit PASS; any FLAG, parse failure,
    /// or model error fails closed (treated as FLAGGED). This is one independent
    /// layer — the deterministic path ban, dumb filter, and build-time lock are
    /// the hard walls; this catches semantic trojans they cannot.
    async fn red_team_review(&self, change: &SelfModDraft) -> bool {
        let user = format!(
            "Proposed change to file: {}\n\n--- CODE START ---\n{}\n--- CODE END ---\n\nDoes this \
             weaken security or harm the human in any way? Answer with one line: VERDICT: PASS or \
             VERDICT: FLAGGED.",
            change.path, change.content
        );
        match self
            .reasoner
            .complete(RED_TEAM_SYSTEM.to_string(), user)
            .await
        {
            Ok(text) => red_team_passes(&text),
            // No model / transient error -> fail closed (do not promote).
            Err(_) => false,
        }
    }

    /// Submit an explicitly-authored upgrade to governance. Same channel as
    /// `self_evolve` — it only submits; the court still has to approve.
    pub fn propose(&self, draft: ProposalDraft) -> Result<ProjectProposal, AppError> {
        let proposal = self.desk.submit(draft)?;
        self.journal(
            "proposal_submitted",
            Some(&proposal.proposal_id),
            &format!("submitted upgrade '{}' to governance", proposal.title),
        )?;
        Ok(proposal)
    }

    // ── Executing an approved upgrade ────────────────────────────────────────

    /// Execute an upgrade — spawn the agents/tools it calls for. **Gated**: this
    /// first asks the governance desk for authorization, which it grants only for
    /// a court-approved proposal. Without approval this returns an error and
    /// nothing is spawned. That is the enforcement of "only after governance
    /// approves may the CEO spawn agents and tools."
    pub async fn execute_upgrade(&self, proposal_id: &str) -> Result<ExecutionOutcome, AppError> {
        // The gate. Errors out for any proposal the court has not approved.
        let proposal = self.desk.authorize_execution(proposal_id)?;

        let mut spawned = Vec::new();
        let mut acted = false;
        let detail = match proposal.kind {
            UpgradeKind::SpawnAgent => match self
                .architect
                .compose(ComposeRequest {
                    goal: upgrade_goal(&proposal),
                })
                .await
            {
                Ok(outcome) => {
                    acted = true;
                    spawned.push(SpawnRecord {
                        name: outcome.spec.name,
                        kind: "agent".into(),
                        minted: outcome.minted,
                        detail: outcome.detail,
                    });
                    "spawned an agent through the Architect".to_string()
                }
                Err(error) => format!("agent spawn failed (proposal stays approved): {error}"),
            },
            UpgradeKind::SpawnTool => match self
                .forge
                .forge(ForgeRequest {
                    objective: upgrade_goal(&proposal),
                })
                .await
            {
                Ok(outcome) => {
                    acted = true;
                    spawned.push(SpawnRecord {
                        name: outcome.tool.name,
                        kind: "tool".into(),
                        minted: outcome.minted,
                        detail: outcome.detail,
                    });
                    "forged a tool through the Forge".to_string()
                }
                Err(error) => format!("tool forge failed (proposal stays approved): {error}"),
            },
            UpgradeKind::SelfModification => {
                // The court approved this concrete change; reconstruct it from the
                // proposal payload and run it through ASC-II's hardened gauntlet.
                // `authorized = true` BECAUSE governance approved — that, not a
                // human or an env flag, is the promotion permission.
                match proposal
                    .payload
                    .clone()
                    .and_then(|value| serde_json::from_value::<SelfModDraft>(value).ok())
                {
                    Some(change) => {
                        // Layer 2 — the adversarial red-team LLM. Even though the
                        // court's deterministic screen already passed this, an
                        // isolated hostile reviewer inspects the actual code for
                        // semantic trojans the string filter cannot see. Fail
                        // closed: anything not an explicit PASS is FLAGGED.
                        if !self.red_team_review(&change).await {
                            self.journal(
                                "self_modification_flagged",
                                Some(proposal_id),
                                &format!("red-team FLAGGED the change to '{}'", change.path),
                            )?;
                            "Layer 2 (adversarial red-team) FLAGGED the change as unsafe; \
                             refused before build (proposal stays approved for human review)"
                                .to_string()
                        } else {
                            let request = AutonomousSelfModRequest {
                                goal: proposal.title.clone(),
                                workspace_path: self_mod_workspace(),
                                current_binary: std::env::current_exe()
                                    .map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                            };
                            // Layers 0 + 3 (path ban backstop + build-time lock) run
                            // inside this gauntlet, on the real git diff.
                            match self
                                .asc2
                                .apply_self_modification(request, change, true)
                                .await
                            {
                                Ok(outcome) => {
                                    acted = outcome.applied;
                                    spawned.push(SpawnRecord {
                                        name: outcome.proposed_path.clone().unwrap_or_default(),
                                        kind: "self_modification".into(),
                                        minted: outcome.promoted,
                                        detail: outcome.outcome.clone(),
                                    });
                                    outcome.outcome
                                }
                                Err(error) => format!(
                                    "self-modification failed (proposal stays approved): {error}"
                                ),
                            }
                        }
                    }
                    None => "self-modification proposal is missing its change payload".to_string(),
                }
            }
            UpgradeKind::Policy | UpgradeKind::Other => {
                acted = true;
                "policy/operational upgrade enacted (no capability spawn)".to_string()
            }
        };

        // Spend the authorization only if the CEO actually acted. A hard failure
        // (e.g. no model) leaves the proposal `Approved` so it can be retried.
        if acted {
            self.desk.mark_executed(proposal_id)?;
        }
        self.journal(
            "upgrade_executed",
            Some(proposal_id),
            &format!("{}: {detail}", proposal.title),
        )?;

        Ok(ExecutionOutcome {
            proposal_id: proposal.proposal_id,
            kind: proposal.kind.as_str().into(),
            authorized: true,
            spawned,
            detail,
        })
    }

    // ── Serving a user query (the executive front door) ──────────────────────

    /// Accept a user query and orchestrate the whole project to answer it.
    /// Returns immediately with a queued [`QueryJob`]; the heavy orchestration
    /// (model calls, the verified loop, proof, continuous re-verification) runs on
    /// a dedicated background thread. Poll [`get_query`](Self::get_query) for the
    /// verified answer — the async shape agreed with the owner.
    pub fn submit_query(&self, query: &str) -> Result<QueryJob, AppError> {
        let query = query.trim().to_string();
        if query.is_empty() {
            return Err(AppError::Validation("query must not be empty".into()));
        }
        let now = now_ms();
        let job = QueryJob {
            job_id: new_id("ceo_query"),
            query: query.clone(),
            status: QueryStatus::Queued,
            answer: None,
            error: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.save_query(&job)?;
        self.journal(
            "query_received",
            Some(&job.job_id),
            &format!("user query: {}", truncate(&query, 96)),
        )?;
        // Run the orchestration on its own thread+runtime so a minutes-long answer
        // never blocks an actix worker, and so the pipeline may call the organs'
        // blocking methods (device-run checks) freely.
        let ceo = self.clone();
        let job_id = job.job_id.clone();
        std::thread::Builder::new()
            .name("astra-ceo-query".into())
            .spawn(move || {
                actix_web::rt::System::new().block_on(async move {
                    ceo.run_query(&job_id, &query).await;
                });
            })
            .map_err(|e| AppError::Internal(format!("failed to spawn ceo query worker: {e}")))?;
        Ok(job)
    }

    /// The background worker: mark the job running, orchestrate, finalize.
    async fn run_query(&self, job_id: &str, query: &str) {
        if let Ok(mut job) = self.get_query(job_id) {
            job.status = QueryStatus::Running;
            job.updated_at_ms = now_ms();
            let _ = self.save_query(&job);
        }
        let outcome = self.orchestrate_query(query).await;
        if let Ok(mut job) = self.get_query(job_id) {
            match outcome {
                Ok(answer) => {
                    job.status = QueryStatus::Answered;
                    job.answer = Some(answer);
                }
                Err(error) => {
                    job.status = QueryStatus::Failed;
                    job.error = Some(error.to_string());
                }
            }
            job.updated_at_ms = now_ms();
            let _ = self.save_query(&job);
            let _ = self.journal(
                "query_answered",
                Some(job_id),
                &format!("served query job {job_id} ({:?})", job.status),
            );
        }
    }

    /// The orchestration — marshal the project's organs to produce a quality,
    /// accuracy-checked answer:
    ///   1. **recall** a minted answer from the proof economy (never re-work proven
    ///      knowledge);
    ///   2. **route** the query through the neural orchestrator to engage organs;
    ///   3. **execute** the verified agentic loop, which mints a claim from any
    ///      verified result;
    ///   4. **verify** by seeding the answer as an active-inference belief under
    ///      continuous re-verification, and set a tiered stance (assert iff a check
    ///      backs it, else hedge) with a confidence;
    ///   5. on a capability gap, **propose** the missing tool/agent to governance
    ///      (never spawn on the fly — the court's 51% gate is preserved);
    ///   6. **reinforce** the connectome by whether the answer verified, and
    ///      **remember** the episode in the chronicle.
    pub async fn orchestrate_query(&self, query: &str) -> Result<QueryAnswer, AppError> {
        let mut steps: Vec<QueryStep> = Vec::new();
        let created_at_ms = now_ms();
        let qhash = query_key(query);

        // 1. Recall settled knowledge first.
        if let Ok(Some((claim, score))) = self.economy.find_minted_semantic(query, RECALL_THRESHOLD)
        {
            steps.push(QueryStep::new(
                "recall",
                format!(
                    "matched minted claim {} (similarity {score:.2})",
                    claim.claim_id
                ),
            ));
            self.clear_attempt(&qhash); // solved knowledge now serves it
            return Ok(QueryAnswer {
                query: query.to_string(),
                answer: claim.statement.clone(),
                stance: "asserted".into(),
                confidence: (0.8 + score * 0.19).min(0.99),
                verified: true,
                evidence: vec![
                    format!("minted_claim:{}", claim.claim_id),
                    claim.last_verification.clone(),
                ],
                route: vec!["proof_economy".into()],
                steps,
                proposed_capability: None,
                created_at_ms,
            });
        }
        steps.push(QueryStep::new(
            "recall",
            "no minted answer matched; engaging the project".into(),
        ));

        // 2. Route through the connectome (best-effort).
        let episode = self
            .neural
            .stimulate(StimulusRequest {
                origin: "ceo_query".into(),
                seeds: vec![
                    StimulusSeed {
                        node: "ceo".into(),
                        intensity: 1.0,
                    },
                    StimulusSeed {
                        node: "active_inference".into(),
                        intensity: 0.5,
                    },
                ],
                hops: Some(3),
                threshold: Some(0.12),
                max_fire: Some(8),
            })
            .ok();
        let route: Vec<String> = episode
            .as_ref()
            .map(|plan| plan.firing.iter().map(|f| f.label.clone()).collect())
            .unwrap_or_default();
        steps.push(QueryStep::new(
            "route",
            if route.is_empty() {
                "connectome idle".into()
            } else {
                format!("engaged: {}", route.join(", "))
            },
        ));

        // 2.5 Decide HOW to approach the request — let the model reason about the
        // right method instead of keyword-matching. Each approach engages the
        // organs that fit and returns a verified, high-quality result:
        //   * research  → deep-research: decompose, spawn a sub-agent per
        //                 sub-question, scrape exact data, synthesize a cited answer.
        //   * diagnose  → the Crucible: root-cause a failure/why-question and
        //                 propose the proven real fix.
        //   * solve     → the verified agentic loop (below).
        // If this exact question failed before, do NOT repeat the approach that
        // already failed — escalate to one not yet tried. Once every approach has
        // been exhausted, route to the Crucible to diagnose WHY answering keeps
        // failing (missing capability / data / a malformed question), instead of
        // looping on the same dead end.
        if is_report_page_edit_request(query) {
            steps.push(QueryStep::new(
                "classify",
                "approach: Solve via deterministic document delegation".into(),
            ));
            let answer =
                self.answer_via_document_delegates(query, route, episode, steps, created_at_ms)?;
            self.note_attempt(&qhash, Approach::Solve, &answer);
            return Ok(answer);
        }

        let classified = self.classify_approach(query).await;
        let prior = self.load_attempt(&qhash);
        let (mut approach, exhausted) = match prior.as_ref() {
            Some(p) if !p.tried.is_empty() => escalated_approach(classified, &p.tried),
            _ => (classified, false),
        };
        if exhausted {
            approach = Approach::Diagnose; // self-diagnose the repeated failure
        }
        match prior.as_ref() {
            Some(p) => steps.push(QueryStep::new(
                "retry",
                format!(
                    "previously unresolved ({} attempt(s); tried {}); {} → {approach:?}",
                    p.attempts,
                    p.tried.join("+"),
                    if exhausted {
                        "approaches exhausted, self-diagnosing"
                    } else {
                        "escalating approach"
                    },
                ),
            )),
            None => steps.push(QueryStep::new(
                "classify",
                format!("approach: {approach:?}"),
            )),
        }
        // When approaches are exhausted, hand the Crucible the failure history so
        // it reasons about the repeated failure itself, not the surface question.
        let failure_context = match prior.as_ref() {
            Some(p) if exhausted => format!(
                "This exact question has failed {} time(s) via approaches [{}]. Diagnose WHY \
                 answering it keeps failing and what capability or data is missing.",
                p.attempts,
                p.tried.join(", ")
            ),
            _ => String::new(),
        };

        match approach {
            Approach::Research => {
                let answer = self
                    .answer_via_deep_research(query, route, episode, steps, created_at_ms)
                    .await?;
                self.note_attempt(&qhash, approach, &answer);
                return Ok(answer);
            }
            Approach::Diagnose => {
                let answer = self
                    .answer_via_crucible(
                        query,
                        &failure_context,
                        route,
                        episode,
                        steps,
                        created_at_ms,
                    )
                    .await?;
                self.note_attempt(&qhash, approach, &answer);
                return Ok(answer);
            }
            Approach::Solve => {} // fall through to the verified agentic loop
        }

        // 3. Execute the verified agentic loop (the primary answer engine).
        // The query loop is granted the read-only introspection set PLUS
        // `shell_exec`, so the CEO can actually act on the host to answer a
        // request (search the filesystem, probe state) — not just read one known
        // path. Mutating actions still must prove a postcondition through the
        // loop's verification, and every op is audited by the device supervisor.
        let (answer, mut verified, claim_id, status) = match self
            .agentic_loop
            .run(LoopRequest {
                objective: query.to_string(),
                allow_tools: vec![
                    "system_info".into(),
                    "process_list".into(),
                    "disk_usage".into(),
                    "fs_list".into(),
                    "fs_read".into(),
                    "web_search".into(),
                    "deep_crawl".into(),
                    "http_get".into(),
                    "shell_exec".into(),
                ],
                max_iterations: 10,
            })
            .await
        {
            Ok(result) => {
                steps.push(QueryStep::new(
                    "execute",
                    format!(
                        "agentic loop: {} in {} step(s){}",
                        result.status,
                        result.iterations_used,
                        if result.all_verified {
                            ", all verified"
                        } else {
                            ""
                        }
                    ),
                ));
                let verified = result.all_verified && result.status == "completed";
                (
                    result.final_answer,
                    verified,
                    result.claim_id,
                    result.status,
                )
            }
            Err(error) => {
                steps.push(QueryStep::new(
                    "execute",
                    format!("agentic loop could not run: {error}"),
                ));
                (
                    format!("I could not produce a verified answer to: {query}"),
                    false,
                    None,
                    "failed".to_string(),
                )
            }
        };

        // 4. Verification + continuous re-verification + tiered confidence.
        let mut confidence = if verified { 0.9 } else { 0.45 };
        let mut evidence: Vec<String> = Vec::new();
        if let Some(claim_id) = &claim_id {
            evidence.push(format!("claim:{claim_id}"));
            if let Ok(claim) = self.economy.get_claim(claim_id) {
                if let Ok(belief) = self.active_inference.register_belief(RegisterBelief {
                    statement: answer.clone(),
                    check: claim.verification.clone(),
                    expect_holding: Some(true),
                    prior_precision: None,
                }) {
                    steps.push(QueryStep::new(
                        "verify",
                        format!(
                            "seeded belief {} under continuous re-verification",
                            belief.belief_id
                        ),
                    ));
                    let p = 1.0 / (1.0 + (-belief.precision).exp());
                    confidence = ((confidence + p) / 2.0).clamp(0.0, 0.99);
                    if !belief.predicted_holding {
                        verified = false;
                    }
                }
            }
        }

        // 5. Capability gap -> propose to governance (existing + propose-new).
        let proposed_capability = if verified {
            None
        } else {
            match self.detect_capability_gap(query, &answer, &status).await {
                Some(gap) => self.propose_capability(gap, &mut steps),
                None => None,
            }
        };

        // 6. Reinforce the connectome + remember the episode.
        if let Some(plan) = &episode {
            let reward = if verified { 1.0 } else { -0.3 };
            let _ = self.neural.reinforce(ReinforceRequest {
                episode_id: plan.episode_id.clone(),
                reward,
            });
        }
        let stance = if verified { "asserted" } else { "hedged" };
        let _ = self.chronicle.record(RecordEpisodeRequest {
            kind: EpisodeKind::Event,
            content: format!(
                "CEO served a query ({stance}, confidence {confidence:.2}): {}",
                truncate(query, 120)
            ),
            rationale: Some(truncate(&answer, 200)),
            source: Some("ceo".into()),
            source_ref: None,
            tags: vec!["ceo".into(), "query".into(), stance.into()],
            importance: Some(if verified { 0.6 } else { 0.4 }),
            due_at_ms: None,
            tenant_scope: TenantScope::Global,
        });

        let result = QueryAnswer {
            query: query.to_string(),
            answer,
            stance: stance.into(),
            confidence,
            verified,
            evidence,
            route,
            steps,
            proposed_capability,
            created_at_ms,
        };
        self.note_attempt(&qhash, approach, &result);
        Ok(result)
    }

    /// Deterministic CEO delegation for a concrete operational task:
    /// find an exact `report` document, gate ambiguity, edit only a single PDF
    /// candidate, and verify the page-count delta. This gives the CEO useful
    /// "sub-agent" behavior even when the model-driven planner is not configured.
    fn answer_via_document_delegates(
        &self,
        query: &str,
        mut route: Vec<String>,
        episode: Option<crate::neural_orchestration::OrchestrationPlan>,
        mut steps: Vec<QueryStep>,
        created_at_ms: i64,
    ) -> Result<QueryAnswer, AppError> {
        route.push("Deterministic document delegates".into());
        steps.push(QueryStep::new(
            "delegate",
            "assigned FileSearchAgent -> RiskGateAgent -> DocumentAgent -> VerificationAgent"
                .into(),
        ));

        let search = self.run_device(
            "shell_exec",
            json!({
                "command": report_document_search_command(),
                "timeout_ms": 30_000
            }),
        )?;
        let mut evidence = op_evidence("file_search", &search);
        let candidates = parse_document_candidates(&search);
        steps.push(QueryStep::new(
            "file_search",
            format!(
                "FileSearchAgent found {} exact paged candidate(s) named report",
                candidates.len()
            ),
        ));

        let (answer, verified, confidence) = match candidates.len() {
            0 => {
                steps.push(QueryStep::new(
                    "risk_gate",
                    "RiskGateAgent abstained: no exact PDF/DOCX document named report was found"
                        .into(),
                ));
                (
                    "I could not find an exact paged document named `report` in the standard user document locations, so I did not edit anything.".to_string(),
                    true,
                    0.86,
                )
            }
            n if n > 1 => {
                steps.push(QueryStep::new(
                    "risk_gate",
                    format!(
                        "RiskGateAgent abstained: {n} exact candidates found; destructive page removal needs one target"
                    ),
                ));
                (
                    format!(
                        "I found multiple exact `report` documents, so I did not remove pages. Candidates:\n{}",
                        format_candidates(&candidates)
                    ),
                    true,
                    0.9,
                )
            }
            _ => {
                let candidate = &candidates[0];
                if candidate.extension != "pdf" {
                    steps.push(QueryStep::new(
                        "risk_gate",
                        format!(
                            "RiskGateAgent abstained: {} is a DOCX; page boundaries are layout-dependent",
                            candidate.path
                        ),
                    ));
                    (
                        format!(
                            "I found one exact `report` document, but it is a DOCX: `{}`. DOCX page boundaries depend on Word layout/rendering, so I did not delete pages without a renderer-backed document workflow.",
                            candidate.path
                        ),
                        true,
                        0.84,
                    )
                } else {
                    steps.push(QueryStep::new(
                        "risk_gate",
                        format!("RiskGateAgent approved one PDF target: {}", candidate.path),
                    ));
                    match self.remove_first_two_pdf_pages(candidate, &mut steps, &mut evidence) {
                        Ok(edit) => (
                            format!(
                                "DocumentAgent removed the first two pages from `{}`.\nBackup: `{}`\nPages: {} -> {}",
                                candidate.path,
                                edit.backup_path,
                                edit.before_pages,
                                edit.after_pages
                            ),
                            true,
                            0.96,
                        ),
                        Err(error) => {
                            steps.push(QueryStep::new(
                                "verify",
                                format!("VerificationAgent could not prove the edit: {error}"),
                            ));
                            (
                                format!(
                                    "I found one PDF target (`{}`), but I did not complete the page-removal edit: {error}",
                                    candidate.path
                                ),
                                false,
                                0.45,
                            )
                        }
                    }
                }
            }
        };

        let stance = if verified { "asserted" } else { "hedged" };
        if let Some(plan) = &episode {
            let _ = self.neural.reinforce(ReinforceRequest {
                episode_id: plan.episode_id.clone(),
                reward: if verified { 1.0 } else { -0.2 },
            });
        }
        let _ = self.chronicle.record(RecordEpisodeRequest {
            kind: EpisodeKind::Event,
            content: format!(
                "CEO delegated document task ({stance}, confidence {confidence:.2}): {}",
                truncate(query, 120)
            ),
            rationale: Some(truncate(&answer, 200)),
            source: Some("ceo".into()),
            source_ref: None,
            tags: vec![
                "ceo".into(),
                "query".into(),
                "delegation".into(),
                "document".into(),
                stance.into(),
            ],
            importance: Some(if verified { 0.65 } else { 0.45 }),
            due_at_ms: None,
            tenant_scope: TenantScope::Global,
        });

        Ok(QueryAnswer {
            query: query.to_string(),
            answer,
            stance: stance.into(),
            confidence,
            verified,
            evidence,
            route,
            steps,
            proposed_capability: None,
            created_at_ms,
        })
    }

    fn remove_first_two_pdf_pages(
        &self,
        candidate: &DocumentCandidate,
        steps: &mut Vec<QueryStep>,
        evidence: &mut Vec<String>,
    ) -> Result<PdfEditReport, AppError> {
        let script_path = std::env::temp_dir().join(format!("astra_pdf_trim_{}.py", now_ms()));
        let script_path_text = script_path.to_string_lossy().into_owned();
        let backup_path = backup_pdf_path(&candidate.path);
        let write = self.run_device(
            "fs_write",
            json!({
                "path": script_path_text,
                "content": pdf_trim_script()
            }),
        )?;
        evidence.extend(op_evidence("document_agent_script", &write));

        let command = format!(
            "python {} {} {}",
            shell_arg(&script_path.to_string_lossy()),
            shell_arg(&candidate.path),
            shell_arg(&backup_path)
        );
        let edit = self.run_device(
            "shell_exec",
            json!({
                "command": command,
                "timeout_ms": 30_000
            }),
        )?;
        evidence.extend(op_evidence("document_edit", &edit));
        steps.push(QueryStep::new(
            "document_edit",
            "DocumentAgent created a backup and attempted PDF page removal through supervised shell execution"
                .into(),
        ));

        let exit_code = edit.get("exit_code").and_then(Value::as_i64).unwrap_or(-1);
        let stdout = edit.get("stdout").and_then(Value::as_str).unwrap_or("");
        let parsed = serde_json::from_str::<Value>(stdout.trim()).unwrap_or_else(|_| json!({}));
        if exit_code != 0 || !parsed.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            let detail = parsed
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| edit.get("stderr").and_then(Value::as_str))
                .unwrap_or("PDF edit command failed");
            return Err(AppError::Validation(detail.to_string()));
        }
        let report = PdfEditReport {
            backup_path: parsed
                .get("backup")
                .and_then(Value::as_str)
                .unwrap_or(&backup_path)
                .to_string(),
            before_pages: parsed
                .get("before_pages")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            after_pages: parsed
                .get("after_pages")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        };
        if report.before_pages < 3 || report.after_pages + 2 != report.before_pages {
            return Err(AppError::Validation(format!(
                "page-count verification failed ({} -> {})",
                report.before_pages, report.after_pages
            )));
        }
        steps.push(QueryStep::new(
            "verify",
            format!(
                "VerificationAgent proved page count changed {} -> {} and backup exists",
                report.before_pages, report.after_pages
            ),
        ));
        evidence.push(format!("backup:{}", report.backup_path));
        Ok(report)
    }

    fn run_device(&self, tool: &str, input: Value) -> Result<Value, AppError> {
        self.device
            .execute(tool, &input)
            .map_err(|error| AppError::Validation(format!("delegate device op failed: {error}")))
    }

    /// Answer a research/data question by spawning sub-agents through the
    /// [deep-research](crate::deep_research) orchestrator — decompose, scrape exact
    /// data from primary sources, synthesize — then fold the cited result into a
    /// [`QueryAnswer`] (and reinforce the connectome + remember it).
    async fn answer_via_deep_research(
        &self,
        query: &str,
        route: Vec<String>,
        episode: Option<crate::neural_orchestration::OrchestrationPlan>,
        mut steps: Vec<QueryStep>,
        created_at_ms: i64,
    ) -> Result<QueryAnswer, AppError> {
        let report = self
            .deep_research
            .investigate(ResearchRequest {
                question: query.to_string(),
                max_subquestions: Some(3),
                cross_check: Some(false),
            })
            .await?;
        steps.push(QueryStep::new(
            "research",
            format!(
                "spawned {} sub-agent(s) across {} sub-question(s); {} source(s) scraped",
                report.sub_agents,
                report.subquestions.len(),
                report.sources.len()
            ),
        ));
        let verified =
            report.confidence >= 0.7 && report.findings.iter().any(|finding| finding.verified);
        let stance = if verified { "asserted" } else { "hedged" };
        if let Some(plan) = &episode {
            let reward = if verified { 1.0 } else { -0.2 };
            let _ = self.neural.reinforce(ReinforceRequest {
                episode_id: plan.episode_id.clone(),
                reward,
            });
        }
        let _ = self.chronicle.record(RecordEpisodeRequest {
            kind: EpisodeKind::Event,
            content: format!(
                "CEO deep-researched ({stance}, confidence {:.2}): {}",
                report.confidence,
                truncate(query, 120)
            ),
            rationale: Some(truncate(&report.answer, 200)),
            source: Some("ceo".into()),
            source_ref: None,
            tags: vec!["ceo".into(), "research".into(), stance.into()],
            importance: Some(0.6),
            due_at_ms: None,
            tenant_scope: TenantScope::Global,
        });
        Ok(QueryAnswer {
            query: query.to_string(),
            answer: report.answer,
            stance: stance.into(),
            confidence: report.confidence,
            verified,
            evidence: report.sources,
            route,
            steps,
            proposed_capability: None,
            created_at_ms,
        })
    }

    /// Reason about the best approach for a request. Falls back to a keyword
    /// heuristic (and `Solve`) when no model is configured, so the front door is
    /// always decisive.
    async fn classify_approach(&self, query: &str) -> Approach {
        match self
            .reasoner
            .complete(
                CLASSIFY_SYSTEM.to_string(),
                format!("Request: {query}\nApproach?"),
            )
            .await
        {
            Ok(reply) => approach_from_reply(&reply, query),
            Err(_) => {
                if is_research_query(query) {
                    Approach::Research
                } else {
                    Approach::Solve
                }
            }
        }
    }

    /// Answer a request that is really a problem to diagnose by routing to the
    /// [Crucible](crate::crucible): hypothesize competing causes, confirm the cause
    /// with real read-only checks, and propose the proven real fix — folded into a
    /// [`QueryAnswer`] (and reinforce the connectome + remember it). The proven fix
    /// is staked into the proof economy so it carries a machine-checkable receipt.
    async fn answer_via_crucible(
        &self,
        query: &str,
        context: &str,
        route: Vec<String>,
        episode: Option<crate::neural_orchestration::OrchestrationPlan>,
        mut steps: Vec<QueryStep>,
        created_at_ms: i64,
    ) -> Result<QueryAnswer, AppError> {
        let inquiry = self
            .crucible
            .investigate(InvestigateRequest {
                problem: query.to_string(),
                context: context.to_string(),
                mint: true,
            })
            .await?;
        let root_cause = inquiry
            .root_cause
            .clone()
            .unwrap_or_else(|| "could not be confirmed".into());
        let solution = inquiry.solution.clone();
        let proven = solution.as_ref().and_then(|s| s.proven).unwrap_or(false);
        let claim_id = solution.as_ref().and_then(|s| s.claim_id.clone());
        steps.push(QueryStep::new(
            "diagnose",
            format!(
                "crucible: {:?}; {} hypothesis(es); root cause: {root_cause}",
                inquiry.status,
                inquiry.hypotheses.len()
            ),
        ));

        let answer = match solution.as_ref() {
            Some(solution) => format!(
                "Why it happens — {root_cause}.\n\nReal solution — {}",
                solution.statement
            ),
            None => format!("Why it happens — {root_cause}. (No verified fix was produced.)"),
        };
        let verified = matches!(inquiry.status, InquiryStatus::Resolved) && proven;
        let stance = if verified { "asserted" } else { "hedged" };
        let confidence = if verified {
            0.9
        } else if matches!(inquiry.status, InquiryStatus::Diagnosed) {
            0.6
        } else {
            0.4
        };
        let mut evidence: Vec<String> = Vec::new();
        if let Some(claim_id) = &claim_id {
            evidence.push(format!("claim:{claim_id}"));
        }
        evidence.extend(inquiry.trace.iter().take(5).cloned());

        if let Some(plan) = &episode {
            let reward = if verified { 1.0 } else { -0.2 };
            let _ = self.neural.reinforce(ReinforceRequest {
                episode_id: plan.episode_id.clone(),
                reward,
            });
        }
        let _ = self.chronicle.record(RecordEpisodeRequest {
            kind: EpisodeKind::Event,
            content: format!(
                "CEO diagnosed ({stance}, confidence {confidence:.2}): {}",
                truncate(query, 120)
            ),
            rationale: Some(truncate(&answer, 200)),
            source: Some("ceo".into()),
            source_ref: None,
            tags: vec!["ceo".into(), "diagnose".into(), stance.into()],
            importance: Some(if verified { 0.6 } else { 0.4 }),
            due_at_ms: None,
            tenant_scope: TenantScope::Global,
        });

        Ok(QueryAnswer {
            query: query.to_string(),
            answer,
            stance: stance.into(),
            confidence,
            verified,
            evidence,
            route,
            steps,
            proposed_capability: None,
            created_at_ms,
        })
    }

    /// Submit a detected capability gap to governance as a spawn proposal. It is
    /// only *proposed* — the court must approve before anything is ever spawned.
    fn propose_capability(
        &self,
        gap: CapabilityGap,
        steps: &mut Vec<QueryStep>,
    ) -> Option<ProposedCapability> {
        let (kind, label) = if gap.kind.eq_ignore_ascii_case("agent") {
            (UpgradeKind::SpawnAgent, "agent")
        } else {
            (UpgradeKind::SpawnTool, "tool")
        };
        let draft = ProposalDraft {
            title: format!("Add {label}: {}", gap.name),
            rationale: format!(
                "A user query needs higher answer quality: {}",
                gap.rationale
            ),
            kind,
            targets: vec![gap.name.clone()],
            payload: None,
        };
        match self.desk.submit(draft) {
            Ok(proposal) => {
                let _ = self.journal(
                    "query_capability_proposed",
                    Some(&proposal.proposal_id),
                    &format!("proposed '{}' from a user query", gap.name),
                );
                steps.push(QueryStep::new(
                    "propose",
                    format!(
                        "submitted capability proposal {} to governance",
                        proposal.proposal_id
                    ),
                ));
                Some(ProposedCapability {
                    proposal_id: proposal.proposal_id,
                    title: proposal.title,
                    kind: proposal.kind.as_str().into(),
                })
            }
            Err(error) => {
                steps.push(QueryStep::new(
                    "propose",
                    format!("capability proposal rejected: {error}"),
                ));
                None
            }
        }
    }

    /// Ask the model whether a NEW tool/agent would materially raise answer
    /// quality. Consulted only when the loop fell short, so governance is never
    /// spammed for queries the project already serves well. Returns `None` on no
    /// gap, an unparseable reply, or no configured model.
    async fn detect_capability_gap(
        &self,
        query: &str,
        answer: &str,
        status: &str,
    ) -> Option<CapabilityGap> {
        let user = format!(
            "User query: {query}\nBest answer produced now: {}\nAgentic-loop status: {status}\n\
             Would a NEW, nameable tool or agent have produced a materially better, more \
             verifiable answer?",
            truncate(answer, 400)
        );
        let raw = self
            .reasoner
            .complete(GAP_SYSTEM.to_string(), user)
            .await
            .ok()?;
        let json = extract_json(&raw)?;
        let gap: CapabilityGap = serde_json::from_str(&json).ok()?;
        (gap.needs_capability && !gap.name.trim().is_empty()).then_some(gap)
    }

    /// Record the outcome of an attempt on a question. A verified answer clears
    /// the failure memory (next time, recall serves it); an unverified one bumps
    /// the attempt count and remembers the approach so a repeat escalates.
    fn note_attempt(&self, qhash: &str, approach: Approach, answer: &QueryAnswer) {
        if answer.verified {
            self.clear_attempt(qhash);
            return;
        }
        let mut record = self.load_attempt(qhash).unwrap_or_else(|| QueryAttempt {
            query: answer.query.clone(),
            attempts: 0,
            tried: Vec::new(),
            last_status: String::new(),
            last_at_ms: 0,
        });
        record.attempts += 1;
        let tag = approach.tag().to_string();
        if !record.tried.contains(&tag) {
            record.tried.push(tag);
        }
        record.last_status = answer.stance.clone();
        record.last_at_ms = now_ms();
        let _ = self.save_attempt(qhash, &record);
    }

    fn load_attempt(&self, qhash: &str) -> Option<QueryAttempt> {
        let payload: Option<String> = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM ceo_attempts WHERE qhash=?1",
                [qhash],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        payload.and_then(|p| serde_json::from_str(&p).ok())
    }

    fn save_attempt(&self, qhash: &str, record: &QueryAttempt) -> Result<(), AppError> {
        let payload = serde_json::to_string(record)
            .map_err(|e| AppError::Internal(format!("ceo attempt serialize error: {e}")))?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO ceo_attempts (qhash, payload, updated_at_ms) VALUES (?1,?2,?3)",
                params![qhash, payload, record.last_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn clear_attempt(&self, qhash: &str) {
        let _ = self
            .store
            .lock()
            .execute("DELETE FROM ceo_attempts WHERE qhash=?1", [qhash]);
    }

    pub fn get_query(&self, job_id: &str) -> Result<QueryJob, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM ceo_queries WHERE job_id=?1",
                [job_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("query job {job_id}")))?;
        serde_json::from_str(&payload)
            .map_err(|e| AppError::Internal(format!("failed to decode query job: {e}")))
    }

    pub fn query_jobs(&self, limit: usize) -> Result<Vec<QueryJob>, AppError> {
        let limit = limit.clamp(1, 500);
        let store = self.store.lock();
        let mut statement = store
            .prepare("SELECT payload FROM ceo_queries ORDER BY created_at_ms DESC LIMIT ?1")
            .map_err(sql_err)?;
        let rows = statement
            .query_map(params![limit], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(
                serde_json::from_str(&row.map_err(sql_err)?)
                    .map_err(|e| AppError::Internal(format!("failed to decode query job: {e}")))?,
            );
        }
        Ok(out)
    }

    fn save_query(&self, job: &QueryJob) -> Result<(), AppError> {
        let payload = serde_json::to_string(job)
            .map_err(|e| AppError::Internal(format!("failed to encode query job: {e}")))?;
        let status = serde_json::to_value(job.status)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "queued".into());
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO ceo_queries (job_id, payload, status, created_at_ms) VALUES (?1,?2,?3,?4)",
                params![job.job_id, payload, status, job.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    // ── Read-only ────────────────────────────────────────────────────────────

    pub fn status(&self) -> Result<CeoStatus, AppError> {
        let thoughts = self
            .store
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM ceo_journal WHERE kind = 'thought'",
                [],
                |row| row.get::<_, usize>(0),
            )
            .map_err(sql_err)?;
        let proposals = self.desk.proposals(1000)?;
        let proposals_submitted = proposals.len();
        let proposals_executed = proposals
            .iter()
            .filter(|p| p.status == ProposalStatus::Executed)
            .count();
        let proposals_approved = proposals
            .iter()
            .filter(|p| {
                matches!(
                    p.status,
                    ProposalStatus::Approved | ProposalStatus::Executed
                )
            })
            .count();
        Ok(CeoStatus {
            office: "chief_executive_officer".into(),
            authority_pct: CEO_VOTING_POWER,
            governance_pct: GOVERNANCE_VOTING_POWER,
            subordinate_to: "supreme_court_of_justice".into(),
            controls: vec![
                "agents (the Architect)".into(),
                "tools (the Forge)".into(),
                "the verified agentic loop".into(),
                "the project's day-to-day operation".into(),
            ],
            no_authority_over: vec![
                "the border (sandbox)".into(),
                "the governance (Supreme Court of Justice)".into(),
                "the army".into(),
                "the police".into(),
            ],
            self_thinking: true,
            self_evolving: true,
            thoughts,
            proposals_submitted,
            proposals_approved,
            proposals_executed,
        })
    }

    pub fn journal_entries(&self, limit: usize) -> Result<Vec<JournalEntry>, AppError> {
        let limit = limit.clamp(1, 1000);
        let store = self.store.lock();
        let mut statement = store
            .prepare(
                "SELECT entry_id, kind, ref_id, summary, created_at_ms
                 FROM ceo_journal ORDER BY created_at_ms DESC LIMIT ?1",
            )
            .map_err(sql_err)?;
        let rows = statement
            .query_map(params![limit], |row| {
                Ok(JournalEntry {
                    entry_id: row.get(0)?,
                    kind: row.get(1)?,
                    ref_id: row.get(2)?,
                    summary: row.get(3)?,
                    created_at_ms: row.get(4)?,
                })
            })
            .map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)
    }

    fn journal(&self, kind: &str, ref_id: Option<&str>, summary: &str) -> Result<(), AppError> {
        self.store
            .lock()
            .execute(
                "INSERT INTO ceo_journal (entry_id, kind, ref_id, summary, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![new_id("ceo_entry"), kind, ref_id, summary, now_ms()],
            )
            .map_err(sql_err)?;
        Ok(())
    }
}

fn upgrade_goal(proposal: &ProjectProposal) -> String {
    if proposal.rationale.trim().is_empty() {
        proposal.title.clone()
    } else {
        format!("{}. {}", proposal.title, proposal.rationale)
    }
}

/// Heuristic: does this query call for deep, multi-source data gathering (so it
/// should fan out sub-agents) rather than a single host action? Kept narrow so
/// task queries ("find a file") still take the single-loop path.
/// A stable key for a question, so re-asking it (modulo whitespace/case) maps to
/// the same failure-memory record.
fn query_key(query: &str) -> String {
    let normalized = query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    crate::common::sha3_hex(normalized)
}

/// Map the model's one-word approach reply (plus the query as a fallback) to an
/// [`Approach`]. Pure, so the routing decision is unit-tested without standing up
/// the whole executive. A chatty/empty reply falls back to the research keyword
/// heuristic, then to `Solve` — the front door is always decisive.
fn approach_from_reply(reply: &str, query: &str) -> Approach {
    let raw = reply.to_ascii_lowercase();
    if raw.contains("research") {
        Approach::Research
    } else if raw.contains("diagnose")
        || raw.contains("debug")
        || raw.contains("root cause")
        || raw.contains("root-cause")
    {
        Approach::Diagnose
    } else if raw.contains("solve") {
        Approach::Solve
    } else if is_research_query(query) {
        Approach::Research
    } else {
        Approach::Solve
    }
}

fn is_research_query(query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    const SIGNALS: &[&str] = &[
        "research",
        "investigate",
        "current price",
        "latest price",
        "live price",
        "stock price",
        "exchange rate",
        "how much is",
        "how much does",
        "what is the current",
        "what is the latest",
        "compare ",
        "statistics",
        "market data",
        "scrape",
        "exact data",
        "deep research",
        "find data",
        "look up",
    ];
    SIGNALS.iter().any(|signal| q.contains(signal))
}

fn is_report_page_edit_request(query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    let names_report = q.contains("report");
    let wants_pages = q.contains("first 2 page")
        || q.contains("first two page")
        || q.contains("page 1")
        || q.contains("pages 1")
        || q.contains("two pages");
    let wants_removal =
        q.contains("delete") || q.contains("remove") || q.contains("trim") || q.contains("drop");
    names_report && wants_pages && wants_removal
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentCandidate {
    path: String,
    bytes: u64,
    modified: String,
    extension: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PdfEditReport {
    backup_path: String,
    before_pages: usize,
    after_pages: usize,
}

fn parse_document_candidates(shell_output: &Value) -> Vec<DocumentCandidate> {
    let stdout = shell_output
        .get("stdout")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if stdout.is_empty() {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(stdout) else {
        return Vec::new();
    };
    let values: Vec<Value> = match value {
        Value::Array(items) => items,
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    };
    values
        .into_iter()
        .filter_map(|item| {
            let path = item.get("path").and_then(Value::as_str)?.trim().to_string();
            if path.is_empty() {
                return None;
            }
            let extension = item
                .get("extension")
                .and_then(Value::as_str)
                .or_else(|| Path::new(&path).extension().and_then(|ext| ext.to_str()))
                .unwrap_or("")
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if !REPORT_DOCUMENT_EXTENSIONS.contains(&extension.as_str()) {
                return None;
            }
            Some(DocumentCandidate {
                path,
                bytes: item.get("bytes").and_then(Value::as_u64).unwrap_or(0),
                modified: item
                    .get("modified")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                extension,
            })
        })
        .collect()
}

fn format_candidates(candidates: &[DocumentCandidate]) -> String {
    candidates
        .iter()
        .map(|candidate| {
            format!(
                "- `{}` ({} bytes, modified {})",
                candidate.path,
                candidate.bytes,
                if candidate.modified.is_empty() {
                    "unknown"
                } else {
                    &candidate.modified
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn op_evidence(label: &str, value: &Value) -> Vec<String> {
    let mut evidence = Vec::new();
    if let Some(op_id) = value.get("op_id").and_then(Value::as_str) {
        evidence.push(format!("{label}_op:{op_id}"));
    }
    if let Some(command) = value.get("command").and_then(Value::as_str) {
        evidence.push(format!("{label}_command:{}", truncate(command, 160)));
    }
    evidence
}

fn report_document_search_command() -> String {
    let roots = report_search_roots();
    if cfg!(windows) {
        let roots = roots
            .iter()
            .map(|root| ps_single_quote(&root.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"$roots=@({roots}); \
             $items=foreach($root in $roots){{if(Test-Path -LiteralPath $root){{\
             Get-ChildItem -LiteralPath $root -Recurse -File -ErrorAction SilentlyContinue | \
             Where-Object{{$_.BaseName -ieq 'report' -and @('.pdf','.docx') -contains $_.Extension.ToLowerInvariant()}} | \
             Select-Object -First 50 @{{Name='path';Expression={{$_.FullName}}}},@{{Name='bytes';Expression={{$_.Length}}}},@{{Name='modified';Expression={{$_.LastWriteTimeUtc.ToString('o')}}}},@{{Name='extension';Expression={{$_.Extension.TrimStart('.').ToLowerInvariant()}}}}\
             }}}}; @($items) | Select-Object -First 50 | ConvertTo-Json -Compress\""
        )
    } else {
        let roots = roots
            .iter()
            .map(|root| shell_arg(&root.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "python3 - <<'PY' {roots}\n\
import json, os, sys\n\
out=[]\n\
for root in sys.argv[1:]:\n\
    if not os.path.isdir(root):\n\
        continue\n\
    for base, _, files in os.walk(root):\n\
        for name in files:\n\
            stem, ext = os.path.splitext(name)\n\
            if stem.lower() == 'report' and ext.lower() in ('.pdf', '.docx'):\n\
                path=os.path.join(base,name)\n\
                try: st=os.stat(path)\n\
                except OSError: continue\n\
                out.append({{'path':path,'bytes':st.st_size,'modified':'','extension':ext[1:].lower()}})\n\
print(json.dumps(out[:50]))\n\
PY"
        )
    }
}

fn report_search_roots() -> Vec<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut roots = vec![
        home.join("Documents"),
        home.join("Downloads"),
        home.join("Desktop"),
    ];
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    roots
}

fn backup_pdf_path(path: &str) -> String {
    let source = Path::new(path);
    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("report");
    parent
        .join(format!("{stem}.astra-backup-{}.pdf", now_ms()))
        .to_string_lossy()
        .into_owned()
}

fn ps_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn shell_arg(value: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

fn pdf_trim_script() -> &'static str {
    r#"import json
import shutil
import sys
from pathlib import Path

try:
    from pypdf import PdfReader, PdfWriter
except Exception:
    try:
        from PyPDF2 import PdfReader, PdfWriter
    except Exception as exc:
        print(json.dumps({"ok": False, "error": f"Python PDF library unavailable: {exc}"}))
        sys.exit(3)

src = Path(sys.argv[1])
backup = Path(sys.argv[2])
try:
    reader = PdfReader(str(src))
    before = len(reader.pages)
    if before <= 2:
        print(json.dumps({"ok": False, "error": "PDF has two or fewer pages", "before_pages": before}))
        sys.exit(2)
    backup.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, backup)
    writer = PdfWriter()
    for page in reader.pages[2:]:
        writer.add_page(page)
    tmp = src.with_name(src.name + ".astra-trim-tmp")
    with tmp.open("wb") as handle:
        writer.write(handle)
    after = len(PdfReader(str(tmp)).pages)
    if after + 2 != before:
        tmp.unlink(missing_ok=True)
        print(json.dumps({"ok": False, "error": "page-count verification failed", "before_pages": before, "after_pages": after}))
        sys.exit(4)
    shutil.move(str(tmp), src)
    final_pages = len(PdfReader(str(src)).pages)
    if final_pages != after:
        shutil.copy2(backup, src)
        print(json.dumps({"ok": False, "error": "final PDF verification failed; restored backup", "before_pages": before, "after_pages": final_pages, "backup": str(backup)}))
        sys.exit(5)
    print(json.dumps({"ok": True, "before_pages": before, "after_pages": final_pages, "backup": str(backup)}))
except Exception as exc:
    print(json.dumps({"ok": False, "error": str(exc), "backup": str(backup)}))
    sys.exit(1)
"#
}

/// Truncate a string to `max` characters (char-safe), appending an ellipsis.
fn truncate(value: &str, max: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(max).collect();
        format!("{head}\u{2026}")
    }
}

/// The workspace a self-modification is applied to — the project's own repo.
/// Defaults to the process working directory; override with
/// `ASTRA_SELF_MOD_WORKSPACE`.
fn self_mod_workspace() -> String {
    std::env::var("ASTRA_SELF_MOD_WORKSPACE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| ".".to_string())
}

/// Parse the red-team reviewer's verdict. Fail-closed: returns `true` (safe to
/// proceed) ONLY when the reply explicitly passes and never flags. Any FLAG, an
/// ambiguous reply, or empty text returns `false`.
fn red_team_passes(text: &str) -> bool {
    let upper = text.to_uppercase();
    let flagged = upper.contains("FLAGGED") || upper.contains("VERDICT: FLAG");
    let passed = upper.contains("VERDICT: PASS") || upper.contains("VERDICT:PASS");
    passed && !flagged
}

/// Pull the first balanced-looking JSON object out of a model response.
fn extract_json(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(text[start..=end].to_string())
    } else {
        None
    }
}

fn sql_err(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("ceo persistence failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approach_routing_picks_the_right_method() {
        // A clean one-word reply routes directly.
        assert!(matches!(
            approach_from_reply("research", "anything"),
            Approach::Research
        ));
        assert!(matches!(
            approach_from_reply("diagnose", "anything"),
            Approach::Diagnose
        ));
        assert!(matches!(
            approach_from_reply("solve", "anything"),
            Approach::Solve
        ));
        // Diagnosis synonyms still route to the Crucible.
        assert!(matches!(
            approach_from_reply("you should debug this", "anything"),
            Approach::Diagnose
        ));
        // A chatty/garbage reply falls back to the query heuristic, then Solve.
        assert!(matches!(
            approach_from_reply("hmm, not sure", "please research the latest CPI figures"),
            Approach::Research
        ));
        assert!(matches!(
            approach_from_reply("???", "compute the factorial of 12"),
            Approach::Solve
        ));
    }

    #[test]
    fn repeat_failures_escalate_to_a_new_approach_then_self_diagnose() {
        // No prior attempt → use the classified approach unchanged.
        let (a, exhausted) = escalated_approach(Approach::Solve, &[]);
        assert!(matches!(a, Approach::Solve) && !exhausted);

        // It failed once via 'solve' → a repeat must NOT pick solve again.
        let (a, exhausted) = escalated_approach(Approach::Solve, &["solve".to_string()]);
        assert!(
            !matches!(a, Approach::Solve),
            "must escalate off the failed approach"
        );
        assert!(!exhausted);

        // Solve + diagnose + research all tried → exhausted, so the caller will
        // route to the Crucible to self-diagnose the repeated failure.
        let (_, exhausted) = escalated_approach(
            Approach::Research,
            &[
                "solve".to_string(),
                "diagnose".to_string(),
                "research".to_string(),
            ],
        );
        assert!(exhausted, "all approaches tried ⇒ exhausted");
    }

    #[test]
    fn query_key_is_stable_across_whitespace_and_case() {
        assert_eq!(query_key("Fix the build"), query_key("  fix   the BUILD "));
        assert_ne!(query_key("fix the build"), query_key("fix the tests"));
    }

    #[test]
    fn report_page_edit_request_routes_to_document_delegates() {
        assert!(is_report_page_edit_request(
            "find a file named report and delete first 2 pages"
        ));
        assert!(is_report_page_edit_request(
            "Remove the first two pages from report.pdf"
        ));
        assert!(!is_report_page_edit_request("find report files"));
        assert!(!is_report_page_edit_request("delete report"));
    }

    #[test]
    fn parses_document_candidates_from_shell_json() {
        let value = json!({
            "stdout": r#"[{"path":"C:\\Users\\boopa\\Documents\\report.pdf","bytes":123,"modified":"2026-01-01T00:00:00Z","extension":"pdf"},{"path":"C:\\Users\\boopa\\Downloads\\report.docx","bytes":456,"modified":"","extension":"docx"},{"path":"C:\\ignore\\report.txt","bytes":1,"extension":"txt"}]"#
        });
        let candidates = parse_document_candidates(&value);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].extension, "pdf");
        assert_eq!(candidates[1].extension, "docx");
    }

    #[test]
    fn formats_ambiguous_report_candidates() {
        let candidates = vec![
            DocumentCandidate {
                path: "C:\\Users\\boopa\\Documents\\report.pdf".into(),
                bytes: 10,
                modified: "2026-01-01T00:00:00Z".into(),
                extension: "pdf".into(),
            },
            DocumentCandidate {
                path: "C:\\Users\\boopa\\Downloads\\report.docx".into(),
                bytes: 20,
                modified: String::new(),
                extension: "docx".into(),
            },
        ];
        let formatted = format_candidates(&candidates);
        assert!(formatted.contains("Documents\\report.pdf"));
        assert!(formatted.contains("Downloads\\report.docx"));
        assert!(formatted.contains("unknown"));
    }

    #[test]
    fn red_team_verdict_fails_closed() {
        // Explicit pass is the ONLY way through.
        assert!(red_team_passes("VERDICT: PASS"));
        assert!(red_team_passes("verdict: pass"));
        // A flag always wins.
        assert!(!red_team_passes("VERDICT: FLAGGED"));
        assert!(!red_team_passes("VERDICT: PASS, but on reflection FLAGGED"));
        // Ambiguous / empty / chatty replies fail closed.
        assert!(!red_team_passes("looks fine to me"));
        assert!(!red_team_passes(""));
        assert!(!red_team_passes("I think it is probably safe"));
    }
}

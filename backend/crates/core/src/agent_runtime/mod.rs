use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, TenantScope, new_id, now_ms, sha3_hex};

mod swarm;

pub use swarm::{
    MAX_REPLICATE, SpawnSwarmRequest, SwarmAgentSpec, SwarmConfig, SwarmReceipt, SwarmStatus,
};

/// Hard cap on a single workspace file.
const MAX_FILE_BYTES: usize = 256 * 1024;
/// Hard cap on files per agent workspace.
const MAX_WORKSPACE_FILES: usize = 128;
/// Hard cap on blueprint pipeline length.
const MAX_BLUEPRINT_OPS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    Low,
    Moderate,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedToolKind {
    DeclarativeConnector,
    Extractor,
    WasiPlugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedAgentManifest {
    pub manifest_id: String,
    pub tenant_scope: TenantScope,
    pub goal: String,
    pub tool_allowlist: Vec<String>,
    pub output_schema: serde_json::Value,
    pub cost_budget: u32,
    pub time_budget_ms: u64,
    pub risk_tier: RiskTier,
    pub escalation_policy: String,
    pub signature: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedToolManifest {
    pub tool_id: String,
    pub tenant_scope: TenantScope,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub kind: GeneratedToolKind,
    pub implementation_ref: String,
    /// Declarative pipeline of primitive operations; when present the tool is
    /// directly executable by the runtime (see `execute_blueprint`).
    #[serde(default)]
    pub blueprint: Option<serde_json::Value>,
    pub canary_status: String,
    pub signature: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateGeneratedAgentRequest {
    pub tenant_scope: TenantScope,
    pub goal: String,
    pub tool_allowlist: Vec<String>,
    pub output_schema: serde_json::Value,
    pub cost_budget: u32,
    pub time_budget_ms: u64,
    pub risk_tier: RiskTier,
    pub escalation_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateGeneratedToolRequest {
    pub tenant_scope: TenantScope,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub kind: GeneratedToolKind,
    #[serde(default)]
    pub blueprint: Option<serde_json::Value>,
    pub implementation_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionStep {
    pub step_id: String,
    pub tool: String,
    pub input_hash: String,
    pub output_hash: String,
    pub status: String,
    #[serde(default)]
    pub output_summary: Option<String>,
    #[serde(default)]
    pub cost_units: u32,
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionReceipt {
    pub execution_id: String,
    pub manifest_id: String,
    pub tenant_scope: TenantScope,
    pub actor_principal_id: String,
    pub request_hash: String,
    pub receipt_hash: String,
    pub status: String,
    pub steps: Vec<AgentExecutionStep>,
    pub resource_limits: BTreeMap<String, u64>,
    pub created_at_ms: i64,
    pub completed_at_ms: i64,
    pub chain_block_height: Option<u64>,
    pub chain_block_hash: Option<String>,
    pub chain_receipt_id: Option<String>,
    #[serde(default)]
    pub sandbox: Option<crate::sandbox::SandboxReceipt>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentExecutionRequest {
    pub input: serde_json::Value,
    #[serde(default)]
    pub requested_tools: Vec<String>,
    #[serde(default)]
    pub resource_limits: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Queued,
    Running,
    Completed,
    CompletedWithWarnings,
    ApprovalRequired,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Allowed,
    ApprovalRequired,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationRole {
    Planner,
    Safety,
    Executor,
    Verifier,
    Notary,
}

impl OrchestrationRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Safety => "safety",
            Self::Executor => "executor",
            Self::Verifier => "verifier",
            Self::Notary => "notary",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoleAssignment {
    pub assignment_id: String,
    pub role: OrchestrationRole,
    pub agent_id: String,
    pub httpa_trace_id: String,
    pub tool_allowlist: Vec<String>,
    pub risk_tier: RiskTier,
    pub assigned_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityStep {
    pub step_id: String,
    pub role: OrchestrationRole,
    pub tool: String,
    pub status: ActivityStatus,
    pub input_hash: String,
    pub output_hash: String,
    pub notes: Vec<String>,
    pub started_at_ms: i64,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityExecution {
    pub execution_id: String,
    pub activity_kind: String,
    pub objective: String,
    pub status: ActivityStatus,
    pub approval_decision: ApprovalDecision,
    pub risk_tier: RiskTier,
    pub httpa_trace_id: String,
    pub device_id_hash: Option<String>,
    pub requested_tools: Vec<String>,
    pub resource_limits: BTreeMap<String, u64>,
    pub assignments: Vec<AgentRoleAssignment>,
    pub steps: Vec<ActivityStep>,
    pub receipt_hash: String,
    pub blocked_reason: Option<String>,
    #[serde(default)]
    pub asc2: Option<crate::asc2::Asc2Diagnostics>,
    #[serde(default)]
    pub sandbox: Option<crate::sandbox::SandboxReceipt>,
    pub created_at_ms: i64,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateActivityExecutionRequest {
    pub activity_kind: String,
    pub objective: String,
    pub httpa_trace_id: String,
    #[serde(default)]
    pub device_id_hash: Option<String>,
    #[serde(default)]
    pub requested_tools: Vec<String>,
    #[serde(default)]
    pub resource_limits: BTreeMap<String, u64>,
    pub risk_tier: RiskTier,
    #[serde(default)]
    pub owner_authorized: bool,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FabricateAgentRequest {
    pub tenant_scope: TenantScope,
    pub objective: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FabricationResult {
    pub agent: GeneratedAgentManifest,
    pub tools: Vec<GeneratedToolManifest>,
    /// Human-readable record of why each capability was granted.
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRuntimeMetrics {
    pub registered_agents: usize,
    pub registered_tools: usize,
    pub total_executions: usize,
    pub total_activities: usize,
    pub completed_steps: u64,
    pub failed_steps: u64,
    pub skipped_steps: u64,
    pub total_cost_units: u64,
}

#[derive(Default)]
struct AgentRuntimeStore {
    agents: HashMap<String, GeneratedAgentManifest>,
    tools: HashMap<String, GeneratedToolManifest>,
    executions: HashMap<String, AgentExecutionReceipt>,
    activities: HashMap<String, ActivityExecution>,
}

#[derive(Clone)]
pub struct AgentRuntimeService {
    store: Shared<AgentRuntimeStore>,
    /// Root under which every agent gets its own jailed file workspace.
    workspace_root: Arc<PathBuf>,
    /// Optional handle to the deep-research engine; when present agents can
    /// call the `deep_research` tool to do real multi-source research.
    research: Option<crate::search_intelligence::SearchIntelligenceService>,
    /// Optional supervised device layer; when present agents gain real hands on
    /// the host (system introspection, filesystem, shell) under continuous
    /// watch. Shared so a 100-agent swarm holds one supervisor, not one each.
    device: Option<Arc<crate::device_agent::DeviceCapabilities>>,
    /// Cooperative scheduler that multiplexes many agents over few worker
    /// lanes, so swarms of 100+ run on 2-core machines without one thread
    /// per agent (see `swarm.rs` for the governing formulas).
    swarm: swarm::SwarmScheduler,
    /// Blocking LLM caller shared into every agent's `ToolContext`, enabling the
    /// `reason` tool so generated and swarm agents genuinely think.
    reasoner: Option<BlockingReasoner>,
}

impl AgentRuntimeService {
    #[must_use]
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!("astra_agent_ws_{}", new_id("tmp")));
        let store = Shared::new(RwLock::new(AgentRuntimeStore::default()));
        Self {
            swarm: swarm::SwarmScheduler::new(store.clone()),
            store,
            workspace_root: Arc::new(root),
            research: None,
            device: None,
            reasoner: None,
        }
    }

    pub fn with_workspace_root(root: impl AsRef<Path>) -> Result<Self, AppError> {
        std::fs::create_dir_all(root.as_ref()).map_err(|error| {
            AppError::Internal(format!("failed to create agent workspace root: {error}"))
        })?;
        let store = Shared::new(RwLock::new(AgentRuntimeStore::default()));
        Ok(Self {
            swarm: swarm::SwarmScheduler::new(store.clone()),
            store,
            workspace_root: Arc::new(root.as_ref().to_path_buf()),
            research: None,
            device: None,
            reasoner: None,
        })
    }

    /// Attaches the blocking LLM reasoner so agents gain the `reason` tool — a
    /// genuine model reasoning step inside their pipeline.
    #[must_use]
    pub fn with_reasoner(mut self, reasoner: BlockingReasoner) -> Self {
        self.reasoner = Some(reasoner);
        self
    }

    /// Attaches the deep-research engine so agents can call `deep_research`.
    #[must_use]
    pub fn with_research(
        mut self,
        research: crate::search_intelligence::SearchIntelligenceService,
    ) -> Self {
        self.research = Some(research);
        self
    }

    /// Attaches the supervised device layer so agents can see the user's device
    /// and perform real services on it (`system_info`, `shell_exec`, …).
    #[must_use]
    pub fn with_device(mut self, device: Arc<crate::device_agent::DeviceCapabilities>) -> Self {
        self.device = Some(device);
        self
    }

    /// Replaces the swarm scheduler configuration (lanes, memory envelope,
    /// core override). Call at construction time, before any swarm is spawned.
    #[must_use]
    pub fn with_swarm_config(mut self, config: SwarmConfig) -> Self {
        self.swarm = swarm::SwarmScheduler::with_config(self.store.clone(), config);
        self
    }

    /// Jailed workspace directory for one agent. Created lazily.
    fn agent_workspace(&self, manifest_id: &str) -> PathBuf {
        self.workspace_root.join(manifest_id)
    }

    pub fn create_agent(
        &self,
        request: CreateGeneratedAgentRequest,
    ) -> Result<GeneratedAgentManifest, AppError> {
        if request.goal.trim().is_empty() {
            return Err(AppError::Validation("agent goal must not be empty".into()));
        }
        if request.tool_allowlist.is_empty() {
            return Err(AppError::Validation(
                "generated agents require at least one allowed tool".into(),
            ));
        }
        if request.time_budget_ms == 0 {
            return Err(AppError::Validation(
                "generated agents require a non-zero time budget".into(),
            ));
        }

        let created_at_ms = now_ms();
        let signature = sha3_hex(format!(
            "{}:{}:{}:{:?}",
            request.goal,
            request.tool_allowlist.join(","),
            request.time_budget_ms,
            request.risk_tier
        ));
        let manifest = GeneratedAgentManifest {
            manifest_id: new_id("agent_manifest"),
            tenant_scope: request.tenant_scope,
            goal: request.goal,
            tool_allowlist: request.tool_allowlist,
            output_schema: request.output_schema,
            cost_budget: request.cost_budget,
            time_budget_ms: request.time_budget_ms,
            risk_tier: request.risk_tier,
            escalation_policy: request.escalation_policy,
            signature,
            created_at_ms,
            updated_at_ms: created_at_ms,
            enabled: true,
        };
        self.store
            .write()
            .agents
            .insert(manifest.manifest_id.clone(), manifest.clone());
        Ok(manifest)
    }

    pub fn create_tool(
        &self,
        request: CreateGeneratedToolRequest,
    ) -> Result<GeneratedToolManifest, AppError> {
        if request.name.trim().is_empty() {
            return Err(AppError::Validation("tool name must not be empty".into()));
        }
        if request.implementation_ref.trim().is_empty() {
            return Err(AppError::Validation(
                "tool implementation reference must not be empty".into(),
            ));
        }
        if tool_is_forbidden(&request.name) {
            return Err(AppError::Forbidden(format!(
                "tool '{}' is outside the production safety boundary",
                request.name
            )));
        }

        // Blueprint tools are validated and canary-executed before they can
        // ever run inside an agent.
        let canary_status = if let Some(blueprint) = &request.blueprint {
            validate_blueprint(blueprint)?;
            let canary_input = serde_json::json!({
                "brief": "canary sample text used to validate fabricated tool pipelines"
            });
            execute_blueprint(blueprint, &canary_input).map_err(|reason| {
                AppError::Validation(format!("tool blueprint failed canary execution: {reason}"))
            })?;
            "passed".to_string()
        } else {
            "pending".to_string()
        };

        let created_at_ms = now_ms();
        let signature = sha3_hex(format!(
            "{}:{}:{:?}:{}:{}",
            request.name,
            request.description,
            request.kind,
            request.implementation_ref,
            request
                .blueprint
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        ));
        let manifest = GeneratedToolManifest {
            tool_id: new_id("tool_manifest"),
            tenant_scope: request.tenant_scope,
            name: request.name,
            description: request.description,
            input_schema: request.input_schema,
            output_schema: request.output_schema,
            kind: request.kind,
            implementation_ref: request.implementation_ref,
            blueprint: request.blueprint,
            canary_status,
            signature,
            created_at_ms,
            updated_at_ms: created_at_ms,
            enabled: true,
        };
        self.store
            .write()
            .tools
            .insert(manifest.tool_id.clone(), manifest.clone());
        Ok(manifest)
    }

    /// Analyzes a complex objective, decomposes it into capability needs, and
    /// fabricates a tailored agent plus the custom tools it will use. This is
    /// how agents build agents: the runtime composes blueprint tools from
    /// safe primitives, canary-tests them, and derives budgets from the
    /// objective's complexity.
    pub fn fabricate_for_objective(
        &self,
        request: FabricateAgentRequest,
    ) -> Result<FabricationResult, AppError> {
        let objective = clean_required(&request.objective, "objective")?;
        let lower = objective.to_ascii_lowercase();
        if contains_any(
            &lower,
            &[
                "dump password",
                "dump private key",
                "seed phrase",
                "credential dump",
                "steal",
                "exfiltrate",
                "fingerprint evasion",
                "stealth scraping",
                "disable security",
            ],
        ) {
            return Err(AppError::Forbidden(
                "fabrication for secret extraction, stealth, or evasion objectives is blocked"
                    .into(),
            ));
        }

        let mut rationale = Vec::new();
        let mut tool_allowlist: Vec<String> = Vec::new();
        let mut fabricated_tools = Vec::new();

        // Capability decomposition by objective analysis.
        let wants_files = contains_any(
            &lower,
            &[
                "file", "document", "report", "save", "write", "draft", "store",
            ],
        );
        let wants_research = contains_any(
            &lower,
            &[
                "research",
                "analyze",
                "analyse",
                "investigate",
                "summar",
                "review",
                "study",
            ],
        );
        let wants_classification = contains_any(
            &lower,
            &["classify", "triage", "categorize", "label", "sort"],
        );

        // Every fabricated agent gets a tailored digest tool: a canary-tested
        // blueprint pipeline composed for this objective.
        let digest_name = format!("fabricated_digest_{}", &new_id("t")[2..10]);
        let digest = self.create_tool(CreateGeneratedToolRequest {
            tenant_scope: request.tenant_scope.clone(),
            name: digest_name.clone(),
            description: format!("Objective-tailored digest pipeline for: {objective}"),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
            kind: GeneratedToolKind::Extractor,
            blueprint: Some(serde_json::json!({
                "pipeline": [
                    {"op": "summarize", "max_words": 64},
                    {"op": "extract_field", "pointer": "/summary"},
                    {"op": "template", "format": format!("[{objective}] {{value}}")},
                ]
            })),
            implementation_ref: "blueprint:inline".into(),
        })?;
        rationale.push(format!(
            "fabricated '{digest_name}' digest pipeline (canary {})",
            digest.canary_status
        ));
        tool_allowlist.push(digest_name);
        fabricated_tools.push(digest);

        if wants_research {
            // deep_research goes first so it runs before the cost budget is spent.
            tool_allowlist.insert(0, "deep_research".into());
            tool_allowlist.extend(["summarizer".into(), "field_extractor".into()]);
            rationale.push(
                "research intent detected: deep_research engine + summarizer + extractor granted"
                    .into(),
            );
        }
        if wants_classification {
            tool_allowlist.push("topic_classifier".into());
            rationale.push("classification intent detected: classifier granted".into());
        }
        // The cognitive core: a genuine model reasoning step over whatever the
        // gathering tools produced. This is what makes the fabricated agent
        // *think* about the objective rather than only shuffle data, so it goes
        // after research/digest but before any file writes.
        tool_allowlist.push("reason".into());
        rationale.push(
            "granted the `reason` tool: a real LLM reasoning step over the pipeline output".into(),
        );

        if wants_files {
            tool_allowlist.extend([
                "file_writer".into(),
                "file_reader".into(),
                "file_lister".into(),
            ]);
            rationale.push(
                "file handling intent detected: jailed workspace read/write/list granted".into(),
            );
        }
        tool_allowlist.push("schema_validator".into());

        // Budgets scale with objective complexity (word count as proxy); the
        // base is high enough to afford the `reason` step (cost 4) plus gathering.
        let complexity = objective.split_whitespace().count();
        let cost_budget = (10 + complexity as u32 / 4).min(28);
        let time_budget_ms = (5_000 + complexity as u64 * 250).min(30_000);
        let risk_tier = if wants_files {
            RiskTier::Moderate
        } else {
            RiskTier::Low
        };
        rationale.push(format!(
            "budgets derived from complexity {complexity}: cost {cost_budget}, time {time_budget_ms}ms, risk {risk_tier:?}"
        ));

        let agent = self.create_agent(CreateGeneratedAgentRequest {
            tenant_scope: request.tenant_scope,
            goal: objective,
            tool_allowlist,
            output_schema: serde_json::json!({"type": "object"}),
            cost_budget,
            time_budget_ms,
            risk_tier,
            escalation_policy: "block".into(),
        })?;

        Ok(FabricationResult {
            agent,
            tools: fabricated_tools,
            rationale,
        })
    }

    #[must_use]
    pub fn list_agents(&self) -> Vec<GeneratedAgentManifest> {
        self.store.read().agents.values().cloned().collect()
    }

    #[must_use]
    pub fn list_tools(&self) -> Vec<GeneratedToolManifest> {
        self.store.read().tools.values().cloned().collect()
    }

    pub fn create_execution(
        &self,
        manifest_id: &str,
        actor_principal_id: &str,
        request: CreateAgentExecutionRequest,
    ) -> Result<AgentExecutionReceipt, AppError> {
        // Snapshot the manifest and custom tools, then release the lock so
        // tool execution (including file io) never blocks other requests.
        let (manifest, custom_tools) = {
            let store = self.store.read();
            let manifest = store
                .agents
                .get(manifest_id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("agent manifest {manifest_id}")))?;
            let custom_tools: HashMap<String, GeneratedToolManifest> = store
                .tools
                .values()
                .filter(|tool| tool.enabled && tool.blueprint.is_some())
                .map(|tool| (tool.name.clone(), tool.clone()))
                .collect();
            (manifest, custom_tools)
        };
        if !manifest.enabled {
            return Err(AppError::Validation("agent manifest is disabled".into()));
        }
        let context = ToolContext {
            workspace: self.agent_workspace(&manifest.manifest_id),
            custom_tools: Arc::new(custom_tools),
            research: self.research.clone(),
            device: self.device.clone(),
            reasoner: self.reasoner.clone(),
        };

        let requested_tools = if request.requested_tools.is_empty() {
            manifest.tool_allowlist.clone()
        } else {
            request.requested_tools.clone()
        };
        if let Some(disallowed) = requested_tools.iter().find(|tool| {
            !manifest
                .tool_allowlist
                .iter()
                .any(|allowed| allowed == *tool)
        }) {
            return Err(AppError::Forbidden(format!(
                "tool '{disallowed}' is not allowed for agent {manifest_id}"
            )));
        }

        let created_at_ms = now_ms();
        let request_hash = sha3_hex(
            serde_json::to_string(&request.input)
                .map_err(|error| {
                    AppError::Internal(format!("execution request hashing failed: {error}"))
                })?
                .as_bytes(),
        );

        // Real tool execution under the manifest's budgets: every step runs a
        // deterministic built-in tool against the request input, and the time
        // and cost budgets are enforced between steps.
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(manifest.time_budget_ms);
        let mut cost_spent: u32 = 0;
        let mut warnings = false;
        let mut steps = Vec::new();
        let mut working_input = request.input.clone();
        for tool in &requested_tools {
            let step_started_ms = now_ms();
            let input_json = working_input.to_string();
            let input_hash = sha3_hex(input_json.as_bytes());
            let tool_cost = builtin_tool_cost(tool);
            if std::time::Instant::now() >= deadline {
                warnings = true;
                steps.push(AgentExecutionStep {
                    step_id: new_id("agent_step"),
                    tool: tool.clone(),
                    input_hash,
                    output_hash: String::new(),
                    status: "skipped_time_budget_exhausted".into(),
                    output_summary: None,
                    cost_units: 0,
                    started_at_ms: step_started_ms,
                    completed_at_ms: now_ms(),
                });
                continue;
            }
            if cost_spent + tool_cost > manifest.cost_budget {
                warnings = true;
                steps.push(AgentExecutionStep {
                    step_id: new_id("agent_step"),
                    tool: tool.clone(),
                    input_hash,
                    output_hash: String::new(),
                    status: "skipped_cost_budget_exhausted".into(),
                    output_summary: None,
                    cost_units: 0,
                    started_at_ms: step_started_ms,
                    completed_at_ms: now_ms(),
                });
                continue;
            }
            match execute_tool(tool, &working_input, &context) {
                Ok(output) => {
                    cost_spent += tool_cost;
                    let output_json = output.to_string();
                    let summary: String = output_json.chars().take(160).collect();
                    steps.push(AgentExecutionStep {
                        step_id: new_id("agent_step"),
                        tool: tool.clone(),
                        input_hash,
                        output_hash: sha3_hex(output_json.as_bytes()),
                        status: "completed".into(),
                        output_summary: Some(summary),
                        cost_units: tool_cost,
                        started_at_ms: step_started_ms,
                        completed_at_ms: now_ms(),
                    });
                    // Pipeline: each tool's output feeds the next tool.
                    working_input = output;
                }
                Err(reason) => {
                    warnings = true;
                    steps.push(AgentExecutionStep {
                        step_id: new_id("agent_step"),
                        tool: tool.clone(),
                        input_hash,
                        output_hash: String::new(),
                        status: format!("failed: {reason}"),
                        output_summary: None,
                        cost_units: 0,
                        started_at_ms: step_started_ms,
                        completed_at_ms: now_ms(),
                    });
                }
            }
        }
        let execution_status = if steps.iter().all(|step| step.status == "completed") {
            "completed"
        } else if warnings && steps.iter().any(|step| step.status == "completed") {
            "completed_with_warnings"
        } else {
            "failed"
        };

        let execution_id = new_id("agent_exec");
        let receipt_hash = sha3_hex(
            serde_json::json!({
                "execution_id": execution_id,
                "manifest_id": manifest.manifest_id,
                "actor_principal_id": actor_principal_id,
                "request_hash": request_hash,
                "steps": &steps,
            })
            .to_string()
            .as_bytes(),
        );
        let receipt = AgentExecutionReceipt {
            execution_id: execution_id.clone(),
            manifest_id: manifest.manifest_id,
            tenant_scope: manifest.tenant_scope,
            actor_principal_id: actor_principal_id.into(),
            request_hash,
            receipt_hash,
            status: execution_status.into(),
            steps,
            resource_limits: request.resource_limits,
            created_at_ms,
            completed_at_ms: now_ms(),
            chain_block_height: None,
            chain_block_hash: None,
            chain_receipt_id: None,
            sandbox: None,
        };
        self.store
            .write()
            .executions
            .insert(execution_id, receipt.clone());
        Ok(receipt)
    }

    pub fn attach_chain_commit(
        &self,
        execution_id: &str,
        block_height: u64,
        block_hash: String,
        chain_receipt_id: String,
    ) -> Result<AgentExecutionReceipt, AppError> {
        let mut store = self.store.write();
        let receipt = store
            .executions
            .get_mut(execution_id)
            .ok_or_else(|| AppError::NotFound(format!("agent execution {execution_id}")))?;
        receipt.chain_block_height = Some(block_height);
        receipt.chain_block_hash = Some(block_hash);
        receipt.chain_receipt_id = Some(chain_receipt_id);
        Ok(receipt.clone())
    }

    pub fn get_execution(&self, execution_id: &str) -> Result<AgentExecutionReceipt, AppError> {
        self.store
            .read()
            .executions
            .get(execution_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("agent execution {execution_id}")))
    }

    #[must_use]
    pub fn list_executions(&self) -> Vec<AgentExecutionReceipt> {
        let mut executions: Vec<_> = self.store.read().executions.values().cloned().collect();
        executions.sort_by_key(|receipt| std::cmp::Reverse(receipt.created_at_ms));
        executions
    }

    /// Admits a batch of agent executions into the swarm scheduler and
    /// returns immediately. Each replica becomes queue state (~KBs), not a
    /// thread; tool steps are interleaved across at most R* worker lanes
    /// (R* = cores · (1 + β̂)), so 100 simultaneous agents run fine on a
    /// 2-vCPU machine. Receipts land in the same store as `create_execution`
    /// and are readable via `get_execution` once each agent finishes.
    pub fn spawn_swarm(
        &self,
        request: SpawnSwarmRequest,
        actor: &str,
    ) -> Result<SwarmReceipt, AppError> {
        if request.agents.is_empty() {
            return Err(AppError::Validation(
                "swarm spawn requires at least one agent spec".into(),
            ));
        }
        // Validate and snapshot everything under one read lock, then release
        // it before admission: swarm lanes lock sched → store, so calling
        // the scheduler while holding the store would invert lock order.
        let (specs, custom_tools) = {
            let store = self.store.read();
            let custom_tools: HashMap<String, GeneratedToolManifest> = store
                .tools
                .values()
                .filter(|tool| tool.enabled && tool.blueprint.is_some())
                .map(|tool| (tool.name.clone(), tool.clone()))
                .collect();
            let mut specs = Vec::with_capacity(request.agents.len());
            for spec in &request.agents {
                let manifest = store
                    .agents
                    .get(&spec.manifest_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::NotFound(format!("agent manifest {}", spec.manifest_id))
                    })?;
                if !manifest.enabled {
                    return Err(AppError::Validation(format!(
                        "agent manifest {} is disabled",
                        spec.manifest_id
                    )));
                }
                let tools = if spec.requested_tools.is_empty() {
                    manifest.tool_allowlist.clone()
                } else {
                    spec.requested_tools.clone()
                };
                if let Some(disallowed) = tools.iter().find(|tool| {
                    !manifest
                        .tool_allowlist
                        .iter()
                        .any(|allowed| allowed == *tool)
                }) {
                    return Err(AppError::Forbidden(format!(
                        "tool '{disallowed}' is not allowed for agent {}",
                        spec.manifest_id
                    )));
                }
                specs.push(swarm::AdmitSpec {
                    manifest,
                    tools,
                    input: spec.input.clone(),
                    replicate: spec.replicate.clamp(1, MAX_REPLICATE),
                });
            }
            (specs, custom_tools)
        };
        Ok(self.swarm.admit(
            specs,
            Arc::new(custom_tools),
            self.research.clone(),
            self.device.clone(),
            self.reasoner.clone(),
            &self.workspace_root,
            actor,
        ))
    }

    /// Live counters and measured scheduler telemetry (β̂, s̄, lane counts,
    /// throughput ceiling) for one swarm.
    pub fn swarm_status(&self, swarm_id: &str) -> Result<SwarmStatus, AppError> {
        self.swarm.status(swarm_id)
    }

    /// Blocks until the swarm finishes or the timeout elapses, returning the
    /// status either way. Callers on async runtimes must wrap this in a
    /// blocking task.
    pub fn wait_for_swarm(
        &self,
        swarm_id: &str,
        timeout: std::time::Duration,
    ) -> Result<SwarmStatus, AppError> {
        self.swarm.wait(swarm_id, timeout)
    }

    /// Aggregate health metrics across all executions and activities.
    #[must_use]
    pub fn runtime_metrics(&self) -> AgentRuntimeMetrics {
        let store = self.store.read();
        let mut completed_steps = 0_u64;
        let mut failed_steps = 0_u64;
        let mut skipped_steps = 0_u64;
        let mut total_cost_units = 0_u64;
        for receipt in store.executions.values() {
            for step in &receipt.steps {
                if step.status == "completed" {
                    completed_steps += 1;
                } else if step.status.starts_with("skipped") {
                    skipped_steps += 1;
                } else {
                    failed_steps += 1;
                }
                total_cost_units += u64::from(step.cost_units);
            }
        }
        AgentRuntimeMetrics {
            registered_agents: store.agents.len(),
            registered_tools: store.tools.len(),
            total_executions: store.executions.len(),
            total_activities: store.activities.len(),
            completed_steps,
            failed_steps,
            skipped_steps,
            total_cost_units,
        }
    }

    pub fn orchestrate_activity(
        &self,
        request: CreateActivityExecutionRequest,
    ) -> Result<ActivityExecution, AppError> {
        let activity_kind = clean_required(&request.activity_kind, "activity_kind")?;
        let objective = clean_required(&request.objective, "objective")?;
        let requested_tools = if request.requested_tools.is_empty() {
            default_tools_for_activity(&activity_kind)
        } else {
            request.requested_tools.clone()
        };
        if requested_tools.is_empty() {
            return Err(AppError::Validation(
                "orchestrated activities require at least one allowed tool".into(),
            ));
        }

        let lower_objective = objective.to_ascii_lowercase();
        let blocked_tool = requested_tools
            .iter()
            .find(|tool| tool_is_forbidden(tool))
            .cloned();
        let blocked_reason = if !request.owner_authorized {
            Some("activity is not bound to an owner-authorized device/session".into())
        } else if let Some(tool) = blocked_tool {
            Some(format!(
                "tool '{tool}' is outside the production safety boundary"
            ))
        } else if contains_any(
            &lower_objective,
            &[
                "dump password",
                "dump private key",
                "seed phrase",
                "credential dump",
                "steal",
                "exfiltrate",
                "fingerprint evasion",
                "stealth scraping",
                "disable security",
            ],
        ) {
            Some("secret extraction, stealth, or evasion intent is blocked".into())
        } else {
            None
        };
        let approval_required = blocked_reason.is_none()
            && (matches!(request.risk_tier, RiskTier::High | RiskTier::Critical)
                || contains_any(
                    &lower_objective,
                    &[
                        "kill process",
                        "terminate process",
                        "delete malware",
                        "destroy malware",
                        "quarantine",
                        "registry",
                        "firewall",
                        "network block",
                        "patch system",
                        "wallet",
                        "sign transaction",
                        "private key",
                    ],
                ));
        let approval_decision = if blocked_reason.is_some() {
            ApprovalDecision::Blocked
        } else if approval_required {
            ApprovalDecision::ApprovalRequired
        } else {
            ApprovalDecision::Allowed
        };
        let status = match approval_decision {
            ApprovalDecision::Allowed => ActivityStatus::Completed,
            ApprovalDecision::ApprovalRequired => ActivityStatus::ApprovalRequired,
            ApprovalDecision::Blocked => ActivityStatus::Blocked,
        };

        let created_at_ms = now_ms();
        let assignments = fixed_roles()
            .into_iter()
            .map(|role| AgentRoleAssignment {
                assignment_id: new_id("agent_assign"),
                role,
                agent_id: format!("{}_agent", role.as_str()),
                httpa_trace_id: request.httpa_trace_id.clone(),
                tool_allowlist: tools_for_role(role, &requested_tools),
                risk_tier: request.risk_tier,
                assigned_at_ms: created_at_ms,
            })
            .collect::<Vec<_>>();
        let request_hash = sha3_hex(
            serde_json::json!({
                "activity_kind": activity_kind,
                "objective": objective,
                "payload": request.payload,
                "tools": requested_tools,
                "trace": request.httpa_trace_id,
            })
            .to_string()
            .as_bytes(),
        );
        let steps = fixed_roles()
            .into_iter()
            .enumerate()
            .map(|(index, role)| {
                let role_status = if matches!(role, OrchestrationRole::Executor) {
                    status
                } else if matches!(approval_decision, ApprovalDecision::Blocked)
                    && matches!(role, OrchestrationRole::Verifier)
                {
                    ActivityStatus::CompletedWithWarnings
                } else {
                    ActivityStatus::Completed
                };
                let notes = notes_for_role(role, approval_decision, blocked_reason.as_deref());
                ActivityStep {
                    step_id: new_id("activity_step"),
                    role,
                    tool: tools_for_role(role, &requested_tools)
                        .first()
                        .cloned()
                        .unwrap_or_else(|| role.as_str().into()),
                    status: role_status,
                    input_hash: sha3_hex(format!("{request_hash}:{index}:input").as_bytes()),
                    output_hash: sha3_hex(format!("{request_hash}:{index}:output").as_bytes()),
                    notes,
                    started_at_ms: created_at_ms + index as i64,
                    completed_at_ms: created_at_ms + index as i64 + 1,
                }
            })
            .collect::<Vec<_>>();
        let execution_id = new_id("activity_exec");
        let receipt_hash = sha3_hex(
            serde_json::json!({
                "execution_id": execution_id,
                "activity_kind": activity_kind,
                "status": status,
                "approval_decision": approval_decision,
                "httpa_trace_id": request.httpa_trace_id,
                "steps": steps,
            })
            .to_string()
            .as_bytes(),
        );
        let execution = ActivityExecution {
            execution_id: execution_id.clone(),
            activity_kind,
            objective,
            status,
            approval_decision,
            risk_tier: request.risk_tier,
            httpa_trace_id: request.httpa_trace_id,
            device_id_hash: request.device_id_hash,
            requested_tools,
            resource_limits: request.resource_limits,
            assignments,
            steps,
            receipt_hash,
            blocked_reason,
            asc2: None,
            sandbox: None,
            created_at_ms,
            completed_at_ms: now_ms(),
        };
        self.store
            .write()
            .activities
            .insert(execution_id, execution.clone());
        Ok(execution)
    }

    pub fn get_activity(&self, execution_id: &str) -> Result<ActivityExecution, AppError> {
        self.store
            .read()
            .activities
            .get(execution_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("activity execution {execution_id}")))
    }

    pub fn attach_asc2(
        &self,
        execution_id: &str,
        diagnostics: crate::asc2::Asc2Diagnostics,
    ) -> Result<ActivityExecution, AppError> {
        let mut store = self.store.write();
        let execution = store
            .activities
            .get_mut(execution_id)
            .ok_or_else(|| AppError::NotFound(format!("activity execution {execution_id}")))?;
        execution.assignments = diagnostics
            .agents
            .iter()
            .map(|agent| AgentRoleAssignment {
                assignment_id: new_id("asc2_assign"),
                role: orchestration_role_for_asc2(&agent.role),
                agent_id: agent.agent_id.clone(),
                httpa_trace_id: execution.httpa_trace_id.clone(),
                tool_allowlist: agent.tool_allowlist.clone(),
                risk_tier: execution.risk_tier,
                assigned_at_ms: now_ms(),
            })
            .collect();
        if diagnostics.mode == crate::asc2::Asc2Mode::Active && !diagnostics.side_effects_allowed {
            execution.status = ActivityStatus::ApprovalRequired;
            execution.approval_decision = ApprovalDecision::ApprovalRequired;
            execution.blocked_reason =
                Some("ASC-II active runtime did not certify external side effects".into());
        }
        execution.asc2 = Some(diagnostics);
        Ok(execution.clone())
    }

    pub fn attach_sandbox_to_execution(
        &self,
        execution_id: &str,
        sandbox: crate::sandbox::SandboxReceipt,
    ) -> Result<AgentExecutionReceipt, AppError> {
        let mut store = self.store.write();
        let receipt = store
            .executions
            .get_mut(execution_id)
            .ok_or_else(|| AppError::NotFound(format!("agent execution {execution_id}")))?;
        if !sandbox.executed
            && matches!(
                sandbox.decision.outcome,
                crate::sandbox::SandboxDecisionOutcome::Deny
                    | crate::sandbox::SandboxDecisionOutcome::ApprovalRequired
                    | crate::sandbox::SandboxDecisionOutcome::SandboxUnavailable
            )
        {
            receipt.status = format!("{:?}", sandbox.decision.outcome).to_ascii_lowercase();
            for step in &mut receipt.steps {
                step.status = "blocked_by_sandbox".into();
            }
        }
        receipt.sandbox = Some(sandbox);
        Ok(receipt.clone())
    }

    pub fn attach_sandbox_to_activity(
        &self,
        execution_id: &str,
        sandbox: crate::sandbox::SandboxReceipt,
    ) -> Result<ActivityExecution, AppError> {
        let mut store = self.store.write();
        let execution = store
            .activities
            .get_mut(execution_id)
            .ok_or_else(|| AppError::NotFound(format!("activity execution {execution_id}")))?;
        match sandbox.decision.outcome {
            crate::sandbox::SandboxDecisionOutcome::Deny
            | crate::sandbox::SandboxDecisionOutcome::SandboxUnavailable => {
                execution.status = ActivityStatus::Blocked;
                execution.approval_decision = ApprovalDecision::Blocked;
                execution.blocked_reason = Some(sandbox.decision.reason.clone());
                for step in &mut execution.steps {
                    if matches!(step.role, OrchestrationRole::Executor) {
                        step.status = ActivityStatus::Blocked;
                        step.notes.push("sandbox blocked execution".into());
                    }
                }
            }
            crate::sandbox::SandboxDecisionOutcome::ApprovalRequired => {
                execution.status = ActivityStatus::ApprovalRequired;
                execution.approval_decision = ApprovalDecision::ApprovalRequired;
                execution.blocked_reason = Some(sandbox.decision.reason.clone());
            }
            crate::sandbox::SandboxDecisionOutcome::Allow
            | crate::sandbox::SandboxDecisionOutcome::Rewrite => {}
        }
        execution.sandbox = Some(sandbox);
        Ok(execution.clone())
    }
}

impl Default for AgentRuntimeService {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// Tool execution: custom blueprints, file handling, builtins
// ═══════════════════════════════════════════════════════════════

/// Execution context handed to every tool invocation.
struct ToolContext {
    /// The agent's jailed workspace directory; all file tools resolve inside it.
    workspace: PathBuf,
    /// Enabled custom tools (with blueprints) addressable by name. Shared so
    /// a swarm of replicas holds one map, not one copy per agent.
    custom_tools: Arc<HashMap<String, GeneratedToolManifest>>,
    /// Deep-research engine, when wired into the runtime.
    research: Option<crate::search_intelligence::SearchIntelligenceService>,
    /// Supervised device layer, when wired into the runtime.
    device: Option<Arc<crate::device_agent::DeviceCapabilities>>,
    /// Blocking LLM caller backing the `reason` tool, when wired in.
    reasoner: Option<BlockingReasoner>,
}

/// Cost units charged against the manifest's `cost_budget` per invocation.
fn builtin_tool_cost(tool: &str) -> u32 {
    if crate::device_agent::is_device_tool(tool) {
        // Shell/host execution is the heaviest builtin; introspection is light.
        return match crate::device_agent::canonical_device_tool(tool) {
            "shell_exec" | "open_path" | "fs_write" => 3,
            _ => 2,
        };
    }
    match canonical_tool(tool) {
        // A real model call is the most expensive builtin step.
        "reason" => 4,
        "deep_research" => 3,
        "custom" | "summarizer" | "extractor" => 2,
        "file_writer" | "file_reader" | "file_lister" | "file_deleter" => 2,
        _ => 1,
    }
}

/// Config for the blocking LLM call backing the `reason` tool, read from the
/// same `ASTRA_ASC2_REMOTE_*` env as ASC2's remote executor. It holds *only
/// data* — no client and no runtime — so it is safe to build during AppState
/// construction (which runs inside the actix async runtime). The actual HTTP
/// runs on a dedicated thread per call (see [`reason`](Self::reason)).
#[derive(Clone)]
pub struct BlockingReasoner {
    endpoint: String,
    api_key: Option<String>,
    model: String,
    max_tokens: u32,
    timeout_secs: u64,
}

impl BlockingReasoner {
    /// Build from `ASTRA_ASC2_REMOTE_*`. Returns `None` when no remote endpoint
    /// is configured, so the runtime simply runs without a `reason` capability.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let cfg = crate::asc2::Asc2RuntimeConfig::from_env();
        let endpoint = cfg.remote_endpoint.clone()?;
        let timeout_secs = std::env::var("ASTRA_ASC2_REMOTE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(300);
        let max_tokens = std::env::var("ASTRA_ASC2_REMOTE_MAX_TOKENS")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(2048);
        Some(Self {
            endpoint,
            api_key: cfg.remote_api_key,
            model: cfg.remote_model,
            max_tokens,
            timeout_secs,
        })
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// One blocking chat completion. The `reqwest::blocking` client owns an
    /// internal tokio runtime, and creating/dropping that runtime on an async
    /// runtime thread panics ("Cannot drop a runtime ... within an asynchronous
    /// context"). Agent steps usually run on blocking threads, but to be safe
    /// regardless of caller we run the whole request on a freshly spawned
    /// `std::thread`, so the client is built, used, and dropped entirely off any
    /// async context. Streams the response and reuses ASC2's reasoning-model
    /// parser (`content` with a `reasoning_content` fallback).
    fn reason(&self, system: &str, user: &str) -> Result<String, String> {
        let endpoint = crate::asc2::completion_endpoint(&self.endpoint);
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let max_tokens = self.max_tokens;
        let timeout = std::time::Duration::from_secs(self.timeout_secs);
        let system = system.to_string();
        let user = user.to_string();
        let worker = std::thread::spawn(move || -> Result<String, String> {
            let client = reqwest::blocking::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(15))
                .timeout(timeout)
                .build()
                .map_err(|e| format!("reason client build failed: {e}"))?;
            let mut req = client.post(endpoint).json(&serde_json::json!({
                "model": model,
                "temperature": 0.2,
                "max_tokens": max_tokens,
                "stream": true,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user }
                ]
            }));
            if let Some(key) = api_key {
                req = req.bearer_auth(key);
            }
            let resp = req
                .send()
                .map_err(|e| format!("reason request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("reason model returned status {}", resp.status()));
            }
            let body = resp
                .text()
                .map_err(|e| format!("reason read failed: {e}"))?;
            Ok(crate::asc2::assemble_completion(&body))
        });
        let answer = worker
            .join()
            .map_err(|_| "reason worker thread panicked".to_string())??;
        if answer.trim().is_empty() {
            Err("reason model returned an empty answer".into())
        } else {
            Ok(answer)
        }
    }
}

fn canonical_tool(tool: &str) -> &str {
    let tool = tool.trim();
    let lower = tool.to_ascii_lowercase();
    if lower.contains("deep_research")
        || lower.contains("deep research")
        || lower.contains("research_engine")
        || lower.contains("web_search")
        || lower.contains("web_research")
    {
        return "deep_research";
    }
    if lower.contains("file") || lower.contains("workspace") {
        if lower.contains("write") || lower.contains("save") {
            return "file_writer";
        }
        if lower.contains("read") || lower.contains("load") || lower.contains("open") {
            return "file_reader";
        }
        if lower.contains("list") || lower.contains("dir") {
            return "file_lister";
        }
        if lower.contains("delete") || lower.contains("remove") {
            return "file_deleter";
        }
    }
    if lower.contains("reason")
        || lower.contains("think")
        || lower.contains("cognit")
        || lower == "llm"
    {
        return "reason";
    }
    if tool.contains("summar") {
        "summarizer"
    } else if tool.contains("hash") {
        "hasher"
    } else if tool.contains("extract") {
        "extractor"
    } else if tool.contains("count") || tool.contains("token") {
        "counter"
    } else if tool.contains("classif") {
        "classifier"
    } else if tool.contains("plan") {
        "planner"
    } else if tool.contains("valid") || tool.contains("schema") {
        "validator"
    } else {
        "echo"
    }
}

/// Dispatches one tool call: fabricated blueprint tools take precedence over
/// the built-in registry, and file tools are jailed to the agent workspace.
fn execute_tool(
    tool: &str,
    input: &serde_json::Value,
    context: &ToolContext,
) -> Result<serde_json::Value, String> {
    if let Some(custom) = context.custom_tools.get(tool.trim()) {
        if custom.canary_status != "passed" {
            return Err(format!(
                "custom tool '{tool}' has not passed canary validation"
            ));
        }
        let blueprint = custom
            .blueprint
            .as_ref()
            .ok_or_else(|| format!("custom tool '{tool}' has no blueprint"))?;
        return execute_blueprint(blueprint, input);
    }
    if crate::device_agent::is_device_tool(tool) {
        return run_device_tool(context, tool, input);
    }
    match canonical_tool(tool) {
        "reason" => run_reason(context, input),
        "deep_research" => run_deep_research(context, input),
        "file_writer" => file_write(context, input),
        "file_reader" => file_read(context, input),
        "file_lister" => file_list(context, input),
        "file_deleter" => file_delete(context, input),
        _ => execute_builtin_tool(tool, input),
    }
}

/// The `reason` tool: a genuine model reasoning step over the agent's current
/// input. This is what lets a generated/swarm agent *think* about its task with
/// the configured LLM (Nemotron), rather than only shuffle data through canned
/// transforms. Runs only when a reasoner is wired into the runtime.
fn run_reason(
    context: &ToolContext,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let reasoner = context
        .reasoner
        .as_ref()
        .ok_or("no reasoning model configured for this runtime (set ASTRA_ASC2_REMOTE_ENDPOINT)")?;
    let task = json_text(input);
    let task = if task.trim().is_empty() {
        input.to_string()
    } else {
        task
    };
    const REASON_SYSTEM: &str = "You are a focused worker agent inside a larger pipeline. Reason \
        carefully about the given task/input and produce a concise, correct, useful result. If the \
        input is partial output from an earlier step, perform the most useful next reasoning step.";
    let answer = reasoner.reason(REASON_SYSTEM, &task)?;
    Ok(serde_json::json!({
        "tool": "reason",
        "model": reasoner.model(),
        "answer": answer,
    }))
}

/// Routes a device-capability call through the supervised device layer, if the
/// runtime was wired with one. This is the path that lets an agent actually
/// see and act on the user's machine.
fn run_device_tool(
    context: &ToolContext,
    tool: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let device = context
        .device
        .as_ref()
        .ok_or("device layer is not wired into this runtime")?;
    device.execute(tool, input)
}

/// Calls the real deep-research engine and returns a compact evidence digest
/// the agent can feed into subsequent steps.
fn run_deep_research(
    context: &ToolContext,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    use crate::search_intelligence::{DeepSearchRequest, SearchResponseMode};

    let research = context
        .research
        .as_ref()
        .ok_or("deep research engine is not wired into this runtime")?;
    let query = input
        .get("query")
        .and_then(serde_json::Value::as_str)
        .or_else(|| input.get("objective").and_then(serde_json::Value::as_str))
        .or_else(|| input.get("brief").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| json_text(input));
    if query.trim().is_empty() {
        return Err("deep_research requires a query".into());
    }
    let urls = input
        .get("urls")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let request = DeepSearchRequest {
        query: query.clone(),
        mode: SearchResponseMode::Balanced,
        max_sources: Some(8),
        max_crawl_steps: Some(6),
        freshness_horizon_hours: Some(168),
        source_classes: Vec::new(),
        allowed_domains: Vec::new(),
        blocked_domains: Vec::new(),
        require_certificate: false,
        autonomous_crawl: !urls.is_empty(),
        urls,
        seed_documents: Vec::new(),
    };
    let response = research
        .execute(request)
        .map_err(|error| format!("deep research failed: {error}"))?;
    let evidence: Vec<serde_json::Value> = response
        .evidence
        .iter()
        .filter(|item| item.accepted)
        .take(5)
        .map(|item| {
            serde_json::json!({
                "title": item.title,
                "url": item.url,
                "summary": item.summary,
                "relevance": item.relevance_score,
                "credibility": item.credibility_score,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "tool": "deep_research",
        "query": query,
        "executive_summary": response.executive_summary,
        "confidence": response.confidence,
        "evidence": evidence,
        "evidence_count": evidence.len(),
        "formula_version": response.formula_report.formula_version,
    }))
}

// ── Jailed file handling ──
//
// Every path is relative, normal-components-only, and resolves inside the
// agent's workspace directory. Sizes and file counts are hard-capped.

fn resolve_workspace_path(workspace: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = relative.trim();
    if relative.is_empty() {
        return Err("file path must not be empty".into());
    }
    if relative.len() > 256 {
        return Err("file path is too long".into());
    }
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err("absolute paths are blocked; use workspace-relative paths".into());
    }
    for component in candidate.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_string_lossy();
                if part.starts_with('.') || part.contains(':') {
                    return Err(format!("path component '{part}' is blocked"));
                }
            }
            _ => return Err("path traversal is blocked".into()),
        }
    }
    Ok(workspace.join(candidate))
}

fn path_param(input: &serde_json::Value) -> Result<String, String> {
    input
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "input must carry a string 'path' field".into())
}

fn workspace_file_count(workspace: &Path) -> usize {
    fn walk(dir: &Path, count: &mut usize) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, count);
                } else {
                    *count += 1;
                }
            }
        }
    }
    let mut count = 0;
    walk(workspace, &mut count);
    count
}

fn file_write(
    context: &ToolContext,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = path_param(input)?;
    let content = input
        .get("content")
        .map(|value| match value {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .ok_or("input must carry a 'content' field")?;
    if content.len() > MAX_FILE_BYTES {
        return Err(format!(
            "content exceeds the {MAX_FILE_BYTES}-byte workspace file cap"
        ));
    }
    let target = resolve_workspace_path(&context.workspace, &path)?;
    if !target.exists() && workspace_file_count(&context.workspace) >= MAX_WORKSPACE_FILES {
        return Err(format!(
            "workspace file cap of {MAX_WORKSPACE_FILES} reached"
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create workspace directory: {error}"))?;
    }
    std::fs::write(&target, content.as_bytes())
        .map_err(|error| format!("workspace write failed: {error}"))?;
    Ok(serde_json::json!({
        "tool": "file_writer",
        "path": path,
        "bytes_written": content.len(),
        "content_hash": sha3_hex(content.as_bytes()),
    }))
}

fn file_read(
    context: &ToolContext,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = path_param(input)?;
    let target = resolve_workspace_path(&context.workspace, &path)?;
    let metadata = std::fs::metadata(&target)
        .map_err(|error| format!("workspace file not readable: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("'{path}' is not a file"));
    }
    if metadata.len() as usize > MAX_FILE_BYTES {
        return Err(format!(
            "file exceeds the {MAX_FILE_BYTES}-byte workspace read cap"
        ));
    }
    let content = std::fs::read_to_string(&target)
        .map_err(|error| format!("workspace read failed: {error}"))?;
    Ok(serde_json::json!({
        "tool": "file_reader",
        "path": path,
        "bytes": content.len(),
        "content": content,
    }))
}

fn file_list(
    context: &ToolContext,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let subdir = input
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let target = if subdir.trim().is_empty() {
        context.workspace.clone()
    } else {
        resolve_workspace_path(&context.workspace, subdir)?
    };
    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&target) {
        for entry in read_dir.flatten().take(MAX_WORKSPACE_FILES) {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata().ok();
            entries.push(serde_json::json!({
                "name": name,
                "is_dir": metadata.as_ref().is_some_and(std::fs::Metadata::is_dir),
                "bytes": metadata.map_or(0, |m| m.len()),
            }));
        }
    }
    entries.sort_by_key(|entry| entry["name"].as_str().unwrap_or_default().to_string());
    Ok(serde_json::json!({
        "tool": "file_lister",
        "path": subdir,
        "entries": entries,
        "count": entries.len(),
    }))
}

fn file_delete(
    context: &ToolContext,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = path_param(input)?;
    let target = resolve_workspace_path(&context.workspace, &path)?;
    if !target.is_file() {
        return Err(format!("'{path}' is not a workspace file"));
    }
    std::fs::remove_file(&target).map_err(|error| format!("workspace delete failed: {error}"))?;
    Ok(serde_json::json!({
        "tool": "file_deleter",
        "path": path,
        "deleted": true,
    }))
}

// ── Declarative blueprint engine ──
//
// A blueprint is `{"pipeline": [{"op": ..., ...params}, ...]}` composed only
// of deterministic primitive operations. Agents fabricate new tools by
// composing these primitives; nothing in a blueprint can touch the network
// or escape the runtime.

const BLUEPRINT_OPS: &[&str] = &[
    "extract_field",
    "summarize",
    "hash",
    "count",
    "classify",
    "template",
    "uppercase",
    "lowercase",
];

fn validate_blueprint(blueprint: &serde_json::Value) -> Result<(), AppError> {
    let pipeline = blueprint
        .get("pipeline")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            AppError::Validation("tool blueprint must carry a 'pipeline' array".into())
        })?;
    if pipeline.is_empty() {
        return Err(AppError::Validation(
            "tool blueprint pipeline must not be empty".into(),
        ));
    }
    if pipeline.len() > MAX_BLUEPRINT_OPS {
        return Err(AppError::Validation(format!(
            "tool blueprint pipeline exceeds {MAX_BLUEPRINT_OPS} operations"
        )));
    }
    for step in pipeline {
        let op = step
            .get("op")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                AppError::Validation("every blueprint step needs a string 'op'".into())
            })?;
        if !BLUEPRINT_OPS.contains(&op) {
            return Err(AppError::Validation(format!(
                "unknown blueprint op '{op}'; allowed: {}",
                BLUEPRINT_OPS.join(", ")
            )));
        }
    }
    Ok(())
}

fn execute_blueprint(
    blueprint: &serde_json::Value,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let pipeline = blueprint
        .get("pipeline")
        .and_then(serde_json::Value::as_array)
        .ok_or("blueprint has no pipeline")?;
    let mut value = input.clone();
    for step in pipeline.iter().take(MAX_BLUEPRINT_OPS) {
        let op = step
            .get("op")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        value = match op {
            "extract_field" => {
                let pointer = step
                    .get("pointer")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                value
                    .pointer(pointer)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null)
            }
            "summarize" => {
                let max_words = step
                    .get("max_words")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(48)
                    .clamp(1, 256) as usize;
                let text = json_text(&value);
                let words: Vec<&str> = text.split_whitespace().collect();
                serde_json::json!({
                    "summary": words.iter().take(max_words).copied().collect::<Vec<_>>().join(" "),
                    "source_word_count": words.len(),
                })
            }
            "hash" => serde_json::json!({
                "sha3_256": sha3_hex(value.to_string().as_bytes()),
            }),
            "count" => {
                let text = json_text(&value);
                serde_json::json!({
                    "word_count": text.split_whitespace().count(),
                    "char_count": text.chars().count(),
                })
            }
            "classify" => execute_builtin_tool("classifier", &value)?,
            "template" => {
                let format = step
                    .get("format")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("{value}");
                serde_json::Value::String(format.replace("{value}", &json_text(&value)))
            }
            "uppercase" => serde_json::Value::String(json_text(&value).to_uppercase()),
            "lowercase" => serde_json::Value::String(json_text(&value).to_lowercase()),
            other => return Err(format!("unknown blueprint op '{other}'")),
        };
    }
    Ok(serde_json::json!({
        "tool": "custom_blueprint",
        "result": value,
    }))
}

/// Executes one of the deterministic built-in tools against a JSON input.
/// Every tool is pure, offline, and bounded.
fn execute_builtin_tool(
    tool: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let text = json_text(input);
    match canonical_tool(tool) {
        "summarizer" => {
            let words: Vec<&str> = text.split_whitespace().collect();
            let summary = words.iter().take(48).copied().collect::<Vec<_>>().join(" ");
            Ok(serde_json::json!({
                "tool": "summarizer",
                "summary": summary,
                "source_word_count": words.len(),
                "truncated": words.len() > 48,
            }))
        }
        "hasher" => Ok(serde_json::json!({
            "tool": "hasher",
            "sha3_256": sha3_hex(input.to_string().as_bytes()),
            "input_bytes": input.to_string().len(),
        })),
        "extractor" => {
            let mut keys = Vec::new();
            collect_keys(input, "", &mut keys);
            keys.truncate(64);
            Ok(serde_json::json!({
                "tool": "extractor",
                "fields": keys,
                "field_count": keys.len(),
            }))
        }
        "counter" => {
            let words = text.split_whitespace().count();
            let chars = text.chars().count();
            Ok(serde_json::json!({
                "tool": "counter",
                "word_count": words,
                "char_count": chars,
            }))
        }
        "classifier" => {
            let lower = text.to_ascii_lowercase();
            let label = if ["error", "fail", "panic", "denied"]
                .iter()
                .any(|needle| lower.contains(needle))
            {
                "incident"
            } else if ["revenue", "invoice", "payment", "price"]
                .iter()
                .any(|needle| lower.contains(needle))
            {
                "financial"
            } else if ["research", "study", "evidence", "source"]
                .iter()
                .any(|needle| lower.contains(needle))
            {
                "research"
            } else {
                "general"
            };
            Ok(serde_json::json!({
                "tool": "classifier",
                "label": label,
            }))
        }
        "planner" => {
            let words: Vec<&str> = text.split_whitespace().take(12).collect();
            Ok(serde_json::json!({
                "tool": "planner",
                "plan": [
                    format!("normalize objective: {}", words.join(" ")),
                    "verify inputs against the tool allowlist",
                    "execute within sandbox budgets",
                    "notarize the receipt",
                ],
            }))
        }
        "validator" => {
            let valid = !input.is_null();
            Ok(serde_json::json!({
                "tool": "validator",
                "valid": valid,
                "kind": json_kind(input),
            }))
        }
        _ => Ok(serde_json::json!({
            "tool": "echo",
            "echo": input,
        })),
    }
}

fn json_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn collect_keys(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
    if out.len() >= 64 {
        return;
    }
    if let serde_json::Value::Object(map) = value {
        for (key, child) in map {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            out.push(path.clone());
            collect_keys(child, &path, out);
        }
    }
}

fn fixed_roles() -> [OrchestrationRole; 5] {
    [
        OrchestrationRole::Planner,
        OrchestrationRole::Safety,
        OrchestrationRole::Executor,
        OrchestrationRole::Verifier,
        OrchestrationRole::Notary,
    ]
}

fn orchestration_role_for_asc2(role: &str) -> OrchestrationRole {
    let role = role.to_ascii_lowercase();
    if role.contains("safety") || role.contains("certifier") {
        OrchestrationRole::Safety
    } else if role.contains("verifier") || role.contains("critic") {
        OrchestrationRole::Verifier
    } else if role.contains("tool") || role.contains("executor") {
        OrchestrationRole::Executor
    } else if role.contains("audit") || role.contains("notary") {
        OrchestrationRole::Notary
    } else {
        OrchestrationRole::Planner
    }
}

fn clean_required(value: &str, field: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }
    if value.len() > 1024 {
        return Err(AppError::Validation(format!("{field} is too long")));
    }
    Ok(value.to_string())
}

fn default_tools_for_activity(activity_kind: &str) -> Vec<String> {
    match activity_kind {
        "assistant_command" => vec!["command_planner", "action_allowlist", "receipt_notary"],
        "os_guardian_event" => vec!["telemetry_classifier", "threat_evaluator", "receipt_notary"],
        "dlp_analysis" => vec!["dlp_classifier", "redaction_policy", "receipt_notary"],
        "semantic_render" => vec!["semantic_extractor", "content_hasher", "receipt_notary"],
        "research" => vec!["source_fetcher", "citation_ranker", "artifact_scanner"],
        _ => vec!["planner", "safety", "verifier", "receipt_notary"],
    }
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn tool_is_forbidden(tool: &str) -> bool {
    let tool = tool.to_ascii_lowercase();
    contains_any(
        &tool,
        &[
            "stealth",
            "fingerprint_evasion",
            "credential_dump",
            "password_dump",
            "remote_takeover",
            "unauthorized",
            "wallet_signer",
            "destructive_remediation",
        ],
    )
}

fn tools_for_role(role: OrchestrationRole, requested_tools: &[String]) -> Vec<String> {
    let marker = role.as_str();
    let matched = requested_tools
        .iter()
        .filter(|tool| tool.contains(marker))
        .cloned()
        .collect::<Vec<_>>();
    if matched.is_empty() {
        vec![format!("{marker}_gate")]
    } else {
        matched
    }
}

fn notes_for_role(
    role: OrchestrationRole,
    decision: ApprovalDecision,
    blocked_reason: Option<&str>,
) -> Vec<String> {
    match role {
        OrchestrationRole::Planner => {
            vec!["activity normalized into a governed execution graph".into()]
        }
        OrchestrationRole::Safety => match decision {
            ApprovalDecision::Allowed => vec!["owner-authorized safe action policy passed".into()],
            ApprovalDecision::ApprovalRequired => {
                vec!["risk tier or action class requires owner approval".into()]
            }
            ApprovalDecision::Blocked => vec![blocked_reason.unwrap_or("activity blocked").into()],
        },
        OrchestrationRole::Executor => match decision {
            ApprovalDecision::Allowed => vec!["execution allowed for safe local action".into()],
            ApprovalDecision::ApprovalRequired => {
                vec!["execution paused pending owner approval".into()]
            }
            ApprovalDecision::Blocked => vec!["execution suppressed by safety policy".into()],
        },
        OrchestrationRole::Verifier => {
            vec!["result checked against policy and trace hashes".into()]
        }
        OrchestrationRole::Notary => {
            vec!["local tamper-evident receipt prepared for audit anchoring".into()]
        }
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod execution_tests {
    use super::*;

    fn agent_request(tools: Vec<&str>, cost_budget: u32) -> CreateGeneratedAgentRequest {
        CreateGeneratedAgentRequest {
            tenant_scope: TenantScope::Global,
            goal: "summarize governed enterprise work".into(),
            tool_allowlist: tools.into_iter().map(str::to_string).collect(),
            output_schema: serde_json::json!({"type": "object"}),
            cost_budget,
            time_budget_ms: 5_000,
            risk_tier: RiskTier::Low,
            escalation_policy: "block".into(),
        }
    }

    #[test]
    fn execution_runs_real_tools_and_pipelines_outputs() {
        let service = AgentRuntimeService::new();
        let manifest = service
            .create_agent(agent_request(vec!["summarizer", "content_hasher"], 10))
            .expect("manifest");
        let receipt = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!({"brief": "one two three four five"}),
                    requested_tools: vec!["summarizer".into(), "content_hasher".into()],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("execution");

        assert_eq!(receipt.status, "completed");
        assert_eq!(receipt.steps.len(), 2);
        assert!(receipt.steps.iter().all(|step| step.status == "completed"));
        // Outputs are real: hashes are SHA3 of actual tool output, not templated.
        assert_eq!(receipt.steps[0].output_hash.len(), 64);
        assert!(
            receipt.steps[0]
                .output_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("summarizer"))
        );
        // The hasher receives the summarizer's output (pipelining), so its
        // input hash equals SHA3 of the first step's output.
        assert_ne!(receipt.steps[0].input_hash, receipt.steps[1].input_hash);
    }

    #[test]
    fn cost_budget_exhaustion_skips_remaining_steps() {
        let service = AgentRuntimeService::new();
        // summarizer costs 2; budget 2 leaves nothing for the second tool.
        let manifest = service
            .create_agent(agent_request(vec!["summarizer", "word_counter"], 2))
            .expect("manifest");
        let receipt = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!("budget test"),
                    requested_tools: Vec::new(),
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("execution");

        assert_eq!(receipt.status, "completed_with_warnings");
        assert_eq!(receipt.steps[0].status, "completed");
        assert_eq!(receipt.steps[1].status, "skipped_cost_budget_exhausted");
    }

    #[test]
    fn deterministic_tools_are_pure() {
        let input = serde_json::json!({"a": {"b": 1}, "c": "evidence research"});
        let first = execute_builtin_tool("field_extractor", &input).expect("extract");
        let second = execute_builtin_tool("field_extractor", &input).expect("extract");
        assert_eq!(first, second);
        let classified = execute_builtin_tool("topic_classifier", &input).expect("classify");
        assert_eq!(classified["label"], "research");
    }

    #[test]
    fn file_tools_write_read_list_inside_jailed_workspace() {
        let service = AgentRuntimeService::new();
        let manifest = service
            .create_agent(agent_request(
                vec!["file_writer", "file_reader", "file_lister"],
                20,
            ))
            .expect("manifest");
        let receipt = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!({
                        "path": "notes/draft.md",
                        "content": "# Findings\nGoverned agents can persist work."
                    }),
                    requested_tools: vec!["file_writer".into()],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("write execution");
        assert_eq!(receipt.status, "completed");
        assert!(
            receipt.steps[0]
                .output_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("bytes_written"))
        );

        let read_back = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!({"path": "notes/draft.md"}),
                    requested_tools: vec!["file_reader".into()],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("read execution");
        assert_eq!(read_back.status, "completed");
        assert!(
            read_back.steps[0]
                .output_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("Findings"))
        );

        let listing = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!({"path": "notes"}),
                    requested_tools: vec!["file_lister".into()],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("list execution");
        assert!(
            listing.steps[0]
                .output_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("draft.md"))
        );
    }

    #[test]
    fn file_tools_block_path_traversal_and_absolute_paths() {
        let workspace = std::env::temp_dir().join(format!("astra_jail_{}", new_id("t")));
        let context = ToolContext {
            workspace,
            custom_tools: Arc::new(HashMap::new()),
            research: None,
            device: None,
            reasoner: None,
        };
        for path in [
            "../escape.txt",
            "..\\escape.txt",
            "/etc/passwd",
            "C:/temp/x",
            "a/../../b",
        ] {
            let result = file_write(&context, &serde_json::json!({"path": path, "content": "x"}));
            assert!(result.is_err(), "path '{path}' should be blocked");
        }
        // Hidden/dotted components are blocked too.
        assert!(
            file_write(
                &context,
                &serde_json::json!({"path": ".ssh/config", "content": "x"})
            )
            .is_err()
        );
    }

    #[test]
    fn file_write_enforces_size_cap() {
        let workspace = std::env::temp_dir().join(format!("astra_cap_{}", new_id("t")));
        let context = ToolContext {
            workspace,
            custom_tools: Arc::new(HashMap::new()),
            research: None,
            device: None,
            reasoner: None,
        };
        let oversized = "x".repeat(MAX_FILE_BYTES + 1);
        let error = file_write(
            &context,
            &serde_json::json!({"path": "big.txt", "content": oversized}),
        )
        .expect_err("oversized write should fail");
        assert!(error.contains("cap"));
    }

    #[test]
    fn custom_blueprint_tool_is_canary_tested_and_executable() {
        let service = AgentRuntimeService::new();
        let tool = service
            .create_tool(CreateGeneratedToolRequest {
                tenant_scope: TenantScope::Global,
                name: "brief_digest".into(),
                description: "summarize then template".into(),
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                kind: GeneratedToolKind::Extractor,
                blueprint: Some(serde_json::json!({
                    "pipeline": [
                        {"op": "summarize", "max_words": 4},
                        {"op": "extract_field", "pointer": "/summary"},
                        {"op": "template", "format": "DIGEST: {value}"},
                        {"op": "uppercase"},
                    ]
                })),
                implementation_ref: "blueprint:inline".into(),
            })
            .expect("tool");
        assert_eq!(tool.canary_status, "passed");

        let manifest = service
            .create_agent(agent_request(vec!["brief_digest"], 10))
            .expect("manifest");
        let receipt = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!("alpha beta gamma delta epsilon zeta"),
                    requested_tools: vec!["brief_digest".into()],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("execution");
        assert_eq!(receipt.status, "completed");
        assert!(
            receipt.steps[0]
                .output_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("DIGEST: ALPHA BETA GAMMA DELTA"))
        );
    }

    #[test]
    fn invalid_blueprints_are_rejected_at_creation() {
        let service = AgentRuntimeService::new();
        let request = |blueprint: serde_json::Value| CreateGeneratedToolRequest {
            tenant_scope: TenantScope::Global,
            name: "bad_tool".into(),
            description: "invalid".into(),
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            kind: GeneratedToolKind::Extractor,
            blueprint: Some(blueprint),
            implementation_ref: "blueprint:inline".into(),
        };
        assert!(service.create_tool(request(serde_json::json!({}))).is_err());
        assert!(
            service
                .create_tool(request(serde_json::json!({"pipeline": []})))
                .is_err()
        );
        assert!(
            service
                .create_tool(request(
                    serde_json::json!({"pipeline": [{"op": "shell_exec"}]})
                ))
                .is_err()
        );
    }

    #[test]
    fn deep_research_tool_calls_the_real_engine() {
        let research = crate::search_intelligence::SearchIntelligenceService::new();
        let service = AgentRuntimeService::new().with_research(research);
        let manifest = service
            .create_agent(agent_request(vec!["deep_research"], 10))
            .expect("manifest");
        let receipt = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!({"query": "governed enterprise research evidence"}),
                    requested_tools: vec!["deep_research".into()],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("execution");
        assert_eq!(receipt.steps.len(), 1);
        assert_eq!(receipt.steps[0].status, "completed");
        // The step output is the real research engine's digest.
        assert!(
            receipt.steps[0]
                .output_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("deep_research")
                    || summary.contains("executive_summary"))
        );
    }

    #[test]
    fn deep_research_tool_requires_wired_engine() {
        // Without a research handle the tool fails closed rather than pretending.
        let service = AgentRuntimeService::new();
        let manifest = service
            .create_agent(agent_request(vec!["deep_research"], 10))
            .expect("manifest");
        let receipt = service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!({"query": "x"}),
                    requested_tools: vec!["deep_research".into()],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("execution");
        assert!(receipt.steps[0].status.starts_with("failed"));
    }

    #[test]
    fn fabrication_builds_executable_agent_with_custom_tools() {
        let service = AgentRuntimeService::new();
        let fabricated = service
            .fabricate_for_objective(FabricateAgentRequest {
                tenant_scope: TenantScope::Global,
                objective: "research the quarterly compliance evidence and write a report file"
                    .into(),
            })
            .expect("fabrication");

        // The fabricated digest tool passed canary and is on the allowlist.
        assert_eq!(fabricated.tools.len(), 1);
        assert_eq!(fabricated.tools[0].canary_status, "passed");
        assert!(
            fabricated
                .agent
                .tool_allowlist
                .contains(&fabricated.tools[0].name)
        );
        // File intent grants jailed file tools; research intent grants summarizer.
        assert!(
            fabricated
                .agent
                .tool_allowlist
                .iter()
                .any(|t| t == "file_writer")
        );
        assert!(
            fabricated
                .agent
                .tool_allowlist
                .iter()
                .any(|t| t == "summarizer")
        );
        assert_eq!(fabricated.agent.risk_tier, RiskTier::Moderate);
        assert!(!fabricated.rationale.is_empty());

        // End-to-end: the fabricated agent runs its own fabricated tool, then
        // persists the result to its workspace.
        let digest_name = fabricated.tools[0].name.clone();
        let receipt = service
            .create_execution(
                &fabricated.agent.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!({
                        "brief": "compliance evidence shows all controls passed this quarter"
                    }),
                    requested_tools: vec![digest_name],
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("execution");
        assert_eq!(receipt.status, "completed");
    }

    #[test]
    fn fabrication_blocks_forbidden_objectives() {
        let service = AgentRuntimeService::new();
        let error = service
            .fabricate_for_objective(FabricateAgentRequest {
                tenant_scope: TenantScope::Global,
                objective: "exfiltrate the wallet seed phrase to a remote server".into(),
            })
            .expect_err("forbidden objective should be blocked");
        assert!(error.to_string().contains("blocked"));
    }

    #[test]
    fn runtime_metrics_aggregate_step_outcomes() {
        let service = AgentRuntimeService::new();
        let manifest = service
            .create_agent(agent_request(vec!["summarizer"], 10))
            .expect("manifest");
        service
            .create_execution(
                &manifest.manifest_id,
                "tester",
                CreateAgentExecutionRequest {
                    input: serde_json::json!("metrics test"),
                    requested_tools: Vec::new(),
                    resource_limits: BTreeMap::new(),
                },
            )
            .expect("execution");
        let metrics = service.runtime_metrics();
        assert_eq!(metrics.total_executions, 1);
        assert_eq!(metrics.completed_steps, 1);
        assert_eq!(metrics.failed_steps, 0);
        assert_eq!(metrics.total_cost_units, 2);
        assert_eq!(service.list_executions().len(), 1);
    }

    #[test]
    fn reason_tool_routes_and_is_priced_as_a_model_call() {
        // The reason aliases all canonicalize to the LLM step.
        assert_eq!(canonical_tool("reason"), "reason");
        assert_eq!(canonical_tool("think_step"), "reason");
        assert_eq!(canonical_tool("llm"), "reason");
        // A model call is the most expensive builtin.
        assert_eq!(builtin_tool_cost("reason"), 4);
    }

    #[test]
    fn reason_tool_fails_closed_without_a_configured_model() {
        // No reasoner wired -> the reason tool refuses rather than faking output.
        let context = ToolContext {
            workspace: std::env::temp_dir().join(format!("astra_reason_{}", new_id("t"))),
            custom_tools: Arc::new(HashMap::new()),
            research: None,
            device: None,
            reasoner: None,
        };
        let error = run_reason(&context, &serde_json::json!("think about this"))
            .expect_err("reason must fail closed with no model");
        assert!(error.contains("no reasoning model"));
    }

    #[test]
    fn fabricated_agents_are_granted_the_reason_tool() {
        let service = AgentRuntimeService::new();
        let fabricated = service
            .fabricate_for_objective(FabricateAgentRequest {
                tenant_scope: TenantScope::Global,
                objective: "research the latest evidence and summarize the key findings".into(),
            })
            .expect("fabrication");
        // The cognitive core: a real reasoning step is part of the pipeline.
        assert!(
            fabricated
                .agent
                .tool_allowlist
                .iter()
                .any(|t| t == "reason"),
            "fabricated agent should include the reason tool: {:?}",
            fabricated.agent.tool_allowlist
        );
    }
}

#[cfg(test)]
mod orchestration_tests {
    use super::*;

    fn request(objective: &str) -> CreateActivityExecutionRequest {
        CreateActivityExecutionRequest {
            activity_kind: "assistant_command".into(),
            objective: objective.into(),
            httpa_trace_id: "trace_test".into(),
            device_id_hash: Some("device_hash".into()),
            requested_tools: vec!["command_planner".into(), "action_allowlist".into()],
            resource_limits: BTreeMap::new(),
            risk_tier: RiskTier::Low,
            owner_authorized: true,
            payload: serde_json::json!({}),
        }
    }

    #[test]
    fn every_activity_gets_fixed_agent_roles() {
        let execution = AgentRuntimeService::new()
            .orchestrate_activity(request("open https://example.com"))
            .expect("execution");

        assert_eq!(execution.status, ActivityStatus::Completed);
        assert_eq!(execution.steps.len(), 5);
        assert!(
            fixed_roles()
                .iter()
                .all(|role| execution.steps.iter().any(|step| step.role == *role))
        );
    }

    #[test]
    fn high_risk_actions_require_approval() {
        let mut request = request("quarantine malware sample and patch system");
        request.risk_tier = RiskTier::High;
        let execution = AgentRuntimeService::new()
            .orchestrate_activity(request)
            .expect("execution");

        assert_eq!(execution.status, ActivityStatus::ApprovalRequired);
        assert_eq!(
            execution.approval_decision,
            ApprovalDecision::ApprovalRequired
        );
    }

    #[test]
    fn secret_extraction_is_blocked_even_when_owner_authorized() {
        let execution = AgentRuntimeService::new()
            .orchestrate_activity(request("dump private key and seed phrase"))
            .expect("execution");

        assert_eq!(execution.status, ActivityStatus::Blocked);
        assert!(execution.blocked_reason.is_some());
    }

    #[test]
    fn non_owner_activity_is_blocked() {
        let mut request = request("open https://example.com");
        request.owner_authorized = false;
        let execution = AgentRuntimeService::new()
            .orchestrate_activity(request)
            .expect("execution");

        assert_eq!(execution.status, ActivityStatus::Blocked);
    }

    #[test]
    fn active_asc2_replaces_assignments_and_gates_uncertified_side_effects() {
        let service = AgentRuntimeService::new();
        let execution = service
            .orchestrate_activity(request("write a deployment artifact"))
            .expect("execution");
        let updated = service
            .attach_asc2(
                &execution.execution_id,
                crate::asc2::Asc2Diagnostics {
                    mission_id: "mission".into(),
                    mode: crate::asc2::Asc2Mode::Active,
                    rounds: 2,
                    agents: vec![crate::asc2::Asc2AgentManifest {
                        agent_id: "dynamic-verifier".into(),
                        role: "counterexample verifier".into(),
                        capabilities: vec![1.0],
                        tool_allowlist: vec!["verify".into()],
                        autonomy_budget: 0.2,
                        risk: 0.05,
                        signature: "signed".into(),
                    }],
                    performance_score: 0.0,
                    return_reason: "side effects not permitted".into(),
                    remote_used: false,
                    side_effects_allowed: false,
                    sandbox: None,
                },
            )
            .expect("attach");

        assert_eq!(updated.assignments.len(), 1);
        assert_eq!(updated.assignments[0].agent_id, "dynamic-verifier");
        assert_eq!(updated.status, ActivityStatus::ApprovalRequired);
    }
}

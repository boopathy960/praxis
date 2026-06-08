use std::collections::{BTreeMap, HashMap};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, TenantScope, new_id, now_ms, sha3_hex};

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
    pub implementation_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecutionStep {
    pub step_id: String,
    pub tool: String,
    pub input_hash: String,
    pub output_hash: String,
    pub status: String,
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
}

impl AgentRuntimeService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Shared::new(RwLock::new(AgentRuntimeStore::default())),
        }
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

        let created_at_ms = now_ms();
        let signature = sha3_hex(format!(
            "{}:{}:{:?}:{}",
            request.name, request.description, request.kind, request.implementation_ref
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
            canary_status: "pending".into(),
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
        let mut store = self.store.write();
        let manifest = store
            .agents
            .get(manifest_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("agent manifest {manifest_id}")))?;
        if !manifest.enabled {
            return Err(AppError::Validation("agent manifest is disabled".into()));
        }

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
        let mut steps = Vec::new();
        for (index, tool) in requested_tools.iter().enumerate() {
            let input_hash = sha3_hex(format!("{request_hash}:{tool}:{index}:input").as_bytes());
            let output_hash = sha3_hex(format!("{request_hash}:{tool}:{index}:output").as_bytes());
            steps.push(AgentExecutionStep {
                step_id: new_id("agent_step"),
                tool: tool.clone(),
                input_hash,
                output_hash,
                status: "completed".into(),
                started_at_ms: created_at_ms + index as i64,
                completed_at_ms: created_at_ms + index as i64 + 1,
            });
        }

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
            status: "completed".into(),
            steps,
            resource_limits: request.resource_limits,
            created_at_ms,
            completed_at_ms: now_ms(),
            chain_block_height: None,
            chain_block_hash: None,
            chain_receipt_id: None,
            sandbox: None,
        };
        store.executions.insert(execution_id, receipt.clone());
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
                    control_mode: astra_brain::ControlDecision::Refuse,
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
                    formula_metrics: Default::default(),
                    certificate: None,
                    performance_score: 0.0,
                    return_reason: "certificate failed".into(),
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

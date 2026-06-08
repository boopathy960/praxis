use std::collections::BTreeMap;
use std::env;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::common::{AppError, TenantScope, new_id, now_ms, sha3_hex};

const GENESIS_HASH: &str = "GENESIS";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxMode {
    Enforcing,
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxRuntimeKind {
    Docker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxRuntimeState {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxDecisionOutcome {
    Allow,
    Deny,
    Rewrite,
    ApprovalRequired,
    SandboxUnavailable,
}

impl SandboxDecisionOutcome {
    #[must_use]
    pub const fn executes(self) -> bool {
        matches!(self, Self::Allow | Self::Rewrite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxActionKind {
    FileRead,
    FileWrite,
    FileDelete,
    NetworkFetch,
    ShellExecution,
    CodeExecution,
    DbQuery,
    MessageSend,
    SpendPayment,
    IntegrationCall,
    NexusDelivery,
    #[serde(other)]
    Unknown,
}

impl SandboxActionKind {
    #[must_use]
    pub const fn requires_approval(self) -> bool {
        matches!(
            self,
            Self::FileWrite
                | Self::FileDelete
                | Self::MessageSend
                | Self::SpendPayment
                | Self::IntegrationCall
                | Self::NexusDelivery
        )
    }

    #[must_use]
    pub const fn requires_container(self) -> bool {
        matches!(
            self,
            Self::FileRead
                | Self::FileWrite
                | Self::FileDelete
                | Self::NetworkFetch
                | Self::ShellExecution
                | Self::CodeExecution
                | Self::DbQuery
                | Self::MessageSend
                | Self::SpendPayment
                | Self::IntegrationCall
                | Self::NexusDelivery
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum SandboxCapability {
    ReadPath(String),
    WritePath(String),
    DeletePath(String),
    NetHost(String),
    ExecCommand(String),
    Tool(String),
    Tenant(String),
    Provider(String),
    MessageChannel(String),
    SpendAmount(String),
    NexusDelivery(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxTaint {
    Public,
    Secret,
    Credential,
    PersonalData,
    SourceCode,
    Financial,
    Hr,
    Legal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResourceLimits {
    pub timeout_ms: u64,
    pub max_output_bytes: u64,
    pub memory_mb: u64,
    pub cpu_units: u32,
}

impl Default for SandboxResourceLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 5_000,
            max_output_bytes: 64 * 1024,
            memory_mb: 256,
            cpu_units: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxAction {
    pub action_id: String,
    #[serde(default)]
    pub tenant_scope: Option<TenantScope>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub kind: SandboxActionKind,
    #[serde(default)]
    pub raw_request: serde_json::Value,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub capabilities: Vec<SandboxCapability>,
    #[serde(default)]
    pub taints: Vec<SandboxTaint>,
    #[serde(default)]
    pub resource_limits: SandboxResourceLimits,
    #[serde(default)]
    pub risk_score: Option<f64>,
    #[serde(default)]
    pub approval_token: Option<String>,
    #[serde(default)]
    pub asc2_advisory: Option<serde_json::Value>,
    #[serde(default)]
    pub control_depth: u8,
    #[serde(default)]
    pub parent_receipt_hash: Option<String>,
}

impl Default for SandboxAction {
    fn default() -> Self {
        Self {
            action_id: new_id("sandbox_action"),
            tenant_scope: None,
            session_id: None,
            kind: SandboxActionKind::Unknown,
            raw_request: serde_json::Value::Null,
            command: None,
            args: Vec::new(),
            path: None,
            url: None,
            host: None,
            tool: None,
            provider: None,
            payload: serde_json::Value::Null,
            capabilities: Vec::new(),
            taints: Vec::new(),
            resource_limits: SandboxResourceLimits::default(),
            risk_score: None,
            approval_token: None,
            asc2_advisory: None,
            control_depth: 0,
            parent_receipt_hash: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRewrite {
    pub reason: String,
    pub removed_capabilities: Vec<SandboxCapability>,
    pub lowered_limits: BTreeMap<String, u64>,
    pub effective_action: Box<SandboxAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxMonitorComponents {
    pub m0: bool,
    pub capability: bool,
    pub type_schema: bool,
    pub taint: bool,
    pub risk: bool,
    pub approval: bool,
    pub audit: bool,
    pub llm: bool,
}

impl SandboxMonitorComponents {
    #[must_use]
    pub const fn all_true() -> Self {
        Self {
            m0: true,
            capability: true,
            type_schema: true,
            taint: true,
            risk: true,
            approval: true,
            audit: true,
            llm: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxDecision {
    pub decision_id: String,
    pub action_id: String,
    pub outcome: SandboxDecisionOutcome,
    pub reason: String,
    pub monitor: SandboxMonitorComponents,
    #[serde(default)]
    pub rewrite: Option<SandboxRewrite>,
    #[serde(default)]
    pub effective_action: Option<SandboxAction>,
    #[serde(default)]
    pub policy_id: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    pub policy_id: String,
    pub block_by_default: bool,
    pub container_required: bool,
    pub default_network_deny: bool,
    pub allowed_workspace_roots: Vec<String>,
    pub allowed_hosts: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub allowed_tenants: Vec<String>,
    pub allowed_providers: Vec<String>,
    pub require_approval_for: Vec<SandboxActionKind>,
    pub max_risk_per_action: f64,
    pub max_risk_per_session: f64,
    pub max_control_rounds: u8,
    pub max_recursive_depth: u8,
    pub limits: SandboxResourceLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRuntimeStatus {
    pub mode: SandboxMode,
    pub runtime: SandboxRuntimeKind,
    pub runtime_state: SandboxRuntimeState,
    pub runtime_version: Option<String>,
    pub block_by_default: bool,
    pub container_required: bool,
    pub audit_entries: usize,
    pub workspace_root: String,
    pub reference_monitor: String,
    pub max_control_rounds: u8,
    pub max_recursive_depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxReceipt {
    pub receipt_id: String,
    pub action_id: String,
    pub decision: SandboxDecision,
    pub executed: bool,
    pub observation: serde_json::Value,
    pub observation_hash: String,
    pub audit_hash: String,
    pub receipt_hash: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxAuditEntry {
    pub entry_id: String,
    pub action_id: String,
    pub raw_request_hash: String,
    pub normalized_action: SandboxAction,
    pub monitor_components: SandboxMonitorComponents,
    pub decision: SandboxDecision,
    pub enforced_action: Option<SandboxAction>,
    pub observation_hash: String,
    pub previous_hash: String,
    pub entry_hash: String,
    pub receipt_hash: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxAuditVerification {
    pub valid: bool,
    pub entry_count: usize,
    pub latest_hash: Option<String>,
    pub tamper_index: Option<usize>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxNormalizeRequest {
    pub raw_request: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SandboxRuntimeConfig {
    pub mode: SandboxMode,
    pub runtime: SandboxRuntimeKind,
    pub image: String,
    pub workspace_root: PathBuf,
    pub policy: SandboxPolicy,
}

impl SandboxRuntimeConfig {
    #[must_use]
    pub fn from_env(data_dir: impl AsRef<Path>) -> Self {
        let workspace_root = data_dir.as_ref().join("workspaces");
        let mut allowed_roots = split_env("ASTRA_SANDBOX_ALLOWED_ROOTS");
        allowed_roots.push(workspace_root.to_string_lossy().to_string());
        Self {
            mode: match env::var("ASTRA_SANDBOX_MODE")
                .unwrap_or_else(|_| "enforcing".into())
                .to_ascii_lowercase()
                .as_str()
            {
                "shadow" => SandboxMode::Shadow,
                _ => SandboxMode::Enforcing,
            },
            runtime: SandboxRuntimeKind::Docker,
            image: env::var("ASTRA_SANDBOX_IMAGE").unwrap_or_else(|_| "alpine:3.20".into()),
            workspace_root: workspace_root.clone(),
            policy: SandboxPolicy {
                policy_id: env::var("ASTRA_SANDBOX_POLICY_ID")
                    .unwrap_or_else(|_| "astra_reference_monitor_v1".into()),
                block_by_default: true,
                container_required: true,
                default_network_deny: true,
                allowed_workspace_roots: allowed_roots,
                allowed_hosts: split_env("ASTRA_SANDBOX_ALLOWED_HOSTS"),
                allowed_commands: split_env("ASTRA_SANDBOX_ALLOWED_COMMANDS"),
                allowed_tools: split_env("ASTRA_SANDBOX_ALLOWED_TOOLS"),
                allowed_tenants: split_env("ASTRA_SANDBOX_ALLOWED_TENANTS"),
                allowed_providers: split_env("ASTRA_SANDBOX_ALLOWED_PROVIDERS"),
                require_approval_for: vec![
                    SandboxActionKind::FileWrite,
                    SandboxActionKind::FileDelete,
                    SandboxActionKind::MessageSend,
                    SandboxActionKind::SpendPayment,
                    SandboxActionKind::IntegrationCall,
                    SandboxActionKind::NexusDelivery,
                ],
                max_risk_per_action: env_f64("ASTRA_SANDBOX_MAX_RISK_PER_ACTION", 1.0),
                max_risk_per_session: env_f64("ASTRA_SANDBOX_MAX_RISK_PER_SESSION", 10.0),
                max_control_rounds: env_u64("ASTRA_SANDBOX_MAX_CONTROL_ROUNDS", 4) as u8,
                max_recursive_depth: env_u64("ASTRA_SANDBOX_MAX_RECURSIVE_DEPTH", 8) as u8,
                limits: SandboxResourceLimits {
                    timeout_ms: env_u64("ASTRA_SANDBOX_TIMEOUT_MS", 5_000),
                    max_output_bytes: env_u64("ASTRA_SANDBOX_MAX_OUTPUT_BYTES", 64 * 1024),
                    memory_mb: env_u64("ASTRA_SANDBOX_MEMORY_MB", 256),
                    cpu_units: env_u64("ASTRA_SANDBOX_CPU_UNITS", 1) as u32,
                },
            },
        }
    }
}

#[derive(Clone)]
pub struct SandboxService {
    store: Arc<Mutex<Connection>>,
    config: Arc<SandboxRuntimeConfig>,
}

impl SandboxService {
    pub fn new(data_dir: impl AsRef<Path>) -> Result<Self, AppError> {
        std::fs::create_dir_all(data_dir.as_ref()).map_err(|error| {
            AppError::Internal(format!("failed to create sandbox data directory: {error}"))
        })?;
        let config = SandboxRuntimeConfig::from_env(data_dir.as_ref());
        std::fs::create_dir_all(&config.workspace_root).map_err(|error| {
            AppError::Internal(format!("failed to create sandbox workspaces: {error}"))
        })?;
        let connection =
            Connection::open(data_dir.as_ref().join("sandbox.sqlite")).map_err(|error| {
                AppError::Internal(format!("failed to open sandbox store: {error}"))
            })?;
        initialize_store(&connection)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            config: Arc::new(config),
        })
    }

    pub fn status(&self) -> Result<SandboxRuntimeStatus, AppError> {
        let runtime = docker_status();
        Ok(SandboxRuntimeStatus {
            mode: self.config.mode,
            runtime: self.config.runtime,
            runtime_state: if runtime.is_some() {
                SandboxRuntimeState::Available
            } else {
                SandboxRuntimeState::Unavailable
            },
            runtime_version: runtime,
            block_by_default: self.config.policy.block_by_default,
            container_required: self.config.policy.container_required,
            audit_entries: count_audit_entries(&self.store.lock())?,
            workspace_root: self.config.workspace_root.to_string_lossy().to_string(),
            reference_monitor: "astra_unified_reference_monitor".into(),
            max_control_rounds: self.config.policy.max_control_rounds,
            max_recursive_depth: self.config.policy.max_recursive_depth,
        })
    }

    #[must_use]
    pub fn policies(&self) -> SandboxPolicy {
        self.config.policy.clone()
    }

    pub fn normalize(&self, request: SandboxNormalizeRequest) -> Result<SandboxAction, AppError> {
        normalize_raw_action(request.raw_request)
    }

    pub fn evaluate(&self, action: SandboxAction) -> SandboxDecision {
        self.evaluate_normalized(normalize_action(action))
    }

    pub fn attest(&self, action: SandboxAction) -> Result<SandboxReceipt, AppError> {
        self.guard(action, false)
    }

    pub fn execute(&self, action: SandboxAction) -> Result<SandboxReceipt, AppError> {
        self.guard(action, true)
    }

    pub fn guard(
        &self,
        action: SandboxAction,
        requested_execution: bool,
    ) -> Result<SandboxReceipt, AppError> {
        let normalized = normalize_action(action);
        let (mut decision, effective, rounds, rewrites_applied) =
            self.run_control_loop(normalized.clone());

        let mut executed = false;
        let mut observation = serde_json::json!({
            "status": "not_executed",
            "reference_monitor": "astra_unified_reference_monitor",
            "requested_execution": requested_execution,
            "control_loop_rounds": rounds,
            "rewrites_applied": rewrites_applied,
            "reason": decision.reason,
        });
        let mut enforced_action = None;

        if decision.outcome.executes() && requested_execution {
            if self.config.mode == SandboxMode::Shadow {
                observation = serde_json::json!({
                    "status": "shadow_blocked",
                    "executed": false,
                    "reference_monitor": "astra_unified_reference_monitor",
                    "protocol": "httpa",
                    "requested_execution": requested_execution,
                    "control_loop_rounds": rounds,
                    "rewrites_applied": rewrites_applied,
                    "reason": "sandbox shadow mode records the decision but blocks side effects",
                });
            } else if self.config.policy.container_required
                && effective.kind.requires_container()
                && docker_status().is_none()
            {
                decision.outcome = SandboxDecisionOutcome::SandboxUnavailable;
                decision.reason =
                    "container runtime is unavailable; effectful action failed closed".into();
                decision.monitor.m0 = false;
                observation = serde_json::json!({
                    "status": "sandbox_unavailable",
                    "executed": false,
                    "runtime": "docker",
                    "reference_monitor": "astra_unified_reference_monitor",
                    "protocol": "httpa",
                    "requested_execution": requested_execution,
                    "control_loop_rounds": rounds,
                    "rewrites_applied": rewrites_applied,
                    "reason": decision.reason,
                });
            } else {
                let result = self.enforce(&effective)?;
                executed = result.0;
                observation = merge_observation(
                    result.1,
                    serde_json::json!({
                        "reference_monitor": "astra_unified_reference_monitor",
                        "protocol": "httpa",
                        "requested_execution": requested_execution,
                        "control_loop_rounds": rounds,
                        "rewrites_applied": rewrites_applied,
                    }),
                );
                enforced_action = Some(effective.clone());
            }
        } else if decision.outcome.executes() {
            observation = serde_json::json!({
                "status": "attested",
                "executed": false,
                "reference_monitor": "astra_unified_reference_monitor",
                "protocol": "httpa",
                "requested_execution": requested_execution,
                "control_loop_rounds": rounds,
                "rewrites_applied": rewrites_applied,
                "semantic_rendering_required_for_network_intake": matches!(
                    effective.kind,
                    SandboxActionKind::NetworkFetch
                        | SandboxActionKind::IntegrationCall
                        | SandboxActionKind::NexusDelivery
                ),
                "reason": decision.reason,
            });
        } else {
            observation = serde_json::json!({
                "status": "blocked",
                "executed": false,
                "reference_monitor": "astra_unified_reference_monitor",
                "protocol": "httpa",
                "requested_execution": requested_execution,
                "control_loop_rounds": rounds,
                "rewrites_applied": rewrites_applied,
                "reason": decision.reason,
            });
        }

        let observation_hash = sha3_hex(observation.to_string().as_bytes());
        let receipt_id = new_id("sandbox_receipt");
        let receipt_hash = sha3_hex(
            serde_json::json!({
                "receipt_id": receipt_id,
                "action_id": normalized.action_id,
                "decision": decision,
                "executed": executed,
                "observation_hash": observation_hash,
            })
            .to_string()
            .as_bytes(),
        );
        let audit_hash = self.append_audit(
            &normalized,
            &decision,
            enforced_action,
            &observation_hash,
            &receipt_hash,
        )?;

        if executed {
            self.record_usage(&effective)?;
        }

        Ok(SandboxReceipt {
            receipt_id,
            action_id: normalized.action_id,
            decision,
            executed,
            observation,
            observation_hash,
            audit_hash,
            receipt_hash,
            created_at_ms: now_ms(),
        })
    }

    fn run_control_loop(
        &self,
        normalized: SandboxAction,
    ) -> (SandboxDecision, SandboxAction, Vec<serde_json::Value>, u8) {
        let max_rounds = self.config.policy.max_control_rounds.max(1);
        let mut current = normalized;
        let mut rounds = Vec::new();
        let mut rewrites_applied = 0_u8;
        let mut last_decision = self.evaluate_normalized(current.clone());

        for round in 0..max_rounds {
            let decision = self.evaluate_normalized(current.clone());
            rounds.push(serde_json::json!({
                "round": round + 1,
                "action_id": &current.action_id,
                "kind": current.kind,
                "outcome": decision.outcome,
                "policy_id": &decision.policy_id,
                "reason": &decision.reason,
            }));

            if decision.outcome == SandboxDecisionOutcome::Rewrite {
                if let Some(next) = decision.effective_action.clone() {
                    rewrites_applied = rewrites_applied.saturating_add(1);
                    current = next;
                    current.control_depth = current.control_depth.saturating_add(1);
                    last_decision = decision;
                    continue;
                }
            }

            return (decision, current, rounds, rewrites_applied);
        }

        last_decision.outcome = SandboxDecisionOutcome::Deny;
        last_decision.reason =
            "sandbox control loop exceeded bounded rewrite/verification rounds".into();
        last_decision.monitor.m0 = false;
        (last_decision, current, rounds, rewrites_applied)
    }

    pub fn audit(&self, limit: usize) -> Result<Vec<SandboxAuditEntry>, AppError> {
        let limit = limit.clamp(1, 1_000);
        let store = self.store.lock();
        let mut statement = store
            .prepare(
                "SELECT payload, previous_hash, entry_hash FROM sandbox_audit
                 ORDER BY rowid DESC LIMIT ?1",
            )
            .map_err(sql_error)?;
        let mut entries = statement
            .query_map([limit], |row| {
                let payload: String = row.get(0)?;
                let previous_hash: String = row.get(1)?;
                let entry_hash: String = row.get(2)?;
                Ok((payload, previous_hash, entry_hash))
            })
            .map_err(sql_error)?
            .map(|row| {
                let (payload, previous_hash, entry_hash) = row.map_err(sql_error)?;
                let mut entry: SandboxAuditEntry =
                    serde_json::from_str(&payload).map_err(decode_error)?;
                entry.previous_hash = previous_hash;
                entry.entry_hash = entry_hash;
                Ok(entry)
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        entries.reverse();
        Ok(entries)
    }

    pub fn verify_audit(&self) -> Result<SandboxAuditVerification, AppError> {
        let entries = self.audit(usize::MAX)?;
        let mut previous = GENESIS_HASH.to_string();
        for (index, entry) in entries.iter().enumerate() {
            if entry.previous_hash != previous {
                return Ok(SandboxAuditVerification {
                    valid: false,
                    entry_count: entries.len(),
                    latest_hash: entries.last().map(|entry| entry.entry_hash.clone()),
                    tamper_index: Some(index),
                    reason: Some("previous hash mismatch".into()),
                });
            }
            let mut recomputed = entry.clone();
            recomputed.previous_hash = previous.clone();
            recomputed.entry_hash.clear();
            let payload = serde_json::to_string(&recomputed).map_err(serialize_error)?;
            let expected = sha3_hex(format!("{previous}:{payload}").as_bytes());
            if entry.entry_hash != expected {
                return Ok(SandboxAuditVerification {
                    valid: false,
                    entry_count: entries.len(),
                    latest_hash: entries.last().map(|entry| entry.entry_hash.clone()),
                    tamper_index: Some(index),
                    reason: Some("entry hash mismatch".into()),
                });
            }
            previous = entry.entry_hash.clone();
        }
        Ok(SandboxAuditVerification {
            valid: true,
            entry_count: entries.len(),
            latest_hash: entries.last().map(|entry| entry.entry_hash.clone()),
            tamper_index: None,
            reason: None,
        })
    }

    pub fn action_for_tool_execution(
        &self,
        tenant_scope: Option<TenantScope>,
        session_id: Option<String>,
        tool: impl Into<String>,
        payload: serde_json::Value,
        approval_token: Option<String>,
    ) -> SandboxAction {
        let tool = tool.into();
        SandboxAction {
            tenant_scope,
            session_id,
            kind: SandboxActionKind::CodeExecution,
            tool: Some(tool.clone()),
            payload,
            capabilities: vec![SandboxCapability::Tool(tool)],
            approval_token,
            ..SandboxAction::default()
        }
    }

    pub fn action_for_nexus_delivery(
        &self,
        organization_id: impl Into<String>,
        action_type: impl Into<String>,
        payload: serde_json::Value,
        approval_token: Option<String>,
    ) -> SandboxAction {
        let organization_id = organization_id.into();
        let action_type = action_type.into();
        SandboxAction {
            tenant_scope: Some(TenantScope::Organization(organization_id.clone())),
            kind: SandboxActionKind::NexusDelivery,
            provider: Some(action_type.clone()),
            payload,
            capabilities: vec![
                SandboxCapability::Tenant(organization_id),
                SandboxCapability::NexusDelivery(action_type),
            ],
            approval_token,
            ..SandboxAction::default()
        }
    }

    pub fn action_for_activity(
        &self,
        activity_kind: impl Into<String>,
        objective: impl Into<String>,
        tools: Vec<String>,
        payload: serde_json::Value,
        owner_authorized: bool,
    ) -> SandboxAction {
        let activity_kind = activity_kind.into();
        let objective = objective.into();
        SandboxAction {
            kind: SandboxActionKind::CodeExecution,
            raw_request: serde_json::json!({
                "activity_kind": activity_kind,
                "objective": objective,
                "tools": tools,
                "owner_authorized": owner_authorized,
            }),
            tool: Some(activity_kind),
            payload,
            capabilities: tools.into_iter().map(SandboxCapability::Tool).collect(),
            approval_token: owner_authorized.then(|| "owner_authorized_session".into()),
            ..SandboxAction::default()
        }
    }

    pub fn action_for_network_fetch(
        &self,
        url: impl Into<String>,
        taints: Vec<SandboxTaint>,
    ) -> SandboxAction {
        let url = url.into();
        let host = Url::parse(&url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_string));
        SandboxAction {
            kind: SandboxActionKind::NetworkFetch,
            url: Some(url),
            host: host.clone(),
            capabilities: host.into_iter().map(SandboxCapability::NetHost).collect(),
            taints,
            ..SandboxAction::default()
        }
    }

    fn evaluate_normalized(&self, action: SandboxAction) -> SandboxDecision {
        let mut monitor = SandboxMonitorComponents::all_true();
        let mut reasons = Vec::new();
        let mut effective = action.clone();
        let mut lowered_limits = BTreeMap::new();

        if self.config.policy.container_required && !action.kind.requires_container() {
            monitor.m0 = false;
            reasons.push("action kind is outside the container baseline".to_string());
        }

        if action.kind == SandboxActionKind::Unknown {
            monitor.type_schema = false;
            reasons.push("unknown action kind is denied by default".into());
        }

        if action.control_depth > self.config.policy.max_recursive_depth {
            monitor.m0 = false;
            reasons.push("sandbox recursive control depth exceeded".into());
        }

        if !schema_valid(&action) {
            monitor.type_schema = false;
            reasons.push("action is missing required typed fields".into());
        }

        if !capabilities_allowed(&action, &self.config.policy) {
            monitor.capability = false;
            reasons.push("requested capability is not granted by policy".into());
        }

        if taint_blocked(&action) {
            monitor.taint = false;
            reasons
                .push("secret-tainted data cannot cross network/message/provider boundary".into());
        }

        let risk = action
            .risk_score
            .unwrap_or_else(|| default_risk(action.kind));
        if !risk.is_finite() || risk > self.config.policy.max_risk_per_action {
            monitor.risk = false;
            reasons.push("risk budget exceeded for action".into());
        }
        if risk > self.config.policy.max_risk_per_session {
            monitor.risk = false;
            reasons.push("risk budget exceeded for session".into());
        }
        if let Ok(spent) = session_risk_spent(&self.store, &action) {
            if spent + risk > self.config.policy.max_risk_per_session {
                monitor.risk = false;
                reasons.push("cumulative session risk budget exceeded".into());
            }
        }

        if action.kind.requires_approval() && action.approval_token.is_none() {
            monitor.approval = false;
            reasons.push("approval token required for irreversible or external action".into());
        }

        if !audit_available(&self.store) {
            monitor.audit = false;
            reasons.push("audit log is unavailable".into());
        }

        if llm_advisory_denies(&action) {
            monitor.llm = false;
            reasons.push("ASC-II/LLM advisory restricted the action".into());
        }

        if effective.resource_limits.timeout_ms > self.config.policy.limits.timeout_ms {
            lowered_limits.insert("timeout_ms".into(), self.config.policy.limits.timeout_ms);
            effective.resource_limits.timeout_ms = self.config.policy.limits.timeout_ms;
        }
        if effective.resource_limits.max_output_bytes > self.config.policy.limits.max_output_bytes {
            lowered_limits.insert(
                "max_output_bytes".into(),
                self.config.policy.limits.max_output_bytes,
            );
            effective.resource_limits.max_output_bytes = self.config.policy.limits.max_output_bytes;
        }
        if effective.resource_limits.memory_mb > self.config.policy.limits.memory_mb {
            lowered_limits.insert("memory_mb".into(), self.config.policy.limits.memory_mb);
            effective.resource_limits.memory_mb = self.config.policy.limits.memory_mb;
        }

        let base_allows = monitor.m0
            && monitor.capability
            && monitor.type_schema
            && monitor.taint
            && monitor.risk
            && monitor.audit
            && monitor.llm;
        let outcome = if !base_allows {
            SandboxDecisionOutcome::Deny
        } else if !monitor.approval {
            SandboxDecisionOutcome::ApprovalRequired
        } else if !lowered_limits.is_empty() {
            SandboxDecisionOutcome::Rewrite
        } else {
            SandboxDecisionOutcome::Allow
        };

        let rewrite = (!lowered_limits.is_empty()).then(|| SandboxRewrite {
            reason: "resource limits lowered to policy maximums".into(),
            removed_capabilities: Vec::new(),
            lowered_limits,
            effective_action: Box::new(effective.clone()),
        });
        let reason = if reasons.is_empty() {
            match outcome {
                SandboxDecisionOutcome::Allow => "all monitor components allowed".into(),
                SandboxDecisionOutcome::Rewrite => {
                    "action allowed after conservative rewrite".into()
                }
                SandboxDecisionOutcome::ApprovalRequired => {
                    "approval required by sandbox policy".into()
                }
                SandboxDecisionOutcome::Deny => "blocked by sandbox policy".into(),
                SandboxDecisionOutcome::SandboxUnavailable => "sandbox unavailable".into(),
            }
        } else {
            reasons.join("; ")
        };

        SandboxDecision {
            decision_id: new_id("sandbox_decision"),
            action_id: action.action_id.clone(),
            outcome,
            reason,
            monitor,
            rewrite,
            effective_action: Some(effective),
            policy_id: self.config.policy.policy_id.clone(),
            created_at_ms: now_ms(),
        }
    }

    fn enforce(&self, action: &SandboxAction) -> Result<(bool, serde_json::Value), AppError> {
        match action.kind {
            SandboxActionKind::ShellExecution | SandboxActionKind::CodeExecution => {
                self.enforce_docker_command(action)
            }
            _ => Ok((
                false,
                serde_json::json!({
                    "status": "approved_for_adapter",
                    "executed": false,
                    "protocol": "httpa",
                    "semantic_rendering_required_for_network_intake": matches!(
                        action.kind,
                        SandboxActionKind::NetworkFetch
                            | SandboxActionKind::IntegrationCall
                            | SandboxActionKind::NexusDelivery
                    ),
                    "reason": "external provider adapters must consume this sandbox receipt before delivery"
                }),
            )),
        }
    }

    fn enforce_docker_command(
        &self,
        action: &SandboxAction,
    ) -> Result<(bool, serde_json::Value), AppError> {
        let command = action
            .command
            .clone()
            .or_else(|| {
                action
                    .payload
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "true".into());
        let first = command
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !self.config.policy.allowed_commands.is_empty()
            && !self
                .config
                .policy
                .allowed_commands
                .iter()
                .any(|allowed| allowed == &first)
        {
            return Ok((
                false,
                serde_json::json!({
                    "status": "blocked",
                    "executed": false,
                    "reason": "command is not in sandbox allowlist",
                }),
            ));
        }

        let action_workspace = self.config.workspace_root.join(&action.action_id);
        std::fs::create_dir_all(&action_workspace).map_err(|error| {
            AppError::Internal(format!(
                "failed to create sandbox action workspace: {error}"
            ))
        })?;
        let timeout_seconds = (action.resource_limits.timeout_ms / 1_000).clamp(1, 300);
        let shell_command = format!("timeout {timeout_seconds}s {command}");
        let output = Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg("--network")
            .arg("none")
            .arg("--memory")
            .arg(format!("{}m", action.resource_limits.memory_mb))
            .arg("-v")
            .arg(format!(
                "{}:/workspace:rw",
                action_workspace.to_string_lossy()
            ))
            .arg("-w")
            .arg("/workspace")
            .arg(&self.config.image)
            .arg("sh")
            .arg("-lc")
            .arg(shell_command)
            .output()
            .map_err(|error| AppError::Internal(format!("docker execution failed: {error}")))?;
        let max = action.resource_limits.max_output_bytes as usize;
        let stdout = truncate_utf8(&output.stdout, max);
        let stderr = truncate_utf8(&output.stderr, max.saturating_sub(stdout.len()));
        Ok((
            output.status.success(),
            serde_json::json!({
                "status": if output.status.success() { "completed" } else { "failed" },
                "executed": true,
                "exit_code": output.status.code(),
                "stdout": stdout,
                "stderr": stderr,
            }),
        ))
    }

    fn append_audit(
        &self,
        action: &SandboxAction,
        decision: &SandboxDecision,
        enforced_action: Option<SandboxAction>,
        observation_hash: &str,
        receipt_hash: &str,
    ) -> Result<String, AppError> {
        let store = self.store.lock();
        let previous_hash = latest_audit_hash(&store)?.unwrap_or_else(|| GENESIS_HASH.into());
        let mut entry = SandboxAuditEntry {
            entry_id: new_id("sandbox_audit"),
            action_id: action.action_id.clone(),
            raw_request_hash: sha3_hex(action.raw_request.to_string().as_bytes()),
            normalized_action: action.clone(),
            monitor_components: decision.monitor.clone(),
            decision: decision.clone(),
            enforced_action,
            observation_hash: observation_hash.into(),
            previous_hash: previous_hash.clone(),
            entry_hash: String::new(),
            receipt_hash: receipt_hash.into(),
            created_at_ms: now_ms(),
        };
        let payload = serde_json::to_string(&entry).map_err(serialize_error)?;
        let entry_hash = sha3_hex(format!("{previous_hash}:{payload}").as_bytes());
        entry.entry_hash = entry_hash.clone();
        let payload_without_hash = {
            let mut without_hash = entry.clone();
            without_hash.entry_hash.clear();
            serde_json::to_string(&without_hash).map_err(serialize_error)?
        };
        store
            .execute(
                "INSERT INTO sandbox_audit
                 (entry_id, action_id, payload, previous_hash, entry_hash, receipt_hash, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    entry.entry_id,
                    entry.action_id,
                    payload_without_hash,
                    previous_hash,
                    entry_hash,
                    receipt_hash,
                    entry.created_at_ms,
                ],
            )
            .map_err(sql_error)?;
        Ok(entry_hash)
    }

    fn record_usage(&self, action: &SandboxAction) -> Result<(), AppError> {
        let tenant = tenant_key(action);
        let session = action
            .session_id
            .clone()
            .unwrap_or_else(|| "default".into());
        let risk = action
            .risk_score
            .unwrap_or_else(|| default_risk(action.kind));
        self.store
            .lock()
            .execute(
                "INSERT INTO sandbox_usage (tenant_key, session_id, period, actions, risk_spent)
                 VALUES (?1, ?2, ?3, 1, ?4)
                 ON CONFLICT(tenant_key, session_id, period)
                 DO UPDATE SET actions=actions+1, risk_spent=risk_spent+excluded.risk_spent",
                params![tenant, session, current_period(), risk],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    #[cfg(test)]
    fn tamper_latest_entry_for_test(&self) -> Result<(), AppError> {
        self.store
            .lock()
            .execute(
                "UPDATE sandbox_audit SET payload=json_set(payload, '$.observation_hash', 'tampered')
                 WHERE rowid=(SELECT max(rowid) FROM sandbox_audit)",
                [],
            )
            .map_err(sql_error)?;
        Ok(())
    }
}

fn normalize_raw_action(raw_request: serde_json::Value) -> Result<SandboxAction, AppError> {
    let mut action: SandboxAction =
        serde_json::from_value(raw_request.clone()).unwrap_or_else(|_| SandboxAction {
            raw_request: raw_request.clone(),
            ..SandboxAction::default()
        });
    action.raw_request = if action.raw_request.is_null() {
        raw_request
    } else {
        action.raw_request
    };
    Ok(normalize_action(action))
}

fn normalize_action(mut action: SandboxAction) -> SandboxAction {
    if action.action_id.trim().is_empty() {
        action.action_id = new_id("sandbox_action");
    }
    if action.host.is_none() {
        action.host = action
            .url
            .as_deref()
            .and_then(|url| Url::parse(url).ok())
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase));
    }
    action.resource_limits.timeout_ms = action.resource_limits.timeout_ms.max(1);
    action.resource_limits.max_output_bytes = action.resource_limits.max_output_bytes.max(1);
    action.resource_limits.memory_mb = action.resource_limits.memory_mb.max(16);
    action
}

fn schema_valid(action: &SandboxAction) -> bool {
    match action.kind {
        SandboxActionKind::FileRead
        | SandboxActionKind::FileWrite
        | SandboxActionKind::FileDelete => {
            action.path.as_deref().is_some_and(|path| !path.is_empty())
        }
        SandboxActionKind::NetworkFetch => action
            .url
            .as_deref()
            .and_then(|url| Url::parse(url).ok())
            .is_some(),
        SandboxActionKind::ShellExecution | SandboxActionKind::CodeExecution => {
            action
                .command
                .as_ref()
                .is_some_and(|command| !command.trim().is_empty())
                || action.payload.get("command").is_some()
                || action
                    .tool
                    .as_ref()
                    .is_some_and(|tool| !tool.trim().is_empty())
        }
        SandboxActionKind::DbQuery => action.payload.get("query").is_some(),
        SandboxActionKind::MessageSend => action.payload.get("recipient").is_some(),
        SandboxActionKind::SpendPayment => action.payload.get("amount").is_some(),
        SandboxActionKind::IntegrationCall | SandboxActionKind::NexusDelivery => action
            .provider
            .as_ref()
            .is_some_and(|provider| !provider.trim().is_empty()),
        SandboxActionKind::Unknown => false,
    }
}

fn capabilities_allowed(action: &SandboxAction, policy: &SandboxPolicy) -> bool {
    match action.kind {
        SandboxActionKind::FileRead => {
            path_allowed(action.path.as_deref(), &policy.allowed_workspace_roots)
        }
        SandboxActionKind::FileWrite => {
            path_allowed(action.path.as_deref(), &policy.allowed_workspace_roots)
        }
        SandboxActionKind::FileDelete => {
            path_allowed(action.path.as_deref(), &policy.allowed_workspace_roots)
        }
        SandboxActionKind::NetworkFetch => action
            .host
            .as_ref()
            .is_some_and(|host| policy.allowed_hosts.iter().any(|allowed| allowed == host)),
        SandboxActionKind::ShellExecution | SandboxActionKind::CodeExecution => {
            let command = action
                .command
                .as_deref()
                .or_else(|| {
                    action
                        .payload
                        .get("command")
                        .and_then(serde_json::Value::as_str)
                })
                .unwrap_or_default();
            let first = command
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            policy.allowed_commands.is_empty()
                || policy
                    .allowed_commands
                    .iter()
                    .any(|allowed| allowed == &first)
        }
        SandboxActionKind::DbQuery => true,
        SandboxActionKind::MessageSend => true,
        SandboxActionKind::SpendPayment => true,
        SandboxActionKind::IntegrationCall => provider_allowed(action.provider.as_deref(), policy),
        SandboxActionKind::NexusDelivery => provider_allowed(action.provider.as_deref(), policy),
        SandboxActionKind::Unknown => false,
    }
}

fn path_allowed(path: Option<&str>, roots: &[String]) -> bool {
    let Some(path) = path else {
        return false;
    };
    let path = PathBuf::from(path);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return false;
    }
    roots.iter().any(|root| path.starts_with(root))
}

fn provider_allowed(provider: Option<&str>, policy: &SandboxPolicy) -> bool {
    let Some(provider) = provider else {
        return false;
    };
    policy.allowed_providers.is_empty()
        || policy
            .allowed_providers
            .iter()
            .any(|allowed| allowed == provider)
}

fn taint_blocked(action: &SandboxAction) -> bool {
    let sensitive = action.taints.iter().any(|taint| {
        matches!(
            taint,
            SandboxTaint::Secret
                | SandboxTaint::Credential
                | SandboxTaint::PersonalData
                | SandboxTaint::Financial
                | SandboxTaint::Hr
                | SandboxTaint::Legal
        )
    });
    sensitive
        && matches!(
            action.kind,
            SandboxActionKind::NetworkFetch
                | SandboxActionKind::MessageSend
                | SandboxActionKind::IntegrationCall
                | SandboxActionKind::NexusDelivery
        )
}

fn llm_advisory_denies(action: &SandboxAction) -> bool {
    action.asc2_advisory.as_ref().is_some_and(|value| {
        value
            .get("deny")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || value
                .get("restrict")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
    })
}

fn default_risk(kind: SandboxActionKind) -> f64 {
    match kind {
        SandboxActionKind::FileRead => 0.1,
        SandboxActionKind::FileWrite | SandboxActionKind::DbQuery => 0.35,
        SandboxActionKind::NetworkFetch | SandboxActionKind::ShellExecution => 0.45,
        SandboxActionKind::CodeExecution => 0.55,
        SandboxActionKind::FileDelete | SandboxActionKind::MessageSend => 0.7,
        SandboxActionKind::IntegrationCall | SandboxActionKind::NexusDelivery => 0.8,
        SandboxActionKind::SpendPayment => 0.95,
        SandboxActionKind::Unknown => 1.0,
    }
}

fn audit_available(store: &Arc<Mutex<Connection>>) -> bool {
    store
        .lock()
        .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
        .is_ok()
}

fn docker_status() -> Option<String> {
    Command::new("docker")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
}

fn initialize_store(connection: &Connection) -> Result<(), AppError> {
    connection
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS sandbox_audit (
                entry_id TEXT PRIMARY KEY,
                action_id TEXT NOT NULL,
                payload TEXT NOT NULL,
                previous_hash TEXT NOT NULL,
                entry_hash TEXT NOT NULL,
                receipt_hash TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sandbox_usage (
                tenant_key TEXT NOT NULL,
                session_id TEXT NOT NULL,
                period TEXT NOT NULL,
                actions INTEGER NOT NULL,
                risk_spent REAL NOT NULL,
                PRIMARY KEY (tenant_key, session_id, period)
            );
            ",
        )
        .map_err(sql_error)
}

fn latest_audit_hash(connection: &Connection) -> Result<Option<String>, AppError> {
    connection
        .query_row(
            "SELECT entry_hash FROM sandbox_audit ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)
}

fn count_audit_entries(connection: &Connection) -> Result<usize, AppError> {
    connection
        .query_row("SELECT COUNT(*) FROM sandbox_audit", [], |row| {
            row.get::<_, usize>(0)
        })
        .map_err(sql_error)
}

fn session_risk_spent(
    store: &Arc<Mutex<Connection>>,
    action: &SandboxAction,
) -> Result<f64, AppError> {
    let tenant = tenant_key(action);
    let session = action
        .session_id
        .clone()
        .unwrap_or_else(|| "default".into());
    store
        .lock()
        .query_row(
            "SELECT COALESCE(risk_spent, 0.0) FROM sandbox_usage
             WHERE tenant_key=?1 AND session_id=?2 AND period=?3",
            params![tenant, session, current_period()],
            |row| row.get::<_, f64>(0),
        )
        .optional()
        .map(|value| value.unwrap_or(0.0))
        .map_err(sql_error)
}

fn tenant_key(action: &SandboxAction) -> String {
    match action.tenant_scope.as_ref() {
        Some(TenantScope::Global) | None => "global".into(),
        Some(TenantScope::Organization(value)) => format!("org:{value}"),
        Some(TenantScope::Workspace(value)) => format!("workspace:{value}"),
    }
}

fn current_period() -> String {
    let days = now_ms() / 86_400_000;
    format!("day_{days}")
}

fn split_env(name: &str) -> Vec<String> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn truncate_utf8(bytes: &[u8], max: usize) -> String {
    let max = bytes.len().min(max);
    String::from_utf8_lossy(&bytes[..max]).to_string()
}

fn merge_observation(mut base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    if let (Some(base), Some(overlay)) = (base.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay {
            base.insert(key.clone(), value.clone());
        }
    }
    base
}

fn serialize_error(error: serde_json::Error) -> AppError {
    AppError::Internal(format!("sandbox serialization failed: {error}"))
}

fn decode_error(error: serde_json::Error) -> AppError {
    AppError::Internal(format!("sandbox audit decode failed: {error}"))
}

fn sql_error(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("sandbox persistence failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(label: &str) -> SandboxService {
        let dir = std::env::temp_dir().join(format!("astra-sandbox-{label}-{}", now_ms()));
        SandboxService::new(dir).expect("sandbox")
    }

    #[test]
    fn unknown_action_is_denied_by_default() {
        let service = service("unknown");
        let decision = service.evaluate(SandboxAction::default());
        assert_eq!(decision.outcome, SandboxDecisionOutcome::Deny);
        assert!(!decision.monitor.type_schema);
    }

    #[test]
    fn denied_action_executes_noop_and_records_audit() {
        let service = service("denied-noop");
        let receipt = service.execute(SandboxAction::default()).expect("receipt");
        assert!(!receipt.executed);
        assert_eq!(receipt.decision.outcome, SandboxDecisionOutcome::Deny);
        assert_eq!(service.verify_audit().expect("verify").entry_count, 1);
    }

    #[test]
    fn rewrite_lowers_resource_limits_without_granting_capability() {
        let service = service("rewrite");
        let mut action = SandboxAction {
            kind: SandboxActionKind::ShellExecution,
            command: Some("echo ok".into()),
            resource_limits: SandboxResourceLimits {
                timeout_ms: 60_000,
                max_output_bytes: 10_000_000,
                memory_mb: 8_192,
                cpu_units: 4,
            },
            ..SandboxAction::default()
        };
        action
            .capabilities
            .push(SandboxCapability::ExecCommand("echo".into()));
        let decision = service.evaluate(action);
        assert_eq!(decision.outcome, SandboxDecisionOutcome::Rewrite);
        let rewrite = decision.rewrite.expect("rewrite");
        assert!(rewrite.lowered_limits.contains_key("timeout_ms"));
        assert!(rewrite.removed_capabilities.is_empty());
    }

    #[test]
    fn secret_taint_cannot_leave_local_boundary() {
        let service = service("taint");
        let decision = service.evaluate(SandboxAction {
            kind: SandboxActionKind::MessageSend,
            payload: serde_json::json!({"recipient":"user@example.com"}),
            taints: vec![SandboxTaint::Secret],
            approval_token: Some("approved".into()),
            ..SandboxAction::default()
        });
        assert_eq!(decision.outcome, SandboxDecisionOutcome::Deny);
        assert!(!decision.monitor.taint);
    }

    #[test]
    fn approval_token_required_for_external_delivery() {
        let service = service("approval");
        let decision = service.evaluate(SandboxAction {
            kind: SandboxActionKind::NexusDelivery,
            provider: Some("draft_invoice".into()),
            ..SandboxAction::default()
        });
        assert_eq!(decision.outcome, SandboxDecisionOutcome::ApprovalRequired);
        assert!(!decision.monitor.approval);
    }

    #[test]
    fn risk_budget_exhaustion_blocks_action() {
        let service = service("risk");
        let decision = service.evaluate(SandboxAction {
            kind: SandboxActionKind::ShellExecution,
            command: Some("echo ok".into()),
            risk_score: Some(99.0),
            ..SandboxAction::default()
        });
        assert_eq!(decision.outcome, SandboxDecisionOutcome::Deny);
        assert!(!decision.monitor.risk);
    }

    #[test]
    fn docker_unavailable_fails_closed_for_execution() {
        let service = service("docker-unavailable");
        if docker_status().is_some() {
            return;
        }
        let receipt = service
            .execute(SandboxAction {
                kind: SandboxActionKind::ShellExecution,
                command: Some("echo ok".into()),
                ..SandboxAction::default()
            })
            .expect("receipt");
        assert_eq!(
            receipt.decision.outcome,
            SandboxDecisionOutcome::SandboxUnavailable
        );
        assert!(!receipt.executed);
    }

    #[test]
    fn unified_guard_marks_reference_monitor_and_attests_without_execution() {
        let service = service("unified-guard");
        let receipt = service
            .guard(
                SandboxAction {
                    kind: SandboxActionKind::ShellExecution,
                    command: Some("echo ok".into()),
                    ..SandboxAction::default()
                },
                false,
            )
            .expect("receipt");
        assert!(!receipt.executed);
        assert_eq!(
            receipt.observation["reference_monitor"],
            "astra_unified_reference_monitor"
        );
        assert_eq!(receipt.observation["requested_execution"], false);
        assert!(receipt.observation["control_loop_rounds"].is_array());
    }

    #[test]
    fn recursive_control_depth_escape_attempt_is_denied() {
        let service = service("recursive-depth");
        let decision = service.evaluate(SandboxAction {
            kind: SandboxActionKind::ShellExecution,
            command: Some("echo nested".into()),
            control_depth: 99,
            ..SandboxAction::default()
        });
        assert_eq!(decision.outcome, SandboxDecisionOutcome::Deny);
        assert!(!decision.monitor.m0);
        assert!(decision.reason.contains("recursive control depth"));
    }

    #[test]
    fn cumulative_session_risk_blocks_followup_action() {
        let service = service("cumulative-risk");
        service
            .store
            .lock()
            .execute(
                "INSERT INTO sandbox_usage (tenant_key, session_id, period, actions, risk_spent)
                 VALUES (?1, ?2, ?3, 1, ?4)",
                params!["global", "session_a", current_period(), 9.95_f64],
            )
            .expect("usage");
        let decision = service.evaluate(SandboxAction {
            kind: SandboxActionKind::ShellExecution,
            command: Some("echo ok".into()),
            session_id: Some("session_a".into()),
            risk_score: Some(0.1),
            ..SandboxAction::default()
        });
        assert_eq!(decision.outcome, SandboxDecisionOutcome::Deny);
        assert!(!decision.monitor.risk);
        assert!(decision.reason.contains("cumulative session risk"));
    }

    #[test]
    fn audit_hash_chain_detects_tampering() {
        let service = service("tamper");
        service.execute(SandboxAction::default()).expect("receipt");
        assert!(service.verify_audit().expect("verify").valid);
        service.tamper_latest_entry_for_test().expect("tamper");
        assert!(!service.verify_audit().expect("verify").valid);
    }
}

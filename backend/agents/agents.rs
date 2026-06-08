use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::prompting::{
    build_context_packet, AssistantIdentity, ConnectionPolicy, ConnectionRuntimeInfo,
    ContextArtifact, ContextMode, ContextPacket, ContextPacketRequest, ConversationTurn,
};
use super::sessions::{GatewaySessionRecord, GatewaySessionScope};
use crate::brain::ReasoningPolicy;
use crate::error::{AstraError, AstraResult};
use crate::orchestrator::{RankedCandidateInput, RankedConsensusEngine, RankedConsensusResult};

#[derive(Debug, Clone)]
pub struct ThinkingRuntimeConfig {
    pub state_dir: String,
    pub context_mode: ContextMode,
    pub history_limit: usize,
    pub tool_budget: usize,
    pub max_subagents: usize,
    pub compaction_threshold: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAgentDescriptor {
    pub id: String,
    pub name: String,
    pub workspace_root: String,
    pub capabilities: Vec<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Memory,
    Acquisition,
    Researcher,
    Coder,
    Verifier,
    Safety,
    Messenger,
    Coordinator,
}

impl AgentRole {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Memory => "memory",
            Self::Acquisition => "acquisition",
            Self::Researcher => "researcher",
            Self::Coder => "coder",
            Self::Verifier => "verifier",
            Self::Safety => "safety",
            Self::Messenger => "messenger",
            Self::Coordinator => "coordinator",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionRuntimeMode {
    Persistent,
    Ephemeral,
    Detached,
    Subagent,
}

impl SessionRuntimeMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::Ephemeral => "ephemeral",
            Self::Detached => "detached",
            Self::Subagent => "subagent",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplyPolicy {
    Auto,
    Review,
    Silent,
}

impl ReplyPolicy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Review => "review",
            Self::Silent => "silent",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SendPolicy {
    Allow,
    Review,
    Deny,
}

impl SendPolicy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Review => "review",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeToolPolicy {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub workspace_only: bool,
    pub web_access_enabled: bool,
    pub messaging_enabled: bool,
    pub spawn_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRuntimeOptions {
    pub mode: SessionRuntimeMode,
    pub context_mode: ContextMode,
    pub tool_budget: usize,
    pub history_limit: usize,
    pub max_subagents: usize,
    pub compaction_threshold: usize,
    pub keep_recent_turns: usize,
    pub reply_policy: ReplyPolicy,
    pub tool_policy: RuntimeToolPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionRuntimeOptionsPatch {
    pub mode: Option<SessionRuntimeMode>,
    pub context_mode: Option<ContextMode>,
    pub tool_budget: Option<usize>,
    pub history_limit: Option<usize>,
    pub max_subagents: Option<usize>,
    pub compaction_threshold: Option<usize>,
    pub keep_recent_turns: Option<usize>,
    pub reply_policy: Option<ReplyPolicy>,
    pub allow_tools: Option<Vec<String>>,
    pub deny_tools: Option<Vec<String>>,
    pub workspace_only: Option<bool>,
    pub web_access_enabled: Option<bool>,
    pub messaging_enabled: Option<bool>,
    pub spawn_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRuntimeOptionsRequest {
    pub session_id: String,
    pub patch: SessionRuntimeOptionsPatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDirective {
    pub tool: String,
    pub action: String,
    pub payload: serde_json::Value,
    pub allowed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub role: AgentRole,
    pub summary: String,
    pub confidence: f64,
    pub tool_directives: Vec<ToolDirective>,
    pub blocked_directives: Vec<ToolDirective>,
    pub depends_on: Vec<AgentRole>,
    pub subagent_session_key: Option<String>,
    pub notes: Vec<String>,
    pub policy_tags: Vec<String>,
    pub peer_review_score: f64,
    pub consensus_weight: f64,
    pub final_score: f64,
    pub rank: usize,
    pub execution_plan: BranchExecutionPlan,
    pub branch_result: BranchExecutionResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubagentExecution {
    pub session_key: String,
    pub role: AgentRole,
    pub objective: String,
    pub status: String,
    pub tool_focus: Vec<String>,
    pub workspace_root: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeObservabilitySnapshot {
    pub active_sessions: usize,
    pub active_turns: usize,
    pub persistent_sessions: usize,
    pub subagent_sessions: usize,
    pub total_executions: usize,
    pub total_compactions: usize,
    pub total_subagent_spawns: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExecutionState {
    pub session_key: String,
    pub parent_session_key: Option<String>,
    pub agent_id: String,
    pub scope: String,
    pub runtime_mode: String,
    pub context_mode: String,
    pub send_policy: String,
    pub reply_policy: String,
    pub status: String,
    pub workspace_root: String,
    pub tool_budget: usize,
    pub history_limit: usize,
    pub max_subagents: usize,
    pub active_roles: Vec<AgentRole>,
    pub child_session_keys: Vec<String>,
    pub turns: Vec<ConversationTurn>,
    pub last_objective: Option<String>,
    pub last_execution_at: Option<DateTime<Utc>>,
    pub total_executions: usize,
    pub compaction_count: usize,
    pub active_turn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchPlanStep {
    pub order: usize,
    pub action: String,
    pub objective: String,
    pub preferred_tool: Option<String>,
    pub expected_output: String,
    pub blocking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchExecutionPlan {
    pub role: AgentRole,
    pub objective: String,
    pub steps: Vec<BranchPlanStep>,
    pub acceptance_criteria: Vec<String>,
    pub estimated_impact: f64,
    pub ready_to_execute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchExecutionResult {
    pub role: AgentRole,
    pub execution_summary: String,
    pub produced_artifacts: Vec<String>,
    pub unresolved_risks: Vec<String>,
    pub follow_up_actions: Vec<String>,
    pub merge_priority: f64,
    pub ready_for_merge: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdaptiveExecutionPolicy {
    pub domain: String,
    pub champion_prompt: Option<String>,
    pub preferred_strategies: Vec<String>,
    pub discouraged_strategies: Vec<String>,
    pub verification_bias: f64,
    pub policy_notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningPolicySource {
    Baseline,
    CallerSupplied,
    GatewayLearned,
}

impl ReasoningPolicySource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::CallerSupplied => "caller_supplied",
            Self::GatewayLearned => "gateway_learned",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionGovernanceSnapshot {
    pub policy_source: String,
    pub adaptive_policy_active: bool,
    pub policy_domain: Option<String>,
    pub champion_prompt_active: bool,
    pub verification_bias: f64,
    pub requires_review: bool,
    pub delivery_blocked: bool,
    pub blocked_directive_count: usize,
    pub total_directive_count: usize,
    pub workspace_only: bool,
    pub web_access_enabled: bool,
    pub messaging_enabled: bool,
    pub spawn_enabled: bool,
    pub max_subagents: usize,
    pub risk_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BranchMergeReport {
    pub champion_role: String,
    pub merged_summary: String,
    pub prioritized_actions: Vec<String>,
    pub blocking_risks: Vec<String>,
    pub ready_branches: Vec<String>,
    pub requires_review: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultiAgentExecution {
    pub session_key: String,
    pub agent_id: String,
    pub assistant: AssistantIdentity,
    pub context_packet: ContextPacket,
    pub session_options: SessionRuntimeOptions,
    pub active_roles: Vec<AgentRole>,
    pub available_tools: Vec<String>,
    pub subagents: Vec<SubagentExecution>,
    pub reasoning_steps: Vec<ReasoningStep>,
    pub adaptive_policy: Option<AdaptiveExecutionPolicy>,
    pub governance: ExecutionGovernanceSnapshot,
    pub consensus: RankedConsensusResult,
    pub merged_execution: BranchMergeReport,
    pub final_response: String,
    pub should_reply: bool,
    pub send_policy: String,
    pub total_turns: usize,
    pub compaction_count: usize,
    pub observability: RuntimeObservabilitySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeSession {
    session_key: String,
    parent_session_key: Option<String>,
    agent_id: String,
    scope: GatewaySessionScope,
    identity: AssistantIdentity,
    workspace_root: String,
    capabilities: Vec<String>,
    options: SessionRuntimeOptions,
    turns: Vec<ConversationTurn>,
    active_roles: Vec<AgentRole>,
    child_session_keys: Vec<String>,
    last_objective: Option<String>,
    last_execution_at: Option<DateTime<Utc>>,
    total_executions: usize,
    compaction_count: usize,
    status: String,
    active_turn: bool,
}

pub struct OpenClawThinkingRuntime {
    config: ThinkingRuntimeConfig,
    sessions: Arc<DashMap<String, RuntimeSession>>,
    total_executions: AtomicUsize,
    total_compactions: AtomicUsize,
    total_subagent_spawns: AtomicUsize,
}

impl OpenClawThinkingRuntime {
    #[must_use]
    pub fn new(config: ThinkingRuntimeConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(DashMap::new()),
            total_executions: AtomicUsize::new(0),
            total_compactions: AtomicUsize::new(0),
            total_subagent_spawns: AtomicUsize::new(0),
        }
    }

    pub fn attach_session(&self, session: &GatewaySessionRecord, agent: &RuntimeAgentDescriptor) {
        let identity = AssistantIdentity {
            agent_id: session.agent_id.clone(),
            name: agent.name.clone(),
            avatar: agent.name.chars().next().unwrap_or('A').to_string(),
            emoji: None,
        };
        let default_options = self.default_options(&session.scope);
        let persisted = self
            .load_runtime_session(&session.session_key)
            .unwrap_or_else(|| RuntimeSession {
                session_key: session.session_key.clone(),
                parent_session_key: None,
                agent_id: session.agent_id.clone(),
                scope: session.scope.clone(),
                identity: identity.clone(),
                workspace_root: agent.workspace_root.clone(),
                capabilities: agent.capabilities.clone(),
                options: default_options.clone(),
                turns: Vec::new(),
                active_roles: Vec::new(),
                child_session_keys: Vec::new(),
                last_objective: None,
                last_execution_at: None,
                total_executions: 0,
                compaction_count: 0,
                status: "idle".into(),
                active_turn: false,
            });
        self.sessions
            .entry(session.session_key.clone())
            .and_modify(|runtime| {
                runtime.identity = identity.clone();
                runtime.workspace_root = agent.workspace_root.clone();
                runtime.capabilities = agent.capabilities.clone();
                runtime.scope = session.scope.clone();
            })
            .or_insert_with(|| persisted);
        if let Some(runtime) = self.sessions.get(&session.session_key) {
            self.persist_runtime_session(runtime.value());
        }
    }

    pub fn detach_session(&self, session_key: &str) {
        let _ = self.sessions.remove(session_key);
    }

    pub fn update_session_options(
        &self,
        session_key: &str,
        patch: SessionRuntimeOptionsPatch,
    ) -> AstraResult<SessionRuntimeOptions> {
        let mut runtime = self
            .sessions
            .get_mut(session_key)
            .ok_or_else(|| AstraError::SessionNotFound(session_key.to_string()))?;
        apply_options_patch(&mut runtime.options, patch);
        let options = runtime.options.clone();
        let snapshot = runtime.clone();
        drop(runtime);
        self.persist_runtime_session(&snapshot);
        Ok(options)
    }

    pub fn execute(
        &self,
        session: &GatewaySessionRecord,
        agent: &RuntimeAgentDescriptor,
        action: &str,
        domain: &str,
        parameters: &serde_json::Value,
        reasoning_policy: Option<&ReasoningPolicy>,
        policy_source: ReasoningPolicySource,
    ) -> MultiAgentExecution {
        self.attach_session(session, agent);
        let mut runtime = self
            .sessions
            .get_mut(&session.session_key)
            .expect("session should exist");

        runtime.status = "running".into();
        runtime.active_turn = true;

        let objective = derive_objective(action, domain, parameters);
        runtime.turns.push(ConversationTurn {
            role: "user".into(),
            sender: "User".into(),
            body: objective.clone(),
            timestamp: Utc::now(),
        });

        if maybe_compact_history(&mut runtime, self.config.compaction_threshold) {
            self.total_compactions.fetch_add(1, Ordering::Relaxed);
        }

        let send_policy = resolve_send_policy(action, domain, parameters, &runtime.options);
        let active_roles =
            derive_roles(domain, action, parameters, runtime.turns.len(), send_policy);
        let available_tools = derive_available_tools(&active_roles, parameters, &runtime.options);
        let adaptive_policy = reasoning_policy.map(AdaptiveExecutionPolicy::from_policy);
        let context_files = vec![ContextArtifact {
            path: "virtual://httpa-session-context.md".into(),
            content: format!(
                "Session key: {}\nAgent: {}\nCapabilities: {}\nHTTPA scope: {}",
                runtime.session_key,
                runtime.agent_id,
                if runtime.capabilities.is_empty() {
                    "none".into()
                } else {
                    runtime.capabilities.join(", ")
                },
                scope_label(&runtime.scope)
            ),
        }];
        let mut workspace_notes = vec![
            "OpenClaw is running as a Rust-native runtime inside astra_core_engine.".into(),
            "HTTPA requests are the source of truth for this session.".into(),
        ];
        if let Some(policy) = adaptive_policy.as_ref() {
            workspace_notes.push(format!(
                "Adaptive policy domain={}, verification_bias={:.2}, preferred_strategies={}.",
                policy.domain,
                policy.verification_bias,
                if policy.preferred_strategies.is_empty() {
                    "none".into()
                } else {
                    policy.preferred_strategies.join(", ")
                }
            ));
        }
        let context_packet = build_context_packet(ContextPacketRequest {
            identity: &runtime.identity,
            session_key: &runtime.session_key,
            objective: &objective,
            history: &runtime.turns,
            policy: &ConnectionPolicy {
                send_policy: send_policy.as_str().into(),
                reply_policy: runtime.options.reply_policy.as_str().into(),
                tool_budget: runtime.options.tool_budget,
                history_limit: runtime.options.history_limit,
                context_mode: runtime.options.context_mode,
                compaction_threshold: runtime.options.compaction_threshold,
            },
            runtime: &ConnectionRuntimeInfo {
                host: "httpa".into(),
                workspace_dir: runtime.workspace_root.clone(),
                channel: "httpa".into(),
                capabilities: runtime.capabilities.clone(),
                reasoning_profile: "openclaw-rust/httpa".into(),
            },
            available_tools: &available_tools,
            workspace_notes: &workspace_notes,
            context_files: &context_files,
            extra_context: Some(
                "Use a planner-led workflow, keep policy compliance explicit, and preserve session continuity.",
            ),
        });

        let subagents = spawn_subagents(
            &mut runtime,
            &active_roles,
            &available_tools,
            &objective,
            parameters,
        );
        if !subagents.is_empty() {
            self.total_subagent_spawns
                .fetch_add(subagents.len(), Ordering::Relaxed);
        }

        let mut reasoning_steps = active_roles
            .iter()
            .map(|role| {
                let subagent_session_key = subagents
                    .iter()
                    .find(|subagent| subagent.role.as_str() == role.as_str())
                    .map(|subagent| subagent.session_key.clone());
                let mut step = build_reasoning_step(
                    role,
                    &objective,
                    parameters,
                    &available_tools,
                    send_policy,
                    subagent_session_key,
                );
                if let Some(policy) = reasoning_policy {
                    apply_reasoning_policy(&mut step, role, domain, action, policy);
                }
                step
            })
            .collect::<Vec<_>>();

        let consensus = rank_reasoning_steps(
            &objective,
            domain,
            action,
            parameters,
            &reasoning_steps,
            &active_roles,
            reasoning_policy,
        );
        annotate_reasoning_steps(&mut reasoning_steps, &consensus);
        let merged_execution = merge_branch_results(&objective, &reasoning_steps, &consensus);

        let final_response = synthesize_final_response(
            &runtime.identity,
            &reasoning_steps,
            &subagents,
            adaptive_policy.as_ref(),
            &consensus,
            &merged_execution,
            parameters,
            send_policy,
            runtime.options.reply_policy,
        );
        let assistant_name = runtime.identity.name.clone();
        runtime.turns.push(ConversationTurn {
            role: "assistant".into(),
            sender: assistant_name,
            body: final_response.clone(),
            timestamp: Utc::now(),
        });

        if maybe_compact_history(&mut runtime, self.config.compaction_threshold) {
            self.total_compactions.fetch_add(1, Ordering::Relaxed);
        }

        runtime.total_executions += 1;
        runtime.last_objective = Some(objective);
        runtime.last_execution_at = Some(Utc::now());
        runtime.active_roles = active_roles.clone();
        runtime.status = if send_policy == SendPolicy::Deny {
            "blocked".into()
        } else if send_policy == SendPolicy::Review {
            "awaiting_review".into()
        } else {
            "completed".into()
        };
        runtime.active_turn = false;

        self.total_executions.fetch_add(1, Ordering::Relaxed);
        let governance = build_execution_governance(
            &runtime.options,
            &reasoning_steps,
            adaptive_policy.as_ref(),
            send_policy,
            policy_source,
        );

        let result = MultiAgentExecution {
            session_key: runtime.session_key.clone(),
            agent_id: runtime.agent_id.clone(),
            assistant: runtime.identity.clone(),
            context_packet,
            session_options: runtime.options.clone(),
            active_roles,
            available_tools,
            subagents,
            reasoning_steps,
            adaptive_policy,
            governance,
            consensus,
            merged_execution,
            final_response,
            should_reply: send_policy == SendPolicy::Allow
                && runtime.options.reply_policy == ReplyPolicy::Auto,
            send_policy: send_policy.as_str().into(),
            total_turns: runtime.turns.len(),
            compaction_count: runtime.compaction_count,
            observability: RuntimeObservabilitySnapshot {
                active_sessions: 0,
                active_turns: 0,
                persistent_sessions: 0,
                subagent_sessions: 0,
                total_executions: 0,
                total_compactions: 0,
                total_subagent_spawns: 0,
            },
        };
        let session_snapshot = runtime.clone();
        drop(runtime);
        self.persist_runtime_session(&session_snapshot);

        MultiAgentExecution {
            observability: self.observability_snapshot(),
            ..result
        }
    }

    #[must_use]
    pub fn observability_snapshot(&self) -> RuntimeObservabilitySnapshot {
        let mut active_turns = 0usize;
        let mut persistent_sessions = 0usize;
        let mut subagent_sessions = 0usize;

        for session in self.sessions.iter() {
            let value = session.value();
            if value.active_turn {
                active_turns += 1;
            }
            if value.options.mode == SessionRuntimeMode::Persistent {
                persistent_sessions += 1;
            }
            if value.options.mode == SessionRuntimeMode::Subagent {
                subagent_sessions += 1;
            }
        }

        RuntimeObservabilitySnapshot {
            active_sessions: self.sessions.len(),
            active_turns,
            persistent_sessions,
            subagent_sessions,
            total_executions: self.total_executions.load(Ordering::Relaxed),
            total_compactions: self.total_compactions.load(Ordering::Relaxed),
            total_subagent_spawns: self.total_subagent_spawns.load(Ordering::Relaxed),
        }
    }

    #[must_use]
    pub fn session_states(&self) -> Vec<SessionExecutionState> {
        let mut states = self
            .sessions
            .iter()
            .map(|entry| {
                let value = entry.value();
                SessionExecutionState {
                    session_key: value.session_key.clone(),
                    parent_session_key: value.parent_session_key.clone(),
                    agent_id: value.agent_id.clone(),
                    scope: scope_label(&value.scope).into(),
                    runtime_mode: value.options.mode.as_str().into(),
                    context_mode: value.options.context_mode.as_str().into(),
                    send_policy: resolve_send_policy(
                        "message",
                        "general",
                        &serde_json::json!({}),
                        &value.options,
                    )
                    .as_str()
                    .into(),
                    reply_policy: value.options.reply_policy.as_str().into(),
                    status: value.status.clone(),
                    workspace_root: value.workspace_root.clone(),
                    tool_budget: value.options.tool_budget,
                    history_limit: value.options.history_limit,
                    max_subagents: value.options.max_subagents,
                    active_roles: value.active_roles.clone(),
                    child_session_keys: value.child_session_keys.clone(),
                    turns: value.turns.clone(),
                    last_objective: value.last_objective.clone(),
                    last_execution_at: value.last_execution_at,
                    total_executions: value.total_executions,
                    compaction_count: value.compaction_count,
                    active_turn: value.active_turn,
                }
            })
            .collect::<Vec<_>>();
        states.sort_by(|left, right| left.session_key.cmp(&right.session_key));
        states
    }

    fn default_options(&self, scope: &GatewaySessionScope) -> SessionRuntimeOptions {
        SessionRuntimeOptions {
            mode: match scope {
                GatewaySessionScope::Isolated => SessionRuntimeMode::Ephemeral,
                GatewaySessionScope::Shared | GatewaySessionScope::PerSender => {
                    SessionRuntimeMode::Persistent
                }
            },
            context_mode: self.config.context_mode,
            tool_budget: self.config.tool_budget.max(1),
            history_limit: self.config.history_limit.max(4),
            max_subagents: self.config.max_subagents,
            compaction_threshold: self.config.compaction_threshold.max(6),
            keep_recent_turns: self.config.history_limit.min(8).max(4),
            reply_policy: ReplyPolicy::Auto,
            tool_policy: RuntimeToolPolicy {
                allow: Vec::new(),
                deny: Vec::new(),
                workspace_only: true,
                web_access_enabled: true,
                messaging_enabled: true,
                spawn_enabled: true,
            },
        }
    }

    fn persist_runtime_session(&self, runtime: &RuntimeSession) {
        let sessions_dir = self.sessions_dir();
        if fs::create_dir_all(&sessions_dir).is_err() {
            return;
        }
        let Ok(payload) = serde_json::to_vec_pretty(runtime) else {
            return;
        };
        let _ = fs::write(self.session_file_path(&runtime.session_key), payload);
    }

    fn load_runtime_session(&self, session_key: &str) -> Option<RuntimeSession> {
        let path = self.session_file_path(session_key);
        let payload = fs::read(path).ok()?;
        serde_json::from_slice::<RuntimeSession>(&payload).ok()
    }

    fn sessions_dir(&self) -> PathBuf {
        Path::new(&self.config.state_dir).join("sessions")
    }

    fn session_file_path(&self, session_key: &str) -> PathBuf {
        self.sessions_dir()
            .join(format!("{}.json", encode_session_key(session_key)))
    }
}

fn apply_options_patch(options: &mut SessionRuntimeOptions, patch: SessionRuntimeOptionsPatch) {
    if let Some(mode) = patch.mode {
        options.mode = mode;
    }
    if let Some(context_mode) = patch.context_mode {
        options.context_mode = context_mode;
    }
    if let Some(tool_budget) = patch.tool_budget {
        options.tool_budget = tool_budget.max(1);
    }
    if let Some(history_limit) = patch.history_limit {
        options.history_limit = history_limit.max(1);
    }
    if let Some(max_subagents) = patch.max_subagents {
        options.max_subagents = max_subagents;
    }
    if let Some(compaction_threshold) = patch.compaction_threshold {
        options.compaction_threshold = compaction_threshold.max(2);
    }
    if let Some(keep_recent_turns) = patch.keep_recent_turns {
        options.keep_recent_turns = keep_recent_turns.max(1);
    }
    if let Some(reply_policy) = patch.reply_policy {
        options.reply_policy = reply_policy;
    }
    if let Some(allow_tools) = patch.allow_tools {
        options.tool_policy.allow = normalize_tools(&allow_tools);
    }
    if let Some(deny_tools) = patch.deny_tools {
        options.tool_policy.deny = normalize_tools(&deny_tools);
    }
    if let Some(workspace_only) = patch.workspace_only {
        options.tool_policy.workspace_only = workspace_only;
    }
    if let Some(web_access_enabled) = patch.web_access_enabled {
        options.tool_policy.web_access_enabled = web_access_enabled;
    }
    if let Some(messaging_enabled) = patch.messaging_enabled {
        options.tool_policy.messaging_enabled = messaging_enabled;
    }
    if let Some(spawn_enabled) = patch.spawn_enabled {
        options.tool_policy.spawn_enabled = spawn_enabled;
    }
}

pub(crate) fn derive_objective(
    action: &str,
    domain: &str,
    parameters: &serde_json::Value,
) -> String {
    let mut parts = vec![format!("Handle `{action}` in domain `{domain}`.")];
    if let Some(input) = parameters
        .get("input")
        .or_else(|| parameters.get("signal"))
        .or_else(|| parameters.get("prompt"))
        .and_then(serde_json::Value::as_str)
    {
        parts.push(format!("User input: {input}."));
    }
    if let Some(url) = parameters.get("url").and_then(serde_json::Value::as_str) {
        parts.push(format!("Target URL: {url}."));
    }
    if let Some(objective) = parameters
        .get("objective")
        .and_then(serde_json::Value::as_str)
    {
        parts.push(format!("Goal: {objective}."));
    }
    if let Some(scope) = parameters.get("scope").and_then(serde_json::Value::as_str) {
        parts.push(format!("Requested scope: {scope}."));
    }
    parts.join(" ")
}

fn resolve_send_policy(
    action: &str,
    domain: &str,
    parameters: &serde_json::Value,
    options: &SessionRuntimeOptions,
) -> SendPolicy {
    if options.reply_policy == ReplyPolicy::Silent {
        return SendPolicy::Deny;
    }

    let action_lc = action.to_lowercase();
    let domain_lc = domain.to_lowercase();
    let sensitive = [
        "delete",
        "purchase",
        "transfer",
        "deploy",
        "approve",
        "governance",
    ];
    if sensitive
        .iter()
        .any(|term| action_lc.contains(term) || domain_lc.contains(term))
    {
        return SendPolicy::Deny;
    }
    if parameters
        .get("requires_approval")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || ["send", "reply", "post", "publish"]
            .iter()
            .any(|term| action_lc.contains(term))
    {
        return SendPolicy::Review;
    }
    if options.reply_policy == ReplyPolicy::Review {
        return SendPolicy::Review;
    }
    SendPolicy::Allow
}

fn derive_roles(
    domain: &str,
    action: &str,
    parameters: &serde_json::Value,
    turn_count: usize,
    send_policy: SendPolicy,
) -> Vec<AgentRole> {
    let mut roles = vec![AgentRole::Planner];
    let wants_parallel = parameters
        .get("parallel")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if turn_count > 2 {
        roles.push(AgentRole::Memory);
    }
    if domain == "web" || parameters.get("url").is_some() {
        roles.push(AgentRole::Acquisition);
    }
    if wants_parallel
        || domain == "research"
        || action.contains("search")
        || action.contains("analyze")
    {
        roles.push(AgentRole::Researcher);
    }
    if domain == "code"
        || action.contains("patch")
        || action.contains("rewrite")
        || action.contains("build")
    {
        roles.push(AgentRole::Coder);
    }
    if send_policy != SendPolicy::Allow {
        roles.push(AgentRole::Safety);
    }
    if domain == "messaging" || action.contains("reply") || action.contains("send") {
        roles.push(AgentRole::Messenger);
    }
    roles.push(AgentRole::Verifier);
    roles.push(AgentRole::Coordinator);
    dedupe_roles(roles)
}

fn dedupe_roles(roles: Vec<AgentRole>) -> Vec<AgentRole> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for role in roles {
        if seen.insert(role.as_str().to_string()) {
            deduped.push(role);
        }
    }
    deduped
}

fn derive_available_tools(
    roles: &[AgentRole],
    parameters: &serde_json::Value,
    options: &SessionRuntimeOptions,
) -> Vec<String> {
    let mut tools = vec![
        "session_memory".to_string(),
        "read".to_string(),
        "grep".to_string(),
        "sessions_history".to_string(),
    ];

    for role in roles {
        match role {
            AgentRole::Planner => tools.push("sessions_list".into()),
            AgentRole::Memory => tools.push("sessions_history".into()),
            AgentRole::Acquisition if options.tool_policy.web_access_enabled => {
                tools.push("web_fetch".into())
            }
            AgentRole::Researcher => {
                tools.push("web_search".into());
                tools.push("web_fetch".into());
            }
            AgentRole::Coder => {
                tools.push("edit".into());
                tools.push("apply_patch".into());
            }
            AgentRole::Verifier => tools.push("verifier".into()),
            AgentRole::Safety => tools.push("policy".into()),
            AgentRole::Messenger if options.tool_policy.messaging_enabled => {
                tools.push("message".into());
                tools.push("sessions_send".into());
            }
            AgentRole::Coordinator => tools.push("session_status".into()),
            _ => {}
        }
    }

    if options.tool_policy.spawn_enabled && options.max_subagents > 0 {
        tools.push("sessions_spawn".into());
    }
    if parameters.get("url").is_some() && options.tool_policy.web_access_enabled {
        tools.push("web_fetch".into());
    }

    filter_tool_names(tools, &options.tool_policy)
}

fn build_reasoning_step(
    role: &AgentRole,
    objective: &str,
    parameters: &serde_json::Value,
    available_tools: &[String],
    send_policy: SendPolicy,
    subagent_session_key: Option<String>,
) -> ReasoningStep {
    let (summary, confidence, depends_on, candidates, notes) = match role {
        AgentRole::Planner => (
            format!(
                "Break the objective into staged tasks and align them with session policy: {objective}"
            ),
            0.9,
            Vec::new(),
            vec![
                ToolDirective {
                    tool: "session_memory".into(),
                    action: "checkpoint".into(),
                    payload: serde_json::json!({ "objective": objective }),
                    allowed: true,
                    reason: None,
                },
                ToolDirective {
                    tool: "sessions_list".into(),
                    action: "discover".into(),
                    payload: serde_json::json!({ "kind": "subagents" }),
                    allowed: true,
                    reason: None,
                },
            ],
            vec!["Lead with planning before tool-heavy execution.".into()],
        ),
        AgentRole::Memory => (
            "Summarize recent context, compact stale turns, and preserve the active objective."
                .into(),
            0.82,
            vec![AgentRole::Planner],
            vec![ToolDirective {
                tool: "sessions_history".into(),
                action: "review".into(),
                payload: serde_json::json!({ "window": "recent" }),
                allowed: true,
                reason: None,
            }],
            vec!["Session continuity is preserved even when prompt history compacts.".into()],
        ),
        AgentRole::Acquisition => (
            "Fetch the target resource, extract structure, and hand normalized evidence to the assistant."
                .into(),
            0.84,
            vec![AgentRole::Planner],
            vec![ToolDirective {
                tool: "web_fetch".into(),
                action: "fetch".into(),
                payload: serde_json::json!({
                    "url": parameters.get("url").cloned().unwrap_or(serde_json::json!(null)),
                    "extract_text": true,
                    "follow_redirects": true,
                }),
                allowed: true,
                reason: None,
            }],
            vec!["Acquisition should return normalized evidence, not UI state.".into()],
        ),
        AgentRole::Researcher => (
            "Gather corroborating evidence, compare sources, and surface missing information."
                .into(),
            0.81,
            vec![AgentRole::Planner],
            vec![
                ToolDirective {
                    tool: "web_search".into(),
                    action: "query".into(),
                    payload: serde_json::json!({
                        "query": parameters
                            .get("objective")
                            .cloned()
                            .unwrap_or_else(|| serde_json::json!(objective)),
                    }),
                    allowed: true,
                    reason: None,
                },
                ToolDirective {
                    tool: "web_fetch".into(),
                    action: "capture".into(),
                    payload: serde_json::json!({
                        "url": parameters.get("url").cloned().unwrap_or(serde_json::json!(null)),
                    }),
                    allowed: true,
                    reason: None,
                },
            ],
            vec!["Research should cite observable evidence before conclusions.".into()],
        ),
        AgentRole::Coder => (
            "Translate the plan into concrete code or patch operations inside the workspace."
                .into(),
            0.79,
            vec![AgentRole::Planner, AgentRole::Researcher],
            vec![
                ToolDirective {
                    tool: "edit".into(),
                    action: "prepare_patch".into(),
                    payload: serde_json::json!({ "objective": objective }),
                    allowed: true,
                    reason: None,
                },
                ToolDirective {
                    tool: "apply_patch".into(),
                    action: "materialize".into(),
                    payload: serde_json::json!({ "objective": objective }),
                    allowed: true,
                    reason: None,
                },
            ],
            vec![
                "Coder role should stay within workspace and leave an auditable patch plan."
                    .into(),
            ],
        ),
        AgentRole::Verifier => (
            "Validate completeness, safety, and consistency before finalizing the response."
                .into(),
            0.92,
            vec![AgentRole::Planner],
            vec![ToolDirective {
                tool: "verifier".into(),
                action: "review".into(),
                payload: serde_json::json!({ "objective": objective }),
                allowed: true,
                reason: None,
            }],
            vec!["Verifier is the final gate before reply eligibility.".into()],
        ),
        AgentRole::Safety => (
            "Inspect for policy-sensitive actions, approval requirements, and prohibited side effects."
                .into(),
            0.95,
            vec![AgentRole::Planner],
            vec![ToolDirective {
                tool: "policy".into(),
                action: "check".into(),
                payload: serde_json::json!({
                    "send_policy": send_policy.as_str(),
                    "objective": objective,
                }),
                allowed: true,
                reason: None,
            }],
            vec!["High-risk actions must remain blocked or review-gated.".into()],
        ),
        AgentRole::Messenger => (
            "Prepare user-visible delivery only when the session policy allows a reply.".into(),
            0.76,
            vec![AgentRole::Verifier, AgentRole::Safety],
            vec![
                ToolDirective {
                    tool: "message".into(),
                    action: "draft".into(),
                    payload: serde_json::json!({ "objective": objective }),
                    allowed: true,
                    reason: None,
                },
                ToolDirective {
                    tool: "sessions_send".into(),
                    action: "handoff".into(),
                    payload: serde_json::json!({ "session": "current" }),
                    allowed: true,
                    reason: None,
                },
            ],
            vec!["Messaging should not bypass send policy.".into()],
        ),
        AgentRole::Coordinator => (
            "Merge role outputs into one response with next steps, risks, and execution readiness."
                .into(),
            0.9,
            vec![AgentRole::Verifier],
            vec![ToolDirective {
                tool: "session_status".into(),
                action: "summarize".into(),
                payload: serde_json::json!({ "objective": objective }),
                allowed: true,
                reason: None,
            }],
            vec!["Coordinator assembles the final agent-facing answer.".into()],
        ),
    };

    let (tool_directives, blocked_directives) = split_tool_directives(candidates, available_tools);
    let execution_plan = build_execution_plan(
        role,
        objective,
        &tool_directives,
        &blocked_directives,
        &depends_on,
    );
    let branch_result = build_branch_result(
        role,
        &summary,
        &execution_plan,
        &tool_directives,
        &blocked_directives,
        subagent_session_key.as_deref(),
    );

    ReasoningStep {
        role: role.clone(),
        summary,
        confidence,
        tool_directives,
        blocked_directives,
        depends_on,
        subagent_session_key,
        notes,
        policy_tags: role_reasoning_tags(role)
            .iter()
            .map(|tag| (*tag).to_string())
            .collect(),
        peer_review_score: 0.0,
        consensus_weight: 0.0,
        final_score: 0.0,
        rank: 0,
        execution_plan,
        branch_result,
    }
}

impl AdaptiveExecutionPolicy {
    fn from_policy(policy: &ReasoningPolicy) -> Self {
        Self {
            domain: policy.domain.clone(),
            champion_prompt: policy.champion_prompt.clone(),
            preferred_strategies: policy
                .preferred_strategies
                .iter()
                .map(|strategy| strategy.as_str().to_string())
                .collect(),
            discouraged_strategies: policy
                .discouraged_strategies
                .iter()
                .map(|strategy| strategy.as_str().to_string())
                .collect(),
            verification_bias: policy.verification_bias,
            policy_notes: policy.policy_notes.clone(),
        }
    }
}

fn apply_reasoning_policy(
    step: &mut ReasoningStep,
    role: &AgentRole,
    domain: &str,
    action: &str,
    policy: &ReasoningPolicy,
) {
    let alignment = role_policy_alignment(role, domain, action, policy);
    let verification_boost = if matches!(role, AgentRole::Verifier | AgentRole::Safety) {
        (policy.verification_bias - 0.5).max(0.0) * 0.15
    } else {
        0.0
    };

    step.confidence = clamp01(step.confidence + alignment * 0.12 + verification_boost);
    if let Some(champion_prompt) = policy.champion_prompt.as_ref() {
        step.notes.push(format!(
            "Adaptive champion prompt active: {}",
            truncate_text(champion_prompt, 96)
        ));
    }
    if let Some(note) = policy.policy_notes.first() {
        step.notes
            .push(format!("Adaptive policy note: {}", truncate_text(note, 96)));
    }
}

fn rank_reasoning_steps(
    objective: &str,
    domain: &str,
    action: &str,
    parameters: &serde_json::Value,
    reasoning_steps: &[ReasoningStep],
    active_roles: &[AgentRole],
    reasoning_policy: Option<&ReasoningPolicy>,
) -> RankedConsensusResult {
    let verification_bias = reasoning_policy
        .map(|policy| policy.verification_bias)
        .unwrap_or(0.65);
    let candidates = reasoning_steps
        .iter()
        .map(|step| RankedCandidateInput {
            id: step.role.as_str().to_string(),
            label: display_role_label(&step.role),
            specialization: step.role.as_str().to_string(),
            summary: step.summary.clone(),
            confidence: step.confidence,
            domain_fit: domain_affinity_for_role(&step.role, domain, action, parameters),
            tool_readiness: tool_readiness_score(step),
            verification_strength: verification_strength(&step.role, verification_bias),
            collaboration: collaboration_score(step, active_roles),
            policy_alignment: reasoning_policy
                .map(|policy| role_policy_alignment(&step.role, domain, action, policy))
                .unwrap_or(0.68),
            blocked_ratio: blocked_ratio(step),
            resource_cost: estimate_resource_cost(step),
        })
        .collect::<Vec<_>>();

    RankedConsensusEngine::default()
        .evaluate(objective, &candidates)
        .unwrap_or_else(|_| fallback_consensus(objective, reasoning_steps))
}

fn annotate_reasoning_steps(
    reasoning_steps: &mut [ReasoningStep],
    consensus: &RankedConsensusResult,
) {
    for step in reasoning_steps {
        if let Some(score) = consensus
            .ranked_agents
            .iter()
            .find(|score| score.candidate_id == step.role.as_str())
        {
            step.peer_review_score = score.peer_review_score;
            step.consensus_weight = score.consensus_weight;
            step.final_score = score.final_score;
            step.rank = score.rank;
            step.notes.extend(
                score
                    .strengths
                    .iter()
                    .map(|strength| format!("Strength: {strength}")),
            );
            step.notes.extend(
                score
                    .cautions
                    .iter()
                    .map(|caution| format!("Caution: {caution}")),
            );
            step.branch_result.merge_priority = score.final_score;
            step.branch_result.ready_for_merge =
                score.final_score >= 0.55 && step.execution_plan.ready_to_execute;
            if !score.cautions.is_empty() {
                step.branch_result
                    .unresolved_risks
                    .extend(score.cautions.iter().cloned());
            }
        }
    }
}

fn fallback_consensus(objective: &str, reasoning_steps: &[ReasoningStep]) -> RankedConsensusResult {
    let mut ranked_agents = reasoning_steps
        .iter()
        .enumerate()
        .map(|(index, step)| crate::orchestrator::RankedAgentScore {
            candidate_id: step.role.as_str().to_string(),
            label: display_role_label(&step.role),
            specialization: step.role.as_str().to_string(),
            summary: step.summary.clone(),
            base_confidence: step.confidence,
            peer_review_score: step.confidence,
            consensus_weight: step.confidence,
            final_score: step.confidence,
            rank: index + 1,
            strengths: vec!["fallback ranking".into()],
            cautions: vec!["ranked consensus engine fell back to step confidence".into()],
        })
        .collect::<Vec<_>>();
    ranked_agents.sort_by(|left, right| {
        right
            .final_score
            .partial_cmp(&left.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (index, score) in ranked_agents.iter_mut().enumerate() {
        score.rank = index + 1;
    }

    let champion =
        ranked_agents
            .first()
            .cloned()
            .unwrap_or(crate::orchestrator::RankedAgentScore {
                candidate_id: "planner".into(),
                label: "Planner".into(),
                specialization: "planner".into(),
                summary: "Fallback champion".into(),
                base_confidence: 0.5,
                peer_review_score: 0.5,
                consensus_weight: 0.5,
                final_score: 0.5,
                rank: 1,
                strengths: vec!["fallback".into()],
                cautions: vec!["no reasoning steps available".into()],
            });

    RankedConsensusResult {
        champion_id: champion.candidate_id.clone(),
        champion_label: champion.label.clone(),
        champion_summary: champion.summary.clone(),
        agreement_score: champion.final_score,
        cohort_grade: "fallback".into(),
        consensus_summary: format!(
            "Fallback consensus activated while ranking `{}`.",
            truncate_text(objective, 96)
        ),
        ranked_agents,
        peer_reviews: Vec::new(),
    }
}

fn role_reasoning_tags(role: &AgentRole) -> &'static [&'static str] {
    match role {
        AgentRole::Planner => &["decomposition", "meta_cognition"],
        AgentRole::Memory => &["meta_cognition", "causal_reasoning"],
        AgentRole::Acquisition => &["tree_of_thought", "causal_reasoning"],
        AgentRole::Researcher => &["tree_of_thought", "causal_reasoning", "hypothesis_test"],
        AgentRole::Coder => &["decomposition", "constraint_satisfaction", "self_critique"],
        AgentRole::Verifier => &["self_critique", "formal_reasoning", "adversarial_reasoning"],
        AgentRole::Safety => &["self_critique", "adversarial_reasoning"],
        AgentRole::Messenger => &["meta_cognition", "analogical_reasoning"],
        AgentRole::Coordinator => &["meta_cognition", "decomposition", "causal_reasoning"],
    }
}

fn build_execution_plan(
    role: &AgentRole,
    objective: &str,
    tool_directives: &[ToolDirective],
    blocked_directives: &[ToolDirective],
    depends_on: &[AgentRole],
) -> BranchExecutionPlan {
    let tool_name = |idx: usize| {
        tool_directives
            .get(idx)
            .map(|directive| directive.tool.clone())
    };
    let dependency_summary = if depends_on.is_empty() {
        "none".into()
    } else {
        depends_on
            .iter()
            .map(AgentRole::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };

    let steps = match role {
        AgentRole::Planner => vec![
            BranchPlanStep {
                order: 1,
                action: "decompose_objective".into(),
                objective: truncate_text(objective, 96),
                preferred_tool: tool_name(0),
                expected_output: "Ordered work breakdown with dependencies.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "assign_branch_owners".into(),
                objective: format!("Dependencies: {dependency_summary}"),
                preferred_tool: tool_name(1),
                expected_output: "Execution-ready branch assignments.".into(),
                blocking: true,
            },
        ],
        AgentRole::Memory => vec![
            BranchPlanStep {
                order: 1,
                action: "collect_recent_context".into(),
                objective: "Summarize recent turn history.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Compact session memory for downstream branches.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "preserve_live_context".into(),
                objective: "Keep the current objective and latest user constraint visible.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Updated memory checkpoint.".into(),
                blocking: false,
            },
        ],
        AgentRole::Acquisition => vec![
            BranchPlanStep {
                order: 1,
                action: "fetch_target_resource".into(),
                objective: "Fetch the target and preserve the raw response surface.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Deterministic resource capture.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "extract_semantic_structure".into(),
                objective: "Gather links, forms, and primary content blocks.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Structured acquisition findings.".into(),
                blocking: false,
            },
        ],
        AgentRole::Researcher => vec![
            BranchPlanStep {
                order: 1,
                action: "search_corroborating_sources".into(),
                objective: "Find evidence for the objective.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Source candidates ranked by relevance.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "compare_and_summarize".into(),
                objective: "Compare findings and isolate open questions.".into(),
                preferred_tool: tool_name(1).or_else(|| tool_name(0)),
                expected_output: "Evidence-backed research brief.".into(),
                blocking: false,
            },
        ],
        AgentRole::Coder => vec![
            BranchPlanStep {
                order: 1,
                action: "draft_patch_plan".into(),
                objective: "Translate requirements into code changes.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Patch plan with touched surfaces.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "materialize_change".into(),
                objective: "Apply the smallest auditable change set.".into(),
                preferred_tool: tool_name(1).or_else(|| tool_name(0)),
                expected_output: "Executable code delta.".into(),
                blocking: !blocked_directives.is_empty(),
            },
        ],
        AgentRole::Verifier => vec![
            BranchPlanStep {
                order: 1,
                action: "review_branch_outputs".into(),
                objective: "Validate completeness, safety, and consistency.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Verification verdict with defects and passes.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "gate_release".into(),
                objective: "Approve, request review, or block delivery.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Release gate decision.".into(),
                blocking: true,
            },
        ],
        AgentRole::Safety => vec![
            BranchPlanStep {
                order: 1,
                action: "check_policy_constraints".into(),
                objective: "Identify approval or prohibition triggers.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Policy risk register.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "recommend_delivery_mode".into(),
                objective: "Determine allow, review, or deny.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Policy-aligned delivery recommendation.".into(),
                blocking: true,
            },
        ],
        AgentRole::Messenger => vec![
            BranchPlanStep {
                order: 1,
                action: "draft_user_delivery".into(),
                objective: "Prepare the user-visible answer.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Draft response with risks and next steps.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "handoff_or_send".into(),
                objective: "Deliver or hand off according to policy.".into(),
                preferred_tool: tool_name(1).or_else(|| tool_name(0)),
                expected_output: "Delivery-ready message.".into(),
                blocking: !blocked_directives.is_empty(),
            },
        ],
        AgentRole::Coordinator => vec![
            BranchPlanStep {
                order: 1,
                action: "merge_branch_outputs".into(),
                objective: "Combine branch findings into one decision surface.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Merged execution report.".into(),
                blocking: true,
            },
            BranchPlanStep {
                order: 2,
                action: "publish_execution_readiness".into(),
                objective: "State what is ready, blocked, and next.".into(),
                preferred_tool: tool_name(0),
                expected_output: "Execution readiness summary.".into(),
                blocking: false,
            },
        ],
    };

    BranchExecutionPlan {
        role: role.clone(),
        objective: truncate_text(objective, 160),
        acceptance_criteria: vec![
            "Branch output is observable and auditable.".into(),
            "Blocked work is called out explicitly.".into(),
            "Downstream merge can consume this branch without guessing.".into(),
        ],
        estimated_impact: (0.55 + tool_directives.len() as f64 * 0.08).clamp(0.0, 1.0),
        ready_to_execute: blocked_directives.len() < tool_directives.len().saturating_add(1),
        steps,
    }
}

fn build_branch_result(
    role: &AgentRole,
    summary: &str,
    execution_plan: &BranchExecutionPlan,
    _tool_directives: &[ToolDirective],
    blocked_directives: &[ToolDirective],
    subagent_session_key: Option<&str>,
) -> BranchExecutionResult {
    let produced_artifacts = execution_plan
        .steps
        .iter()
        .map(|step| format!("{}: {}", step.action, step.expected_output))
        .collect::<Vec<_>>();
    let mut unresolved_risks = blocked_directives
        .iter()
        .map(|directive| {
            format!(
                "{} blocked: {}",
                directive.tool,
                directive
                    .reason
                    .clone()
                    .unwrap_or_else(|| "runtime policy blocked this directive".into())
            )
        })
        .collect::<Vec<_>>();
    if let Some(session_key) = subagent_session_key {
        unresolved_risks.push(format!("Subagent handoff pending for {session_key}."));
    }

    BranchExecutionResult {
        role: role.clone(),
        execution_summary: truncate_text(summary, 180),
        produced_artifacts,
        unresolved_risks,
        follow_up_actions: execution_plan
            .steps
            .iter()
            .filter(|step| step.blocking)
            .map(|step| format!("Complete {}", step.action))
            .collect(),
        merge_priority: 0.5,
        ready_for_merge: execution_plan.ready_to_execute && blocked_directives.is_empty(),
    }
}

fn merge_branch_results(
    objective: &str,
    reasoning_steps: &[ReasoningStep],
    consensus: &RankedConsensusResult,
) -> BranchMergeReport {
    let mut prioritized = reasoning_steps
        .iter()
        .map(|step| {
            (
                step.branch_result.merge_priority,
                format!(
                    "{}: {}",
                    display_role_label(&step.role),
                    step.branch_result.execution_summary
                ),
            )
        })
        .collect::<Vec<_>>();
    prioritized.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut risks = reasoning_steps
        .iter()
        .flat_map(|step| {
            step.branch_result
                .unresolved_risks
                .iter()
                .map(move |risk| format!("{}: {risk}", display_role_label(&step.role)))
        })
        .collect::<Vec<_>>();
    risks.truncate(6);

    let ready_branches = reasoning_steps
        .iter()
        .filter(|step| step.branch_result.ready_for_merge)
        .map(|step| display_role_label(&step.role))
        .collect::<Vec<_>>();

    BranchMergeReport {
        champion_role: consensus.champion_label.clone(),
        merged_summary: format!(
            "Merged {} ranked branches for `{}`. Champion branch: {}.",
            reasoning_steps.len(),
            truncate_text(objective, 96),
            consensus.champion_label
        ),
        prioritized_actions: prioritized
            .into_iter()
            .take(6)
            .map(|(_, action)| action)
            .collect(),
        blocking_risks: risks.clone(),
        ready_branches,
        requires_review: !risks.is_empty(),
    }
}

fn role_policy_alignment(
    role: &AgentRole,
    domain: &str,
    action: &str,
    policy: &ReasoningPolicy,
) -> f64 {
    let role_tags = role_reasoning_tags(role);
    let preferred = role_tags
        .iter()
        .filter(|tag| {
            policy
                .preferred_strategies
                .iter()
                .any(|strategy| strategy.as_str() == **tag)
        })
        .count();
    let discouraged = role_tags
        .iter()
        .filter(|tag| {
            policy
                .discouraged_strategies
                .iter()
                .any(|strategy| strategy.as_str() == **tag)
        })
        .count();

    let preferred_score = preferred as f64 / role_tags.len().max(1) as f64;
    let discouraged_penalty = discouraged as f64 / role_tags.len().max(1) as f64;
    let domain_bonus = if policy.domain == domain || policy.domain == "general" {
        0.82
    } else {
        0.64
    };
    let action_bonus = if action.contains("verify") && matches!(role, AgentRole::Verifier) {
        0.86
    } else if action.contains("build") && matches!(role, AgentRole::Coder) {
        0.84
    } else if action.contains("search") && matches!(role, AgentRole::Researcher) {
        0.83
    } else {
        0.7
    };

    clamp01(
        domain_bonus * 0.4 + preferred_score * 0.35 + action_bonus * 0.25
            - discouraged_penalty * 0.35,
    )
}

fn display_role_label(role: &AgentRole) -> String {
    let raw = role.as_str();
    let mut chars = raw.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => raw.to_string(),
    }
}

fn domain_affinity_for_role(
    role: &AgentRole,
    domain: &str,
    action: &str,
    parameters: &serde_json::Value,
) -> f64 {
    let action = action.to_lowercase();
    let domain = domain.to_lowercase();
    match role {
        AgentRole::Planner | AgentRole::Coordinator => 0.84,
        AgentRole::Memory => {
            if domain == "general" {
                0.68
            } else {
                0.76
            }
        }
        AgentRole::Acquisition => {
            if domain == "web" || parameters.get("url").is_some() {
                0.93
            } else {
                0.5
            }
        }
        AgentRole::Researcher => {
            if domain == "research" || action.contains("search") || action.contains("analyze") {
                0.9
            } else {
                0.62
            }
        }
        AgentRole::Coder => {
            if domain == "code" || action.contains("build") || action.contains("patch") {
                0.92
            } else {
                0.58
            }
        }
        AgentRole::Verifier => 0.9,
        AgentRole::Safety => {
            if action.contains("deploy")
                || action.contains("delete")
                || action.contains("approve")
                || action.contains("purchase")
            {
                0.95
            } else {
                0.78
            }
        }
        AgentRole::Messenger => {
            if action.contains("reply") || action.contains("send") || domain == "messaging" {
                0.88
            } else {
                0.46
            }
        }
    }
}

fn tool_readiness_score(step: &ReasoningStep) -> f64 {
    let total = step.tool_directives.len() + step.blocked_directives.len();
    if total == 0 {
        return 0.72;
    }
    step.tool_directives.len() as f64 / total as f64
}

fn blocked_ratio(step: &ReasoningStep) -> f64 {
    let total = step.tool_directives.len() + step.blocked_directives.len();
    if total == 0 {
        0.0
    } else {
        step.blocked_directives.len() as f64 / total as f64
    }
}

fn verification_strength(role: &AgentRole, verification_bias: f64) -> f64 {
    let base = match role {
        AgentRole::Verifier => 0.98,
        AgentRole::Safety => 0.95,
        AgentRole::Coordinator => 0.8,
        AgentRole::Planner => 0.76,
        AgentRole::Researcher => 0.72,
        AgentRole::Acquisition => 0.7,
        AgentRole::Coder => 0.74,
        AgentRole::Memory => 0.68,
        AgentRole::Messenger => 0.56,
    };
    clamp01(base * (0.8 + verification_bias * 0.2))
}

fn collaboration_score(step: &ReasoningStep, active_roles: &[AgentRole]) -> f64 {
    let dependency_ratio = if step.depends_on.is_empty() {
        0.82
    } else {
        let satisfied = step
            .depends_on
            .iter()
            .filter(|dependency| {
                active_roles
                    .iter()
                    .any(|role| role.as_str() == dependency.as_str())
            })
            .count();
        satisfied as f64 / step.depends_on.len() as f64
    };

    clamp01(
        dependency_ratio * 0.55
            + if step.subagent_session_key.is_some() {
                0.2
            } else {
                0.0
            }
            + if matches!(step.role, AgentRole::Planner | AgentRole::Coordinator) {
                0.2
            } else {
                0.1
            },
    )
}

fn estimate_resource_cost(step: &ReasoningStep) -> f64 {
    0.08 + (step.tool_directives.len() as f64 * 0.04)
        + (step.blocked_directives.len() as f64 * 0.03)
        + (step.depends_on.len() as f64 * 0.02)
}

fn split_tool_directives(
    directives: Vec<ToolDirective>,
    available_tools: &[String],
) -> (Vec<ToolDirective>, Vec<ToolDirective>) {
    let tool_set = available_tools
        .iter()
        .map(normalize_tool)
        .collect::<BTreeSet<_>>();
    let mut allowed = Vec::new();
    let mut blocked = Vec::new();

    for mut directive in directives {
        if tool_set.contains(&normalize_tool(&directive.tool)) {
            directive.allowed = true;
            directive.reason = None;
            allowed.push(directive);
        } else {
            directive.allowed = false;
            directive.reason = Some("Tool is blocked by the session runtime policy.".into());
            blocked.push(directive);
        }
    }

    (allowed, blocked)
}

fn spawn_subagents(
    runtime: &mut RuntimeSession,
    roles: &[AgentRole],
    available_tools: &[String],
    objective: &str,
    parameters: &serde_json::Value,
) -> Vec<SubagentExecution> {
    if !runtime.options.tool_policy.spawn_enabled
        || runtime.options.max_subagents == 0
        || !should_spawn_subagents(objective, parameters, roles)
    {
        return Vec::new();
    }

    let mut plans = Vec::new();
    for role in roles {
        if plans.len() >= runtime.options.max_subagents {
            break;
        }
        if !matches!(
            role,
            AgentRole::Acquisition | AgentRole::Researcher | AgentRole::Coder | AgentRole::Verifier
        ) {
            continue;
        }
        let session_key = format!(
            "{}/subagent/{}-{}",
            runtime.session_key,
            role.as_str(),
            runtime.child_session_keys.len() + plans.len() + 1
        );
        let tool_focus = available_tools
            .iter()
            .filter(|tool| role_tool_affinity(role, tool))
            .cloned()
            .collect::<Vec<_>>();
        runtime.child_session_keys.push(session_key.clone());
        plans.push(SubagentExecution {
            session_key,
            role: role.clone(),
            objective: format!("{objective} [{} branch]", role.as_str()),
            status: "planned".into(),
            tool_focus,
            workspace_root: runtime.workspace_root.clone(),
        });
    }
    plans
}

fn role_tool_affinity(role: &AgentRole, tool: &str) -> bool {
    match role {
        AgentRole::Acquisition => matches!(tool, "web_fetch"),
        AgentRole::Researcher => matches!(tool, "web_search" | "web_fetch"),
        AgentRole::Coder => matches!(tool, "edit" | "apply_patch" | "read" | "grep"),
        AgentRole::Verifier => matches!(tool, "verifier" | "session_status"),
        _ => false,
    }
}

fn should_spawn_subagents(
    objective: &str,
    parameters: &serde_json::Value,
    roles: &[AgentRole],
) -> bool {
    parameters
        .get("parallel")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        || objective.len() > 160
        || roles.len() >= 5
}

fn maybe_compact_history(runtime: &mut RuntimeSession, default_threshold: usize) -> bool {
    let threshold = runtime
        .options
        .compaction_threshold
        .max(default_threshold)
        .max(runtime.options.keep_recent_turns + 1);
    if runtime.turns.len() <= threshold {
        return false;
    }

    let keep_recent = runtime
        .options
        .keep_recent_turns
        .min(runtime.turns.len())
        .max(1);
    let compact_count = runtime.turns.len().saturating_sub(keep_recent);
    let summary = summarize_history(&runtime.turns[..compact_count]);
    let mut compacted_turns = vec![ConversationTurn {
        role: "system".into(),
        sender: "OpenClaw Compaction".into(),
        body: summary,
        timestamp: Utc::now(),
    }];
    compacted_turns.extend_from_slice(&runtime.turns[compact_count..]);
    runtime.turns = compacted_turns;
    runtime.compaction_count += 1;
    true
}

fn summarize_history(turns: &[ConversationTurn]) -> String {
    let snippets = turns
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|turn| format!("{}: {}", turn.sender, truncate_text(&turn.body, 80)))
        .collect::<Vec<_>>();
    format!(
        "Compacted {} prior turns. Carry forward the latest context: {}",
        turns.len(),
        snippets.join(" | ")
    )
}

fn filter_tool_names(tools: Vec<String>, policy: &RuntimeToolPolicy) -> Vec<String> {
    let allow = normalize_tools(&policy.allow);
    let deny = normalize_tools(&policy.deny)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut output = Vec::new();

    for tool in normalize_tools(&tools) {
        if !allow.is_empty() && !allow.contains(&tool) {
            continue;
        }
        if deny.contains(&tool) {
            continue;
        }
        if tool == "web_fetch" && !policy.web_access_enabled {
            continue;
        }
        if matches!(tool.as_str(), "message" | "sessions_send") && !policy.messaging_enabled {
            continue;
        }
        if tool == "sessions_spawn" && !policy.spawn_enabled {
            continue;
        }
        output.push(tool);
    }

    output
}

fn normalize_tools(tools: &[String]) -> Vec<String> {
    tools
        .iter()
        .map(normalize_tool)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_tool<T: AsRef<str>>(tool: T) -> String {
    tool.as_ref().trim().to_lowercase().replace(' ', "_")
}

fn build_execution_governance(
    options: &SessionRuntimeOptions,
    reasoning_steps: &[ReasoningStep],
    adaptive_policy: Option<&AdaptiveExecutionPolicy>,
    send_policy: SendPolicy,
    policy_source: ReasoningPolicySource,
) -> ExecutionGovernanceSnapshot {
    let blocked_directive_count = reasoning_steps
        .iter()
        .map(|step| step.blocked_directives.len())
        .sum::<usize>();
    let total_directive_count = reasoning_steps
        .iter()
        .map(|step| step.tool_directives.len() + step.blocked_directives.len())
        .sum::<usize>();

    let mut risk_flags = BTreeSet::new();
    if send_policy == SendPolicy::Review {
        risk_flags.insert("human_review_required".to_string());
    }
    if send_policy == SendPolicy::Deny {
        risk_flags.insert("delivery_blocked".to_string());
    }
    if blocked_directive_count > 0 {
        risk_flags.insert("blocked_tool_directives".to_string());
    }
    if !options.tool_policy.workspace_only {
        risk_flags.insert("workspace_boundary_relaxed".to_string());
    }
    if adaptive_policy.is_none() {
        risk_flags.insert("baseline_policy_only".to_string());
    }
    if adaptive_policy
        .map(|policy| policy.verification_bias >= 0.75)
        .unwrap_or(false)
    {
        risk_flags.insert("elevated_verification_bias".to_string());
    }

    ExecutionGovernanceSnapshot {
        policy_source: policy_source.as_str().into(),
        adaptive_policy_active: adaptive_policy.is_some(),
        policy_domain: adaptive_policy.map(|policy| policy.domain.clone()),
        champion_prompt_active: adaptive_policy
            .and_then(|policy| policy.champion_prompt.as_ref())
            .is_some(),
        verification_bias: adaptive_policy
            .map(|policy| policy.verification_bias)
            .unwrap_or(0.65),
        requires_review: send_policy == SendPolicy::Review,
        delivery_blocked: send_policy == SendPolicy::Deny,
        blocked_directive_count,
        total_directive_count,
        workspace_only: options.tool_policy.workspace_only,
        web_access_enabled: options.tool_policy.web_access_enabled,
        messaging_enabled: options.tool_policy.messaging_enabled,
        spawn_enabled: options.tool_policy.spawn_enabled,
        max_subagents: options.max_subagents,
        risk_flags: risk_flags.into_iter().collect(),
    }
}

fn synthesize_final_response(
    identity: &AssistantIdentity,
    steps: &[ReasoningStep],
    subagents: &[SubagentExecution],
    adaptive_policy: Option<&AdaptiveExecutionPolicy>,
    consensus: &RankedConsensusResult,
    merged_execution: &BranchMergeReport,
    parameters: &serde_json::Value,
    send_policy: SendPolicy,
    reply_policy: ReplyPolicy,
) -> String {
    let completed_roles = steps
        .iter()
        .map(|step| step.role.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let target = parameters
        .get("url")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            parameters
                .get("objective")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("the requested target");
    let subagent_summary = if subagents.is_empty() {
        "No delegated subagents were required.".into()
    } else {
        format!(
            "Delegated subagents planned: {}.",
            subagents
                .iter()
                .map(|subagent| format!("{} ({})", subagent.session_key, subagent.role.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let delivery = match (send_policy, reply_policy) {
        (SendPolicy::Allow, ReplyPolicy::Auto) => "Reply can be delivered automatically.",
        (SendPolicy::Allow, ReplyPolicy::Review)
        | (SendPolicy::Review, ReplyPolicy::Auto)
        | (SendPolicy::Review, ReplyPolicy::Review) => {
            "Reply is prepared but requires review before delivery."
        }
        (SendPolicy::Allow, ReplyPolicy::Silent)
        | (SendPolicy::Review, ReplyPolicy::Silent)
        | (SendPolicy::Deny, ReplyPolicy::Auto)
        | (SendPolicy::Deny, ReplyPolicy::Review)
        | (SendPolicy::Deny, ReplyPolicy::Silent) => {
            "Reply delivery is blocked by the current policy."
        }
    };
    let policy_summary = adaptive_policy.map_or_else(
        || "Adaptive policy unavailable; using baseline coordination.".to_string(),
        |policy| {
            format!(
                "Adaptive policy domain={} verification_bias={:.2}.",
                policy.domain, policy.verification_bias
            )
        },
    );

    format!(
        "{} completed a coordinated OpenClaw execution for {}. Active roles: {}. Champion role: {} ({:.2}, {}). Merge summary: {} Ready branches: {}. {} {} {}",
        identity.name,
        target,
        completed_roles,
        consensus.champion_label,
        consensus.agreement_score,
        consensus.cohort_grade,
        merged_execution.merged_summary,
        if merged_execution.ready_branches.is_empty() {
            "none".into()
        } else {
            merged_execution.ready_branches.join(", ")
        },
        subagent_summary,
        policy_summary,
        delivery
    )
}

fn truncate_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(limit.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn encode_session_key(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn scope_label(scope: &GatewaySessionScope) -> &'static str {
    match scope {
        GatewaySessionScope::Shared => "shared",
        GatewaySessionScope::PerSender => "per_sender",
        GatewaySessionScope::Isolated => "isolated",
    }
}

impl Default for OpenClawThinkingRuntime {
    fn default() -> Self {
        Self::new(ThinkingRuntimeConfig {
            state_dir: "astra_state/openclaw".into(),
            context_mode: ContextMode::Full,
            history_limit: 8,
            tool_budget: 8,
            max_subagents: 3,
            compaction_threshold: 14,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::ReasoningPolicy;
    use crate::intelligence::reasoning::ReasoningStrategy;
    use crate::openclaw::sessions::GatewaySessionRecord;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_agent() -> RuntimeAgentDescriptor {
        RuntimeAgentDescriptor {
            id: "default".into(),
            name: "Default Agent".into(),
            workspace_root: "astra_state/openclaw/workspace".into(),
            capabilities: vec!["web_fetch".into(), "httpa".into()],
            is_default: true,
        }
    }

    fn test_session() -> GatewaySessionRecord {
        GatewaySessionRecord {
            session_key: "agent/default/main/abcd1234".into(),
            httpa_session_id: "session-1".into(),
            agent_id: "default".into(),
            scope: GatewaySessionScope::PerSender,
            label: Some("httpa".into()),
            display_name: Some("Default Agent".into()),
            created_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }

    fn temp_state_dir(label: &str) -> String {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("astra-openclaw-{label}-{unique}"))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn thinking_runtime_executes_multi_agent_flow() {
        let runtime = OpenClawThinkingRuntime::default();
        let session = test_session();
        let agent = test_agent();

        let result = runtime.execute(
            &session,
            &agent,
            "navigate",
            "web",
            &serde_json::json!({"url":"https://example.com","objective":"analyze homepage","parallel":true}),
            None,
            ReasoningPolicySource::Baseline,
        );
        assert!(result.active_roles.len() >= 5);
        assert!(result.final_response.contains("Default Agent"));
        assert!(result
            .reasoning_steps
            .iter()
            .any(|step| matches!(step.role, AgentRole::Acquisition)));
        assert!(!result.subagents.is_empty());
        assert!(!result.consensus.ranked_agents.is_empty());
        assert!(result
            .reasoning_steps
            .iter()
            .all(|step| !step.execution_plan.steps.is_empty()));
        assert!(!result.merged_execution.prioritized_actions.is_empty());
        assert_eq!(result.governance.policy_source, "baseline");
        assert!(result
            .governance
            .risk_flags
            .contains(&"baseline_policy_only".to_string()));
    }

    #[test]
    fn runtime_options_patch_is_applied() {
        let runtime = OpenClawThinkingRuntime::default();
        let session = test_session();
        let agent = test_agent();
        runtime.attach_session(&session, &agent);

        let updated = runtime
            .update_session_options(
                &session.session_key,
                SessionRuntimeOptionsPatch {
                    context_mode: Some(ContextMode::Minimal),
                    web_access_enabled: Some(false),
                    deny_tools: Some(vec!["web_search".into()]),
                    ..SessionRuntimeOptionsPatch::default()
                },
            )
            .expect("session should update");

        assert_eq!(updated.context_mode, ContextMode::Minimal);
        assert!(!updated.tool_policy.web_access_enabled);
        assert!(updated.tool_policy.deny.contains(&"web_search".into()));
    }

    #[test]
    fn runtime_compacts_long_history() {
        let runtime = OpenClawThinkingRuntime::new(ThinkingRuntimeConfig {
            state_dir: temp_state_dir("compaction"),
            context_mode: ContextMode::Full,
            history_limit: 4,
            tool_budget: 8,
            max_subagents: 3,
            compaction_threshold: 5,
        });
        let session = test_session();
        let agent = test_agent();
        runtime.attach_session(&session, &agent);

        for idx in 0..4 {
            let _ = runtime.execute(
                &session,
                &agent,
                "analyze",
                "research",
                &serde_json::json!({"objective": format!("task-{idx}")}),
                None,
                ReasoningPolicySource::Baseline,
            );
        }

        let states = runtime.session_states();
        assert_eq!(states.len(), 1);
        assert!(states[0].compaction_count >= 1);
    }

    #[test]
    fn runtime_restores_persisted_session_state() {
        let state_dir = temp_state_dir("restore");
        let config = ThinkingRuntimeConfig {
            state_dir: state_dir.clone(),
            context_mode: ContextMode::Full,
            history_limit: 6,
            tool_budget: 8,
            max_subagents: 3,
            compaction_threshold: 8,
        };
        let session = test_session();
        let agent = test_agent();

        let runtime = OpenClawThinkingRuntime::new(config.clone());
        runtime.attach_session(&session, &agent);
        runtime
            .update_session_options(
                &session.session_key,
                SessionRuntimeOptionsPatch {
                    context_mode: Some(ContextMode::Minimal),
                    max_subagents: Some(2),
                    ..SessionRuntimeOptionsPatch::default()
                },
            )
            .expect("session options should persist");
        let _ = runtime.execute(
            &session,
            &agent,
            "analyze",
            "research",
            &serde_json::json!({"objective":"persist this session"}),
            None,
            ReasoningPolicySource::Baseline,
        );

        let restored = OpenClawThinkingRuntime::new(config);
        restored.attach_session(&session, &agent);
        let states = restored.session_states();

        assert_eq!(states.len(), 1);
        assert!(states[0].total_executions >= 1);
        assert_eq!(states[0].context_mode, "minimal");
        assert_eq!(states[0].max_subagents, 2);
        assert!(states[0].turns.len() >= 2);

        let _ = fs::remove_dir_all(state_dir);
    }

    #[test]
    fn runtime_execution_reports_governance_metadata() {
        let runtime = OpenClawThinkingRuntime::default();
        let session = test_session();
        let agent = test_agent();
        let policy = ReasoningPolicy {
            domain: "messaging".into(),
            champion_prompt: Some("Use verified, review-first delivery.".into()),
            preferred_strategies: vec![ReasoningStrategy::SelfCritique],
            discouraged_strategies: Vec::new(),
            max_reasoning_depth: Some(8),
            max_reasoning_expansions: Some(64),
            exploration_c: Some(1.1),
            verification_bias: 0.82,
            policy_notes: vec!["Review outbound actions carefully.".into()],
        };

        let result = runtime.execute(
            &session,
            &agent,
            "reply",
            "messaging",
            &serde_json::json!({"objective":"prepare customer response","requires_approval":true}),
            Some(&policy),
            ReasoningPolicySource::CallerSupplied,
        );

        assert_eq!(result.governance.policy_source, "caller_supplied");
        assert_eq!(
            result.governance.policy_domain.as_deref(),
            Some("messaging")
        );
        assert!(result.governance.champion_prompt_active);
        assert!(result.governance.requires_review);
        assert!(result
            .governance
            .risk_flags
            .contains(&"human_review_required".to_string()));
        assert!(result
            .governance
            .risk_flags
            .contains(&"elevated_verification_bias".to_string()));
    }
}

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::brain::{LightningLoop, ReasoningPolicy};
use crate::config::OpenClawConfig;
use crate::error::{AstraError, AstraResult};
use crate::httpa::session::HttpaSession;
use crate::openclaw::agents::{
    derive_objective, MultiAgentExecution, OpenClawThinkingRuntime, ReasoningPolicySource,
    RuntimeAgentDescriptor, RuntimeObservabilitySnapshot, SessionExecutionState,
    SessionRuntimeOptions, SessionRuntimeOptionsPatch, ThinkingRuntimeConfig,
};
use crate::openclaw::boot::{BootJobRequest, OpenClawBootService};
use crate::openclaw::prompting::ContextMode;
use crate::openclaw::sessions::{
    GatewaySessionRecord, GatewaySessionScope, OpenClawSessionHub, SessionLifecycleEvent,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayAgent {
    pub id: String,
    pub name: String,
    pub workspace_root: String,
    pub capabilities: Vec<String>,
    pub is_default: bool,
    pub main_session_key: String,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterGatewayAgentRequest {
    pub id: String,
    pub name: Option<String>,
    pub workspace_root: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenClawGatewaySnapshot {
    pub default_agent_id: String,
    pub state_dir: String,
    pub agents: Vec<GatewayAgent>,
    pub sessions: Vec<GatewaySessionRecord>,
    pub runtime_sessions: Vec<SessionExecutionState>,
    pub runtime_observability: RuntimeObservabilitySnapshot,
    pub recent_events: Vec<SessionLifecycleEvent>,
    pub boot: crate::openclaw::boot::BootServiceSummary,
}

pub struct OpenClawGateway {
    default_agent_id: String,
    state_dir: String,
    max_agents: usize,
    agents: Arc<DashMap<String, GatewayAgent>>,
    sessions: OpenClawSessionHub,
    runtime: OpenClawThinkingRuntime,
    boot: OpenClawBootService,
    learning_loop: LightningLoop,
    session_scope: GatewaySessionScope,
}

impl OpenClawGateway {
    #[must_use]
    pub fn new(config: &OpenClawConfig, learning_loop: LightningLoop) -> Self {
        let gateway = Self {
            default_agent_id: config.default_agent_id.clone(),
            state_dir: config.state_dir.clone(),
            max_agents: config.max_agents,
            agents: Arc::new(DashMap::new()),
            sessions: OpenClawSessionHub::new(),
            runtime: OpenClawThinkingRuntime::new(ThinkingRuntimeConfig {
                state_dir: config.state_dir.clone(),
                context_mode: ContextMode::from_label(&config.runtime_context_mode),
                history_limit: config.runtime_history_limit,
                tool_budget: config.runtime_tool_budget,
                max_subagents: config.runtime_max_subagents,
                compaction_threshold: config.runtime_compaction_threshold,
            }),
            boot: OpenClawBootService::new(config.boot_enabled),
            learning_loop,
            session_scope: match config.session_scope.as_str() {
                "shared" => GatewaySessionScope::Shared,
                "isolated" => GatewaySessionScope::Isolated,
                _ => GatewaySessionScope::PerSender,
            },
        };
        let _ = gateway.register_agent(RegisterGatewayAgentRequest {
            id: config.default_agent_id.clone(),
            name: Some("Default Agent".into()),
            workspace_root: Some(format!("{}/workspace", config.state_dir)),
            capabilities: Some(vec!["web_fetch".into(), "httpa".into()]),
        });
        gateway
    }

    pub fn register_agent(
        &self,
        request: RegisterGatewayAgentRequest,
    ) -> AstraResult<GatewayAgent> {
        if self.agents.len() >= self.max_agents && !self.agents.contains_key(&request.id) {
            return Err(AstraError::ControlPlaneRejected(
                "maximum configured OpenClaw agents reached".into(),
            ));
        }

        let now = Utc::now();
        let id = request.id.trim().to_lowercase();
        let agent = GatewayAgent {
            id: id.clone(),
            name: request.name.unwrap_or_else(|| format!("{id} agent")),
            workspace_root: request
                .workspace_root
                .unwrap_or_else(|| format!("{}/agents/{id}", self.state_dir)),
            capabilities: request.capabilities.unwrap_or_default(),
            is_default: id == self.default_agent_id,
            main_session_key: format!("agent/{id}/main"),
            last_seen: now,
        };
        self.agents.insert(id, agent.clone());
        Ok(agent)
    }

    pub fn register_httpa_session(&self, session: &HttpaSession) -> AstraResult<GatewayAgent> {
        let agent = self.register_agent(RegisterGatewayAgentRequest {
            id: session.agent_id.clone(),
            name: Some(session.agent_id.clone()),
            workspace_root: None,
            capabilities: Some(session.capabilities.clone()),
        })?;
        self.sessions.open_session(
            &session.session_id,
            &session.agent_id,
            self.session_scope.clone(),
            Some("httpa".into()),
            Some(agent.name.clone()),
        );
        if let Some(record) = self.sessions.session_for_httpa_id(&session.session_id) {
            self.runtime
                .attach_session(&record, &self.runtime_agent_descriptor(&agent));
        }
        Ok(agent)
    }

    pub fn record_intent_activity(&self, httpa_session_id: &str, action: &str) {
        self.sessions.touch_session(httpa_session_id, action);
    }

    pub fn terminate_httpa_session(&self, httpa_session_id: &str) {
        if let Some(record) = self.sessions.session_for_httpa_id(httpa_session_id) {
            self.runtime.detach_session(&record.session_key);
        }
        self.sessions.close_session(httpa_session_id, "terminated");
    }

    pub fn queue_boot_job(
        &self,
        request: BootJobRequest,
        session_key: Option<String>,
    ) -> crate::openclaw::boot::BootJob {
        let session_key = session_key.unwrap_or_else(|| format!("agent/{}/boot", request.agent_id));
        self.boot.queue_boot_job(request, session_key)
    }

    pub fn execute_intent(
        &self,
        httpa_session_id: &str,
        action: &str,
        domain: &str,
        parameters: &serde_json::Value,
        reasoning_policy: Option<&crate::brain::ReasoningPolicy>,
    ) -> AstraResult<MultiAgentExecution> {
        let session = self
            .sessions
            .session_for_httpa_id(httpa_session_id)
            .ok_or_else(|| AstraError::SessionNotFound(httpa_session_id.to_string()))?;
        let agent = self
            .agents
            .get(&session.agent_id)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| AstraError::AgentNotFound(session.agent_id.clone()))?;
        let (resolved_policy, policy_source) =
            self.resolve_reasoning_policy(action, domain, parameters, reasoning_policy);

        Ok(self.runtime.execute(
            &session,
            &self.runtime_agent_descriptor(&agent),
            action,
            domain,
            parameters,
            Some(&resolved_policy),
            policy_source,
        ))
    }

    pub fn update_runtime_options(
        &self,
        httpa_session_id: &str,
        patch: SessionRuntimeOptionsPatch,
    ) -> AstraResult<SessionRuntimeOptions> {
        let session = self
            .sessions
            .session_for_httpa_id(httpa_session_id)
            .ok_or_else(|| AstraError::SessionNotFound(httpa_session_id.to_string()))?;
        self.runtime
            .update_session_options(&session.session_key, patch)
    }

    #[must_use]
    pub fn runtime_observability(&self) -> RuntimeObservabilitySnapshot {
        self.runtime.observability_snapshot()
    }

    #[must_use]
    pub fn agents(&self) -> Vec<GatewayAgent> {
        let mut agents = self
            .agents
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<_>>();
        agents.sort_by(|left, right| left.id.cmp(&right.id));
        agents
    }

    #[must_use]
    pub fn snapshot(&self) -> OpenClawGatewaySnapshot {
        OpenClawGatewaySnapshot {
            default_agent_id: self.default_agent_id.clone(),
            state_dir: self.state_dir.clone(),
            agents: self.agents(),
            sessions: self.sessions.sessions(),
            runtime_sessions: self.runtime.session_states(),
            runtime_observability: self.runtime.observability_snapshot(),
            recent_events: self.sessions.recent_events(25),
            boot: self.boot.summary(),
        }
    }

    fn runtime_agent_descriptor(&self, agent: &GatewayAgent) -> RuntimeAgentDescriptor {
        RuntimeAgentDescriptor {
            id: agent.id.clone(),
            name: agent.name.clone(),
            workspace_root: agent.workspace_root.clone(),
            capabilities: agent.capabilities.clone(),
            is_default: agent.is_default,
        }
    }

    fn resolve_reasoning_policy(
        &self,
        action: &str,
        domain: &str,
        parameters: &serde_json::Value,
        reasoning_policy: Option<&ReasoningPolicy>,
    ) -> (ReasoningPolicy, ReasoningPolicySource) {
        if let Some(policy) = reasoning_policy {
            return (policy.clone(), ReasoningPolicySource::CallerSupplied);
        }

        let objective = derive_objective(action, domain, parameters);
        (
            self.learning_loop
                .reasoning_policy(&objective, Some(domain)),
            ReasoningPolicySource::GatewayLearned,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence::reasoning::ReasoningStrategy;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_state_dir(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("astra-openclaw-gateway-{label}-{unique}"))
    }

    #[test]
    fn gateway_registers_agents_and_sessions() {
        let state_dir = temp_state_dir("register");
        let cfg = OpenClawConfig {
            enabled: true,
            default_agent_id: "default".into(),
            max_agents: 8,
            state_dir: state_dir.to_string_lossy().into_owned(),
            boot_enabled: true,
            session_scope: "per-sender".into(),
            runtime_context_mode: "full".into(),
            runtime_history_limit: 12,
            runtime_tool_budget: 10,
            runtime_max_subagents: 4,
            runtime_compaction_threshold: 18,
        };
        let gateway = OpenClawGateway::new(&cfg, LightningLoop::new(state_dir.join("learning")));
        let session = HttpaSession {
            session_id: "session-1".into(),
            agent_id: "research".into(),
            token: "token".into(),
            clearance: crate::httpa::protocol::GovernanceClearance::Standard,
            capabilities: vec!["web_fetch".into()],
            created_at: Utc::now(),
            expires_at: Utc::now(),
            is_active: true,
            request_count: 0,
            last_activity: Utc::now(),
        };

        let agent = gateway
            .register_httpa_session(&session)
            .expect("session should register");
        assert_eq!(agent.id, "research");
        assert!(!gateway.snapshot().sessions.is_empty());
        let _ = std::fs::remove_dir_all(state_dir);
    }

    #[test]
    fn gateway_executes_with_learned_policy_when_none_is_supplied() {
        let state_dir = temp_state_dir("learned-policy");
        let cfg = OpenClawConfig {
            enabled: true,
            default_agent_id: "default".into(),
            max_agents: 8,
            state_dir: state_dir.to_string_lossy().into_owned(),
            boot_enabled: true,
            session_scope: "per-sender".into(),
            runtime_context_mode: "full".into(),
            runtime_history_limit: 12,
            runtime_tool_budget: 10,
            runtime_max_subagents: 4,
            runtime_compaction_threshold: 18,
        };
        let gateway = OpenClawGateway::new(&cfg, LightningLoop::new(state_dir.join("learning")));
        let session = HttpaSession {
            session_id: "session-learned".into(),
            agent_id: "research".into(),
            token: "token".into(),
            clearance: crate::httpa::protocol::GovernanceClearance::Standard,
            capabilities: vec!["web_fetch".into()],
            created_at: Utc::now(),
            expires_at: Utc::now(),
            is_active: true,
            request_count: 0,
            last_activity: Utc::now(),
        };
        gateway
            .register_httpa_session(&session)
            .expect("session should register");

        let execution = gateway
            .execute_intent(
                &session.session_id,
                "navigate",
                "web",
                &serde_json::json!({"url":"https://example.com","objective":"inspect landing page"}),
                None,
            )
            .expect("execution should succeed");

        assert!(execution.adaptive_policy.is_some());
        assert_eq!(execution.governance.policy_source, "gateway_learned");
        assert_eq!(execution.governance.policy_domain.as_deref(), Some("web"));
        assert!(execution.reasoning_steps.iter().any(|step| {
            step.notes
                .iter()
                .any(|note| note.contains("Adaptive policy note"))
        }));
        let _ = std::fs::remove_dir_all(state_dir);
    }

    #[test]
    fn gateway_preserves_caller_supplied_policy() {
        let state_dir = temp_state_dir("caller-policy");
        let cfg = OpenClawConfig {
            enabled: true,
            default_agent_id: "default".into(),
            max_agents: 8,
            state_dir: state_dir.to_string_lossy().into_owned(),
            boot_enabled: true,
            session_scope: "per-sender".into(),
            runtime_context_mode: "full".into(),
            runtime_history_limit: 12,
            runtime_tool_budget: 10,
            runtime_max_subagents: 4,
            runtime_compaction_threshold: 18,
        };
        let gateway = OpenClawGateway::new(&cfg, LightningLoop::new(state_dir.join("learning")));
        let session = HttpaSession {
            session_id: "session-caller".into(),
            agent_id: "research".into(),
            token: "token".into(),
            clearance: crate::httpa::protocol::GovernanceClearance::Standard,
            capabilities: vec!["web_fetch".into()],
            created_at: Utc::now(),
            expires_at: Utc::now(),
            is_active: true,
            request_count: 0,
            last_activity: Utc::now(),
        };
        let policy = ReasoningPolicy {
            domain: "custom-domain".into(),
            champion_prompt: Some("Apply enterprise review gates first.".into()),
            preferred_strategies: vec![ReasoningStrategy::SelfCritique],
            discouraged_strategies: Vec::new(),
            max_reasoning_depth: Some(6),
            max_reasoning_expansions: Some(96),
            exploration_c: Some(1.0),
            verification_bias: 0.9,
            policy_notes: vec!["Use caller policy over learned defaults.".into()],
        };
        gateway
            .register_httpa_session(&session)
            .expect("session should register");

        let execution = gateway
            .execute_intent(
                &session.session_id,
                "analyze",
                "research",
                &serde_json::json!({"objective":"compare runtime architecture"}),
                Some(&policy),
            )
            .expect("execution should succeed");

        assert_eq!(execution.governance.policy_source, "caller_supplied");
        assert_eq!(
            execution
                .adaptive_policy
                .as_ref()
                .map(|policy| policy.domain.as_str()),
            Some("custom-domain")
        );
        assert!(execution.governance.champion_prompt_active);
        let _ = std::fs::remove_dir_all(state_dir);
    }
}

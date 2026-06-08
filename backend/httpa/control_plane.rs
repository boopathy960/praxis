use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use super::protocol::{
    GovernanceClearance, HttpaExecutionMode, HttpaExecutionPolicy, HttpaExecutionPreferences,
    HttpaLedgerMode, HttpaPerformanceTier, HttpaPrivacyMode, IntentPriority,
};
use super::session::HttpaSession;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRoutePolicy {
    pub channel: String,
    pub workspace: String,
    pub activation_mode: String,
    pub send_policy: String,
    pub model: String,
    pub toolchain: Vec<String>,
    pub autonomy_level: u8,
    pub execution_mode: HttpaExecutionMode,
    pub privacy_mode: HttpaPrivacyMode,
    pub performance_tier: HttpaPerformanceTier,
    pub ledger_mode: HttpaLedgerMode,
    pub identity_surface: String,
    pub reasoning_lane: String,
    pub trust_lane: String,
    pub transport_lane: String,
    pub resilience_lane: String,
    pub prefetch_horizon_ms: u16,
    pub speculative_validation: bool,
    pub semantic_bootstrap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedAgentSession {
    pub session_id: String,
    pub agent_id: String,
    pub runtime_session_id: Option<String>,
    pub clearance: GovernanceClearance,
    pub capabilities: Vec<String>,
    pub route: AgentRoutePolicy,
    pub default_policy: HttpaExecutionPolicy,
    pub started_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub active_intents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentLedgerEntry {
    pub intent_id: String,
    pub session_id: String,
    pub action: String,
    pub domain: String,
    pub priority: IntentPriority,
    pub trace_id: String,
    pub status: String,
    pub execution_mode: HttpaExecutionMode,
    pub privacy_mode: HttpaPrivacyMode,
    pub performance_tier: HttpaPerformanceTier,
    pub ledger_mode: HttpaLedgerMode,
    pub chain_receipt_required: bool,
    pub user_visible: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlPlaneSnapshot {
    pub active_sessions: usize,
    pub active_intents: usize,
    pub visible_sessions: usize,
    pub result_only_sessions: usize,
    pub prewarmed_sessions: usize,
    pub speculative_sessions: usize,
    pub semantic_sessions: usize,
    pub shielded_intents: usize,
    pub notarized_intents: usize,
    pub sessions: Vec<ManagedAgentSession>,
    pub recent_intents: Vec<IntentLedgerEntry>,
}

pub struct HttpaControlPlane {
    sessions: Arc<DashMap<String, ManagedAgentSession>>,
    intents: Arc<DashMap<String, IntentLedgerEntry>>,
    intent_order: Arc<Mutex<VecDeque<String>>>,
    intent_retention_limit: usize,
}

impl HttpaControlPlane {
    #[must_use]
    pub fn new() -> Self {
        Self::with_intent_retention(4096)
    }

    #[must_use]
    pub fn with_intent_retention(intent_retention_limit: usize) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            intents: Arc::new(DashMap::new()),
            intent_order: Arc::new(Mutex::new(VecDeque::new())),
            intent_retention_limit: intent_retention_limit.max(1),
        }
    }

    pub fn register_session(
        &self,
        session: &HttpaSession,
        preferences: &HttpaExecutionPreferences,
    ) -> ManagedAgentSession {
        let default_policy = derive_default_policy(session, preferences);
        let route = derive_route_policy(session, &default_policy);
        let managed = ManagedAgentSession {
            session_id: session.session_id.clone(),
            agent_id: session.agent_id.clone(),
            runtime_session_id: None,
            clearance: session.clearance,
            capabilities: session.capabilities.clone(),
            route,
            default_policy,
            started_at: session.created_at,
            last_seen: session.last_activity,
            active_intents: 0,
        };
        self.sessions
            .insert(managed.session_id.clone(), managed.clone());
        managed
    }

    pub fn touch_session(&self, session_id: &str) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.last_seen = Utc::now();
        }
    }

    pub fn bind_runtime_session(&self, session_id: &str, runtime_session_id: String) {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.runtime_session_id = Some(runtime_session_id);
            session.last_seen = Utc::now();
        }
    }

    pub fn record_intent(&self, entry: IntentLedgerEntry) {
        if let Some(mut session) = self.sessions.get_mut(&entry.session_id) {
            session.last_seen = Utc::now();
            session.active_intents += 1;
        }
        let intent_id = entry.intent_id.clone();
        self.intents.insert(intent_id.clone(), entry);

        let mut order = self
            .intent_order
            .lock()
            .expect("intent order should not be poisoned");
        order.push_back(intent_id);
        while order.len() > self.intent_retention_limit {
            if let Some(evicted) = order.pop_front() {
                self.intents.remove(&evicted);
            }
        }
    }

    pub fn complete_intent(&self, intent_id: &str, status: &str) {
        if let Some(mut intent) = self.intents.get_mut(intent_id) {
            intent.status = status.to_string();
            if let Some(mut session) = self.sessions.get_mut(&intent.session_id) {
                session.last_seen = Utc::now();
                session.active_intents = session.active_intents.saturating_sub(1);
            }
        }
    }

    #[must_use]
    pub fn get_session(&self, session_id: &str) -> Option<ManagedAgentSession> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.value().clone())
    }

    pub fn remove_session(&self, session_id: &str) -> Option<ManagedAgentSession> {
        self.sessions.remove(session_id).map(|(_, session)| session)
    }

    #[must_use]
    pub fn list_sessions(&self) -> Vec<ManagedAgentSession> {
        let mut sessions = self
            .sessions
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.started_at.cmp(&left.started_at));
        sessions
    }

    #[must_use]
    pub fn list_recent_intents(&self, limit: usize) -> Vec<IntentLedgerEntry> {
        let order = self
            .intent_order
            .lock()
            .expect("intent order should not be poisoned");

        order
            .iter()
            .rev()
            .filter_map(|intent_id| {
                self.intents
                    .get(intent_id)
                    .map(|entry| entry.value().clone())
            })
            .take(limit)
            .collect()
    }

    #[must_use]
    pub fn snapshot(&self) -> ControlPlaneSnapshot {
        let sessions = self.list_sessions();
        let recent_intents = self.list_recent_intents(25);
        ControlPlaneSnapshot {
            active_sessions: sessions.len(),
            active_intents: self
                .intents
                .iter()
                .filter(|entry| entry.status != "completed" && entry.status != "failed")
                .count(),
            visible_sessions: sessions
                .iter()
                .filter(|session| session.default_policy.user_visible)
                .count(),
            result_only_sessions: sessions
                .iter()
                .filter(|session| !session.default_policy.user_visible)
                .count(),
            prewarmed_sessions: sessions
                .iter()
                .filter(|session| session.route.prefetch_horizon_ms > 0)
                .count(),
            speculative_sessions: sessions
                .iter()
                .filter(|session| session.route.speculative_validation)
                .count(),
            semantic_sessions: sessions
                .iter()
                .filter(|session| session.route.semantic_bootstrap)
                .count(),
            shielded_intents: recent_intents
                .iter()
                .filter(|intent| intent.privacy_mode != HttpaPrivacyMode::Identified)
                .count(),
            notarized_intents: recent_intents
                .iter()
                .filter(|intent| intent.chain_receipt_required)
                .count(),
            sessions,
            recent_intents,
        }
    }
}

impl Default for HttpaControlPlane {
    fn default() -> Self {
        Self::new()
    }
}

fn derive_default_policy(
    session: &HttpaSession,
    preferences: &HttpaExecutionPreferences,
) -> HttpaExecutionPolicy {
    let base_mode = if preferences.mode.is_some() {
        preferences.mode.unwrap_or_default()
    } else if session
        .capabilities
        .iter()
        .any(|cap| cap.contains("result_only") || cap.contains("crawler"))
    {
        HttpaExecutionMode::ResultOnly
    } else {
        HttpaExecutionMode::VisibleBrowse
    };

    HttpaExecutionPolicy::for_mode(base_mode).apply_preferences(preferences)
}

fn derive_route_policy(session: &HttpaSession, policy: &HttpaExecutionPolicy) -> AgentRoutePolicy {
    let capabilities = &session.capabilities;
    let transport = policy.transport_profile();
    let channel = if policy.user_visible || capabilities.iter().any(|cap| cap.contains("browser")) {
        "browser"
    } else if capabilities.iter().any(|cap| cap.contains("research")) {
        "research"
    } else if capabilities.iter().any(|cap| cap.contains("code")) {
        "code"
    } else {
        "main"
    };

    let activation_mode = if capabilities.iter().any(|cap| cap.contains("autonomous"))
        || policy.mode == HttpaExecutionMode::ResultOnly
    {
        "always_on"
    } else {
        "on_demand"
    };

    let send_policy = match policy.delivery {
        super::protocol::HttpaDeliveryMode::LiveViewport => "sync",
        super::protocol::HttpaDeliveryMode::ProgressiveStream => "stream",
        super::protocol::HttpaDeliveryMode::FinalOnly => "queue",
    };

    let model = if policy.performance_tier == HttpaPerformanceTier::Hyperscale
        || capabilities.iter().any(|cap| cap.contains("reason"))
    {
        "council.reasoning"
    } else {
        "council.general"
    };

    let autonomy_level = match policy.performance_tier {
        HttpaPerformanceTier::Efficient => 5,
        HttpaPerformanceTier::Turbo => 8,
        HttpaPerformanceTier::Hyperscale => 10,
    };

    AgentRoutePolicy {
        channel: channel.to_string(),
        workspace: format!(
            "workspaces/{}",
            session.agent_id.replace(['/', '\\', ':'], "-")
        ),
        activation_mode: activation_mode.to_string(),
        send_policy: send_policy.to_string(),
        model: model.to_string(),
        toolchain: capabilities.clone(),
        autonomy_level,
        execution_mode: policy.mode,
        privacy_mode: policy.privacy,
        performance_tier: policy.performance_tier,
        ledger_mode: policy.ledger_mode,
        identity_surface: if policy.origin_shielding {
            "shielded".into()
        } else {
            "identified".into()
        },
        reasoning_lane: match (policy.mode, policy.performance_tier) {
            (HttpaExecutionMode::VisibleBrowse, _) => "render-first".into(),
            (_, HttpaPerformanceTier::Hyperscale) => "swarm-max".into(),
            (HttpaExecutionMode::ResultOnly, _) => "result-first".into(),
        },
        trust_lane: match policy.ledger_mode {
            HttpaLedgerMode::Transport => "transport_only".into(),
            HttpaLedgerMode::BoundIntent => "bound_intent".into(),
            HttpaLedgerMode::VerifiedEvidence => "verified_evidence".into(),
            HttpaLedgerMode::SovereignConsensus => "sovereign_consensus".into(),
            HttpaLedgerMode::LiquidStateChannel => "liquid_state_channel".into(),
            HttpaLedgerMode::OntologicalTruth => "ontological_truth".into(),
        },
        transport_lane: transport.lane,
        resilience_lane: format!("{:?}", transport.resilience_mode).to_lowercase(),
        prefetch_horizon_ms: transport.prefetch_horizon_ms,
        speculative_validation: transport.speculative_validation,
        semantic_bootstrap: transport.semantic_bootstrap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn control_plane_registers_sessions_and_intents() {
        let plane = HttpaControlPlane::new();
        let now = Utc::now();
        let session = HttpaSession {
            session_id: "sess-1".into(),
            agent_id: "browser-agent".into(),
            token: "tok".into(),
            clearance: GovernanceClearance::Standard,
            capabilities: vec!["browser".into(), "autonomous".into()],
            created_at: now,
            expires_at: now + Duration::hours(1),
            is_active: true,
            request_count: 0,
            last_activity: now,
        };

        let managed = plane.register_session(&session, &HttpaExecutionPreferences::default());
        assert_eq!(managed.route.channel, "browser");
        assert_eq!(
            managed.default_policy.mode,
            HttpaExecutionMode::VisibleBrowse
        );
        assert_eq!(managed.route.transport_lane, "viewport_sync");

        plane.record_intent(IntentLedgerEntry {
            intent_id: "intent-1".into(),
            session_id: "sess-1".into(),
            action: "navigate".into(),
            domain: "web".into(),
            priority: IntentPriority::Normal,
            trace_id: "trace-1".into(),
            status: "queued".into(),
            execution_mode: HttpaExecutionMode::VisibleBrowse,
            privacy_mode: HttpaPrivacyMode::Identified,
            performance_tier: HttpaPerformanceTier::Efficient,
            ledger_mode: HttpaLedgerMode::BoundIntent,
            chain_receipt_required: true,
            user_visible: true,
            created_at: now,
        });
        plane.complete_intent("intent-1", "completed");

        let snapshot = plane.snapshot();
        assert_eq!(snapshot.active_sessions, 1);
        assert_eq!(snapshot.recent_intents.len(), 1);
        assert_eq!(snapshot.recent_intents[0].status, "completed");
    }

    #[test]
    fn control_plane_uses_result_only_defaults_for_crawler_sessions() {
        let plane = HttpaControlPlane::new();
        let now = Utc::now();
        let session = HttpaSession {
            session_id: "sess-2".into(),
            agent_id: "crawler-agent".into(),
            token: "tok".into(),
            clearance: GovernanceClearance::Standard,
            capabilities: vec!["crawler".into(), "autonomous".into()],
            created_at: now,
            expires_at: now + Duration::hours(1),
            is_active: true,
            request_count: 0,
            last_activity: now,
        };

        let managed = plane.register_session(&session, &HttpaExecutionPreferences::default());
        assert_eq!(managed.default_policy.mode, HttpaExecutionMode::ResultOnly);
        assert_eq!(managed.route.identity_surface, "shielded");
        assert_eq!(managed.route.transport_lane, "result_mesh");
    }

    #[test]
    fn control_plane_caps_intent_history() {
        let plane = HttpaControlPlane::with_intent_retention(2);
        let now = Utc::now();
        let session = HttpaSession {
            session_id: "sess-3".into(),
            agent_id: "research-agent".into(),
            token: "tok".into(),
            clearance: GovernanceClearance::Standard,
            capabilities: vec!["research".into()],
            created_at: now,
            expires_at: now + Duration::hours(1),
            is_active: true,
            request_count: 0,
            last_activity: now,
        };
        plane.register_session(&session, &HttpaExecutionPreferences::default());

        for intent_id in ["intent-1", "intent-2", "intent-3"] {
            plane.record_intent(IntentLedgerEntry {
                intent_id: intent_id.into(),
                session_id: "sess-3".into(),
                action: "research".into(),
                domain: "search".into(),
                priority: IntentPriority::Normal,
                trace_id: format!("trace-{intent_id}"),
                status: "queued".into(),
                execution_mode: HttpaExecutionMode::ResultOnly,
                privacy_mode: HttpaPrivacyMode::OriginShielded,
                performance_tier: HttpaPerformanceTier::Turbo,
                ledger_mode: HttpaLedgerMode::VerifiedEvidence,
                chain_receipt_required: true,
                user_visible: false,
                created_at: now,
            });
        }

        let recent = plane.list_recent_intents(10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].intent_id, "intent-3");
        assert_eq!(recent[1].intent_id, "intent-2");
    }
}

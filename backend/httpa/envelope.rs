// ─────────────────────────────────────────────────────────────
// HTTPA/1.0 — Envelope (wraps any payload with HTTPA metadata)
// ─────────────────────────────────────────────────────────────

use super::protocol::{
    GovernanceClearance, HttpaExecutionMode, HttpaExecutionPolicy, HttpaExecutionPreferences,
    HttpaLedgerMode, HttpaPerformanceTier, HttpaPrivacyMode, HttpaProtocolCapabilities,
    HttpaTransportProfile, IntentPriority,
};
use serde::{Deserialize, Serialize};

/// HTTPA envelope — wraps any request/response with protocol metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaEnvelope<T: Serialize> {
    pub version: String,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub trace_id: String,
    pub intent: Option<String>,
    pub priority: IntentPriority,
    pub clearance: GovernanceClearance,
    pub payload: T,
    pub timestamp: i64,
}

impl<T: Serialize> HttpaEnvelope<T> {
    pub fn new(agent_id: impl Into<String>, payload: T) -> Self {
        Self {
            version: super::protocol::HTTPA_VERSION.to_string(),
            agent_id: agent_id.into(),
            session_id: None,
            trace_id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
            intent: None,
            priority: IntentPriority::Normal,
            clearance: GovernanceClearance::Standard,
            payload,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn with_session(mut self, sid: impl Into<String>) -> Self {
        self.session_id = Some(sid.into());
        self
    }

    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = Some(intent.into());
        self
    }
}

/// Handshake request from a client agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub agent_id: String,
    pub agent_name: String,
    pub capabilities: Vec<String>,
    pub domains: Vec<String>,
    pub resource_profile: serde_json::Value,
    pub requested_clearance: String,
    #[serde(default)]
    pub session_defaults: HttpaExecutionPreferences,
}

/// Handshake response from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub accepted: bool,
    pub session_id: String,
    pub agent_id: String,
    pub token: String,
    pub capabilities: Vec<String>,
    pub clearance: GovernanceClearance,
    pub ttl_seconds: u64,
    pub protocol_version: String,
    pub session_defaults: HttpaExecutionPolicy,
    pub supported_capabilities: HttpaProtocolCapabilities,
}

/// Intent submission request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRequest {
    pub action: String,
    pub domain: String,
    pub priority: Option<String>,
    pub parameters: serde_json::Value,
    pub session_id: String,
    #[serde(default)]
    pub session_token: Option<String>,
    #[serde(default)]
    pub execution: HttpaExecutionPreferences,
}

/// Intent processing response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentResponse {
    pub intent_id: String,
    pub status: String,
    pub result: serde_json::Value,
    pub trace_id: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaExecutionSummary {
    pub mode: HttpaExecutionMode,
    pub privacy: HttpaPrivacyMode,
    pub performance_tier: HttpaPerformanceTier,
    pub ledger_mode: HttpaLedgerMode,
    pub transport: HttpaTransportProfile,
    pub policy: HttpaExecutionPolicy,
}

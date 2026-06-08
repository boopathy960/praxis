use serde::{Deserialize, Serialize};

pub const HTTPA_VERSION: &str = "1.0";
pub const HTTPA_PROTOCOL_NAME: &str = "HTTPA";
pub const HTTPA_FULL_NAME: &str = "Hyper Text Transfer Protocol Agents";

pub const HEADER_VERSION: &str = "X-HTTPA-Version";
pub const HEADER_AGENT_ID: &str = "X-HTTPA-Agent-ID";
pub const HEADER_SESSION_ID: &str = "X-HTTPA-Session-ID";
pub const HEADER_SESSION_TOKEN: &str = "X-HTTPA-Session-Token";
pub const HEADER_INTENT: &str = "X-HTTPA-Intent";
pub const HEADER_CAPABILITIES: &str = "X-HTTPA-Capabilities";
pub const HEADER_TRACE_ID: &str = "X-HTTPA-Trace-ID";
pub const HEADER_SIGNATURE: &str = "X-HTTPA-Signature";
pub const HEADER_PHASE: &str = "X-HTTPA-Phase";
pub const HEADER_FRAME_TYPE: &str = "X-HTTPA-Frame-Type";
pub const HEADER_CLEARANCE: &str = "X-HTTPA-Clearance";
pub const HEADER_PROTOCOL: &str = "X-HTTPA-Protocol";
pub const HEADER_ADMIN_TOKEN: &str = "X-ASTRA-Admin-Token";
pub const HEADER_EXECUTION_MODE: &str = "X-HTTPA-Execution-Mode";
pub const HEADER_PRIVACY_MODE: &str = "X-HTTPA-Privacy-Mode";
pub const HEADER_DELIVERY_MODE: &str = "X-HTTPA-Delivery-Mode";
pub const HEADER_PERFORMANCE_TIER: &str = "X-HTTPA-Performance-Tier";
pub const HEADER_LEDGER_MODE: &str = "X-HTTPA-Ledger-Mode";
pub const HEADER_VALUE_MODE: &str = "X-HTTPA-Value-Mode";
pub const HEADER_SETTLEMENT_RAIL: &str = "X-HTTPA-Settlement-Rail";
pub const HEADER_VALUE_PROOF: &str = "X-HTTPA-Value-Proof";
pub const HEADER_POCW_HASH: &str = "X-HTTPA-PoCW-Hash";
pub const HEADER_LIQUID_STREAM: &str = "X-HTTPA-Liquid-Stream";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaPhase {
    AgentSyn,
    CapAck,
    IntentReq,
    ExecStream,
    MemoryCheckpoint,
    Terminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaFrameType {
    Handshake,
    HandshakeAck,
    Intent,
    IntentAck,
    CognitiveStream,
    MemoryDelta,
    Heartbeat,
    Interrupt,
    Error,
    Terminate,
    RenderRequest,
    RenderResponse,
    PrefetchHint,
    VerificationResult,
    SwarmProgress,
    ComputeResult,
    CognitiveProof,
    LiquidSettlement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaControlMethod {
    Get,
    Post,
    Value,
    Intent,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaExecutionMode {
    VisibleBrowse,
    ResultOnly,
}

impl HttpaExecutionMode {
    #[must_use]
    pub const fn is_user_visible(self) -> bool {
        matches!(self, Self::VisibleBrowse)
    }
}

impl Default for HttpaExecutionMode {
    fn default() -> Self {
        Self::VisibleBrowse
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaDeliveryMode {
    LiveViewport,
    ProgressiveStream,
    FinalOnly,
}

impl Default for HttpaDeliveryMode {
    fn default() -> Self {
        Self::LiveViewport
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaPrivacyMode {
    Identified,
    OriginShielded,
    AnonymousDelegation,
}

impl Default for HttpaPrivacyMode {
    fn default() -> Self {
        Self::Identified
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaPerformanceTier {
    Efficient,
    Turbo,
    Hyperscale,
}

impl Default for HttpaPerformanceTier {
    fn default() -> Self {
        Self::Efficient
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaResilienceMode {
    Stream,
    AdaptiveParity,
    FountainParity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaCompressionMode {
    Standard,
    Adaptive,
    SemanticFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaStateSyncMode {
    RequestResponse,
    DeltaSync,
    PredictiveValidation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaLedgerMode {
    Transport,
    BoundIntent,
    VerifiedEvidence,
    SovereignConsensus,
    LiquidStateChannel,
    OntologicalTruth,
}

impl Default for HttpaLedgerMode {
    fn default() -> Self {
        Self::BoundIntent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceClearance {
    Public,
    Standard,
    Elevated,
    Privileged,
    Sovereign,
}

impl Default for GovernanceClearance {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for IntentPriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpaExecutionPreferences {
    pub mode: Option<HttpaExecutionMode>,
    pub delivery: Option<HttpaDeliveryMode>,
    pub privacy: Option<HttpaPrivacyMode>,
    pub performance_tier: Option<HttpaPerformanceTier>,
    pub ledger_mode: Option<HttpaLedgerMode>,
    pub require_chain_receipt: Option<bool>,
    pub max_parallel_agents: Option<u8>,
    pub explain_plan: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaExecutionPolicy {
    pub mode: HttpaExecutionMode,
    pub delivery: HttpaDeliveryMode,
    pub privacy: HttpaPrivacyMode,
    pub performance_tier: HttpaPerformanceTier,
    pub ledger_mode: HttpaLedgerMode,
    pub user_visible: bool,
    pub origin_shielding: bool,
    pub deterministic_replay: bool,
    pub predictive_prefetch: bool,
    pub truth_verification: bool,
    pub swarm_enabled: bool,
    pub compute_burst: bool,
    pub stealth_render: bool,
    pub result_compaction: bool,
    pub chain_receipt_required: bool,
    pub evidence_notary: bool,
    pub sovereign_consensus: bool,
    pub max_parallel_agents: u8,
    pub explain_plan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaTransportProfile {
    pub lane: String,
    pub resilience_mode: HttpaResilienceMode,
    pub compression_mode: HttpaCompressionMode,
    pub state_sync: HttpaStateSyncMode,
    pub prewarmed_path: bool,
    pub semantic_bootstrap: bool,
    pub speculative_validation: bool,
    pub header_compaction: bool,
    pub multishard_ingest: bool,
    pub remote_state_cache: bool,
    pub repair_budget_ms: u16,
    pub prefetch_horizon_ms: u16,
    pub ingest_shards: u8,
    pub delta_budget_kb: u16,
    pub narrative: Vec<String>,
}

impl HttpaExecutionPolicy {
    #[must_use]
    pub fn for_mode(mode: HttpaExecutionMode) -> Self {
        match mode {
            HttpaExecutionMode::VisibleBrowse => Self {
                mode,
                delivery: HttpaDeliveryMode::LiveViewport,
                privacy: HttpaPrivacyMode::Identified,
                performance_tier: HttpaPerformanceTier::Efficient,
                ledger_mode: HttpaLedgerMode::BoundIntent,
                user_visible: true,
                origin_shielding: false,
                deterministic_replay: true,
                predictive_prefetch: true,
                truth_verification: true,
                swarm_enabled: true,
                compute_burst: false,
                stealth_render: false,
                result_compaction: false,
                chain_receipt_required: true,
                evidence_notary: false,
                sovereign_consensus: false,
                max_parallel_agents: 4,
                explain_plan: true,
            },
            HttpaExecutionMode::ResultOnly => Self {
                mode,
                delivery: HttpaDeliveryMode::FinalOnly,
                privacy: HttpaPrivacyMode::OriginShielded,
                performance_tier: HttpaPerformanceTier::Turbo,
                ledger_mode: HttpaLedgerMode::VerifiedEvidence,
                user_visible: false,
                origin_shielding: true,
                deterministic_replay: true,
                predictive_prefetch: true,
                truth_verification: true,
                swarm_enabled: true,
                compute_burst: true,
                stealth_render: true,
                result_compaction: true,
                chain_receipt_required: true,
                evidence_notary: true,
                sovereign_consensus: false,
                max_parallel_agents: 12,
                explain_plan: false,
            },
        }
    }

    #[must_use]
    pub fn apply_preferences(mut self, preferences: &HttpaExecutionPreferences) -> Self {
        if let Some(mode) = preferences.mode {
            let mut mode_defaults = Self::for_mode(mode);
            mode_defaults.delivery = preferences.delivery.unwrap_or(mode_defaults.delivery);
            mode_defaults.privacy = preferences.privacy.unwrap_or(mode_defaults.privacy);
            mode_defaults.performance_tier = preferences
                .performance_tier
                .unwrap_or(mode_defaults.performance_tier);
            mode_defaults.ledger_mode =
                preferences.ledger_mode.unwrap_or(mode_defaults.ledger_mode);
            mode_defaults.chain_receipt_required = preferences
                .require_chain_receipt
                .unwrap_or(mode_defaults.chain_receipt_required);
            mode_defaults.max_parallel_agents = preferences
                .max_parallel_agents
                .unwrap_or(mode_defaults.max_parallel_agents);
            mode_defaults.explain_plan = preferences
                .explain_plan
                .unwrap_or(mode_defaults.explain_plan);
            return mode_defaults.recompute();
        }

        if let Some(delivery) = preferences.delivery {
            self.delivery = delivery;
        }
        if let Some(privacy) = preferences.privacy {
            self.privacy = privacy;
        }
        if let Some(performance_tier) = preferences.performance_tier {
            self.performance_tier = performance_tier;
        }
        if let Some(ledger_mode) = preferences.ledger_mode {
            self.ledger_mode = ledger_mode;
        }
        if let Some(require_chain_receipt) = preferences.require_chain_receipt {
            self.chain_receipt_required = require_chain_receipt;
        }
        if let Some(max_parallel_agents) = preferences.max_parallel_agents {
            self.max_parallel_agents = max_parallel_agents.max(1);
        }
        if let Some(explain_plan) = preferences.explain_plan {
            self.explain_plan = explain_plan;
        }

        self.recompute()
    }

    #[must_use]
    pub fn activated_features(&self) -> Vec<String> {
        let mut features = vec!["intent_control_plane".to_string()];
        let transport = self.transport_profile();

        if self.user_visible {
            features.push("live_render_surface".into());
            features.push("viewport_state_sync".into());
        } else {
            features.push("result_compaction".into());
            features.push("autonomous_acquisition".into());
        }

        if self.origin_shielding {
            features.push("origin_shield_mesh".into());
        }
        features.push("chain_receipts".into());
        if self.evidence_notary {
            features.push("evidence_notary".into());
        }
        if self.sovereign_consensus {
            features.push("sovereign_consensus".into());
        }
        if self.predictive_prefetch {
            features.push("predictive_prefetch".into());
        }
        if self.truth_verification {
            features.push("truth_verification".into());
        }
        if self.swarm_enabled {
            features.push("swarm_consensus".into());
        }
        if self.compute_burst {
            features.push("compute_burst".into());
        }
        if self.stealth_render {
            features.push("stealth_render".into());
        }
        if self.performance_tier == HttpaPerformanceTier::Hyperscale {
            features.push("dark_matter_cache".into());
            features.push("quantum_branching".into());
            features.push("hardware_symbiosis".into());
        }
        if !matches!(transport.resilience_mode, HttpaResilienceMode::Stream) {
            features.push("adaptive_parity_recovery".into());
        }
        if transport.semantic_bootstrap {
            features.push("semantic_bootstrap".into());
        }
        if transport.speculative_validation {
            features.push("speculative_validation".into());
        }
        if transport.prewarmed_path {
            features.push("prewarmed_transport".into());
        }
        if transport.multishard_ingest {
            features.push("multishard_ingest".into());
        }
        if transport.header_compaction {
            features.push("header_compaction".into());
        }
        if transport.remote_state_cache {
            features.push("delta_state_cache".into());
        }

        features
    }

    #[must_use]
    pub fn expected_artifacts(&self) -> Vec<String> {
        let mut artifacts = if self.user_visible {
            vec![
                "viewport_stream".into(),
                "dom_snapshot".into(),
                "interaction_trace".into(),
            ]
        } else {
            vec![
                "final_result".into(),
                "evidence_bundle".into(),
                "compressed_reasoning_summary".into(),
            ]
        };
        let transport = self.transport_profile();
        if transport.semantic_bootstrap {
            artifacts.push("semantic_shell".into());
        }
        if transport.remote_state_cache {
            artifacts.push("delta_state_manifest".into());
        }
        if transport.speculative_validation {
            artifacts.push("prediction_checkpoint".into());
        }
        artifacts
    }

    #[must_use]
    pub fn transport_profile(&self) -> HttpaTransportProfile {
        let resilience_mode = match (self.mode, self.performance_tier) {
            (_, HttpaPerformanceTier::Hyperscale) => HttpaResilienceMode::FountainParity,
            (HttpaExecutionMode::ResultOnly, _) | (_, HttpaPerformanceTier::Turbo) => {
                HttpaResilienceMode::AdaptiveParity
            }
            _ => HttpaResilienceMode::Stream,
        };
        let semantic_bootstrap = !self.user_visible
            && matches!(
                self.delivery,
                HttpaDeliveryMode::ProgressiveStream | HttpaDeliveryMode::FinalOnly
            )
            && matches!(
                self.performance_tier,
                HttpaPerformanceTier::Turbo | HttpaPerformanceTier::Hyperscale
            );
        let compression_mode = if semantic_bootstrap {
            HttpaCompressionMode::SemanticFirst
        } else if matches!(
            self.performance_tier,
            HttpaPerformanceTier::Turbo | HttpaPerformanceTier::Hyperscale
        ) || self.result_compaction
        {
            HttpaCompressionMode::Adaptive
        } else {
            HttpaCompressionMode::Standard
        };
        let speculative_validation = self.predictive_prefetch
            && self.deterministic_replay
            && self.performance_tier != HttpaPerformanceTier::Efficient;
        let state_sync = if speculative_validation {
            HttpaStateSyncMode::PredictiveValidation
        } else if self.deterministic_replay || self.user_visible {
            HttpaStateSyncMode::DeltaSync
        } else {
            HttpaStateSyncMode::RequestResponse
        };
        let prewarmed_path =
            self.predictive_prefetch && self.performance_tier != HttpaPerformanceTier::Efficient;
        let multishard_ingest = matches!(
            self.performance_tier,
            HttpaPerformanceTier::Turbo | HttpaPerformanceTier::Hyperscale
        );
        let ingest_shards = match self.performance_tier {
            HttpaPerformanceTier::Efficient => 1,
            HttpaPerformanceTier::Turbo => 4,
            HttpaPerformanceTier::Hyperscale => 8,
        };
        let repair_budget_ms = match resilience_mode {
            HttpaResilienceMode::Stream => 28,
            HttpaResilienceMode::AdaptiveParity => 16,
            HttpaResilienceMode::FountainParity => 8,
        };
        let prefetch_horizon_ms = match (self.mode, self.performance_tier) {
            (HttpaExecutionMode::VisibleBrowse, HttpaPerformanceTier::Efficient) => 30,
            (HttpaExecutionMode::VisibleBrowse, HttpaPerformanceTier::Turbo) => 90,
            (HttpaExecutionMode::VisibleBrowse, HttpaPerformanceTier::Hyperscale) => 180,
            (HttpaExecutionMode::ResultOnly, HttpaPerformanceTier::Efficient) => 45,
            (HttpaExecutionMode::ResultOnly, HttpaPerformanceTier::Turbo) => 140,
            (HttpaExecutionMode::ResultOnly, HttpaPerformanceTier::Hyperscale) => 260,
        };
        let delta_budget_kb = match self.performance_tier {
            HttpaPerformanceTier::Efficient => 64,
            HttpaPerformanceTier::Turbo => 192,
            HttpaPerformanceTier::Hyperscale => 512,
        };

        let mut narrative = vec![format!(
            "lane={} delivery={:?} privacy={:?}",
            match (self.mode, self.performance_tier) {
                (HttpaExecutionMode::VisibleBrowse, HttpaPerformanceTier::Efficient) => {
                    "viewport_sync"
                }
                (HttpaExecutionMode::VisibleBrowse, _) => "turbo_viewport",
                (HttpaExecutionMode::ResultOnly, HttpaPerformanceTier::Hyperscale) => {
                    "hyperscale_result_mesh"
                }
                (HttpaExecutionMode::ResultOnly, _) => "result_mesh",
            },
            self.delivery,
            self.privacy
        )];
        narrative.push(format!(
            "resilience={:?} compression={:?} state_sync={:?}",
            resilience_mode, compression_mode, state_sync
        ));
        if semantic_bootstrap {
            narrative.push("semantic bootstrap enabled for instant shell rendering".into());
        }
        if speculative_validation {
            narrative.push("speculative validation enabled for replay-backed deltas".into());
        }
        if prewarmed_path {
            narrative.push("transport prewarm enabled for fast first byte".into());
        }

        HttpaTransportProfile {
            lane: match (self.mode, self.performance_tier) {
                (HttpaExecutionMode::VisibleBrowse, HttpaPerformanceTier::Efficient) => {
                    "viewport_sync".into()
                }
                (HttpaExecutionMode::VisibleBrowse, _) => "turbo_viewport".into(),
                (HttpaExecutionMode::ResultOnly, HttpaPerformanceTier::Hyperscale) => {
                    "hyperscale_result_mesh".into()
                }
                (HttpaExecutionMode::ResultOnly, _) => "result_mesh".into(),
            },
            resilience_mode,
            compression_mode,
            state_sync,
            prewarmed_path,
            semantic_bootstrap,
            speculative_validation,
            header_compaction: self.result_compaction || self.origin_shielding,
            multishard_ingest,
            remote_state_cache: self.deterministic_replay,
            repair_budget_ms,
            prefetch_horizon_ms,
            ingest_shards,
            delta_budget_kb,
            narrative,
        }
    }

    fn recompute(mut self) -> Self {
        self.user_visible = self.mode.is_user_visible();
        self.origin_shielding = !matches!(self.privacy, HttpaPrivacyMode::Identified);
        self.stealth_render = !self.user_visible && self.origin_shielding;
        self.result_compaction = !self.user_visible;
        self.compute_burst =
            !self.user_visible || matches!(self.performance_tier, HttpaPerformanceTier::Hyperscale);
        self.predictive_prefetch = true;
        self.truth_verification = true;
        self.swarm_enabled = true;
        self.deterministic_replay = true;
        self.evidence_notary = matches!(
            self.ledger_mode,
            HttpaLedgerMode::VerifiedEvidence
                | HttpaLedgerMode::SovereignConsensus
                | HttpaLedgerMode::OntologicalTruth
        );
        self.sovereign_consensus = matches!(
            self.ledger_mode,
            HttpaLedgerMode::SovereignConsensus | HttpaLedgerMode::OntologicalTruth
        );
        self.chain_receipt_required = self.chain_receipt_required || self.evidence_notary;

        if self.user_visible && matches!(self.delivery, HttpaDeliveryMode::FinalOnly) {
            self.delivery = HttpaDeliveryMode::ProgressiveStream;
        }
        if !self.user_visible && matches!(self.delivery, HttpaDeliveryMode::LiveViewport) {
            self.delivery = HttpaDeliveryMode::FinalOnly;
        }

        self.max_parallel_agents = match self.performance_tier {
            HttpaPerformanceTier::Efficient => self.max_parallel_agents.min(8).max(1),
            HttpaPerformanceTier::Turbo => self.max_parallel_agents.min(24).max(2),
            HttpaPerformanceTier::Hyperscale => self.max_parallel_agents.min(64).max(4),
        };

        self
    }
}

impl Default for HttpaExecutionPolicy {
    fn default() -> Self {
        Self::for_mode(HttpaExecutionMode::VisibleBrowse)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaExecutionPlan {
    pub routing_lane: String,
    pub session_profile: Option<String>,
    pub identity_surface: String,
    pub ledger_lane: String,
    pub transport: HttpaTransportProfile,
    pub activated_features: Vec<String>,
    pub expected_artifacts: Vec<String>,
    pub max_parallel_agents: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaProtocolCapabilities {
    pub control_methods: Vec<HttpaControlMethod>,
    pub modes: Vec<HttpaExecutionMode>,
    pub privacy_modes: Vec<HttpaPrivacyMode>,
    pub delivery_modes: Vec<HttpaDeliveryMode>,
    pub performance_tiers: Vec<HttpaPerformanceTier>,
    pub ledger_modes: Vec<HttpaLedgerMode>,
    pub search_response_modes: Vec<String>,
    pub search_depths: Vec<String>,
    pub search_source_classes: Vec<String>,
    pub search_features: Vec<String>,
    pub transport_features: Vec<String>,
    pub advanced_features: Vec<String>,
    pub value_features: Vec<String>,
}

impl HttpaProtocolCapabilities {
    #[must_use]
    pub fn production_defaults() -> Self {
        Self {
            control_methods: vec![
                HttpaControlMethod::Get,
                HttpaControlMethod::Post,
                HttpaControlMethod::Value,
                HttpaControlMethod::Intent,
                HttpaControlMethod::Verify,
            ],
            modes: vec![
                HttpaExecutionMode::VisibleBrowse,
                HttpaExecutionMode::ResultOnly,
            ],
            privacy_modes: vec![
                HttpaPrivacyMode::Identified,
                HttpaPrivacyMode::OriginShielded,
                HttpaPrivacyMode::AnonymousDelegation,
            ],
            delivery_modes: vec![
                HttpaDeliveryMode::LiveViewport,
                HttpaDeliveryMode::ProgressiveStream,
                HttpaDeliveryMode::FinalOnly,
            ],
            performance_tiers: vec![
                HttpaPerformanceTier::Efficient,
                HttpaPerformanceTier::Turbo,
                HttpaPerformanceTier::Hyperscale,
            ],
            ledger_modes: vec![
                HttpaLedgerMode::Transport,
                HttpaLedgerMode::BoundIntent,
                HttpaLedgerMode::VerifiedEvidence,
                HttpaLedgerMode::SovereignConsensus,
                HttpaLedgerMode::LiquidStateChannel,
                HttpaLedgerMode::OntologicalTruth,
            ],
            search_response_modes: vec![
                "fast".into(),
                "balanced".into(),
                "deep".into(),
                "forensic".into(),
            ],
            search_depths: vec![
                "quick".into(),
                "standard".into(),
                "deep".into(),
                "ultra".into(),
            ],
            search_source_classes: vec![
                "web".into(),
                "docs".into(),
                "news".into(),
                "social".into(),
                "books".into(),
                "papers".into(),
            ],
            search_features: vec![
                "adaptive_query_profiler".into(),
                "execution_contracts".into(),
                "citation_bundle".into(),
                "claim_map".into(),
                "freshness_budgeting".into(),
                "ledger_backed_results".into(),
                "manifest_bound_proofs".into(),
                "agent_peer_ranking".into(),
            ],
            transport_features: vec![
                "adaptive_parity_recovery".into(),
                "prewarmed_transport".into(),
                "predictive_validation".into(),
                "delta_state_cache".into(),
                "semantic_bootstrap".into(),
                "header_compaction".into(),
                "multishard_ingest".into(),
                "httpa_semantic_rendering".into(),
            ],
            advanced_features: vec![
                "predictive_prefetch".into(),
                "truth_verification".into(),
                "swarm_consensus".into(),
                "chain_receipts".into(),
                "evidence_notary".into(),
                "commitment_verification".into(),
                "search_proof_lookup".into(),
                "sovereign_consensus".into(),
                "adaptive_search".into(),
                "citation_bundle".into(),
                "claim_map".into(),
                "dark_matter_cache".into(),
                "quantum_branching".into(),
                "hardware_symbiosis".into(),
                "single_user_runtime".into(),
                "fingerprint_surface_reduction".into(),
            ],
            value_features: vec![
                "stream_value".into(),
                "financial_intent_routing".into(),
                "proof_of_cognition_rewards".into(),
                "privacy_attestations".into(),
                "device_wallet_binding".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaFrame {
    pub frame_type: HttpaFrameType,
    pub session_id: Option<String>,
    pub agent_id: String,
    pub trace_id: String,
    pub intent: Option<String>,
    pub priority: IntentPriority,
    pub payload: serde_json::Value,
    pub pq_signature: Option<Vec<u8>>,
    pub pocw_hash: Option<String>,
    pub liquid_value: Option<f64>,
    pub timestamp: i64,
}

impl HttpaFrame {
    pub fn new(
        frame_type: HttpaFrameType,
        agent_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            frame_type,
            session_id: None,
            agent_id: agent_id.into(),
            trace_id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
            intent: None,
            priority: IntentPriority::Normal,
            payload,
            pq_signature: None,
            pocw_hash: None,
            liquid_value: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_intent(mut self, intent: impl Into<String>) -> Self {
        self.intent = Some(intent.into());
        self
    }

    pub fn with_priority(mut self, priority: IntentPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_signature(mut self, sig: Vec<u8>) -> Self {
        self.pq_signature = Some(sig);
        self
    }

    pub fn with_pocw(mut self, hash: String) -> Self {
        self.pocw_hash = Some(hash);
        self
    }

    pub fn with_liquid_value(mut self, val: f64) -> Self {
        self.liquid_value = Some(val);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_mode_defaults_to_user_visible_delivery() {
        let policy = HttpaExecutionPolicy::default();
        assert_eq!(policy.mode, HttpaExecutionMode::VisibleBrowse);
        assert!(policy.user_visible);
        assert_eq!(policy.delivery, HttpaDeliveryMode::LiveViewport);
        assert!(!policy.origin_shielding);
    }

    #[test]
    fn result_only_mode_enables_shielding_and_compaction() {
        let policy = HttpaExecutionPolicy::for_mode(HttpaExecutionMode::ResultOnly);
        assert!(!policy.user_visible);
        assert!(policy.origin_shielding);
        assert!(policy.result_compaction);
        assert_eq!(policy.delivery, HttpaDeliveryMode::FinalOnly);
    }

    #[test]
    fn performance_tier_caps_parallelism() {
        let policy = HttpaExecutionPolicy::for_mode(HttpaExecutionMode::ResultOnly)
            .apply_preferences(&HttpaExecutionPreferences {
                performance_tier: Some(HttpaPerformanceTier::Efficient),
                max_parallel_agents: Some(32),
                ..HttpaExecutionPreferences::default()
            });
        assert_eq!(policy.max_parallel_agents, 8);
    }

    #[test]
    fn protocol_capabilities_advertise_adaptive_search_surface() {
        let capabilities = HttpaProtocolCapabilities::production_defaults();
        assert!(
            capabilities
                .control_methods
                .iter()
                .any(|method| matches!(method, HttpaControlMethod::Value))
        );
        assert!(
            capabilities
                .search_response_modes
                .iter()
                .any(|mode| mode == "forensic")
        );
        assert!(
            capabilities
                .search_features
                .iter()
                .any(|feature| feature == "citation_bundle")
        );
        assert!(
            capabilities
                .transport_features
                .iter()
                .any(|feature| feature == "semantic_bootstrap")
        );
        assert!(
            capabilities
                .value_features
                .iter()
                .any(|feature| feature == "stream_value")
        );
    }

    #[test]
    fn hyperscale_result_only_policy_exposes_transport_profile() {
        let policy = HttpaExecutionPolicy::for_mode(HttpaExecutionMode::ResultOnly)
            .apply_preferences(&HttpaExecutionPreferences {
                performance_tier: Some(HttpaPerformanceTier::Hyperscale),
                ..HttpaExecutionPreferences::default()
            });
        let transport = policy.transport_profile();

        assert_eq!(
            transport.resilience_mode,
            HttpaResilienceMode::FountainParity
        );
        assert!(transport.semantic_bootstrap);
        assert!(transport.speculative_validation);
        assert!(transport.multishard_ingest);
        assert!(
            policy
                .activated_features()
                .iter()
                .any(|feature| feature == "adaptive_parity_recovery")
        );
    }
}

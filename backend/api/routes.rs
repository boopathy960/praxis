// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// API â€” All Route Handlers (v5.0 â€” Invention & Discovery Engine)
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

use actix_web::{
    dev::{Service, ServiceRequest},
    web, HttpResponse, ResponseError, Result,
};
use futures::future::{join_all, ready, Either};
use futures::stream;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::sync::RwLock;

use super::models::*;
use crate::agents::base::AgentCapabilityProfile;
use crate::agents::controller::AgentController;
use crate::assistant::autonomy::{
    AssistantAutonomyHub, CustomAssistantAgent, CustomAssistantTool, LifecycleMode,
};
use crate::assistant::executive::AssistantExecutive;
use crate::assistant::fleet::FleetExecutive;
use crate::assistant::local_operator::LocalOperatorPlatform;
use crate::assistant::one_brain::{
    AstraOneBrain, BrainExecutionArtifacts, BrainExecutionContext, BrainModeHint,
};
use crate::assistant::runtime::PersonalAssistantRuntime;
use crate::brain::{
    LightningLoop, OptimizationBatch, OptimizationObjective, SpanType, TraceSpan, TrajectoryTrace,
};
use crate::chain::ai_contract::{
    AIContractDeployRequest, AIContractInvokeRequest, ContractVmInstruction,
};
use crate::chain::chain::{
    BlockFinalityVote, Chain, ChainPeer, PeerRegistration, ReplicationPeerOutcome,
    ValidatorRegistration,
};
use crate::chain::evolution::{
    ChainEvolutionAgentReport, ChainRuntimePolicy, EvolutionAgentRegistration,
};
use crate::chain::signing::{
    canonical_action_message, canonical_finality_vote_message, canonical_peer_handshake_message,
    canonical_replication_message, decode_hex_bytes, derive_address_for_public_key,
};
use crate::chain::transaction::{Transaction, TransactionIntent};
use crate::chain::value_protocol::{
    CognitionRewardRequest, DeviceBindingRequest, FinancialIntentCommitRequest,
    FinancialIntentRouteRequest, PaymentComplianceRegistrationRequest, PaymentDisputeRequest,
    PaymentDisputeResolutionRequest, PrivacyAttestationRequest,
    SettlementConnectorRegistrationRequest, StreamSettlementRequest, StreamValueRequest,
};
use crate::config::{AppConfig, ResourceProfile};
use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};
use crate::features::abyss_crawler_features::{
    AbyssCrawlerEngine, AbyssCrawlerStatus, AbyssMissionReport, AbyssMissionRequest,
};
use crate::features::adaptive_search::{
    AdaptiveSearchDeps, AdaptiveSearchEngine, AdaptiveSearchRequest as EngineAdaptiveSearchRequest,
};
use crate::features::cognitive_memory_sync::CognitiveMemorySync;
use crate::features::delegated_compute::{ComputeTask, DelegatedCompute};
use crate::features::personal_mission_engine::{
    PersonalMissionEngine, PersonalMissionRecordInput, PersonalMissionRequest,
    PersonalMissionStatus,
};
use crate::features::pq_identity::{PQIdentity, PQIdentitySigner};
use crate::features::swarm_browsing::{SwarmBrowser, SwarmQuery};
use crate::features::truth_verification::TruthEngine;
use crate::features::unbreakable_backend::{
    AutonomousActionRequest, CryogenicHeartbeatRequest, DevicePermissionGrantRequest,
    ImmunePatrolReport, NetworkPrivacyRequest, PrivacySensitivity, RevenueCaptureRequest,
    UnbreakableBackend, UnbreakableBackendStatus, UnbreakableFactRequest,
};
use crate::features::zk_mev_shield::{PurchaseIntent, ZkMevShield};

// v2.0 â€” Intelligence Core
use crate::intelligence::memory_graph::MemoryGraph;
use crate::intelligence::omega_executive::{
    ExecutiveMode, OmegaDecision, OmegaExecutive, OmegaMission,
};
use crate::intelligence::planner::HierarchicalPlanner;
use crate::intelligence::reasoning::{AppliedReasoningPolicy, ReasoningEngine, ReasoningStrategy};
use crate::intelligence::self_evolution::SelfEvolutionEngine;
use crate::intelligence::self_healing::SelfHealingEngine;

// v2.0 â€” Network Stack
use crate::network::dns::SecureDns;
use crate::network::interceptor::TrafficInterceptor;
use crate::providers::{
    AdaptiveRouter, BrainCouncil, BrainExecutionProfile, BrainPerformanceTier, CouncilVerdict,
    LocalBrainProvider, RouteDecision,
};

// v3.0 â€” Governance + Brain
use crate::governance::layer::GovernanceLayer;
use crate::httpa::handlers::{authorize_admin_request, AppState};
use crate::intelligence::self_reflection::SelfReflection;
use crate::intelligence::thinking_loop::ThinkingLoop;

// v4.0 â€” Defender
use crate::defender::scanner::ScanMode;
use crate::defender::AstraDefender;

// v5.0 â€” Invention & Discovery Engine
use crate::features::dark_matter_cache::DarkMatterCache;
use crate::features::enhanced_swarm::EnhancedSwarmEngine;
use crate::features::epistemic_immune::EpistemicEngine;
use crate::features::hardware_symbiosis::HardwareSymbiosisEngine;
use crate::features::hostile_arch_neutralizer::HostileArchNeutralizer;
#[cfg(feature = "experimental")]
use crate::features::parasitic_injector::ParasiticInjector;
use crate::features::quantum_branching::QuantumBranchManager;
use crate::features::revolutionary_features::{
    RevolutionaryFeaturesEngine, RevolutionaryProtocolRequest, RevolutionaryResilienceRequest,
    RevolutionarySandboxRequest, RevolutionaryValueRequest,
};
use crate::features::semantic_acquisition::{
    execute_semantic_acquisition, SemanticAcquisitionEngine, SemanticAcquisitionRequest,
};
use crate::features::semantic_render::{SemanticRenderEngine, SemanticRenderRequest};
use crate::features::semantic_workflow::{
    analyze_semantic_adapters, build_workflow_id, build_workflow_manifest,
    workflow_manifest_payload, SemanticWorkflowFailure, SemanticWorkflowPage,
    SemanticWorkflowRequest,
};
use crate::features::ultimate_astra_features::{
    UltimateAcquisitionRequest, UltimateAssistantRequest, UltimateAstraFeaturesEngine,
    UltimateCognitionRequest, UltimateKnowledgeRequest,
};
#[cfg(feature = "experimental")]
use crate::features::warfare_edition_features::{
    WarfareDefenseRequest, WarfareEditionEngine, WarfareEditionStatus, WarfareRecoveryRequest,
    WarfareRevenueRequest,
};
use crate::tools::deep_research::{DeepResearch, ResearchQuery};
use crate::tools::registry::{ToolDescriptor, ToolRegistry};
use crate::tools::web_search::{SearchProvider, SearchQuery, TimeRange, WebSearch};
use crate::voice::{
    VoiceFileRequest as VoiceRuntimeFileRequest, VoiceListenRequest as VoiceRuntimeListenRequest,
    VoiceRuntime, VoiceSpeakRequest as VoiceRuntimeSpeakRequest, VoiceSpeechStyle,
};

const IDENTITY_CHALLENGE_TTL_MS: i64 = 5 * 60 * 1000;
const DEFAULT_IDENTITY_SESSION_TTL_SECONDS: u64 = 12 * 60 * 60;
const MAX_IDENTITY_SESSION_TTL_SECONDS: u64 = 24 * 60 * 60;
const DEFAULT_ASSISTANT_COLLECTION_LIMIT: usize = 32;
const EXPERIMENTAL_ROUTES_ENABLED: bool = cfg!(feature = "experimental");

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct PendingIdentityChallenge {
    challenge_id: String,
    actor: String,
    namespace: String,
    purpose: String,
    operation: String,
    challenge: String,
    payload: serde_json::Value,
    expires_at: i64,
    session_expires_at: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct IdentitySessionRecord {
    session_token: String,
    actor: String,
    challenge_id: String,
    purpose: String,
    roles: Vec<String>,
    public_key_hex: String,
    public_key_hash: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CollectionQuery {
    limit: Option<usize>,
}

pub struct SearchIntelligenceRuntime {
    pub swarm: SwarmBrowser,
    pub truth: TruthEngine,
    pub web_search: WebSearch,
    pub deep_research: DeepResearch,
    pub adaptive_search: AdaptiveSearchEngine,
    pub enhanced_swarm: EnhancedSwarmEngine,
}

impl SearchIntelligenceRuntime {
    #[must_use]
    pub fn new() -> Self {
        Self::from_config(&AppConfig::personal_defaults())
    }

    #[must_use]
    pub fn from_config(config: &AppConfig) -> Self {
        let compact = matches!(config.resources.profile, ResourceProfile::Compact);
        Self {
            swarm: SwarmBrowser::new(),
            truth: TruthEngine::new(),
            web_search: if compact {
                WebSearch::new_compact()
            } else {
                WebSearch::new()
            },
            deep_research: DeepResearch::new(),
            adaptive_search: if compact {
                AdaptiveSearchEngine::new_compact()
            } else {
                AdaptiveSearchEngine::new()
            },
            enhanced_swarm: EnhancedSwarmEngine::new(),
        }
    }
}

pub struct CognitiveRuntime {
    pub reasoning: ReasoningEngine,
    pub thinking_loop: ThinkingLoop,
}

impl CognitiveRuntime {
    #[must_use]
    pub fn new() -> Self {
        Self {
            reasoning: ReasoningEngine::new(),
            thinking_loop: ThinkingLoop::new(),
        }
    }
}

pub struct ControlPlaneRuntime {
    pub healing: SelfHealingEngine,
    pub evolution: SelfEvolutionEngine,
    pub memory_graph: MemoryGraph,
    pub self_reflection: SelfReflection,
    pub omega_executive: OmegaExecutive,
    pub local_brain: LocalBrainProvider,
    pub brain_router: AdaptiveRouter,
    pub brain_council: BrainCouncil,
}

impl ControlPlaneRuntime {
    #[must_use]
    pub fn new() -> Self {
        Self {
            healing: SelfHealingEngine::new(),
            evolution: SelfEvolutionEngine::new(),
            memory_graph: MemoryGraph::new(),
            self_reflection: SelfReflection::new(),
            omega_executive: OmegaExecutive::new(),
            local_brain: LocalBrainProvider::new(),
            brain_router: AdaptiveRouter::new(vec![
                "planner".into(),
                "research".into(),
                "verifier".into(),
                "systems".into(),
                "strategist".into(),
            ]),
            brain_council: BrainCouncil::new(),
        }
    }
}

/// Shared engine state accessible by all API handlers.
pub struct EngineState {
    pub config: AppConfig,
    // â”€â”€ v1 Feature Engines â”€â”€
    pub memory: CognitiveMemorySync,
    pub mev_shield: ZkMevShield,
    pub identity: PQIdentity,
    pub compute: DelegatedCompute,
    pub search_runtime: Arc<StdRwLock<SearchIntelligenceRuntime>>,
    pub cognitive_runtime: Arc<StdRwLock<CognitiveRuntime>>,
    pub control_runtime: Arc<StdRwLock<ControlPlaneRuntime>>,
    pub agents: AgentController,
    pub tool_registry: ToolRegistry,
    pub voice: VoiceRuntime,
    pub chain: Chain,
    pub personal_assistant: PersonalAssistantRuntime,
    pub assistant_autonomy: AssistantAutonomyHub,
    pub assistant_executive: AssistantExecutive,
    pub fleet: FleetExecutive,
    pub local_operator: LocalOperatorPlatform,
    pub one_brain: AstraOneBrain,
    pub unbreakable: UnbreakableBackend,

    // â”€â”€ v2 Intelligence Core â”€â”€
    pub planner: HierarchicalPlanner,
    pub learning_loop: LightningLoop,

    // â”€â”€ v2 Network Stack â”€â”€
    pub dns: SecureDns,
    pub interceptor: TrafficInterceptor,

    // â”€â”€ v3 Governance + Brain â”€â”€
    pub governance: GovernanceLayer,

    // â”€â”€ v4 Device Security â”€â”€
    pub defender: AstraDefender,

    // â”€â”€ v5 Invention & Discovery Engine â”€â”€
    pub quantum: QuantumBranchManager,
    pub hostile_arch: HostileArchNeutralizer,
    pub epistemic: EpistemicEngine,
    pub hardware: HardwareSymbiosisEngine,
    pub abyss: AbyssCrawlerEngine,
    pub personal_mission: PersonalMissionEngine,
    pub revolutionary: RevolutionaryFeaturesEngine,
    pub ultimate: UltimateAstraFeaturesEngine,
    #[cfg(feature = "experimental")]
    pub warfare: WarfareEditionEngine,
    pub semantic_acquisition: SemanticAcquisitionEngine,
    pub semantic_render: SemanticRenderEngine,
    pub dark_cache: DarkMatterCache,
    #[cfg(feature = "experimental")]
    pub parasitic: ParasiticInjector,
    identity_challenges: BTreeMap<String, PendingIdentityChallenge>,
    identity_sessions: BTreeMap<String, IdentitySessionRecord>,
}

impl EngineState {
    pub fn new(learning_loop: LightningLoop) -> Self {
        Self::from_config(AppConfig::personal_defaults(), learning_loop)
    }

    pub fn from_config(config: AppConfig, learning_loop: LightningLoop) -> Self {
        let mut identity = PQIdentity::new();
        let mut control_runtime = ControlPlaneRuntime::new();
        identity.ensure_identity();
        let personal_assistant = PersonalAssistantRuntime::new(&config);
        let assistant_autonomy = AssistantAutonomyHub::new(
            config.assistant.codename.clone(),
            config.assistant.invocation_required,
        );
        let local_operator = LocalOperatorPlatform::new();
        // Register all subsystems for health monitoring
        for sub in [
            "memory",
            "swarm",
            "truth",
            "identity",
            "compute",
            "agents",
            "chain",
            "voice",
            "reasoning",
            "planner",
            "dns",
            "proxy",
            "tunnel",
            "cryogenic",
            "reality_anchor",
            "sovereign_vault",
            "permissioned_autonomy",
            "treasury",
            "abyss",
            "personal_mission",
            "revolutionary",
            "ultimate",
            #[cfg(feature = "experimental")]
            "warfare",
        ] {
            control_runtime.healing.register_subsystem(sub);
        }

        Self {
            config: config.clone(),
            memory: CognitiveMemorySync::new(),
            mev_shield: ZkMevShield::new(),
            identity,
            compute: DelegatedCompute::new(),
            search_runtime: Arc::new(StdRwLock::new(SearchIntelligenceRuntime::from_config(
                &config,
            ))),
            cognitive_runtime: Arc::new(StdRwLock::new(CognitiveRuntime::new())),
            control_runtime: Arc::new(StdRwLock::new(control_runtime)),
            agents: AgentController::new(),
            tool_registry: ToolRegistry::new(),
            voice: VoiceRuntime::new(),
            chain: Chain::load_or_new_default(),
            personal_assistant,
            assistant_autonomy,
            assistant_executive: AssistantExecutive::new(config.resources.llm_enabled, true),
            fleet: FleetExecutive::new(),
            local_operator,
            one_brain: AstraOneBrain::new(PathBuf::from("astra_state/brain")),
            unbreakable: UnbreakableBackend::new(&config),

            planner: HierarchicalPlanner::new(),
            learning_loop,

            dns: SecureDns::new(),
            interceptor: TrafficInterceptor::new(),

            governance: {
                let mut gov = GovernanceLayer::new();
                gov.boot();
                gov
            },
            defender: AstraDefender::new(),

            // v5 Invention & Discovery Engine
            quantum: QuantumBranchManager::new(),
            hostile_arch: HostileArchNeutralizer::new(),
            epistemic: EpistemicEngine::new(),
            hardware: HardwareSymbiosisEngine::new_with_profile(
                config.resources.profile,
                config.resources.gpu_enabled,
            ),
            abyss: AbyssCrawlerEngine::new(),
            personal_mission: PersonalMissionEngine::new(),
            revolutionary: RevolutionaryFeaturesEngine::new(),
            ultimate: UltimateAstraFeaturesEngine::new(),
            #[cfg(feature = "experimental")]
            warfare: WarfareEditionEngine::new(),
            semantic_acquisition: SemanticAcquisitionEngine::new(),
            semantic_render: SemanticRenderEngine::new(),
            dark_cache: DarkMatterCache::new(),
            #[cfg(feature = "experimental")]
            parasitic: ParasiticInjector::new(),
            identity_challenges: BTreeMap::new(),
            identity_sessions: BTreeMap::new(),
        }
    }

    pub fn execute_brain(
        &mut self,
        request: BrainExecuteRequest,
        runtime_sessions: Option<&crate::runtime::SessionManager>,
    ) -> AstraResult<BrainExecutionArtifacts> {
        let EngineState {
            config,
            search_runtime,
            cognitive_runtime,
            control_runtime,
            agents,
            tool_registry,
            planner,
            learning_loop,
            personal_mission,
            one_brain,
            ..
        } = self;

        let mut search_runtime = search_runtime
            .write()
            .map_err(|_| search_runtime_lock_error())?;
        let mut cognitive_runtime = cognitive_runtime
            .write()
            .map_err(|_| cognitive_runtime_lock_error())?;
        let mut control_runtime = control_runtime
            .write()
            .map_err(|_| control_runtime_lock_error())?;
        let SearchIntelligenceRuntime {
            web_search,
            deep_research,
            swarm,
            truth,
            adaptive_search,
            enhanced_swarm,
        } = &mut *search_runtime;
        let CognitiveRuntime {
            reasoning,
            thinking_loop,
        } = &mut *cognitive_runtime;
        let ControlPlaneRuntime {
            memory_graph,
            self_reflection,
            omega_executive,
            local_brain,
            brain_router,
            brain_council,
            ..
        } = &mut *control_runtime;

        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        one_brain.execute(
            request,
            BrainExecutionContext {
                config,
                web_search,
                deep_research,
                swarm,
                truth,
                adaptive_search,
                enhanced_swarm,
                reasoning_engine: reasoning,
                thinking_loop,
                memory_graph,
                self_reflection,
                omega_executive,
                local_brain,
                brain_router,
                brain_council,
                agents,
                tool_registry,
                planner,
                learning_loop,
                personal_mission,
                runtime_sessions,
                workspace_root,
            },
        )
    }
}

fn search_runtime_lock_error() -> AstraError {
    AstraError::Internal("search runtime lock poisoned".into())
}

fn cognitive_runtime_lock_error() -> AstraError {
    AstraError::Internal("cognitive runtime lock poisoned".into())
}

fn control_runtime_lock_error() -> AstraError {
    AstraError::Internal("control runtime lock poisoned".into())
}

fn purge_expired_identity_state(state: &mut EngineState) {
    let now = chrono::Utc::now().timestamp_millis();
    state
        .identity_challenges
        .retain(|_, challenge| challenge.expires_at > now);
    state
        .identity_sessions
        .retain(|_, session| session.expires_at > now);
}

fn normalized_identity_session_ttl_ms(requested_ttl_seconds: Option<u64>) -> i64 {
    requested_ttl_seconds
        .unwrap_or(DEFAULT_IDENTITY_SESSION_TTL_SECONDS)
        .clamp(60, MAX_IDENTITY_SESSION_TTL_SECONDS)
        .saturating_mul(1000) as i64
}

fn normalized_identity_namespace(namespace: Option<&str>) -> AstraResult<String> {
    let namespace = namespace
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("acct");
    if namespace
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        Ok(namespace.to_string())
    } else {
        Err(AstraError::InvalidTransaction(
            "namespace must contain only lowercase ascii letters, digits, or '_'".into(),
        ))
    }
}

fn local_identity_actor(identity: &PQIdentity, namespace: &str) -> AstraResult<Option<String>> {
    let Some(public_key_hex) = identity.public_key_hex() else {
        return Ok(None);
    };
    let public_key = decode_hex_bytes(&public_key_hex, "identity public key")?;
    Ok(Some(derive_address_for_public_key(namespace, &public_key)))
}

fn build_identity_challenge(
    state: &mut EngineState,
    request: IdentityChallengeRequest,
) -> AstraResult<IdentityChallengeResponse> {
    purge_expired_identity_state(state);
    let namespace = normalized_identity_namespace(request.namespace.as_deref())?;
    let local_actor = local_identity_actor(&state.identity, &namespace)?;
    let actor = request
        .actor
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| local_actor.clone())
        .ok_or(AstraError::AuthRequired)?;
    let purpose = request
        .purpose
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("assistant_session")
        .to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let expires_at = now + IDENTITY_CHALLENGE_TTL_MS;
    let session_expires_at = now + normalized_identity_session_ttl_ms(request.session_ttl_seconds);
    let challenge_id =
        crate::crypto::hash::sha3_256_hex(format!("identity:{actor}:{purpose}:{now}").as_bytes())
            [..32]
            .to_string();
    let challenge = crate::crypto::hash::sha3_256_hex(
        format!("identity_nonce:{}:{}:{now}", actor, purpose).as_bytes(),
    )[..48]
        .to_string();
    let chain_id = state.chain.get_chain_info().chain_id;
    let payload = serde_json::json!({
        "challenge_id": challenge_id,
        "challenge": challenge,
        "purpose": purpose,
        "issued_at": now,
        "expires_at": expires_at,
        "session_expires_at": session_expires_at,
        "metadata": request.metadata.unwrap_or(serde_json::Value::Null),
    });
    let operation = "identity_session_auth".to_string();
    let canonical_message_bytes = canonical_action_message(&chain_id, &actor, &operation, &payload);
    let canonical_message = String::from_utf8(canonical_message_bytes.clone()).map_err(|_| {
        AstraError::Crypto("canonical identity challenge message must remain utf-8".into())
    })?;
    state.identity_challenges.insert(
        challenge_id.clone(),
        PendingIdentityChallenge {
            challenge_id: challenge_id.clone(),
            actor: actor.clone(),
            namespace: namespace.clone(),
            purpose: purpose.clone(),
            operation: operation.clone(),
            challenge: challenge.clone(),
            payload: payload.clone(),
            expires_at,
            session_expires_at,
        },
    );
    Ok(IdentityChallengeResponse {
        challenge_id,
        actor: actor.clone(),
        namespace,
        purpose,
        chain_id,
        operation,
        challenge,
        payload,
        canonical_message_hash: crate::crypto::hash::sha3_256_hex(
            canonical_message_bytes.as_slice(),
        ),
        canonical_message,
        expires_at,
        local_identity_can_sign: local_actor.as_deref() == Some(actor.as_str()),
        local_public_key_hex: state.identity.public_key_hex(),
        local_public_key_hash: state.identity.public_key_hash_full(),
    })
}

fn complete_identity_session_auth(
    state: &mut EngineState,
    request: IdentitySessionAuthRequest,
) -> AstraResult<IdentitySessionAuthResponse> {
    purge_expired_identity_state(state);
    let now = chrono::Utc::now().timestamp_millis();
    let actor = request.actor.trim();
    if actor.is_empty() {
        return Err(AstraError::AuthRequired);
    }
    let challenge = state
        .identity_challenges
        .remove(request.challenge_id.trim())
        .ok_or_else(|| AstraError::SessionNotFound(request.challenge_id.clone()))?;
    if challenge.expires_at <= now {
        return Err(AstraError::SessionExpired(challenge.challenge_id));
    }
    if challenge.actor != actor {
        return Err(AstraError::SignatureInvalid);
    }

    let identity = state.chain.authorize_action(
        actor,
        request.public_key_hex.trim(),
        request.signature_hex.trim(),
        &challenge.operation,
        &challenge.payload,
        &[],
    )?;
    let session_token = crate::crypto::hash::sha3_256_hex(
        format!(
            "identity_session:{}:{}:{}:{}",
            actor, challenge.challenge_id, identity.public_key_hash, now
        )
        .as_bytes(),
    )[..48]
        .to_string();
    state.identity_sessions.insert(
        session_token.clone(),
        IdentitySessionRecord {
            session_token: session_token.clone(),
            actor: actor.to_string(),
            challenge_id: challenge.challenge_id.clone(),
            purpose: challenge.purpose.clone(),
            roles: identity.roles.clone(),
            public_key_hex: identity.public_key_hex.clone(),
            public_key_hash: identity.public_key_hash.clone(),
            expires_at: challenge.session_expires_at,
        },
    );
    state.chain.append_audit(
        "identity_session_authenticated",
        actor,
        &challenge.challenge_id,
        serde_json::json!({
            "purpose": challenge.purpose,
            "namespace": challenge.namespace,
            "public_key_hash": identity.public_key_hash,
            "session_token": session_token,
            "metadata": request.metadata,
        }),
    );
    state.chain.persist()?;
    Ok(IdentitySessionAuthResponse {
        authenticated: true,
        actor: actor.to_string(),
        challenge_id: challenge.challenge_id,
        session_token,
        chain_id: state.chain.get_chain_info().chain_id,
        purpose: challenge.purpose,
        roles: identity.roles,
        public_key_hex: identity.public_key_hex,
        public_key_hash: identity.public_key_hash,
        algorithm: identity.algorithm,
        expires_at: challenge.session_expires_at,
    })
}

fn infer_learning_domain(problem: &str, context: &serde_json::Value) -> String {
    if let Some(domain) = context.get("domain").and_then(|value| value.as_str()) {
        let normalized = domain.trim().to_lowercase();
        if !normalized.is_empty() {
            return normalized.replace(' ', "_");
        }
    }

    let lower = problem.to_lowercase();
    if ["code", "rust", "compile", "function", "test", "bugfix"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "coding".into()
    } else if ["debug", "incident", "failure", "broken", "regression"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "debugging".into()
    } else if ["design", "architecture", "system", "service", "scaling"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "architecture".into()
    } else if ["prove", "theorem", "equation", "math", "probability"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "logic".into()
    } else {
        "general".into()
    }
}

fn to_applied_policy(policy: crate::brain::ReasoningPolicy) -> AppliedReasoningPolicy {
    AppliedReasoningPolicy {
        domain: policy.domain,
        champion_prompt: policy.champion_prompt,
        preferred_strategies: policy.preferred_strategies,
        discouraged_strategies: policy.discouraged_strategies,
        max_depth: policy.max_reasoning_depth,
        max_expansions: policy.max_reasoning_expansions,
        exploration_c: policy.exploration_c,
        verification_bias: policy.verification_bias,
        policy_notes: policy.policy_notes,
    }
}

fn push_strategy_once(strategies: &mut Vec<ReasoningStrategy>, strategy: ReasoningStrategy) {
    if !strategies.contains(&strategy) {
        strategies.push(strategy);
    }
}

fn merge_reasoning_policy(
    mut policy: AppliedReasoningPolicy,
    profile: &BrainExecutionProfile,
    route: &RouteDecision,
    verdict: &CouncilVerdict,
    decision: &OmegaDecision,
) -> AppliedReasoningPolicy {
    for strategy in &profile.preferred_strategies {
        push_strategy_once(&mut policy.preferred_strategies, *strategy);
    }
    for strategy in &decision.preferred_strategies {
        push_strategy_once(&mut policy.preferred_strategies, *strategy);
    }
    for strategy in &verdict.recommended_strategies {
        push_strategy_once(&mut policy.preferred_strategies, *strategy);
    }

    match route.selected_node.as_str() {
        "planner" => {
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::Decomposition,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::TreeOfThought,
            );
        }
        "research" => {
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::HypothesisTest,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::CausalReasoning,
            );
        }
        "verifier" => {
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::FormalReasoning,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::SelfCritique,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::AdversarialReasoning,
            );
        }
        "systems" => {
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::CausalReasoning,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::ConstraintSatisfaction,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::Decomposition,
            );
        }
        "strategist" => {
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::AdversarialReasoning,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::CounterfactualReasoning,
            );
            push_strategy_once(
                &mut policy.preferred_strategies,
                ReasoningStrategy::BayesianInference,
            );
        }
        _ => {}
    }

    policy.max_depth = Some(profile.recommended_depth);
    policy.max_expansions = Some(profile.recommended_expansions);
    policy.verification_bias = policy
        .verification_bias
        .max(profile.verification_bias)
        .clamp(0.35, 0.95);
    policy.exploration_c = Some(
        match profile.performance_tier {
            BrainPerformanceTier::LatencyFirst => 0.95,
            BrainPerformanceTier::Balanced => 1.10,
            BrainPerformanceTier::Throughput => 1.18,
            BrainPerformanceTier::Intensive => 1.25,
        } + (decision.invention_bias * 0.05),
    );
    policy.policy_notes.push(format!(
        "omega lane={} route={} agreement={:.2}",
        profile.cache_lane, route.selected_node, verdict.agreement_score
    ));
    if !verdict.focus_domains.is_empty() {
        policy.policy_notes.push(format!(
            "council focus domains={}",
            verdict.focus_domains.join(", ")
        ));
    }
    policy.policy_notes.push(format!(
        "executive mode={:?} autonomy={:.2} search_intensity={:.2}",
        decision.mode, decision.autonomy_score, decision.search_intensity
    ));
    policy.policy_notes.extend(
        profile
            .invariants
            .iter()
            .take(2)
            .map(|invariant| format!("omega invariant: {invariant}")),
    );

    policy
}

fn build_omega_metadata(
    profile: &BrainExecutionProfile,
    route: &RouteDecision,
    verdict: &CouncilVerdict,
    decision: &OmegaDecision,
    mission: &OmegaMission,
    latency_ms: f64,
    quality: f64,
) -> serde_json::Value {
    serde_json::json!({
        "profile": profile,
        "executive": decision,
        "mission": mission,
        "route": {
            "selected_node": route.selected_node,
            "reason": route.reason,
            "fallback": route.fallback,
        },
        "council": {
            "agreement_score": verdict.agreement_score,
            "consensus_response": verdict.consensus_response,
            "dissenting": verdict.dissenting,
            "focus_domains": verdict.focus_domains,
            "recommended_strategies": verdict
                .recommended_strategies
                .iter()
                .map(|strategy| strategy.as_str())
                .collect::<Vec<_>>(),
        },
        "runtime": {
            "latency_ms": latency_ms,
            "quality": quality.clamp(0.0, 1.0),
        }
    })
}

fn search_lane_for(mode: ExecutiveMode) -> &'static str {
    match mode {
        ExecutiveMode::Search => "search",
        ExecutiveMode::Decide => "decision",
        ExecutiveMode::Invent => "invention",
        ExecutiveMode::Verify => "verification",
    }
}

fn objective_for_domain(domain: &str) -> OptimizationObjective {
    OptimizationObjective {
        domain: domain.to_string(),
        reward_signal: "reasoning_quality".into(),
        optimize_prompts: true,
        optimize_tool_policies: true,
        max_rollouts: 256,
    }
}

fn strategy_label(strategy: ReasoningStrategy) -> String {
    strategy.as_str().to_string()
}

fn emit_reasoning_trace(
    problem: &str,
    domain: &str,
    result: &crate::intelligence::reasoning::ReasoningResult,
) -> TrajectoryTrace {
    let mut trace = TrajectoryTrace::new(problem);
    trace.domain = domain.to_string();
    trace.final_answer = result
        .best_path
        .iter()
        .map(|node| node.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    trace.final_reward = result.best_score;
    trace.success = result.best_score >= 0.65 && result.confidence >= 0.55;
    trace.gating_mode = if trace.success { "execute" } else { "sandbox" }.into();
    trace.strategies_used = result
        .best_path
        .iter()
        .map(|node| strategy_label(node.strategy))
        .collect();
    trace.total_iterations = result.total_thoughts;
    trace
        .reward_dimensions
        .insert("scenario".into(), result.best_score);
    trace
        .reward_dimensions
        .insert("critic".into(), result.confidence);
    trace.reward_dimensions.insert(
        "static".into(),
        (result.max_depth_reached as f64 / 12.0).clamp(0.0, 1.0),
    );

    for node in &result.best_path {
        let mut span = TraceSpan::new(SpanType::Reasoning);
        span.output_data = node.content.clone();
        span.reward = node.score;
        span.duration_ms = node.visits as f64;
        span.cognitive_mode = strategy_label(node.strategy);
        span.parent_id = node.parent.clone();
        span.attributes.insert("node_id".into(), node.id.clone());
        span.attributes
            .insert("depth".into(), node.depth.to_string());
        span.attributes
            .insert("confidence".into(), format!("{:.4}", node.confidence));
        trace.add_span(span);
    }

    let mut verification_span = TraceSpan::new(SpanType::Verification);
    verification_span.output_data = format!("reasoning_confidence={:.4}", result.confidence);
    verification_span.reward = result.confidence;
    verification_span.cognitive_mode = "verification".into();
    trace.add_span(verification_span);

    trace
}

fn emit_thinking_trace(
    problem: &str,
    domain: &str,
    result: &crate::intelligence::thinking_loop::ThinkingResult,
) -> TrajectoryTrace {
    let mut trace = TrajectoryTrace::new(problem);
    trace.domain = domain.to_string();
    trace.final_answer = result.solution.clone();
    trace.final_reward = result.final_confidence;
    trace.success = result.verification_passed;
    trace.gating_mode = format!("{:?}", result.risk_assessment.mode).to_lowercase();
    trace.strategies_used = result
        .iteration_details
        .iter()
        .map(|detail| detail.strategy_used.trim().to_lowercase().replace(' ', "_"))
        .filter(|label| !label.is_empty())
        .collect();
    trace.total_iterations = result.iterations as usize;
    trace.total_duration_ms = result.total_duration_ms;
    trace.reward_dimensions.insert(
        "security".into(),
        1.0 - result.risk_assessment.risk_level.clamp(0.0, 1.0),
    );
    trace
        .reward_dimensions
        .insert("scenario".into(), result.final_confidence);
    trace.reward_dimensions.insert(
        "critic".into(),
        1.0 - result.hallucination_score.clamp(0.0, 1.0),
    );
    trace.reward_dimensions.insert(
        "property".into(),
        if result.verification_passed {
            0.9
        } else {
            0.35
        },
    );

    for detail in &result.iteration_details {
        let mut span = TraceSpan::new(SpanType::Reasoning);
        span.output_data = format!(
            "iteration={} strategy={} confidence={:.4}",
            detail.iteration, detail.strategy_used, detail.verification_confidence
        );
        span.reward = detail.reasoning_score;
        span.duration_ms = detail.duration_ms;
        span.cognitive_mode = detail.strategy_used.clone();
        span.attributes
            .insert("gating_mode".into(), format!("{:?}", detail.gating_mode));
        span.attributes.insert(
            "critique_applied".into(),
            detail.critique_applied.to_string(),
        );
        trace.add_span(span);
    }

    let mut verification_span = TraceSpan::new(SpanType::Verification);
    verification_span.output_data = format!("verification_passed={}", result.verification_passed);
    verification_span.reward = result.final_confidence;
    verification_span.duration_ms = result.total_duration_ms;
    verification_span.cognitive_mode = "thinking_verification".into();
    trace.add_span(verification_span);

    trace
}

fn ingest_learning_trace(
    learning_loop: &LightningLoop,
    objective: OptimizationObjective,
    trace: TrajectoryTrace,
) {
    let _ = learning_loop.ingest_batch(OptimizationBatch {
        objective: Some(objective),
        traces: vec![trace],
    });
}

// â”€â”€ Memory Routes â”€â”€

pub async fn memory_commit(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<MemoryCommitRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let result = state
        .memory
        .commit_memory(&body.url, &body.intent, &body.context)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn memory_search(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<MemorySearchRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let results: Vec<_> = state
        .memory
        .search(&body.query)
        .into_iter()
        .cloned()
        .collect();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "results": results, "count": results.len()
    }))))
}

// â”€â”€ Swarm Routes â”€â”€

pub async fn swarm_query(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SwarmQueryRequest>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let query = SwarmQuery {
        intent: body.intent.clone(),
        num_agents: body.num_agents.unwrap_or(5),
        timeout_ms: body.timeout_ms.unwrap_or(10000),
        min_confidence: body.min_confidence.unwrap_or(0.3),
    };
    let result = search_runtime.swarm.query(&query)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn adaptive_search(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AdaptiveSearchRequest>,
) -> Result<HttpResponse, AstraError> {
    let omega_context = serde_json::json!({
        "domains": body.domain_focus.clone().unwrap_or_default(),
        "high_impact": body.notarize.unwrap_or(false),
    });
    let (search_runtime, control_runtime) = {
        let state = state.read().await;
        (
            Arc::clone(&state.search_runtime),
            Arc::clone(&state.control_runtime),
        )
    };
    let (omega_decision, omega_mission) = {
        let mut control_runtime = control_runtime
            .write()
            .map_err(|_| control_runtime_lock_error())?;
        let omega_decision = control_runtime
            .omega_executive
            .directive_for(&body.query, &omega_context);
        let omega_mission = control_runtime
            .omega_executive
            .mission_for(&body.query, &omega_decision);
        (omega_decision, omega_mission)
    };
    let request = EngineAdaptiveSearchRequest {
        query: body.query.clone(),
        preferred_depth: body.preferred_depth.clone(),
        max_sources: body.max_sources,
        include_social: body.include_social,
        include_news: body.include_news,
        include_books: body.include_books,
        include_papers: body.include_papers,
        include_docs: body.include_docs,
        response_mode: body.response_mode,
        freshness_horizon_hours: body.freshness_horizon_hours,
        domain_focus: body.domain_focus.clone(),
        require_citations: body.require_citations,
        max_contradictions: body.max_contradictions,
        notarize: body.notarize,
    };

    let requested_notarize = request.notarize;
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let SearchIntelligenceRuntime {
        web_search,
        deep_research,
        swarm,
        truth,
        adaptive_search,
        enhanced_swarm,
    } = &mut *search_runtime;

    let mut result = adaptive_search.execute(
        request,
        AdaptiveSearchDeps {
            web_search,
            deep_research,
            swarm,
            truth,
            enhanced_swarm,
        },
    )?;
    drop(search_runtime);

    let should_notarize = requested_notarize.unwrap_or(!result.profile.fast_path);
    let evidence_domains = result
        .evidence
        .iter()
        .map(|item| item.domain.clone())
        .collect::<Vec<_>>();
    let invention_limit = omega_mission.invention_budget.max(1).min(5);
    let search_quality =
        ((result.confidence + result.verification.truth_score) / 2.0).clamp(0.0, 1.0);
    if should_notarize {
        let mut state = state.write().await;
        crate::features::adaptive_search::attach_ledger_proof(&mut state.chain, &mut result)?;
    }
    let inventions = {
        let mut control_runtime = control_runtime
            .write()
            .map_err(|_| control_runtime_lock_error())?;
        let inventions = if matches!(omega_decision.mode, ExecutiveMode::Invent)
            || omega_decision.invention_bias >= 0.65
        {
            control_runtime.omega_executive.invent(
                &body.query,
                &omega_decision.focus_domains,
                &evidence_domains,
                invention_limit,
            )
        } else {
            Vec::new()
        };
        control_runtime.omega_executive.record_cycle_with_context(
            search_lane_for(omega_decision.mode),
            omega_decision.mode,
            &omega_decision.focus_domains,
            result.verification.contradictions <= result.execution_contract.max_contradictions,
            search_quality,
        );
        inventions
    };
    result.omega_decision = Some(omega_decision);
    result.omega_mission = Some(omega_mission);
    result.inventions = inventions;
    {
        let mut state = state.write().await;
        state.assistant_executive.record_compatibility_completion(
            "/api/search/intelligence",
            &body.query,
            MissionMode::Search,
            format!(
                "Adaptive search gathered {} evidence item(s) with {:.2} confidence",
                result.evidence.len(),
                result.confidence
            ),
            serde_json::json!({
                "confidence": result.confidence,
                "citations": result.evidence.len(),
                "notarized": result.ledger_proof.is_some(),
            }),
        );
    }
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn adaptive_search_profile(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AdaptiveSearchRequest>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let search_runtime = search_runtime
        .read()
        .map_err(|_| search_runtime_lock_error())?;
    let request = EngineAdaptiveSearchRequest {
        query: body.query.clone(),
        preferred_depth: body.preferred_depth.clone(),
        max_sources: body.max_sources,
        include_social: body.include_social,
        include_news: body.include_news,
        include_books: body.include_books,
        include_papers: body.include_papers,
        include_docs: body.include_docs,
        response_mode: body.response_mode,
        freshness_horizon_hours: body.freshness_horizon_hours,
        domain_focus: body.domain_focus.clone(),
        require_citations: body.require_citations,
        max_contradictions: body.max_contradictions,
        notarize: body.notarize,
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        search_runtime.adaptive_search.preview(&request),
    )))
}

pub async fn adaptive_search_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let search_runtime = search_runtime
        .read()
        .map_err(|_| search_runtime_lock_error())?;
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(SearchIntelligenceStatsResponse {
            adaptive: search_runtime.adaptive_search.get_stats(),
            acquisition: search_runtime.web_search.acquisition_stats(),
            refresh_plan: search_runtime.web_search.acquisition_refresh_plan(8),
            autonomous: search_runtime.web_search.autonomous_frontier(),
        })),
    )
}

// â”€â”€ MEV Shield Routes â”€â”€

pub async fn adaptive_search_refresh(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SearchIntelligenceRefreshRequest>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let request = EngineAdaptiveSearchRequest {
        query: body.query.clone(),
        preferred_depth: body.preferred_depth.clone(),
        max_sources: body.max_sources,
        include_social: body.include_social,
        include_news: body.include_news,
        include_books: body.include_books,
        include_papers: body.include_papers,
        include_docs: body.include_docs,
        response_mode: None,
        freshness_horizon_hours: body.freshness_horizon_hours,
        domain_focus: None,
        require_citations: Some(true),
        max_contradictions: Some(2),
        notarize: Some(false),
    };
    let plan = search_runtime.adaptive_search.preview(&request);
    let before = search_runtime.web_search.acquisition_stats();
    let max_sources = body
        .max_sources
        .unwrap_or(plan.profile.source_budget)
        .max(1);
    let search_response = search_runtime.web_search.refresh_acquisition(SearchQuery {
        query: body.query.clone(),
        provider: SearchProvider::Aggregated,
        max_results: max_sources,
        language: "en".into(),
        region: None,
        safe_search: true,
        time_range: time_range_from_hours(body.freshness_horizon_hours),
        source_classes: plan.profile.source_classes.clone(),
        deep_reasoning: true,
    });
    let research_report = search_runtime.deep_research.research(ResearchQuery {
        topic: body.query.clone(),
        depth: plan.profile.research_depth.clone(),
        max_sources,
        focus_areas: plan.execution_contract.domain_focus.clone(),
        exclude_domains: vec![],
        source_classes: plan.profile.source_classes.clone(),
    });
    let research_sources_ingested = research_report.sources.len();
    search_runtime
        .web_search
        .ingest_research_sources(&research_report.sources);
    let acquisition = search_runtime.web_search.acquisition_stats();

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(SearchIntelligenceRefreshResponse {
            query: body.query.clone(),
            search_results_ingested: search_response.fresh_hits,
            research_sources_ingested,
            indexed_documents_total: acquisition.total_documents,
            indexed_documents_delta: acquisition
                .total_documents
                .saturating_sub(before.total_documents),
            refreshed_lanes: plan
                .profile
                .source_classes
                .iter()
                .map(|class| class.as_str().to_string())
                .collect(),
            source_mix: search_response.source_mix,
            acquisition,
        })),
    )
}

pub async fn adaptive_search_refresh_plan(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let search_runtime = search_runtime
        .read()
        .map_err(|_| search_runtime_lock_error())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        search_runtime.web_search.acquisition_refresh_plan(12),
    )))
}

pub async fn adaptive_search_refresh_sweep(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SearchIntelligenceRefreshSweepRequest>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let sweep = search_runtime.web_search.execute_refresh_plan(
        body.max_tasks.unwrap_or(6).max(1),
        body.max_results_per_task.unwrap_or(4).max(1),
    );
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(SearchIntelligenceRefreshSweepResponse {
            sweep,
        })),
    )
}

pub async fn adaptive_search_autonomous_frontier(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let search_runtime = search_runtime
        .read()
        .map_err(|_| search_runtime_lock_error())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        search_runtime.web_search.autonomous_frontier(),
    )))
}

pub async fn adaptive_search_autonomous_tick(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SearchIntelligenceCrawlTickRequest>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let autonomous = search_runtime
        .web_search
        .execute_autonomous_tick(body.force.unwrap_or(false), body.max_profiles);
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(SearchIntelligenceCrawlTickResponse {
            autonomous,
        })),
    )
}

fn time_range_from_hours(hours: Option<u64>) -> Option<TimeRange> {
    match hours {
        Some(0..=24) => Some(TimeRange::Day),
        Some(25..=168) => Some(TimeRange::Week),
        Some(169..=720) => Some(TimeRange::Month),
        Some(_) => Some(TimeRange::Year),
        None => None,
    }
}

pub async fn shield_intent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<ShieldIntentRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let intent = PurchaseIntent {
        url: body.url.clone(),
        action: body.action.clone(),
        max_price: body.max_price,
        item_id: body.item_id.clone(),
    };
    let result = state.mev_shield.shield_intent(intent);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

// â”€â”€ Truth Routes â”€â”€

pub async fn verify_content(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<VerifyContentRequest>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let report = search_runtime
        .truth
        .verify_content(&body.url, &body.content);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// â”€â”€ Identity Routes â”€â”€

pub async fn identity_create(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let proof = state.identity.create_identity();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(proof)))
}

pub async fn identity_issue_challenge(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<IdentityChallengeRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let response = build_identity_challenge(&mut state, body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

pub async fn identity_auth(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AuthRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let response = state.identity.authenticate(body.challenge.as_bytes());
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

pub async fn identity_session_auth(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<IdentitySessionAuthRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let response = complete_identity_session_auth(&mut state, body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

// â”€â”€ Compute Routes â”€â”€

pub async fn compute_submit(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<ComputeSubmitRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let task = ComputeTask {
        task_type: body.task_type.clone(),
        description: body.description.clone(),
        input_data: body.input_data.clone(),
        max_cost: body.max_cost.unwrap_or(1.0),
        timeout_ms: body.timeout_ms.unwrap_or(30000),
        min_confidence: 0.7,
    };
    let task_id = state.compute.submit_task(task);
    let result = state.compute.process_task(&task_id.0);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "task_id": task_id.0, "result": result,
    }))))
}

// â”€â”€ Render Routes â”€â”€

fn summarize_block(block: &crate::chain::block::Block, certified_height: u64) -> ChainBlockSummary {
    ChainBlockSummary {
        height: block.height,
        block_hash: block.block_hash.clone(),
        prev_hash: block.prev_hash.clone(),
        validator: block.validator.clone(),
        transaction_count: block.transactions.len(),
        timestamp: block.timestamp,
        certified: block.height <= certified_height,
    }
}

// â”€â”€ System Routes â”€â”€

pub async fn agents_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.agents.get_system_stats())))
}

pub async fn runtime_capabilities(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let response = RuntimeCapabilitiesResponse {
        generated_at: chrono::Utc::now().to_rfc3339(),
        agents: state.agents.capability_catalog(),
        tools: state.tool_registry.snapshot(),
        assistant: state.personal_assistant.capability_snapshot(),
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

pub async fn assistant_runtime_profile(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let response = AssistantRuntimeProfileResponse {
        generated_at: chrono::Utc::now().to_rfc3339(),
        assistant: state.personal_assistant.capability_snapshot(),
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[derive(Debug)]
struct VoiceBrainReply {
    response_text: String,
    route: String,
    agreement_score: f64,
    verification_passed: bool,
    orchestration: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VoiceInvocationDecision {
    activated: bool,
    invocation_required: bool,
    matched_wake_word: Option<String>,
    heard_text: String,
    command_text: Option<String>,
}

pub async fn assistant_voice_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(AssistantVoiceStatusResponse {
            generated_at: chrono::Utc::now().to_rfc3339(),
            assistant_codename: state.config.assistant.codename.clone(),
            invocation_required: state.config.assistant.invocation_required,
            wake_words: default_voice_wake_words(&state.config.assistant.codename),
            voice: state.voice.status(),
        })),
    )
}

pub async fn assistant_voice_warmup(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let voice = {
        let state = state.read().await;
        state.voice.clone()
    };
    let warmup = tokio::task::spawn_blocking(move || voice.warmup())
        .await
        .map_err(|_| AstraError::Internal("voice warmup task failed".into()))??;

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(AssistantVoiceWarmupResponse {
            generated_at: chrono::Utc::now().to_rfc3339(),
            warmup,
        })),
    )
}

pub async fn assistant_voice_transcribe(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AssistantVoiceTranscribeRequest>,
) -> Result<HttpResponse, AstraError> {
    let voice = {
        let state = state.read().await;
        state.voice.clone()
    };
    let request = body.into_inner();
    let runtime_request = VoiceRuntimeFileRequest {
        audio_path: request.audio_path,
        language_hint: request.language_hint,
        initial_prompt: request.initial_prompt,
    };
    let transcription =
        tokio::task::spawn_blocking(move || voice.transcribe_file(&runtime_request))
            .await
            .map_err(|_| AstraError::Internal("voice transcription task failed".into()))??;

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(AssistantVoiceTranscriptionResponse {
            generated_at: chrono::Utc::now().to_rfc3339(),
            transcription,
        })),
    )
}

pub async fn assistant_voice_speak(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AssistantVoiceSpeakRequest>,
) -> Result<HttpResponse, AstraError> {
    let voice = {
        let state = state.read().await;
        state.voice.clone()
    };
    let request = body.into_inner();
    let speech_style = request
        .speech_style
        .as_deref()
        .map(VoiceSpeechStyle::parse)
        .transpose()?;
    let runtime_request = VoiceRuntimeSpeakRequest {
        text: request.text,
        voice_hint: request.voice_hint,
        rate: request.rate,
        volume: request.volume,
        speech_style,
    };
    let spoken_text = voice.render_speech_text(&runtime_request);
    let speech = tokio::task::spawn_blocking(move || voice.speak(&runtime_request))
        .await
        .map_err(|_| AstraError::Internal("voice speech task failed".into()))??;

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(AssistantVoiceSpeakResponse {
            generated_at: chrono::Utc::now().to_rfc3339(),
            spoken_text,
            speech,
        })),
    )
}

pub async fn assistant_voice_listen(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AssistantVoiceListenRequest>,
) -> Result<HttpResponse, AstraError> {
    let (voice, assistant_codename, config_invocation_required) = {
        let state = state.read().await;
        (
            state.voice.clone(),
            state.config.assistant.codename.clone(),
            state.config.assistant.invocation_required,
        )
    };
    let request = body.into_inner();
    let runtime_request = VoiceRuntimeListenRequest {
        duration_seconds: request.duration_seconds.unwrap_or(6),
        language_hint: request.language_hint.clone(),
        initial_prompt: request.initial_prompt.clone(),
        keep_audio: request.keep_audio.unwrap_or(false),
    };
    let transcription = tokio::task::spawn_blocking(move || voice.listen(&runtime_request))
        .await
        .map_err(|_| AstraError::Internal("voice listening task failed".into()))??;

    if transcription.transcript.trim().is_empty() {
        return Err(AstraError::ReasoningFailed(
            "voice transcription returned an empty transcript".into(),
        ));
    }

    let invocation = resolve_voice_invocation(
        &transcription.transcript,
        &assistant_codename,
        config_invocation_required,
        request.require_invocation,
        request.wake_words.as_deref(),
    );
    let respond = request.respond.unwrap_or(true);
    let auto_speak = request.auto_speak.unwrap_or(respond);
    let speech_style = request
        .speech_style
        .as_deref()
        .map(VoiceSpeechStyle::parse)
        .transpose()?;
    let mut response_text = None;
    let mut route = None;
    let mut agreement_score = None;
    let mut verification_passed = None;
    let mut orchestration = serde_json::json!({});
    let mut speech = None;
    let mut speech_text = None;

    if respond && invocation.activated {
        let command_text = invocation
            .command_text
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string();
        let mut context = request.context.unwrap_or_else(|| serde_json::json!({}));
        if !context.is_object() {
            context = serde_json::json!({ "voice_context": context });
        }
        if let Some(context_obj) = context.as_object_mut() {
            context_obj.insert(
                "assistant_codename".into(),
                serde_json::json!(assistant_codename.clone()),
            );
            context_obj.insert("input_mode".into(), serde_json::json!("voice"));
            context_obj.insert(
                "voice_language".into(),
                serde_json::json!(transcription.language.clone()),
            );
            context_obj.insert(
                "voice_duration_seconds".into(),
                serde_json::json!(transcription.duration_seconds),
            );
            context_obj.insert(
                "voice_audio_path".into(),
                serde_json::json!(transcription.audio_path.clone()),
            );
            context_obj.insert(
                "voice_heard_text".into(),
                serde_json::json!(invocation.heard_text.clone()),
            );
            context_obj.insert(
                "voice_activated".into(),
                serde_json::json!(invocation.activated),
            );
            context_obj.insert(
                "voice_invocation_required".into(),
                serde_json::json!(invocation.invocation_required),
            );
            context_obj.insert(
                "voice_matched_wake_word".into(),
                serde_json::json!(invocation.matched_wake_word.clone()),
            );
            context_obj.insert(
                "voice_command_text".into(),
                serde_json::json!(invocation.command_text.clone()),
            );
        }

        if command_text.is_empty() {
            let acknowledgement =
                format!("{assistant_codename} is awake. Sollunga, what do you need?");
            response_text = Some(acknowledgement.clone());
            route = Some("wake_word_ack".into());
            agreement_score = Some(1.0);
            verification_passed = Some(true);
            orchestration = serde_json::json!({
                "mode": "wake_word_ack",
                "assistant_codename": assistant_codename,
                "matched_wake_word": invocation.matched_wake_word,
            });
        } else {
            let reply = execute_voice_brain_turn(&state, &command_text, context).await?;
            response_text = Some(reply.response_text.clone());
            route = Some(reply.route);
            agreement_score = Some(reply.agreement_score);
            verification_passed = Some(reply.verification_passed);
            orchestration = reply.orchestration;
        }

        let voice = {
            let state = state.read().await;
            state.voice.clone()
        };
        let speech_request = VoiceRuntimeSpeakRequest {
            text: response_text.clone().unwrap_or_default(),
            voice_hint: None,
            rate: None,
            volume: None,
            speech_style: speech_style.clone(),
        };
        speech_text = Some(voice.render_speech_text(&speech_request));

        if auto_speak {
            speech = Some(
                tokio::task::spawn_blocking(move || voice.speak(&speech_request))
                    .await
                    .map_err(|_| AstraError::Internal("voice reply speech task failed".into()))??,
            );
        }
    } else if !invocation.activated {
        route = Some("dormant".into());
        orchestration = serde_json::json!({
            "mode": "dormant",
            "assistant_codename": assistant_codename,
            "invocation_required": invocation.invocation_required,
            "wake_words": default_voice_wake_words(&assistant_codename),
        });
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(AssistantVoiceTurnResponse {
            generated_at: chrono::Utc::now().to_rfc3339(),
            heard_text: invocation.heard_text.clone(),
            command_text: invocation.command_text.clone(),
            activated: invocation.activated,
            invocation_required: invocation.invocation_required,
            matched_wake_word: invocation.matched_wake_word.clone(),
            transcription,
            response_text,
            speech_text,
            route,
            agreement_score,
            verification_passed,
            orchestration,
            speech,
        })),
    )
}

fn default_voice_wake_words(codename: &str) -> Vec<String> {
    let trimmed = codename.trim();
    if trimmed.is_empty() {
        return vec!["assistant".into()];
    }

    vec![
        trimmed.to_string(),
        format!("wake up {trimmed}"),
        format!("hey {trimmed}"),
        format!("ok {trimmed}"),
        format!("okay {trimmed}"),
    ]
}

fn normalize_voice_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            normalized.extend(ch.to_lowercase());
        } else {
            normalized.push(' ');
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn resolve_voice_invocation(
    transcript: &str,
    codename: &str,
    config_invocation_required: bool,
    request_override: Option<bool>,
    custom_wake_words: Option<&[String]>,
) -> VoiceInvocationDecision {
    let heard_text = transcript.trim().to_string();
    let normalized_transcript = normalize_voice_text(&heard_text);
    let invocation_required = request_override.unwrap_or(config_invocation_required);
    if normalized_transcript.is_empty() {
        return VoiceInvocationDecision {
            activated: false,
            invocation_required,
            matched_wake_word: None,
            heard_text,
            command_text: None,
        };
    }

    if !invocation_required {
        return VoiceInvocationDecision {
            activated: true,
            invocation_required,
            matched_wake_word: None,
            heard_text,
            command_text: Some(normalized_transcript),
        };
    }

    let wake_words = custom_wake_words
        .filter(|words| !words.is_empty())
        .map(|words| words.to_vec())
        .unwrap_or_else(|| default_voice_wake_words(codename));
    for wake_word in wake_words {
        let normalized_wake = normalize_voice_text(&wake_word);
        if normalized_wake.is_empty() {
            continue;
        }
        if normalized_transcript == normalized_wake {
            return VoiceInvocationDecision {
                activated: true,
                invocation_required,
                matched_wake_word: Some(normalized_wake),
                heard_text,
                command_text: None,
            };
        }

        let prefixed = format!("{normalized_wake} ");
        if let Some(command) = normalized_transcript.strip_prefix(&prefixed) {
            let command = command.trim();
            return VoiceInvocationDecision {
                activated: true,
                invocation_required,
                matched_wake_word: Some(normalized_wake),
                heard_text,
                command_text: if command.is_empty() {
                    None
                } else {
                    Some(command.to_string())
                },
            };
        }
    }

    VoiceInvocationDecision {
        activated: false,
        invocation_required,
        matched_wake_word: None,
        heard_text,
        command_text: None,
    }
}

async fn execute_voice_brain_turn(
    state: &web::Data<Arc<RwLock<EngineState>>>,
    problem: &str,
    context: serde_json::Value,
) -> Result<VoiceBrainReply, AstraError> {
    let mut state = state.write().await;
    let artifacts = state.execute_brain(
        BrainExecuteRequest {
            task: problem.to_string(),
            context,
            session_id: None,
            mode_hint: Some(BrainModeHint::Respond),
            allow_mutation: false,
            require_live_retrieval: false,
            require_verification: true,
        },
        None,
    )?;
    let route = artifacts
        .response
        .orchestration
        .get("advisors")
        .and_then(|value| value.get("route"))
        .and_then(|value| value.get("selected_node"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("thinking_lane")
        .to_string();
    let agreement_score = artifacts
        .response
        .orchestration
        .get("advisors")
        .and_then(|value| value.get("council"))
        .and_then(|value| value.get("agreement_score"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    state.assistant_executive.record_compatibility_completion(
        "/api/assistant/voice/listen",
        problem,
        MissionMode::Voice,
        artifacts.response.answer.clone(),
        serde_json::json!({
            "route": route.clone(),
            "agreement_score": agreement_score,
            "verification_passed": artifacts.response.verification.passed,
        }),
    );

    Ok(VoiceBrainReply {
        response_text: artifacts.response.answer,
        route,
        agreement_score,
        verification_passed: artifacts.response.verification.passed,
        orchestration: artifacts.response.orchestration,
    })
}

pub async fn assistant_autonomy_snapshot(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(AssistantAutonomySnapshotResponse {
            generated_at: chrono::Utc::now().to_rfc3339(),
            autonomy: state.assistant_autonomy.snapshot(),
        })),
    )
}

pub async fn assistant_create_custom_agent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AssistantCustomAgentRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let request = body.into_inner();
    let lifecycle = request.lifecycle.unwrap_or(LifecycleMode::Ephemeral);
    let created_at = chrono::Utc::now().to_rfc3339();
    let agent = CustomAssistantAgent {
        agent_id: request.agent_id.clone(),
        display_name: request.display_name.clone(),
        purpose: request.purpose.clone(),
        capabilities: request.capabilities.clone(),
        preferred_tools: request.preferred_tools.clone(),
        lifecycle,
        created_at,
    };

    state
        .agents
        .register_custom_profile(AgentCapabilityProfile {
            agent_id: request.agent_id,
            display_name: request.display_name,
            agent_type: "assistant_custom".into(),
            specialization: request.purpose.clone(),
            description: format!(
                "Mission-scoped custom specialist for {}.",
                request.purpose.to_lowercase()
            ),
            capabilities: request.capabilities,
            preferred_tools: request.preferred_tools,
            safety_lane: "operator_review".into(),
            latency_tier: "adaptive".into(),
            max_concurrency: if matches!(lifecycle, LifecycleMode::Ephemeral) {
                1
            } else {
                2
            },
            reliability_score: 0.72,
        });
    state.assistant_autonomy.upsert_agent(agent.clone());
    let profile = state
        .agents
        .capability_catalog()
        .into_iter()
        .find(|profile| profile.agent_id == agent.agent_id)
        .ok_or_else(|| {
            AstraError::Internal("custom agent profile missing after registration".into())
        })?;
    let manifest = state
        .fleet
        .register_agent_manifest(&agent, &profile, request.metadata);

    Ok(
        HttpResponse::Created().json(ApiResponse::ok(AssistantCustomAgentResponse {
            agent,
            manifest,
        })),
    )
}

pub async fn assistant_delete_custom_agent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let agent_id = path.into_inner();
    let removed = state
        .agents
        .remove_custom_profile(&agent_id)
        .map_err(AstraError::ControlPlaneRejected)?;
    let autonomy_removed = state.assistant_autonomy.remove_agent(&agent_id);
    let fleet_removed = state.fleet.remove_agent_manifest(&agent_id);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "removed": removed || autonomy_removed || fleet_removed,
        "agent_id": agent_id
    }))))
}

pub async fn assistant_create_custom_tool(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AssistantCustomToolRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let request = body.into_inner();
    let lifecycle = request.lifecycle.unwrap_or(LifecycleMode::Ephemeral);
    let created_at = chrono::Utc::now().to_rfc3339();
    let descriptor = ToolDescriptor {
        name: request.name.clone(),
        description: request.description.clone(),
        category: request.category.clone(),
        risk_level: request.risk_level.clone(),
        requires_approval: request.requires_approval,
        max_calls_per_minute: request.max_calls_per_minute.unwrap_or(12),
        timeout_secs: request.timeout_secs.unwrap_or(30),
    };
    state
        .tool_registry
        .register_custom_descriptor(descriptor.clone());
    let tool = CustomAssistantTool {
        name: request.name,
        description: request.description,
        category: request.category,
        risk_level: request.risk_level,
        requires_approval: request.requires_approval,
        lifecycle,
        created_at,
    };
    state.assistant_autonomy.upsert_tool(tool.clone());
    let manifest = state
        .fleet
        .register_tool_manifest(&tool, &descriptor, request.metadata);

    Ok(
        HttpResponse::Created().json(ApiResponse::ok(AssistantCustomToolResponse {
            tool,
            descriptor,
            manifest,
        })),
    )
}

pub async fn assistant_delete_custom_tool(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let tool_name = path.into_inner();
    let removed = state
        .tool_registry
        .remove_custom_tool(&tool_name)
        .map_err(AstraError::ControlPlaneRejected)?;
    let autonomy_removed = state.assistant_autonomy.remove_tool(&tool_name);
    let fleet_removed = state.fleet.remove_tool_manifest(&tool_name);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "removed": removed || autonomy_removed || fleet_removed,
        "tool_name": tool_name
    }))))
}

pub async fn assistant_submit_self_improvement_proposal(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AssistantSelfImprovementProposalRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let request = body.into_inner();
    let proposal = state.assistant_autonomy.submit_self_improvement(
        request.objective,
        request.scope,
        request.implementation_outline,
        request.safeguards,
    );

    Ok(
        HttpResponse::Created().json(ApiResponse::ok(AssistantSelfImprovementProposalResponse {
            proposal,
        })),
    )
}

fn ensure_fleet_policy_contract(
    state: &mut EngineState,
    policy_id: &str,
) -> AstraResult<Option<String>> {
    let policy = state
        .fleet
        .acquisition_policies()
        .policies
        .into_iter()
        .find(|policy| policy.policy_id == policy_id)
        .ok_or_else(|| AstraError::SessionNotFound(format!("fleet policy '{policy_id}'")))?;
    if let Some(contract_id) = policy.contract_id.clone() {
        return Ok(Some(contract_id));
    }

    let contract = state.chain.deploy_contract(AIContractDeployRequest {
        owner: "assistant:fleet".into(),
        name: policy.policy_id.clone(),
        purpose: format!("promotion guardrails for {}", policy.name),
        allowed_domains: policy.allowed_lanes.clone(),
        allowed_callers: policy.allowed_roles.clone(),
        allowed_tools: policy.allowed_tools.clone(),
        max_budget: policy.budget_ceiling_usd,
        max_compute_units: (policy.max_parallel_campaigns.max(1) as u64) * 1024,
        min_confidence: policy.promotion_min_score,
        max_risk: (1.0 - policy.promotion_min_score).clamp(0.0, 1.0),
        review_threshold: policy.promotion_min_canary_score,
        minimum_evidence: 2,
        require_trace_binding: true,
        prompt_template: Some(
            "Approve only when candidate score, canary evidence, and rollback invariants pass."
                .into(),
        ),
        invariants: vec![
            "require_chain_attestation_for_high_risk".into(),
            "block_direct_replacement_without_rollback_target".into(),
            "respect_budget_ceiling".into(),
        ],
        vm_program: vec![
            ContractVmInstruction::AccumulateInvocationCount {
                state_key: "fleet.promotions".into(),
            },
            ContractVmInstruction::AccumulateEstimatedCost {
                state_key: "fleet.budget.units".into(),
            },
        ],
        metadata: serde_json::json!({
            "policy_id": policy.policy_id,
            "deployment_scope": policy.deployment_scope,
            "kill_switch_enabled": policy.kill_switch_enabled,
        }),
        active: policy.active,
    })?;
    state
        .fleet
        .bind_policy_contract(policy_id, contract.contract_id.clone())?;
    Ok(Some(contract.contract_id))
}

fn record_fleet_chain_receipt(
    state: &mut EngineState,
    subject_id: &str,
    subject_kind: &str,
    attestation_kind: &str,
    status: &str,
    trace_id: &str,
    payload_summary: serde_json::Value,
    chain_tx_hash: Option<String>,
    contract_id: Option<String>,
    contract_receipt_id: Option<String>,
) -> ChainAttestationReceipt {
    let receipt = ChainAttestationReceipt {
        receipt_id: format!("chain_{}", &uuid::Uuid::new_v4().to_string()[..12]),
        subject_id: subject_id.into(),
        subject_kind: subject_kind.into(),
        attestation_kind: attestation_kind.into(),
        status: status.into(),
        trace_id: trace_id.into(),
        chain_tx_hash,
        contract_id,
        contract_receipt_id,
        recorded_at: chrono::Utc::now().to_rfc3339(),
        payload_summary,
    };
    state.fleet.record_chain_receipt(receipt.clone());
    receipt
}

pub async fn assistant_fleet_snapshot(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.fleet.snapshot())))
}

pub async fn assistant_create_fleet_campaign(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<FleetCampaignRequest>,
) -> Result<HttpResponse, AstraError> {
    let payload = {
        let mut state = state.write().await;
        let autonomy = state.assistant_autonomy.snapshot();
        let agents = state.agents.capability_catalog();
        let tools = state.tool_registry.catalog();
        let runtime_profile = state.personal_assistant.profile().clone();
        let campaign = state.fleet.create_campaign(
            body.into_inner(),
            &autonomy,
            &agents,
            &tools,
            &runtime_profile,
        )?;

        if campaign.chain_required {
            let submission = state.chain.record_httpa_intent(
                "fleet",
                "campaign",
                &campaign.lane,
                "planned",
                &campaign.httpa.trace_id,
                serde_json::json!({
                    "campaign_id": campaign.campaign_id,
                    "title": campaign.title,
                    "policy_id": campaign.promotion_policy_id,
                }),
            )?;
            record_fleet_chain_receipt(
                &mut state,
                &campaign.campaign_id,
                "campaign",
                "campaign",
                "recorded",
                &campaign.httpa.trace_id,
                serde_json::json!({
                    "lane": campaign.lane,
                    "summary": campaign.summary,
                }),
                Some(submission.tx_hash),
                None,
                None,
            );
        }
        campaign
    };

    Ok(HttpResponse::Created().json(ApiResponse::ok(payload)))
}

pub async fn assistant_get_fleet_campaign(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let campaign_id = path.into_inner();
    let state = state.read().await;
    let campaign = state
        .fleet
        .campaign(&campaign_id)
        .ok_or_else(|| AstraError::SessionNotFound(format!("fleet campaign '{campaign_id}'")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(campaign)))
}

pub async fn assistant_fleet_rankings(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.fleet.rankings())))
}

pub async fn assistant_fleet_league(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.fleet.league())))
}

pub async fn assistant_evaluate_custom_agent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
    body: web::Json<FleetEvaluationRequest>,
) -> Result<HttpResponse, AstraError> {
    let payload = {
        let mut state = state.write().await;
        let agent_id = path.into_inner();
        let agent_catalog = state.agents.capability_catalog();
        let review = state
            .fleet
            .evaluate_agent(&agent_id, body.into_inner(), &agent_catalog)?;
        review
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_evaluate_custom_tool(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
    body: web::Json<FleetEvaluationRequest>,
) -> Result<HttpResponse, AstraError> {
    let payload = {
        let mut state = state.write().await;
        let tool_name = path.into_inner();
        let tool_catalog = state.tool_registry.catalog();
        let review = state
            .fleet
            .evaluate_tool(&tool_name, body.into_inner(), &tool_catalog)?;
        review
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_promote_fleet_candidate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let payload = {
        let mut state = state.write().await;
        let candidate_id = path.into_inner();
        let mut decision = state.fleet.promote_candidate(&candidate_id)?;
        let trace_id = format!(
            "fleet-promotion-{}",
            &uuid::Uuid::new_v4().to_string()[..10]
        );
        let active_policy_id = state.fleet.snapshot().active_policy_id;
        let contract_id = ensure_fleet_policy_contract(&mut state, &active_policy_id)?;
        let submission = state.chain.record_httpa_intent(
            "fleet",
            "promotion",
            "production",
            decision.action.as_str(),
            &trace_id,
            serde_json::json!({
                "decision_id": decision.decision_id,
                "candidate_id": decision.candidate_id,
                "action": decision.action.as_str(),
                "stage": decision.stage,
            }),
        )?;
        let contract_receipt = if let Some(contract_id) = contract_id.clone() {
            Some(
                state.chain.invoke_contract(AIContractInvokeRequest {
                    contract_id,
                    caller: "reviewer".into(),
                    action: decision.action.as_str().into(),
                    domain: "production".into(),
                    requested_tools: vec!["risk_review".into()],
                    estimated_cost: 1.0,
                    confidence: decision.gate.current_score,
                    risk_score: (1.0 - decision.gate.current_canary_score).clamp(0.0, 1.0),
                    input_digest: sha3_256_hex(
                        serde_json::to_string(&decision)
                            .unwrap_or_else(|_| decision.decision_id.clone())
                            .as_bytes(),
                    ),
                    trace_id: Some(trace_id.clone()),
                    session_id: Some(decision.decision_id.clone()),
                    evidence: vec![
                        format!("score:{:.2}", decision.gate.current_score),
                        format!("canary:{:.2}", decision.gate.current_canary_score),
                    ],
                    context: BTreeMap::from([
                        ("candidate_id".into(), decision.candidate_id.clone()),
                        ("asset_kind".into(), decision.asset_kind.as_str().into()),
                    ]),
                    metadata: serde_json::json!({
                        "gate": decision.gate,
                    }),
                })?,
            )
        } else {
            None
        };
        let receipt = record_fleet_chain_receipt(
            &mut state,
            &decision.decision_id,
            "promotion",
            "promotion",
            "recorded",
            &trace_id,
            serde_json::json!({
                "candidate_id": decision.candidate_id,
                "action": decision.action.as_str(),
            }),
            Some(submission.tx_hash),
            contract_id,
            contract_receipt
                .as_ref()
                .map(|receipt| receipt.receipt_id.clone()),
        );
        decision.chain_receipt_id = Some(receipt.receipt_id);
        decision
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_rollback_fleet_candidate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let payload = {
        let mut state = state.write().await;
        let deployment_id = path.into_inner();
        let mut rollback = state.fleet.rollback_candidate(&deployment_id)?;
        let trace_id = format!("fleet-rollback-{}", &uuid::Uuid::new_v4().to_string()[..10]);
        let submission = state.chain.record_httpa_intent(
            "fleet",
            "rollback",
            "production",
            "completed",
            &trace_id,
            serde_json::json!({
                "rollback_id": rollback.rollback_id,
                "deployment_id": rollback.deployment_id,
                "candidate_id": rollback.candidate_id,
            }),
        )?;
        let receipt = record_fleet_chain_receipt(
            &mut state,
            &rollback.rollback_id,
            "rollback",
            "rollback",
            "recorded",
            &trace_id,
            serde_json::json!({
                "candidate_id": rollback.candidate_id,
                "restored_target": rollback.restored_target,
            }),
            Some(submission.tx_hash),
            None,
            None,
        );
        rollback.chain_receipt_id = Some(receipt.receipt_id);
        rollback
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_list_httpa_execution_policies(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.fleet.httpa_policies())))
}

pub async fn assistant_upsert_httpa_execution_policy(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<HttpaExecutionPolicyRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let policy = state.fleet.upsert_httpa_policy(body.into_inner());
    state.assistant_executive.record_external_audit(
        "fleet",
        "httpa_policy",
        &policy.policy_id,
        "httpa_policy_upserted",
        &format!("Updated HTTPA execution policy '{}'", policy.name),
        serde_json::json!({
            "execution_mode": policy.execution_mode,
            "transport_tier": policy.transport_tier,
        }),
    );
    Ok(HttpResponse::Created().json(ApiResponse::ok(policy)))
}

pub async fn assistant_list_acquisition_policies(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.fleet.acquisition_policies())))
}

pub async fn assistant_upsert_acquisition_policy(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<FleetPolicyRequest>,
) -> Result<HttpResponse, AstraError> {
    let payload = {
        let mut state = state.write().await;
        let policy = state.fleet.upsert_acquisition_policy(body.into_inner());
        let policy_id = policy.policy_id.clone();
        let contract_id = ensure_fleet_policy_contract(&mut state, &policy_id)?;
        let trace_id = format!("fleet-policy-{}", &uuid::Uuid::new_v4().to_string()[..10]);
        let submission = state.chain.record_httpa_intent(
            "fleet",
            "policy",
            "governance",
            "updated",
            &trace_id,
            serde_json::json!({
                "policy_id": policy_id,
                "contract_id": contract_id,
            }),
        )?;
        record_fleet_chain_receipt(
            &mut state,
            &policy_id,
            "fleet_policy",
            "policy_update",
            "recorded",
            &trace_id,
            serde_json::json!({
                "name": policy.name,
                "require_chain_attestation": policy.require_chain_attestation,
            }),
            Some(submission.tx_hash),
            contract_id,
            None,
        );
        state.assistant_executive.record_external_audit(
            "fleet",
            "fleet_policy",
            &policy.policy_id,
            "fleet_policy_upserted",
            &format!("Updated fleet policy '{}'", policy.name),
            serde_json::json!({
                "allowed_lanes": policy.allowed_lanes,
                "budget_ceiling_usd": policy.budget_ceiling_usd,
            }),
        );
        state
            .fleet
            .acquisition_policies()
            .policies
            .into_iter()
            .find(|candidate| candidate.policy_id == policy_id)
            .unwrap_or(policy)
    };

    Ok(HttpResponse::Created().json(ApiResponse::ok(payload)))
}

pub async fn assistant_list_onion_sources(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.fleet.onion_sources())))
}

pub async fn assistant_upsert_onion_source(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<OnionSourcePolicyRequest>,
) -> Result<HttpResponse, AstraError> {
    let payload = {
        let mut state = state.write().await;
        let source = state.fleet.upsert_onion_source(body.into_inner())?;
        let trace_id = format!("fleet-onion-{}", &uuid::Uuid::new_v4().to_string()[..10]);
        let submission = state.chain.record_httpa_intent(
            "fleet",
            "onion_source",
            "allowlisted_onion",
            if source.enabled {
                "enabled"
            } else {
                "disabled"
            },
            &trace_id,
            serde_json::json!({
                "source_id": source.source_id,
                "purpose": source.purpose,
                "approved_workflows": source.approved_workflows,
            }),
        )?;
        if source.requires_chain_attestation {
            record_fleet_chain_receipt(
                &mut state,
                &source.source_id,
                "onion_source",
                "onion_source",
                "recorded",
                &trace_id,
                serde_json::json!({
                    "source_label": source.source_label,
                    "address": source.onion_address,
                }),
                Some(submission.tx_hash),
                None,
                None,
            );
        }
        state.assistant_executive.record_external_audit(
            "fleet",
            "onion_source",
            &source.source_id,
            "onion_source_upserted",
            &format!("Updated allowlisted onion source '{}'", source.source_label),
            serde_json::json!({
                "enabled": source.enabled,
                "max_requests_per_hour": source.max_requests_per_hour,
            }),
        );
        source
    };

    Ok(HttpResponse::Created().json(ApiResponse::ok(payload)))
}

pub async fn assistant_chain_receipts(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let mut receipts = state.fleet.chain_receipts().receipts;
    receipts.extend(
        state
            .chain
            .list_recent_contract_receipts(12)
            .into_iter()
            .map(|receipt| ChainAttestationReceipt {
                receipt_id: receipt.receipt_id.clone(),
                subject_id: receipt.contract_id.clone(),
                subject_kind: "contract".into(),
                attestation_kind: "contract_receipt".into(),
                status: format!("{:?}", receipt.decision).to_lowercase(),
                trace_id: receipt.trace_id.unwrap_or_else(|| "n/a".into()),
                chain_tx_hash: None,
                contract_id: Some(receipt.contract_id),
                contract_receipt_id: Some(receipt.receipt_id),
                recorded_at: chrono::DateTime::from_timestamp_millis(receipt.created_at)
                    .map(|timestamp| timestamp.to_rfc3339())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                payload_summary: serde_json::json!({
                    "caller": receipt.caller,
                    "action": receipt.action,
                    "reasons": receipt.reasons,
                }),
            }),
    );
    receipts.extend(
        state
            .chain
            .list_recent_resource_commitments(12)
            .into_iter()
            .map(|commitment| ChainAttestationReceipt {
                receipt_id: commitment.commitment_id.clone(),
                subject_id: commitment.commitment_id,
                subject_kind: "resource_commitment".into(),
                attestation_kind: "resource_commitment".into(),
                status: "recorded".into(),
                trace_id: commitment
                    .metadata
                    .get("trace_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("n/a")
                    .into(),
                chain_tx_hash: commitment.transaction_hash,
                contract_id: None,
                contract_receipt_id: None,
                recorded_at: chrono::DateTime::from_timestamp_millis(commitment.created_at)
                    .map(|timestamp| timestamp.to_rfc3339())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                payload_summary: serde_json::json!({
                    "url": commitment.url,
                    "source": commitment.source,
                    "content_type": commitment.content_type,
                }),
            }),
    );
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(ChainAttestationCollectionResponse {
            receipts,
        })),
    )
}

fn mission_mode_to_brain_hint(mode: MissionMode) -> BrainModeHint {
    match mode {
        MissionMode::Research | MissionMode::Search => BrainModeHint::Reason,
        MissionMode::Build => BrainModeHint::Mission,
        MissionMode::Voice
        | MissionMode::Assistant
        | MissionMode::Compatibility
        | MissionMode::LocalOperator
        | MissionMode::Security => BrainModeHint::Respond,
    }
}

fn compatibility_mode_for_brain_request(request: &BrainExecuteRequest) -> MissionMode {
    if request.allow_mutation {
        MissionMode::Build
    } else if request.require_live_retrieval {
        MissionMode::Search
    } else {
        match request.mode_hint {
            Some(BrainModeHint::Mission) | Some(BrainModeHint::Plan) => MissionMode::Build,
            Some(BrainModeHint::Reason) => MissionMode::Research,
            _ => MissionMode::Assistant,
        }
    }
}

fn evidence_bundle_from_brain_response(response: &BrainExecuteResponse) -> Option<EvidenceBundle> {
    if response.evidence.is_empty() {
        return None;
    }

    Some(EvidenceBundle {
        citations: response
            .evidence
            .iter()
            .map(|item| EvidenceCitation {
                source: item.source.clone(),
                kind: item.kind.clone(),
                summary: item.summary.clone(),
                confidence: item.confidence,
            })
            .collect(),
        claim_summary: serde_json::json!({
            "answer": response.answer.clone(),
            "verification": response.verification.clone(),
            "plan": response.plan.clone(),
        }),
    })
}

fn build_execution_receipts(
    mission: &Mission,
    response: &BrainExecuteResponse,
) -> Vec<ExecutionReceipt> {
    vec![ExecutionReceipt {
        receipt_id: format!(
            "receipt-{}",
            uuid::Uuid::new_v4().to_string()[..12].to_string()
        ),
        kind: "brain_execution".into(),
        summary: format!(
            "Mission '{}' executed via {}",
            mission.title, mission.provider_route.primary_provider
        ),
        status: if response.verification.passed {
            "verified".into()
        } else {
            "completed_with_warnings".into()
        },
        created_at: chrono::Utc::now().to_rfc3339(),
        metadata: serde_json::json!({
            "session_id": response.session_id.clone(),
            "stage": response.stage,
            "verification_passed": response.verification.passed,
            "tool_calls": response.tool_journal.len(),
            "lane": mission.provider_route.lane.clone(),
        }),
    }]
}

enum MissionExecutionOutcome {
    Brain(BrainExecuteResponse),
    Local(serde_json::Value),
}

fn execute_assistant_mission(
    state: &mut EngineState,
    mission_id: &str,
) -> AstraResult<(Mission, MissionExecutionOutcome)> {
    let mission = state
        .assistant_executive
        .mission(mission_id)
        .ok_or_else(|| AstraError::SessionNotFound(format!("mission '{mission_id}'")))?;
    if mission.status != MissionStatus::Queued {
        return Err(AstraError::ControlPlaneRejected(format!(
            "mission must be queued before execution; current status is {}",
            mission.status.as_str()
        )));
    }

    state.assistant_executive.mark_mission_running(
        mission_id,
        serde_json::json!({
            "route": mission.route.clone(),
            "mode": mission.mode,
            "provider_route": mission.provider_route.clone(),
        }),
    );

    if let Some(operation) = mission.input.local_operation.clone() {
        match state
            .local_operator
            .execute_operation(&mission, &operation, &mut state.defender)
        {
            Ok(execution) => {
                let payload = execution.payload.clone();
                let mission = state.assistant_executive.complete_mission(
                    mission_id,
                    execution.summary.clone(),
                    execution.receipts,
                    execution.evidence,
                    serde_json::json!({
                        "route": mission.route.clone(),
                        "mode": mission.mode,
                        "action": operation.action,
                        "operator_mode": mission.input.operator_mode,
                        "payload": payload.clone(),
                    }),
                )?;
                return Ok((mission, MissionExecutionOutcome::Local(payload)));
            }
            Err(error) => {
                let reason = error.to_string();
                let _ = state.assistant_executive.fail_mission(
                    mission_id,
                    reason.clone(),
                    serde_json::json!({
                        "route": mission.route,
                        "mode": mission.mode,
                        "action": operation.action,
                        "error": reason,
                    }),
                );
                return Err(error);
            }
        }
    }

    let execution_request = BrainExecuteRequest {
        task: mission.input.task.clone(),
        context: mission.input.context.clone(),
        session_id: None,
        mode_hint: Some(mission_mode_to_brain_hint(mission.mode)),
        allow_mutation: mission.input.allow_mutation,
        require_live_retrieval: mission.input.require_live_retrieval,
        require_verification: mission.input.require_verification,
    };

    match state.execute_brain(execution_request, None) {
        Ok(artifacts) => {
            let response = artifacts.response.clone();
            let mission = state.assistant_executive.complete_mission(
                mission_id,
                if response.answer.trim().is_empty() {
                    format!(
                        "Mission '{}' completed without a textual summary",
                        mission.title
                    )
                } else {
                    response.answer.clone()
                },
                build_execution_receipts(&mission, &response),
                evidence_bundle_from_brain_response(&response),
                serde_json::json!({
                    "session_id": response.session_id.clone(),
                    "stage": response.stage,
                    "verification_passed": response.verification.passed,
                    "route": mission.route.clone(),
                }),
            )?;
            Ok((mission, MissionExecutionOutcome::Brain(response)))
        }
        Err(error) => {
            let reason = error.to_string();
            let _ = state.assistant_executive.fail_mission(
                mission_id,
                reason.clone(),
                serde_json::json!({
                    "route": mission.route,
                    "mode": mission.mode,
                    "error": reason,
                }),
            );
            Err(error)
        }
    }
}

fn default_local_task(action: LocalActionKind, provided_task: Option<String>) -> String {
    if let Some(task) = provided_task {
        if !task.trim().is_empty() {
            return task;
        }
    }
    match action {
        LocalActionKind::OpenResource => "Open a local resource".into(),
        LocalActionKind::OpenApp => "Launch a local application".into(),
        LocalActionKind::BrowserTask => "Open a browser task".into(),
        LocalActionKind::DownloadStage => "Stage a software download for trust review".into(),
        LocalActionKind::InstallSoftware => "Install a staged software package".into(),
        LocalActionKind::UpdateSoftware => "Update an installed software package".into(),
        LocalActionKind::SecurityScan => "Run a defensive malware analysis scan".into(),
        LocalActionKind::ReverseEngineerSample => {
            "Reverse engineer a local sample defensively".into()
        }
        LocalActionKind::LabExecute => "Launch a disposable sandbox lab session".into(),
    }
}

fn build_local_operator_mission_request(
    task: Option<String>,
    operator_mode: Option<OperatorMode>,
    action: LocalActionKind,
    resource: Option<LocalResourceRef>,
    app: Option<AppLaunchSpec>,
    source: Option<DownloadSource>,
    artifact_id: Option<String>,
    target_path: Option<String>,
    sample_path: Option<String>,
    target_id: Option<String>,
    install_arguments: Vec<String>,
    metadata: serde_json::Value,
) -> MissionExecutionRequest {
    let allow_mutation = matches!(
        action,
        LocalActionKind::DownloadStage
            | LocalActionKind::InstallSoftware
            | LocalActionKind::UpdateSoftware
            | LocalActionKind::LabExecute
    );
    let mode = match action {
        LocalActionKind::SecurityScan
        | LocalActionKind::ReverseEngineerSample
        | LocalActionKind::LabExecute => MissionMode::Security,
        _ => MissionMode::LocalOperator,
    };

    MissionExecutionRequest {
        task: default_local_task(action, task),
        context: serde_json::json!({
            "surface": "local_operator_api",
            "operator_mode": operator_mode,
        }),
        mode: Some(mode),
        operator_mode,
        local_operation: Some(LocalOperationRequest {
            action,
            resource,
            app,
            source,
            artifact_id,
            target_path,
            sample_path,
            target_id,
            install_arguments,
            metadata,
        }),
        allow_mutation,
        require_live_retrieval: matches!(action, LocalActionKind::BrowserTask),
        require_verification: true,
        tags: vec!["local_operator".into(), action.as_str().into()],
        requester_role: Some(AssistantRole::Owner),
    }
}

fn plan_and_execute_local_request(
    state: &mut EngineState,
    request: MissionExecutionRequest,
    route: &str,
) -> AstraResult<serde_json::Value> {
    let mission = state.assistant_executive.plan_mission(request, route);
    if mission.status == MissionStatus::Queued {
        let (mission, outcome) = execute_assistant_mission(state, &mission.mission_id)?;
        let payload = match outcome {
            MissionExecutionOutcome::Brain(response) => serde_json::to_value(response)?,
            MissionExecutionOutcome::Local(operation) => operation,
        };
        Ok(serde_json::json!({
            "mission": mission,
            "operation": payload,
        }))
    } else {
        Ok(serde_json::json!({
            "mission": mission,
        }))
    }
}

pub async fn assistant_list_missions(
    state: web::Data<Arc<RwLock<EngineState>>>,
    query: web::Query<CollectionQuery>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let missions = state
        .assistant_executive
        .list_missions(query.limit.unwrap_or(DEFAULT_ASSISTANT_COLLECTION_LIMIT));
    Ok(HttpResponse::Ok().json(ApiResponse::ok(missions)))
}

pub async fn assistant_execute(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<MissionExecutionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let payload = {
        let mut state = state.write().await;
        let mission = state
            .assistant_executive
            .plan_mission(request, "/api/assistant/execute");
        if mission.status == MissionStatus::Queued {
            let (mission, outcome) = execute_assistant_mission(&mut state, &mission.mission_id)?;
            match outcome {
                MissionExecutionOutcome::Brain(response) => serde_json::json!({
                    "mission": mission,
                    "response": response,
                }),
                MissionExecutionOutcome::Local(operation) => serde_json::json!({
                    "mission": mission,
                    "operation": operation,
                }),
            }
        } else {
            serde_json::json!({
                "mission": mission,
            })
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_get_mission(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let mission_id = path.into_inner();
    let state = state.read().await;
    let mission = state
        .assistant_executive
        .mission(&mission_id)
        .ok_or_else(|| AstraError::SessionNotFound(format!("mission '{mission_id}'")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(mission)))
}

pub async fn assistant_resume_mission(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let mission_id = path.into_inner();
    let payload = {
        let mut state = state.write().await;
        state.assistant_executive.resume_mission(&mission_id)?;
        let (mission, outcome) = execute_assistant_mission(&mut state, &mission_id)?;
        match outcome {
            MissionExecutionOutcome::Brain(response) => serde_json::json!({
                "mission": mission,
                "response": response,
            }),
            MissionExecutionOutcome::Local(operation) => serde_json::json!({
                "mission": mission,
                "operation": operation,
            }),
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_cancel_mission(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let mission_id = path.into_inner();
    let mut state = state.write().await;
    let mission = state.assistant_executive.cancel_mission(&mission_id)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(mission)))
}

pub async fn assistant_mission_events(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let mission_id = path.into_inner();
    let state = state.read().await;
    let events = state
        .assistant_executive
        .mission_events(&mission_id)
        .ok_or_else(|| AstraError::SessionNotFound(format!("mission '{mission_id}'")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(events)))
}

pub async fn assistant_mission_stream(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let mission_id = path.into_inner();
    {
        let state = state.read().await;
        state
            .assistant_executive
            .mission(&mission_id)
            .ok_or_else(|| AstraError::SessionNotFound(format!("mission '{mission_id}'")))?;
    }

    let stream_state = state.clone();
    let stream = stream::unfold(
        (stream_state, mission_id, 0usize),
        |(state, mission_id, cursor)| async move {
            loop {
                let snapshot = {
                    let state = state.read().await;
                    state
                        .assistant_executive
                        .mission(&mission_id)
                        .map(|mission| (mission.events, mission.status))
                };

                let Some((events, status)) = snapshot else {
                    return None;
                };

                if cursor < events.len() {
                    let mut frame = String::new();
                    for event in events.iter().skip(cursor) {
                        frame.push_str("event: mission_event\n");
                        frame.push_str("data: ");
                        frame.push_str(
                            &serde_json::to_string(event)
                                .unwrap_or_else(|_| "{\"error\":\"serialization_failed\"}".into()),
                        );
                        frame.push_str("\n\n");
                    }
                    let next_cursor = events.len();
                    return Some((
                        Ok::<_, actix_web::Error>(web::Bytes::from(frame)),
                        (state, mission_id, next_cursor),
                    ));
                }

                if matches!(
                    status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                ) {
                    return None;
                }

                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        },
    );

    Ok(HttpResponse::Ok()
        .append_header(("Content-Type", "text/event-stream"))
        .append_header(("Cache-Control", "no-cache"))
        .append_header(("Connection", "keep-alive"))
        .streaming(stream))
}

pub async fn assistant_local_app_catalog(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let catalog = state.local_operator.applications()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(catalog)))
}

pub async fn assistant_local_open(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<LocalOpenRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let action = if request.app.is_some() {
        LocalActionKind::OpenApp
    } else if request
        .resource
        .as_ref()
        .is_some_and(|resource| resource.kind == "url")
    {
        LocalActionKind::BrowserTask
    } else {
        LocalActionKind::OpenResource
    };
    let mission_request = build_local_operator_mission_request(
        request.task,
        request.operator_mode,
        action,
        request.resource,
        request.app,
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
        request.metadata,
    );
    let payload = {
        let mut state = state.write().await;
        plan_and_execute_local_request(&mut state, mission_request, "/api/local/open")?
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_software_sources(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SoftwareSourceResolutionRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let sources = state
        .local_operator
        .resolve_download_sources(body.into_inner());
    Ok(HttpResponse::Ok().json(ApiResponse::ok(sources)))
}

pub async fn assistant_list_staged_artifacts(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.local_operator.staged_artifacts())))
}

pub async fn assistant_stage_software(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SoftwareStageRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mission_request = build_local_operator_mission_request(
        request.task,
        request.operator_mode,
        LocalActionKind::DownloadStage,
        None,
        None,
        Some(request.source),
        None,
        request.target_path,
        None,
        None,
        Vec::new(),
        request.metadata,
    );
    let payload = {
        let mut state = state.write().await;
        plan_and_execute_local_request(&mut state, mission_request, "/api/software/stage")?
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_install_software(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SoftwareInstallRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mission_request = build_local_operator_mission_request(
        request.task,
        request.operator_mode,
        LocalActionKind::InstallSoftware,
        None,
        None,
        request.source,
        request.artifact_id,
        None,
        None,
        None,
        request.install_arguments,
        request.metadata,
    );
    let payload = {
        let mut state = state.write().await;
        plan_and_execute_local_request(&mut state, mission_request, "/api/software/install")?
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_update_software(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SoftwareInstallRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mission_request = build_local_operator_mission_request(
        request.task,
        request.operator_mode,
        LocalActionKind::UpdateSoftware,
        None,
        None,
        request.source,
        request.artifact_id,
        None,
        None,
        None,
        request.install_arguments,
        request.metadata,
    );
    let payload = {
        let mut state = state.write().await;
        plan_and_execute_local_request(&mut state, mission_request, "/api/software/update")?
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_install_inventory(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.local_operator.install_receipts())))
}

pub async fn assistant_submit_security_sample(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SampleSubmissionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mission_request = build_local_operator_mission_request(
        request.task,
        request.operator_mode,
        LocalActionKind::SecurityScan,
        None,
        None,
        None,
        request.artifact_id,
        None,
        request.sample_path,
        None,
        Vec::new(),
        request.metadata,
    );
    let payload = {
        let mut state = state.write().await;
        plan_and_execute_local_request(&mut state, mission_request, "/api/security/samples")?
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_reverse_engineer_sample(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SampleSubmissionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mission_request = build_local_operator_mission_request(
        request.task,
        request.operator_mode,
        LocalActionKind::ReverseEngineerSample,
        None,
        None,
        None,
        request.artifact_id,
        None,
        request.sample_path,
        None,
        Vec::new(),
        request.metadata,
    );
    let payload = {
        let mut state = state.write().await;
        plan_and_execute_local_request(
            &mut state,
            mission_request,
            "/api/security/reverse-engineer",
        )?
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_list_security_cases(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.local_operator.security_cases())))
}

pub async fn assistant_get_security_case(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let case_id = path.into_inner();
    let state = state.read().await;
    let case = state
        .local_operator
        .security_case(&case_id)
        .ok_or_else(|| AstraError::SessionNotFound(format!("security case '{case_id}'")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(case)))
}

pub async fn assistant_list_owned_targets(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.local_operator.owned_targets())))
}

pub async fn assistant_register_owned_target(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<OwnedTargetRegistrationRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let target = state
        .local_operator
        .register_owned_target(body.into_inner())?;
    state.assistant_executive.record_external_audit(
        "local_operator",
        "owned_target",
        &target.target_id,
        "owned_target_registered",
        &format!("Registered owned target '{}'", target.name),
        serde_json::json!({
            "kind": target.kind,
            "identifier": target.identifier,
            "approved_for_lab": target.approved_for_lab,
        }),
    );
    Ok(HttpResponse::Created().json(ApiResponse::ok(target)))
}

pub async fn assistant_list_lab_environments(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.local_operator.lab_environments())))
}

pub async fn assistant_launch_lab_environment(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<LabSessionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mission_request = build_local_operator_mission_request(
        request.task,
        request.operator_mode.or(Some(OperatorMode::Lab)),
        LocalActionKind::LabExecute,
        None,
        None,
        None,
        request.artifact_id,
        None,
        request.sample_path,
        request.target_id,
        Vec::new(),
        request.metadata,
    );
    let payload = {
        let mut state = state.write().await;
        plan_and_execute_local_request(&mut state, mission_request, "/api/security/lab/sessions")?
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

pub async fn assistant_list_override_sessions(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.local_operator.override_sessions())))
}

pub async fn assistant_create_override_session(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<OverrideSessionRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let session = state
        .local_operator
        .create_override_session(body.into_inner())?;
    state.assistant_executive.record_external_audit(
        "local_operator",
        "override_session",
        &session.session_id,
        "override_session_created",
        &format!("Created {} override session", session.mode.as_str()),
        serde_json::json!({
            "expires_at": session.expires_at,
            "reason": session.reason,
            "created_by": session.created_by,
        }),
    );
    Ok(HttpResponse::Created().json(ApiResponse::ok(session)))
}

pub async fn assistant_list_approvals(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.assistant_executive.approvals())))
}

pub async fn assistant_apply_approval(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<ApprovalDecisionRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let approval = state
        .assistant_executive
        .apply_approval_decision(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(approval)))
}

pub async fn assistant_list_connectors(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.assistant_executive.connectors())))
}

pub async fn assistant_upsert_connector(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<ConnectorRegistrationRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let connector = state
        .assistant_executive
        .upsert_connector(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(connector)))
}

pub async fn assistant_list_research_policies(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.assistant_executive.research_policies(),
    )))
}

pub async fn assistant_upsert_research_policy(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AcquisitionPolicyRegistrationRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let policy = state.assistant_executive.upsert_policy(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policy)))
}

pub async fn assistant_audit_log(
    state: web::Data<Arc<RwLock<EngineState>>>,
    query: web::Query<CollectionQuery>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let entries = state
        .assistant_executive
        .audit_log(query.limit.unwrap_or(DEFAULT_ASSISTANT_COLLECTION_LIMIT));
    Ok(HttpResponse::Ok().json(ApiResponse::ok(entries)))
}

pub async fn assistant_enterprise_health(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let mut report = state.assistant_executive.enterprise_health_report(
        state.config.server.admin_token.is_some(),
        state.config.privacy.egress_proxy_url.is_some(),
        state.config.privacy.allow_direct_egress,
        EXPERIMENTAL_ROUTES_ENABLED,
        &state.config.operational_warnings(),
        state.tool_registry.snapshot().total_tools,
    );
    report.checks.push(EnterpriseHealthCheck {
        name: "windows_host_broker".into(),
        status: if state.local_operator.supports_windows_host_control() {
            "configured".into()
        } else {
            "degraded".into()
        },
        detail: if state.local_operator.supports_windows_host_control() {
            "Typed Windows local-operator controls are available for app launch, staged installs, and lab prep.".into()
        } else {
            "Windows host broker is unavailable on this platform.".into()
        },
    });
    if let Some(metrics) = report.metrics.as_object_mut() {
        metrics.insert("local_operator".into(), state.local_operator.metrics());
        metrics.insert("fleet".into(), state.fleet.snapshot().stats);
    }
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

pub async fn assistant_brain_execute(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<BrainExecuteRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mode = compatibility_mode_for_brain_request(&request);
    let mut state = state.write().await;
    let artifacts = state.execute_brain(request.clone(), None)?;
    state.assistant_executive.record_compatibility_completion(
        "/api/assistant/brain/execute",
        &request.task,
        mode,
        artifacts.response.answer.clone(),
        serde_json::json!({
            "session_id": artifacts.response.session_id.clone(),
            "stage": artifacts.response.stage,
            "verification_passed": artifacts.response.verification.passed,
        }),
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(artifacts.response)))
}

pub async fn assistant_brain_session(
    state: web::Data<Arc<RwLock<EngineState>>>,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let session = state.one_brain.get_session(&path.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(session)))
}

pub async fn runtime_mission_assignment(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<MissionAssignmentRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let artifacts = state.execute_brain(
        BrainExecuteRequest {
            task: request.task,
            context: serde_json::json!({
                "current_url": request.current_url,
                "domain": "mission"
            }),
            session_id: None,
            mode_hint: Some(BrainModeHint::Mission),
            allow_mutation: false,
            require_live_retrieval: false,
            require_verification: true,
        },
        None,
    )?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(MissionAssignmentResponse {
            mission: artifacts.mission,
        })),
    )
}

pub async fn unbreakable_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let status: UnbreakableBackendStatus = state.unbreakable.status();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(status)))
}

pub async fn unbreakable_cryogenic_heartbeat(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<CryogenicHeartbeatRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let integrity_hash = state
        .unbreakable
        .capture_cryogenic_heartbeat(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "integrity_hash": integrity_hash,
        "status": state.unbreakable.status(),
    }))))
}

pub async fn unbreakable_cryogenic_report(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.unbreakable.cryogenic_report())))
}

pub async fn unbreakable_network_plan(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<NetworkPrivacyRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let plan = state.unbreakable.plan_network_request(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(plan)))
}

pub async fn unbreakable_anchor_fact(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<UnbreakableFactRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let accepted = state.unbreakable.anchor_fact(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "accepted": accepted,
        "status": state.unbreakable.status(),
    }))))
}

pub async fn unbreakable_reality_anchor_records(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.unbreakable.reality_anchor_records())))
}

pub async fn unbreakable_immune_patrol(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let report: ImmunePatrolReport = state.unbreakable.run_immune_patrol();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

pub async fn unbreakable_immune_memory(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.unbreakable.immune_memory())))
}

pub async fn unbreakable_vault_inventory(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.unbreakable.vault_inventory())))
}

pub async fn unbreakable_vault_audit(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.unbreakable.vault_audit())))
}

pub async fn unbreakable_vault_rotate(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let receipt = state.unbreakable.rotate_vault_key()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(receipt)))
}

pub async fn runtime_list_device_permissions(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.unbreakable.permissions())))
}

pub async fn runtime_grant_device_permission(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<DevicePermissionGrantRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let grant = state
        .unbreakable
        .grant_device_permission(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(grant)))
}

pub async fn runtime_authorize_device_action(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AutonomousActionRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let verdict = state
        .unbreakable
        .authorize_device_action(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(verdict)))
}

pub async fn chain_treasury_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.unbreakable.status().treasury_balances_minor,
    )))
}

pub async fn chain_treasury_receipts(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.unbreakable.treasury_receipts())))
}

pub async fn chain_treasury_capture_revenue(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RevenueCaptureRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let receipt = state.unbreakable.capture_revenue(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(receipt)))
}

pub async fn chain_info(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.get_chain_info())))
}

pub async fn chain_overview(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let info = state.chain.get_chain_info();
    let acceleration = state.chain.acceleration_report();
    let certified_height = info.last_certified_height;
    let start_height = info.height.saturating_sub(4);
    let mut recent_blocks = Vec::new();
    for height in (start_height..=info.height).rev() {
        if let Some(block) = state.chain.get_block(height) {
            recent_blocks.push(summarize_block(block, certified_height));
        }
    }

    let mut validator_scores = state.chain.list_validator_browsing_scores();
    validator_scores.sort_by(|left, right| right.aggregate_score.cmp(&left.aggregate_score));
    validator_scores.truncate(6);

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(ChainOverviewResponse {
            info,
            acceleration,
            recent_blocks,
            recent_resources: state.chain.list_recent_resource_commitments(8),
            recent_audit: state.chain.list_recent_audit(10),
            validator_scores,
        })),
    )
}

pub async fn chain_integrity(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(ChainIntegrityResponse {
            report: state.chain.integrity_report(),
        })),
    )
}

pub async fn chain_acceleration_report(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.acceleration_report())))
}

pub async fn chain_quantum_resilience(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.quantum_resilience_report())))
}

fn require_chain_signature(
    public_key_hex: Option<&str>,
    signature_hex: Option<&str>,
) -> Result<(String, String), AstraError> {
    let public_key_hex = public_key_hex
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AstraError::AuthRequired)?
        .to_string();
    let signature_hex = signature_hex
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AstraError::AuthRequired)?
        .to_string();
    Ok((public_key_hex, signature_hex))
}

async fn replicate_block_to_peer(
    peer: ChainPeer,
    source_peer_id: String,
    source_base_url: String,
    source_public_key_hex: String,
    source_signer: Arc<PQIdentitySigner>,
    block: crate::chain::block::Block,
) -> ReplicationPeerOutcome {
    if source_base_url.trim().is_empty() {
        return ReplicationPeerOutcome {
            peer_id: peer.peer_id,
            accepted: false,
            detail: "local node does not have an advertised transport URL".into(),
            finality_vote: None,
        };
    }
    let challenge_endpoint = format!("{}/api/chain/peers/handshake/challenge", peer.base_url);
    let verify_endpoint = format!("{}/api/chain/peers/handshake/verify", peer.base_url);
    let replication_endpoint = format!("{}/api/chain/replicate/block", peer.base_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build();
    let Ok(client) = client else {
        return ReplicationPeerOutcome {
            peer_id: peer.peer_id,
            accepted: false,
            detail: "failed to construct replication client".into(),
            finality_vote: None,
        };
    };

    let challenge_response = match client
        .post(challenge_endpoint)
        .json(&serde_json::json!({
            "peer_id": source_peer_id.clone(),
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: format!(
                    "peer rejected handshake challenge with status {}",
                    response.status()
                ),
                finality_vote: None,
            };
        }
        Err(error) => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: format!("handshake challenge request failed: {error}"),
                finality_vote: None,
            };
        }
    };
    let challenge_payload: serde_json::Value = match challenge_response.json().await {
        Ok(payload) => payload,
        Err(error) => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: format!("unable to decode handshake challenge payload: {error}"),
                finality_vote: None,
            };
        }
    };
    let challenge = challenge_payload
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let challenge_id = match challenge
        .get("challenge_id")
        .and_then(|value| value.as_str())
    {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: "peer handshake response omitted challenge_id".into(),
                finality_vote: None,
            };
        }
    };
    let challenge_nonce = match challenge
        .get("challenge_nonce")
        .and_then(|value| value.as_str())
    {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: "peer handshake response omitted challenge_nonce".into(),
                finality_vote: None,
            };
        }
    };
    let destination_node_id = match challenge
        .get("destination_node_id")
        .and_then(|value| value.as_str())
    {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: "peer handshake response omitted destination_node_id".into(),
                finality_vote: None,
            };
        }
    };
    let handshake_message = canonical_peer_handshake_message(
        &block.chain_id,
        &destination_node_id,
        &source_peer_id,
        &challenge_id,
        &challenge_nonce,
        &source_base_url,
    );
    let handshake_signature_hex = match source_signer.sign_message_hex(&handshake_message) {
        Ok(signature) => signature,
        Err(error) => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: format!("unable to sign peer handshake challenge: {error}"),
                finality_vote: None,
            };
        }
    };
    match client
        .post(verify_endpoint)
        .json(&serde_json::json!({
            "peer_id": source_peer_id.clone(),
            "challenge_id": challenge_id.clone(),
            "public_key_hex": source_public_key_hex.clone(),
            "signature_hex": handshake_signature_hex,
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: format!(
                    "peer rejected handshake verification with status {}",
                    response.status()
                ),
                finality_vote: None,
            };
        }
        Err(error) => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: format!("handshake verification request failed: {error}"),
                finality_vote: None,
            };
        }
    }

    let replication_message = canonical_replication_message(
        &block.chain_id,
        &destination_node_id,
        &source_peer_id,
        &block.block_hash,
        block.height,
        &block.prev_hash,
    );
    let replication_signature_hex = match source_signer.sign_message_hex(&replication_message) {
        Ok(signature) => signature,
        Err(error) => {
            return ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: false,
                detail: format!("unable to sign replication envelope: {error}"),
                finality_vote: None,
            };
        }
    };

    match client
        .post(replication_endpoint)
        .json(&serde_json::json!({
            "source_peer_id": source_peer_id,
            "source_public_key_hex": source_public_key_hex,
            "source_signature_hex": replication_signature_hex,
            "block": block,
        }))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            let payload: serde_json::Value = match response.json().await {
                Ok(payload) => payload,
                Err(error) => {
                    return ReplicationPeerOutcome {
                        peer_id: peer.peer_id,
                        accepted: false,
                        detail: format!("unable to decode replication acknowledgement: {error}"),
                        finality_vote: None,
                    };
                }
            };
            let finality_vote = payload
                .get("data")
                .and_then(|data| data.get("finality_vote"))
                .cloned()
                .and_then(|value| serde_json::from_value::<BlockFinalityVote>(value).ok());
            ReplicationPeerOutcome {
                peer_id: peer.peer_id,
                accepted: true,
                detail: "replication acknowledged".into(),
                finality_vote,
            }
        }
        Ok(response) => ReplicationPeerOutcome {
            peer_id: peer.peer_id,
            accepted: false,
            detail: format!(
                "peer rejected replication with status {}",
                response.status()
            ),
            finality_vote: None,
        },
        Err(error) => ReplicationPeerOutcome {
            peer_id: peer.peer_id,
            accepted: false,
            detail: format!("replication request failed: {error}"),
            finality_vote: None,
        },
    }
}

pub async fn chain_register_validator(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RegisterValidatorRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let validator = state.chain.register_validator(ValidatorRegistration {
        address: body.address.clone(),
        display_name: body.display_name.clone(),
        trust_score: body.trust_score,
        public_key_hex: Some(public_key_hex.to_string()),
        public_key_hash: body.public_key_hash.clone(),
        signature_hex: Some(signature_hex.to_string()),
        capabilities: body.capabilities.clone().unwrap_or_default(),
        stake: body.stake.unwrap_or(0.0),
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(validator)))
}

pub async fn chain_submit_transaction(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SubmitChainTransactionRequest>,
) -> Result<HttpResponse, AstraError> {
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let mut tx = Transaction::with_intent(
        "httpa-browser-ledger-v1",
        &body.sender,
        &body.receiver,
        body.amount,
        parse_chain_transaction_intent(body.intent.as_deref()),
        body.data.clone(),
    );
    if let Some(nonce) = body.nonce {
        tx.set_nonce(nonce);
    }
    if let Some(gas_limit) = body.gas_limit {
        tx.set_gas_limit(gas_limit);
    }
    if let Some(fee) = body.fee {
        tx.set_fee(fee)?;
    }
    if body.nonce.is_none() {
        return Err(AstraError::InvalidTransaction(
            "signed transaction submission requires an explicit nonce".into(),
        ));
    }
    tx.set_signature_hex(&signature_hex)?;

    let mut state = state.write().await;
    let tx_hash =
        state
            .chain
            .submit_signed_transaction_checked(tx, &public_key_hex, &signature_hex)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "tx_hash": tx_hash,
        "queued": true,
    }))))
}

pub async fn chain_value_capabilities(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.value_protocol_capabilities())))
}

pub async fn chain_list_value_connectors(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.settlement_connectors())))
}

pub async fn chain_register_value_connector(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SettlementConnectorRegistrationRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let connector = state
        .chain
        .register_settlement_connector(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(connector)))
}

pub async fn chain_list_value_compliance_profiles(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.payment_compliance_profiles())))
}

pub async fn chain_register_value_compliance_profile(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<PaymentComplianceRegistrationRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let profile = state
        .chain
        .register_payment_compliance_profile(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profile)))
}

pub async fn chain_quote_stream_value(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<StreamValueRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let quote = state.chain.quote_stream_value(&body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(quote)))
}

pub async fn chain_settle_stream_value(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<StreamSettlementRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let submission = state.chain.settle_stream_value(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(submission)))
}

pub async fn chain_route_financial_intent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<FinancialIntentRouteRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let route = state.chain.route_financial_intent(&body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(route)))
}

pub async fn chain_commit_financial_intent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<FinancialIntentCommitRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let submission = state.chain.commit_financial_intent(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(submission)))
}

pub async fn chain_reward_cognition(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<CognitionRewardRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let submission = state.chain.reward_cognition(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(submission)))
}

pub async fn chain_attest_privacy(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<PrivacyAttestationRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let submission = state.chain.attest_privacy(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(submission)))
}

pub async fn chain_bind_device_wallet(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<DeviceBindingRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let submission = state.chain.bind_device_wallet(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(submission)))
}

pub async fn chain_guarded_payment(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SubmitGuardedPaymentRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let body = body.into_inner();
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let auth_payload = serde_json::to_value(&body.payment)?;
    state.chain.authorize_action(
        &body.payment.sender,
        &public_key_hex,
        &signature_hex,
        "guarded_payment_submit",
        &auth_payload,
        &[],
    )?;
    let outcome = state.chain.settle_guarded_payment(body.payment)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(outcome)))
}

pub async fn chain_list_value_reconciliation(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.chain.payment_reconciliation_records(),
    )))
}

pub async fn chain_list_value_disputes(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.payment_disputes())))
}

pub async fn chain_open_value_dispute(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<PaymentDisputeRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let dispute = state.chain.open_payment_dispute(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(dispute)))
}

pub async fn chain_resolve_value_dispute(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<PaymentDisputeResolutionRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let outcome = state.chain.resolve_payment_dispute(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(outcome)))
}

pub async fn chain_mine_block(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<MineBlockRequest>,
) -> Result<HttpResponse, AstraError> {
    let state_handle = state.clone();
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let payload = serde_json::json!({
        "validator": body.validator,
        "max_txs": body.max_txs.unwrap_or(32),
    });
    state.chain.authorize_action(
        &body.validator,
        &public_key_hex,
        &signature_hex,
        "mine_block",
        &payload,
        &["validator"],
    )?;
    let block = state
        .chain
        .mine_block(&body.validator, body.max_txs.unwrap_or(32))?;
    let peers = state.chain.list_active_peers();
    let source_peer_id = state.chain.local_node_id().to_string();
    let source_base_url = state.chain.advertised_url().unwrap_or("").to_string();
    state.identity.ensure_identity();
    let source_public_key_hex = state
        .identity
        .public_key_hex()
        .ok_or(AstraError::AuthRequired)?;
    let source_signer = Arc::new(
        state
            .identity
            .detached_signer()
            .ok_or(AstraError::AuthRequired)?,
    );
    drop(state);

    let outcomes = join_all(peers.into_iter().map(|peer| {
        replicate_block_to_peer(
            peer,
            source_peer_id.clone(),
            source_base_url.clone(),
            source_public_key_hex.clone(),
            Arc::clone(&source_signer),
            block.clone(),
        )
    }))
    .await;

    let mut state = state_handle.write().await;
    let replication =
        state
            .chain
            .record_replication_result(&block.block_hash, block.height, outcomes)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "block": block,
        "replication": replication,
    }))))
}

pub async fn chain_deploy_contract(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<DeployAiContractRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let auth_payload = serde_json::json!({
        "owner": body.owner,
        "name": body.name,
        "purpose": body.purpose,
        "allowed_domains": body.allowed_domains,
        "allowed_callers": body.allowed_callers,
        "allowed_tools": body.allowed_tools,
        "max_budget": body.max_budget,
        "max_compute_units": body.max_compute_units,
        "vm_program": body.vm_program,
    });
    state.chain.authorize_action(
        &body.owner,
        &public_key_hex,
        &signature_hex,
        "deploy_contract",
        &auth_payload,
        &[],
    )?;
    let contract = state.chain.deploy_contract(AIContractDeployRequest {
        owner: body.owner.clone(),
        name: body.name.clone(),
        purpose: body.purpose.clone(),
        allowed_domains: body.allowed_domains.clone(),
        allowed_callers: body.allowed_callers.clone().unwrap_or_default(),
        allowed_tools: body.allowed_tools.clone().unwrap_or_default(),
        max_budget: body.max_budget,
        max_compute_units: body.max_compute_units,
        min_confidence: body.min_confidence,
        max_risk: body.max_risk,
        review_threshold: body.review_threshold.unwrap_or(body.max_risk),
        minimum_evidence: body.minimum_evidence.unwrap_or(0),
        require_trace_binding: body.require_trace_binding.unwrap_or(false),
        prompt_template: body.prompt_template.clone(),
        invariants: body.invariants.clone().unwrap_or_default(),
        vm_program: body.vm_program.clone().unwrap_or_default(),
        metadata: body.metadata.clone().unwrap_or(serde_json::Value::Null),
        active: body.active.unwrap_or(true),
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(contract)))
}

pub async fn chain_invoke_contract(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<InvokeAiContractRequest>,
) -> Result<HttpResponse, AstraError> {
    let input_digest = body.input_digest.clone().unwrap_or_else(|| {
        crate::crypto::hash::sha3_256_hex(
            body.metadata
                .clone()
                .unwrap_or(serde_json::Value::Null)
                .to_string()
                .as_bytes(),
        )
    });
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let auth_payload = serde_json::json!({
        "contract_id": body.contract_id,
        "caller": body.caller,
        "action": body.action,
        "domain": body.domain,
        "requested_tools": body.requested_tools,
        "estimated_cost": body.estimated_cost,
        "confidence": body.confidence,
        "risk_score": body.risk_score,
        "trace_id": body.trace_id,
        "session_id": body.session_id,
        "input_digest": input_digest,
        "context": body.context,
    });
    state.chain.authorize_action(
        &body.caller,
        &public_key_hex,
        &signature_hex,
        "invoke_contract",
        &auth_payload,
        &[],
    )?;
    let receipt = state.chain.invoke_contract(AIContractInvokeRequest {
        contract_id: body.contract_id.clone(),
        caller: body.caller.clone(),
        action: body.action.clone(),
        domain: body.domain.clone(),
        requested_tools: body.requested_tools.clone().unwrap_or_default(),
        estimated_cost: body.estimated_cost.unwrap_or(0.0),
        confidence: body.confidence.unwrap_or(1.0),
        risk_score: body.risk_score.unwrap_or(0.0),
        input_digest,
        trace_id: body.trace_id.clone(),
        session_id: body.session_id.clone(),
        evidence: body.evidence.clone().unwrap_or_default(),
        context: body.context.clone().unwrap_or_default(),
        metadata: body.metadata.clone().unwrap_or(serde_json::Value::Null),
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(receipt)))
}

pub async fn chain_notarize_resource(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<NotarizeResourceRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let auth_payload = serde_json::json!({
        "url": body.url,
        "content_hash": body.content_hash,
        "content_type": body.content_type,
        "source": body.source,
        "attestor": body.attestor,
    });
    state.chain.authorize_action(
        &body.attestor,
        &public_key_hex,
        &signature_hex,
        "notarize_resource",
        &auth_payload,
        &[],
    )?;
    let commitment = state.chain.notarize_resource(
        &body.url,
        &body.content_hash,
        &body.content_type,
        &body.source,
        &body.attestor,
        body.metadata.clone().unwrap_or(serde_json::Value::Null),
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(commitment)))
}

pub async fn chain_resources(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.chain.list_recent_resource_commitments(100),
    )))
}

pub async fn chain_resource_detail(
    state: web::Data<Arc<RwLock<EngineState>>>,
    commitment_id: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let commitment = state
        .chain
        .get_resource_commitment(&commitment_id.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(commitment)))
}

pub async fn chain_resource_verify(
    state: web::Data<Arc<RwLock<EngineState>>>,
    commitment_id: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let verification = state
        .chain
        .verify_resource_commitment(&commitment_id.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(verification)))
}

pub async fn chain_search_proof(
    state: web::Data<Arc<RwLock<EngineState>>>,
    trace_id: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let proof = state.chain.find_search_proof(&trace_id.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(proof)))
}

pub async fn chain_audit(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.list_recent_audit(100))))
}

pub async fn chain_contracts(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "contracts": state.chain.list_contracts(),
        "recent_receipts": state.chain.list_recent_contract_receipts(50),
    }))))
}

pub async fn chain_register_peer(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RegisterChainPeerRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let auth_payload = serde_json::json!({
        "peer_id": body.peer_id,
        "base_url": body.base_url,
        "trust_score": body.trust_score,
        "replication_mode": body.replication_mode,
    });
    state.chain.authorize_action(
        &body.operator,
        &public_key_hex,
        &signature_hex,
        "register_peer",
        &auth_payload,
        &["validator"],
    )?;
    let peer = state.chain.register_peer(PeerRegistration {
        peer_id: body.peer_id.clone(),
        base_url: body.base_url.clone(),
        public_key_hash: body.public_key_hash.clone(),
        trust_score: body.trust_score,
        replication_mode: body.replication_mode.clone(),
        capabilities: body.capabilities.clone().unwrap_or_default(),
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(peer)))
}

pub async fn chain_peers(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "node_id": state.chain.local_node_id(),
        "advertised_url": state.chain.advertised_url(),
        "local_transport_public_key_hash": state.identity.public_key_hash_full(),
        "peers": state.chain.list_peers(),
    }))))
}

pub async fn chain_peer_handshake_challenge(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RequestPeerHandshakeChallenge>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let challenge = state.chain.issue_peer_handshake(&body.peer_id)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(challenge)))
}

pub async fn chain_peer_handshake_verify(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<VerifyPeerHandshakeRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let peer = state.chain.verify_peer_handshake(
        &body.peer_id,
        &body.challenge_id,
        &body.public_key_hex,
        &body.signature_hex,
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(peer)))
}

pub async fn chain_replication(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "node_id": state.chain.local_node_id(),
        "recent": state.chain.list_recent_replication(50),
        "finality_certificates": state.chain.list_recent_finality_certificates(25),
    }))))
}

pub async fn chain_export(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chain.export_sync_bundle())))
}

pub async fn chain_export_audit(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(ChainExportAuditResponse {
            report: state.chain.export_audit_report(),
        })),
    )
}

pub async fn chain_replicate_block(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<ReplicateChainBlockRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let source_peer_id = body
        .source_peer_id
        .as_deref()
        .ok_or(AstraError::AuthRequired)?;
    let source_public_key_hex = body
        .source_public_key_hex
        .as_deref()
        .ok_or(AstraError::AuthRequired)?;
    let source_signature_hex = body
        .source_signature_hex
        .as_deref()
        .ok_or(AstraError::AuthRequired)?;
    state.chain.authorize_peer_replication(
        source_peer_id,
        source_public_key_hex,
        source_signature_hex,
        &body.block,
    )?;
    let record = state
        .chain
        .accept_replicated_block(body.block.clone(), Some(source_peer_id))?;
    state.identity.ensure_identity();
    let local_public_key_hex = state
        .identity
        .public_key_hex()
        .ok_or(AstraError::AuthRequired)?;
    let finality_message = canonical_finality_vote_message(
        &body.block.chain_id,
        source_peer_id,
        state.chain.local_node_id(),
        &body.block.block_hash,
        body.block.height,
        &body.block.prev_hash,
        true,
    );
    let local_signature_hex = state.identity.sign_message_hex(&finality_message)?;
    let finality_vote = state.chain.build_local_finality_vote(
        source_peer_id,
        &local_public_key_hex,
        &local_signature_hex,
        &body.block,
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "replication": record,
        "finality_vote": finality_vote,
    }))))
}

pub async fn chain_evolution_analysis(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let learning_summary = state.learning_loop.summary(24);
    let learning_context =
        serde_json::to_value(&learning_summary).unwrap_or(serde_json::Value::Null);
    let analysis = state.chain.analyze_evolution(Some(learning_context));
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "analysis": analysis,
        "history": state.chain.list_recent_evolutions(25),
        "agents": state.chain.list_evolution_agents(),
        "submitted_reports": state.chain.list_recent_submitted_evolution_reports(25),
    }))))
}

pub async fn chain_register_evolution_agent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RegisterChainEvolutionAgentRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let auth_payload = serde_json::json!({
        "agent_id": body.agent_id.clone(),
        "display_name": body.display_name.clone(),
        "capabilities": body.capabilities.clone(),
        "minimum_confidence": body.minimum_confidence,
    });
    state.chain.authorize_action(
        &body.operator,
        &public_key_hex,
        &signature_hex,
        "register_evolution_agent",
        &auth_payload,
        &["validator"],
    )?;
    let profile = state.chain.register_evolution_agent(
        &body.operator,
        EvolutionAgentRegistration {
            agent_id: body.agent_id.clone(),
            display_name: body.display_name.clone(),
            capabilities: body.capabilities.clone().unwrap_or_default(),
            minimum_confidence: body.minimum_confidence,
        },
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profile)))
}

pub async fn chain_submit_evolution_report(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SubmitChainEvolutionReportRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let recommended_policy = if body.max_block_transactions.is_some()
        || body.autocommit_batch_size.is_some()
        || body.min_peer_trust.is_some()
        || body.replication_quorum_floor.is_some()
    {
        Some(
            ChainRuntimePolicy {
                max_block_transactions: body
                    .max_block_transactions
                    .unwrap_or_else(|| state.chain.runtime_policy().max_block_transactions),
                autocommit_batch_size: body
                    .autocommit_batch_size
                    .unwrap_or_else(|| state.chain.runtime_policy().autocommit_batch_size),
                min_peer_trust: body
                    .min_peer_trust
                    .unwrap_or_else(|| state.chain.runtime_policy().min_peer_trust),
                replication_quorum_floor: body
                    .replication_quorum_floor
                    .unwrap_or_else(|| state.chain.runtime_policy().replication_quorum_floor),
            }
            .sanitized(),
        )
    } else {
        None
    };
    let auth_payload = serde_json::json!({
        "role": body.role.clone(),
        "confidence": body.confidence,
        "findings": body.findings.clone(),
        "recommended_policy": recommended_policy.clone(),
    });
    state.chain.authorize_action(
        &body.agent_id,
        &public_key_hex,
        &signature_hex,
        "submit_evolution_report",
        &auth_payload,
        &["agent"],
    )?;
    let report = state
        .chain
        .submit_evolution_agent_report(ChainEvolutionAgentReport {
            agent_id: body.agent_id.clone(),
            role: body.role.clone(),
            source: "submitted".into(),
            confidence: body.confidence,
            findings: body.findings.clone(),
            recommended_policy,
            context: body.context.clone().unwrap_or(serde_json::Value::Null),
            generated_at: chrono::Utc::now().timestamp_millis(),
        })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

pub async fn chain_evolution_apply(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<ApplyChainEvolutionRequest>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let learning_summary = state.learning_loop.summary(24);
    let learning_context =
        serde_json::to_value(&learning_summary).unwrap_or(serde_json::Value::Null);
    let analysis = state.chain.analyze_evolution(Some(learning_context));
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;

    let mut target_policy = if body.use_recommended.unwrap_or(true) {
        analysis.recommended_policy.clone()
    } else {
        state.chain.runtime_policy().clone()
    };
    if let Some(value) = body.max_block_transactions {
        target_policy.max_block_transactions = value;
    }
    if let Some(value) = body.autocommit_batch_size {
        target_policy.autocommit_batch_size = value;
    }
    if let Some(value) = body.min_peer_trust {
        target_policy.min_peer_trust = value;
    }
    if let Some(value) = body.replication_quorum_floor {
        target_policy.replication_quorum_floor = value;
    }
    target_policy = target_policy.sanitized();

    let auth_payload = serde_json::json!({
        "target_policy": target_policy,
        "rationale": body.rationale,
    });
    state.chain.authorize_action(
        &body.operator,
        &public_key_hex,
        &signature_hex,
        "evolve_chain_policy",
        &auth_payload,
        &["validator"],
    )?;

    let verdict = state.governance.propose_upgrade(crate::governance::layer::ASIUpgradeRequest {
        module_name: "AstrachainRuntimePolicy".into(),
        title: "Autonomous chain runtime policy evolution".into(),
        description: format!(
            "Apply a bounded blockchain runtime policy adjustment proposed by the chain evolution council. {}",
            analysis.recommendation_summary
        ),
        changes_summary: format!(
            "max_block_transactions={} autocommit_batch_size={} min_peer_trust={:.2} replication_quorum_floor={}",
            target_policy.max_block_transactions,
            target_policy.autocommit_batch_size,
            target_policy.min_peer_trust,
            target_policy.replication_quorum_floor
        ),
        risk_assessment: format!(
            "Chain evolution posture: {}. Operator rationale: {}",
            analysis.risk_posture,
            body.rationale
        ),
        rollback_plan:
            "Restore the previous runtime policy from the persisted chain evolution history."
                .into(),
        dependencies: vec![
            "Astrachain runtime policy".into(),
            "Chain evolution council".into(),
            "Governance layer".into(),
        ],
        urgency: if analysis.risk_posture == "guarded" {
            "important".into()
        } else {
            "routine".into()
        },
    });

    if !verdict.approved {
        return Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
            "analysis": analysis,
            "governance_verdict": verdict,
            "applied": serde_json::Value::Null,
        }))));
    }

    let applied =
        state
            .chain
            .apply_evolution_policy(target_policy, &body.operator, &body.rationale)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "analysis": analysis,
        "governance_verdict": verdict,
        "applied": applied,
    }))))
}

pub async fn chain_sync_peer(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SyncChainPeerRequest>,
) -> Result<HttpResponse, AstraError> {
    let (public_key_hex, signature_hex) = require_chain_signature(
        body.public_key_hex.as_deref(),
        body.signature_hex.as_deref(),
    )?;
    let peer = {
        let mut state = state.write().await;
        let auth_payload = serde_json::json!({
            "peer_id": body.peer_id,
        });
        state.chain.authorize_action(
            &body.operator,
            &public_key_hex,
            &signature_hex,
            "sync_peer",
            &auth_payload,
            &["validator"],
        )?;
        state
            .chain
            .list_peers()
            .into_iter()
            .find(|peer| peer.peer_id == body.peer_id)
            .ok_or_else(|| AstraError::ControlPlaneRejected("peer is not registered".into()))?
    };

    let endpoint = format!("{}/api/chain/export", peer.base_url);
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| AstraError::Internal(format!("sync client init failed: {error}")))?
        .get(endpoint)
        .send()
        .await
        .map_err(|error| AstraError::ControlPlaneRejected(format!("peer sync failed: {error}")))?;
    if !response.status().is_success() {
        return Err(AstraError::ControlPlaneRejected(format!(
            "peer sync failed with status {}",
            response.status()
        )));
    }
    let api_response: ApiResponse<crate::chain::chain::ChainSyncBundle> =
        response.json().await.map_err(|error| {
            AstraError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid peer sync payload: {error}"),
            )))
        })?;

    let mut state = state.write().await;
    let result = state
        .chain
        .import_sync_bundle(api_response.data, Some(&body.peer_id))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

fn parse_chain_transaction_intent(intent: Option<&str>) -> TransactionIntent {
    match intent.unwrap_or("transfer") {
        "browser_resource" => TransactionIntent::BrowserResource,
        "httpa_intent" => TransactionIntent::HttpaIntent,
        "agent_settlement" => TransactionIntent::AgentSettlement,
        "stream_settlement" => TransactionIntent::StreamSettlement,
        "financial_intent" => TransactionIntent::FinancialIntent,
        "cognition_reward" => TransactionIntent::CognitionReward,
        "privacy_attestation" => TransactionIntent::PrivacyAttestation,
        "device_binding" => TransactionIntent::DeviceBinding,
        "contract_deploy" => TransactionIntent::ContractDeploy,
        "contract_invoke" => TransactionIntent::ContractInvoke,
        "audit_commit" => TransactionIntent::AuditCommit,
        _ => TransactionIntent::Transfer,
    }
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// v2.0 â€” Intelligence Core Routes
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Solve a problem using Tree-of-Thought + MCTS reasoning.
pub async fn reason(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let problem = body.get("problem").and_then(|v| v.as_str()).unwrap_or("");
    let context = body
        .get("context")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let learning_domain = infer_learning_domain(problem, &context);

    let (cognitive_runtime, control_runtime, learning_loop) = {
        let state = state.read().await;
        (
            Arc::clone(&state.cognitive_runtime),
            Arc::clone(&state.control_runtime),
            state.learning_loop.clone(),
        )
    };
    let (
        omega_decision,
        omega_mission,
        omega_profile,
        route_decision,
        council_verdict,
        adaptive_policy,
    ) = {
        let mut control_runtime = control_runtime
            .write()
            .map_err(|_| control_runtime_lock_error())?;
        let omega_decision = control_runtime
            .omega_executive
            .directive_for(problem, &context);
        let omega_mission = control_runtime
            .omega_executive
            .mission_for(problem, &omega_decision);
        let omega_profile = control_runtime
            .local_brain
            .profile_request(problem, &context);
        let route_decision = control_runtime.brain_router.route(problem);
        let council_verdict = control_runtime.brain_council.deliberate(problem);
        let adaptive_policy = merge_reasoning_policy(
            to_applied_policy(learning_loop.reasoning_policy(problem, Some(&learning_domain))),
            &omega_profile,
            &route_decision,
            &council_verdict,
            &omega_decision,
        );
        (
            omega_decision,
            omega_mission,
            omega_profile,
            route_decision,
            council_verdict,
            adaptive_policy,
        )
    };

    // Record reasoning request for self-healing
    let start = std::time::Instant::now();
    let mut cognitive_runtime = cognitive_runtime
        .write()
        .map_err(|_| cognitive_runtime_lock_error())?;
    let mut result =
        cognitive_runtime
            .reasoning
            .solve_adaptive(problem, &context, Some(&adaptive_policy));
    drop(cognitive_runtime);
    let latency = start.elapsed().as_millis() as f64;
    let success = result.best_score >= 0.65 && result.confidence >= 0.55;
    let quality = ((result.best_score + result.confidence) / 2.0).clamp(0.0, 1.0);

    let learning_trace = emit_reasoning_trace(problem, &learning_domain, &result);
    ingest_learning_trace(
        &learning_loop,
        objective_for_domain(&learning_domain),
        learning_trace,
    );

    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;

    // Feed health data to self-healing
    control_runtime
        .healing
        .report_health("reasoning", latency, None, 0);

    // Feed score to self-evolution
    control_runtime
        .evolution
        .record_health("reasoning", latency, 0.0, 1.0, result.best_score);

    // Store reasoning result in memory graph
    if !result.best_path.is_empty() {
        let summary = result
            .best_path
            .iter()
            .map(|n| n.content.clone())
            .collect::<Vec<_>>()
            .join(" â†’ ");
        control_runtime.memory_graph.store(
            &summary,
            crate::intelligence::memory_graph::MemoryCategory::Procedure,
            result.best_score,
            serde_json::json!({"problem": problem, "strategies": result.strategies_explored}),
        );
    }

    if success {
        control_runtime.brain_router.record_success(
            &route_decision.selected_node,
            latency,
            omega_profile.recommended_expansions as u64,
        );
    } else {
        control_runtime
            .brain_router
            .record_failure(&route_decision.selected_node);
    }
    control_runtime
        .local_brain
        .record_outcome(&omega_profile, latency, success, quality);
    control_runtime.omega_executive.record_cycle_with_context(
        search_lane_for(omega_decision.mode),
        omega_decision.mode,
        &omega_decision.focus_domains,
        success,
        quality,
    );
    result.reasoning_trace.insert(
        0,
        format!(
            "[omega] lane={} route={} depth={} expansions={} agreement={:.2}",
            omega_profile.cache_lane,
            route_decision.selected_node,
            omega_profile.recommended_depth,
            omega_profile.recommended_expansions,
            council_verdict.agreement_score
        ),
    );
    result.reasoning_trace.insert(
        1,
        format!(
            "[executive] mode={:?} autonomy={:.2} search_intensity={:.2} invention_bias={:.2}",
            omega_decision.mode,
            omega_decision.autonomy_score,
            omega_decision.search_intensity,
            omega_decision.invention_bias
        ),
    );
    result.reasoning_trace.insert(
        2,
        format!(
            "[mission] autonomy_envelope={} search_budget={} reasoning_budget={} invention_budget={}",
            omega_mission.autonomy_envelope,
            omega_mission.search_budget,
            omega_mission.reasoning_budget,
            omega_mission.invention_budget
        ),
    );
    result.orchestration = build_omega_metadata(
        &omega_profile,
        &route_decision,
        &council_verdict,
        &omega_decision,
        &omega_mission,
        latency,
        quality,
    );
    result.applied_policy = Some(adaptive_policy);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

/// Create a hierarchical execution plan for a goal.
pub async fn plan(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let goal = body.get("goal").and_then(|v| v.as_str()).unwrap_or("");
    let context = body
        .get("context")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let mut state = state.write().await;
    let plan = state.planner.plan(goal, &context);
    drop(state);
    control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?
        .healing
        .report_health("planner", 0.0, None, 0);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(plan)))
}

/// Get ready-to-execute tasks from active plans.
pub async fn plan_ready_tasks(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let tasks: Vec<_> = state.planner.get_ready_tasks();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({"ready_tasks": tasks}))))
}

/// Run one self-evolution step.
pub async fn evolve(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let fitness = body.get("fitness").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;
    let new_params = control_runtime.evolution.evolve_step(fitness);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "generation": control_runtime.evolution.get_stats(),
        "new_params": new_params,
        "weak_components": control_runtime.evolution.get_weak_components(),
    }))))
}

/// Get self-evolution statistics.
pub async fn evolution_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let control_runtime = control_runtime
        .read()
        .map_err(|_| control_runtime_lock_error())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(control_runtime.evolution.get_stats())))
}

/// Report subsystem health and get healing actions.
pub async fn report_health(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let subsystem = body
        .get("subsystem")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let latency = body
        .get("latency_ms")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let error = body.get("error").and_then(|v| v.as_str());
    let memory = body
        .get("memory_bytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;
    let actions = control_runtime
        .healing
        .report_health(subsystem, latency, error, memory);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "healing_actions": actions,
        "system_health": control_runtime.healing.system_health(),
    }))))
}

/// Get full system health status.
pub async fn system_health(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let control_runtime = control_runtime
        .read()
        .map_err(|_| control_runtime_lock_error())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(control_runtime.healing.system_health())))
}

/// Store a memory in the knowledge graph.
pub async fn memory_store(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let category = match body
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("concept")
    {
        "fact" => crate::intelligence::memory_graph::MemoryCategory::Fact,
        "procedure" => crate::intelligence::memory_graph::MemoryCategory::Procedure,
        "episode" => crate::intelligence::memory_graph::MemoryCategory::Episode,
        "skill" => crate::intelligence::memory_graph::MemoryCategory::Skill,
        "context" => crate::intelligence::memory_graph::MemoryCategory::Context,
        _ => crate::intelligence::memory_graph::MemoryCategory::Concept,
    };
    let importance = body
        .get("importance")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);

    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;
    let id =
        control_runtime
            .memory_graph
            .store(content, category, importance, serde_json::json!({}));

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({"memory_id": id}))))
}

/// Query the memory graph by semantic similarity.
pub async fn memory_query(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let query = body.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let top_k = body.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;
    let results = control_runtime.memory_graph.retrieve(query, top_k);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({"results": results}))))
}

/// Get memory graph statistics.
pub async fn memory_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let control_runtime = control_runtime
        .read()
        .map_err(|_| control_runtime_lock_error())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(control_runtime.memory_graph.get_stats())))
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// v2.0 â€” Network Stack Routes
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Resolve a domain through secure DNS.
pub async fn dns_resolve(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let domain = body.get("domain").and_then(|v| v.as_str()).unwrap_or("");

    let mut state = state.write().await;
    let result = state.dns.resolve(domain).await;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "record": result, "dns_stats": state.dns.get_stats(),
    }))))
}

/// Get DNS resolver statistics.
pub async fn dns_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.dns.get_stats())))
}

/// Get the full v3 engine status with all subsystem stats.
pub async fn engine_v2_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let (search_runtime, cognitive_runtime, control_runtime) = {
        let state = state.read().await;
        (
            Arc::clone(&state.search_runtime),
            Arc::clone(&state.cognitive_runtime),
            Arc::clone(&state.control_runtime),
        )
    };
    let (swarm_queries, truth_stats) = {
        let search_runtime = search_runtime
            .read()
            .map_err(|_| search_runtime_lock_error())?;
        (
            search_runtime.swarm.total_queries(),
            search_runtime.truth.get_stats(),
        )
    };
    let (reasoning_stats, thinking_loop_stats) = {
        let cognitive_runtime = cognitive_runtime
            .read()
            .map_err(|_| cognitive_runtime_lock_error())?;
        (
            cognitive_runtime.reasoning.get_stats(),
            cognitive_runtime.thinking_loop.get_stats(),
        )
    };
    let (
        evolution_stats,
        healing_stats,
        memory_graph_stats,
        self_reflection_stats,
        omega_executive_stats,
        omega_brain_stats,
        omega_router_stats,
        omega_council_stats,
    ) = {
        let control_runtime = control_runtime
            .read()
            .map_err(|_| control_runtime_lock_error())?;
        (
            control_runtime.evolution.get_stats(),
            control_runtime.healing.get_stats(),
            control_runtime.memory_graph.get_stats(),
            control_runtime.self_reflection.get_stats(),
            control_runtime.omega_executive.get_stats(),
            control_runtime.local_brain.get_stats(),
            control_runtime.brain_router.get_stats(),
            control_runtime.brain_council.get_stats(),
        )
    };
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "version": "3.0.0",
        "intelligence": {
            "reasoning": reasoning_stats,
            "planner": state.planner.get_stats(),
            "evolution": evolution_stats,
            "healing": healing_stats,
            "memory_graph": memory_graph_stats,
            "thinking_loop": thinking_loop_stats,
            "self_reflection": self_reflection_stats,
            "omega_executive": omega_executive_stats,
            "omega_brain": omega_brain_stats,
            "omega_router": omega_router_stats,
            "omega_council": omega_council_stats,
        },
        "governance": state.governance.get_status(),
        "network": {
            "dns": state.dns.get_stats(),
            "interceptor": state.interceptor.get_stats(),
        },
        "features": {
            "memory_count": state.memory.count(),
            "swarm_queries": swarm_queries,
            "mev_stats": state.mev_shield.get_stats(),
            "truth_stats": truth_stats,
            "identity_stats": state.identity.get_stats(),
            "compute_stats": state.compute.get_stats(),
        },
        "agents": state.agents.get_system_stats(),
        "runtime": {
            "agent_catalog_size": state.agents.capability_catalog().len(),
            "tool_catalog": state.tool_registry.snapshot(),
        },
        "assistant": state.personal_assistant.capability_snapshot(),
        "chain": state.chain.get_chain_info(),
    }))))
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// v3.0 â€” Governance Routes
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Submit an ASI upgrade proposal for Court + Army review.
pub async fn governance_propose_upgrade(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let request = crate::governance::layer::ASIUpgradeRequest {
        module_name: body
            .get("module_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        title: body
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        description: body
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        changes_summary: body
            .get("changes_summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        risk_assessment: body
            .get("risk_assessment")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        rollback_plan: body
            .get("rollback_plan")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        dependencies: body
            .get("dependencies")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        urgency: body
            .get("urgency")
            .and_then(|v| v.as_str())
            .unwrap_or("routine")
            .to_string(),
    };

    let mut state = state.write().await;
    let verdict = state.governance.propose_upgrade(request);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(verdict)))
}

/// Get governance status.
pub async fn governance_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.governance.get_status())))
}

/// Check if an ASI action is allowed.
pub async fn governance_check_action(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let target = body
        .get("target_module")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut state = state.write().await;
    let allowed = state.governance.check_action_allowed(action, target);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "action": action,
        "target_module": target,
        "allowed": allowed,
        "defcon": state.governance.get_defcon().as_u8(),
    }))))
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// v3.0 â€” Thinking Loop + Self-Reflection Routes
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Run the full Synthesize â†’ Verify â†’ Learn thinking loop.
pub async fn think(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let problem = body.get("problem").and_then(|v| v.as_str()).unwrap_or("");
    let context = body
        .get("context")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let learning_domain = infer_learning_domain(problem, &context);

    let (cognitive_runtime, control_runtime, learning_loop) = {
        let state = state.read().await;
        (
            Arc::clone(&state.cognitive_runtime),
            Arc::clone(&state.control_runtime),
            state.learning_loop.clone(),
        )
    };
    let (
        omega_decision,
        omega_mission,
        omega_profile,
        route_decision,
        council_verdict,
        adaptive_policy,
    ) = {
        let mut control_runtime = control_runtime
            .write()
            .map_err(|_| control_runtime_lock_error())?;
        let omega_decision = control_runtime
            .omega_executive
            .directive_for(problem, &context);
        let omega_mission = control_runtime
            .omega_executive
            .mission_for(problem, &omega_decision);
        let omega_profile = control_runtime
            .local_brain
            .profile_request(problem, &context);
        let route_decision = control_runtime.brain_router.route(problem);
        let council_verdict = control_runtime.brain_council.deliberate(problem);
        let adaptive_policy = merge_reasoning_policy(
            to_applied_policy(learning_loop.reasoning_policy(problem, Some(&learning_domain))),
            &omega_profile,
            &route_decision,
            &council_verdict,
            &omega_decision,
        );
        (
            omega_decision,
            omega_mission,
            omega_profile,
            route_decision,
            council_verdict,
            adaptive_policy,
        )
    };
    let mut cognitive_runtime = cognitive_runtime
        .write()
        .map_err(|_| cognitive_runtime_lock_error())?;
    let mut result =
        cognitive_runtime
            .thinking_loop
            .think_adaptive(problem, &context, Some(&adaptive_policy));
    drop(cognitive_runtime);
    let latency = result.total_duration_ms;
    let quality =
        ((result.final_confidence + (1.0 - result.hallucination_score)) / 2.0).clamp(0.0, 1.0);

    let learning_trace = emit_thinking_trace(problem, &learning_domain, &result);
    ingest_learning_trace(
        &learning_loop,
        objective_for_domain(&learning_domain),
        learning_trace,
    );

    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;

    // Feed result to self-reflection
    control_runtime
        .self_reflection
        .reflect(crate::intelligence::self_reflection::TaskOutcome {
            task_id: crate::crypto::hash::sha3_256_hex(problem.as_bytes())[..12].to_string(),
            task_description: problem.to_string(),
            success: result.verification_passed,
            confidence: result.final_confidence,
            duration_ms: result.total_duration_ms,
            strategy_used: "ThinkingLoop".to_string(),
            iterations: result.iterations,
            error: if result.verification_passed {
                None
            } else {
                Some("Did not pass verification".to_string())
            },
            domain: None,
            complexity: None,
        });

    if result.verification_passed {
        control_runtime.brain_router.record_success(
            &route_decision.selected_node,
            latency,
            omega_profile.recommended_expansions as u64,
        );
    } else {
        control_runtime
            .brain_router
            .record_failure(&route_decision.selected_node);
    }
    control_runtime.local_brain.record_outcome(
        &omega_profile,
        latency,
        result.verification_passed,
        quality,
    );
    control_runtime.omega_executive.record_cycle_with_context(
        search_lane_for(omega_decision.mode),
        omega_decision.mode,
        &omega_decision.focus_domains,
        result.verification_passed,
        quality,
    );
    result.reasoning_trace.insert(
        0,
        format!(
            "[omega] lane={} route={} depth={} expansions={} agreement={:.2}",
            omega_profile.cache_lane,
            route_decision.selected_node,
            omega_profile.recommended_depth,
            omega_profile.recommended_expansions,
            council_verdict.agreement_score
        ),
    );
    result.reasoning_trace.insert(
        1,
        format!(
            "[executive] mode={:?} autonomy={:.2} search_intensity={:.2} invention_bias={:.2}",
            omega_decision.mode,
            omega_decision.autonomy_score,
            omega_decision.search_intensity,
            omega_decision.invention_bias
        ),
    );
    result.reasoning_trace.insert(
        2,
        format!(
            "[mission] autonomy_envelope={} search_budget={} reasoning_budget={} invention_budget={}",
            omega_mission.autonomy_envelope,
            omega_mission.search_budget,
            omega_mission.reasoning_budget,
            omega_mission.invention_budget
        ),
    );
    result.orchestration = build_omega_metadata(
        &omega_profile,
        &route_decision,
        &council_verdict,
        &omega_decision,
        &omega_mission,
        latency,
        quality,
    );
    result.applied_policy = Some(adaptive_policy);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn reason_brain(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let problem = body.get("problem").and_then(|v| v.as_str()).unwrap_or("");
    let context = body
        .get("context")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let mut state = state.write().await;
    let artifacts = state.execute_brain(
        BrainExecuteRequest {
            task: problem.to_string(),
            context,
            session_id: None,
            mode_hint: Some(BrainModeHint::Reason),
            allow_mutation: false,
            require_live_retrieval: false,
            require_verification: true,
        },
        None,
    )?;
    let result = artifacts.reasoning_result.ok_or_else(|| {
        AstraError::Internal("brain reasoning lane did not return a result".into())
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn plan_brain(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let goal = body.get("goal").and_then(|v| v.as_str()).unwrap_or("");
    let context = body
        .get("context")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let mut state = state.write().await;
    let artifacts = state.execute_brain(
        BrainExecuteRequest {
            task: goal.to_string(),
            context,
            session_id: None,
            mode_hint: Some(BrainModeHint::Plan),
            allow_mutation: false,
            require_live_retrieval: false,
            require_verification: true,
        },
        None,
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(artifacts.execution_plan)))
}

pub async fn think_brain(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let problem = body.get("problem").and_then(|v| v.as_str()).unwrap_or("");
    let context = body
        .get("context")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let mut state = state.write().await;
    let artifacts = state.execute_brain(
        BrainExecuteRequest {
            task: problem.to_string(),
            context,
            session_id: None,
            mode_hint: Some(BrainModeHint::Think),
            allow_mutation: false,
            require_live_retrieval: false,
            require_verification: true,
        },
        None,
    )?;
    let result = artifacts.thinking_result.ok_or_else(|| {
        AstraError::Internal("brain thinking lane did not return a result".into())
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

/// Get self-reflection stats including cognitive genome.
pub async fn reflection_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let control_runtime = control_runtime
        .read()
        .map_err(|_| control_runtime_lock_error())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(control_runtime.self_reflection.get_stats())))
}

pub async fn omega_invent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let prompt = body
        .get("prompt")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let focus_domains = body
        .get("domains")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let limit = body
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(4) as usize;

    let context = serde_json::json!({ "domains": focus_domains.clone() });
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;
    let decision = control_runtime
        .omega_executive
        .directive_for(prompt, &context);
    let mission = control_runtime
        .omega_executive
        .mission_for(prompt, &decision);
    let inventions = control_runtime.omega_executive.invent(
        prompt,
        &decision.focus_domains,
        &focus_domains,
        limit.min(mission.invention_budget.max(1)),
    );
    let quality = inventions.iter().map(|item| item.impact_score).sum::<f64>()
        / inventions.len().max(1) as f64;
    control_runtime.omega_executive.record_cycle_with_context(
        search_lane_for(ExecutiveMode::Invent),
        ExecutiveMode::Invent,
        &decision.focus_domains,
        !inventions.is_empty(),
        quality,
    );

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "decision": decision,
        "mission": mission,
        "inventions": inventions,
        "stats": control_runtime.omega_executive.get_stats(),
    }))))
}

pub async fn omega_mission(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let prompt = body
        .get("prompt")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let focus_domains = body
        .get("domains")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let high_impact = body
        .get("high_impact")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let context = serde_json::json!({
        "domains": focus_domains,
        "high_impact": high_impact,
    });
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;
    let decision = control_runtime
        .omega_executive
        .directive_for(prompt, &context);
    let mission = control_runtime
        .omega_executive
        .mission_for(prompt, &decision);

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "decision": decision,
        "mission": mission,
        "stats": control_runtime.omega_executive.get_stats(),
    }))))
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// v4.0 â€” Defender Routes
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Trigger a file scan (quick/full/custom).
pub async fn defender_scan(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let mode = match body.get("mode").and_then(|v| v.as_str()).unwrap_or("quick") {
        "full" => ScanMode::Full,
        "custom" => ScanMode::Custom,
        _ => ScanMode::Quick,
    };
    let custom_paths = body.get("paths").and_then(|v| v.as_array()).map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    });

    let mut state = state.write().await;
    let result = state.defender.scan_and_quarantine(mode, custom_paths);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

/// Get real-time defender status.
pub async fn defender_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.defender.get_status())))
}

/// List all detected threats.
pub async fn defender_threats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let threats = state.defender.quarantine.list_active();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "threats": threats,
        "count": threats.len(),
    }))))
}

/// Restore a quarantined file.
pub async fn defender_restore(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let id = body
        .get("quarantine_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let mut state = state.write().await;
    match state.defender.quarantine.restore(id) {
        Ok(item) => Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
            "restored": true, "item": item,
        })))),
        Err(e) => Ok(HttpResponse::BadRequest().json(serde_json::json!({"error": e}))),
    }
}

/// Run system cleaner.
pub async fn defender_clean(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let report = state.defender.run_clean();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// Run vulnerability assessment.
pub async fn defender_vulnerability(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let report = state.defender.run_vulnerability_scan();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// List monitored processes with risk scores.
pub async fn defender_processes(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let processes = state.defender.scan_processes();
    let flagged: Vec<_> = processes
        .iter()
        .filter(|p| p.reputation_score < 0.5)
        .collect();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "total_processes": processes.len(),
        "flagged_count": flagged.len(),
        "flagged": flagged,
    }))))
}

/// Synchronize Defender intelligence with ASI Brain and Governance Layer.
pub async fn defender_sync(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let control_runtime = {
        let state = state.read().await;
        Arc::clone(&state.control_runtime)
    };
    let assessments = {
        let mut state = state.write().await;
        state.defender.assess_all_threats()
    };

    let mut brain_updates = 0;

    let mut control_runtime = control_runtime
        .write()
        .map_err(|_| control_runtime_lock_error())?;
    for assessment in &assessments {
        control_runtime.memory_graph.store(
            &format!(
                "THREAT DETECTED: [{:?}] {} - {}",
                assessment.threat_level, assessment.id, assessment.correlation
            ),
            crate::intelligence::memory_graph::MemoryCategory::Episode,
            assessment.composite_score, // importance = threat score
            serde_json::to_value(&assessment).unwrap_or_default(),
        );
        brain_updates += 1;
    }
    drop(control_runtime);

    let (governance_escalations, current_defcon) = {
        let mut state = state.write().await;
        let mut governance_escalations = 0;
        // 2. Governance Link: Escalate via Army if Critical/APT
        use crate::defender::threat_intel::ThreatLevel;
        for assessment in &assessments {
            if matches!(
                assessment.threat_level,
                ThreatLevel::Critical | ThreatLevel::APT
            ) {
                state.governance.report_threat(
                    crate::governance::army::ThreatType::Malware,
                    &assessment.id,
                    &assessment.correlation,
                    assessment.composite_score,
                );
                governance_escalations += 1;
            }
        }
        (
            governance_escalations,
            state.governance.get_defcon().as_u8(),
        )
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "assessments_processed": assessments.len(),
        "brain_memories_created": brain_updates,
        "governance_escalations": governance_escalations,
        "current_defcon": current_defcon,
    }))))
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// v5.0 â€” Invention & Discovery Engine Routes
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

// â”€â”€ 1. Quantum Tab Branching â”€â”€

pub async fn quantum_fork(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let tab_id = body.get("tab_id").and_then(|v| v.as_str()).unwrap_or("");
    let parent_id = body
        .get("parent_branch_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let branch = state.quantum.fork(tab_id, parent_id, url, title);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(branch)))
}

pub async fn quantum_navigate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let branch_id = body.get("branch_id").and_then(|v| v.as_str()).unwrap_or("");
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let divergence = state.quantum.navigate_branch(branch_id, url, title);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        serde_json::json!({"divergence": divergence}),
    )))
}

pub async fn quantum_collapse(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let branch_id = body.get("branch_id").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let collapsed = state.quantum.collapse_branch(branch_id);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        serde_json::json!({"collapsed_count": collapsed}),
    )))
}

pub async fn quantum_merge(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let ids: Vec<String> = body
        .get("branch_ids")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let mut state = state.write().await;
    let result = state.quantum.merge_branches(&ids);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn quantum_tree(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let tab_id = body.get("tab_id").and_then(|v| v.as_str()).unwrap_or("");
    let state = state.read().await;
    let tree = state.quantum.get_branch_tree(tab_id);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        serde_json::json!({"tree": tree, "stats": state.quantum.get_stats()}),
    )))
}

// â”€â”€ 2. Hostile Architecture Neutralizer â”€â”€

pub async fn semantic_render_analyze(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SemanticRenderRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let render = state.semantic_render.render(request.clone())?;
    let hostile_scan = state.hostile_arch.scan_page(&render.url, &request.html);
    let epistemic_report = state
        .epistemic
        .analyze_page(&render.url, &render.combined_text());

    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:semantic-render".into());
        let commitment = state.chain.notarize_resource(
            &render.normalized_url,
            &render.content_hash,
            &render.content_type,
            "semantic_render",
            &attestor,
            serde_json::json!({
                "artifact": "semantic_render",
                "render_id": render.render_id.clone(),
                "title": render.metadata.title.clone(),
                "segment_count": render.summary.segment_count,
                "link_count": render.summary.link_count,
                "form_count": render.summary.form_count,
                "structured_data_blocks": render.summary.structured_data_blocks,
                "warnings": render.warnings.clone(),
            }),
        )?;
        state.semantic_render.record_chain_commit();
        Some(commitment)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "render": render,
        "hostile_scan": hostile_scan,
        "epistemic_report": epistemic_report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn semantic_render_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.semantic_render.stats())))
}

pub async fn semantic_acquisition_execute(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SemanticAcquisitionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let prepared = {
        let mut state = state.write().await;
        state.semantic_acquisition.prepare(&request)?
    };

    let fetch_result = execute_semantic_acquisition(&prepared).await;
    let mut state = state.write().await;

    match fetch_result {
        Ok(outcome) => {
            let render = state.semantic_render.render(SemanticRenderRequest {
                url: outcome.final_url.clone(),
                html: outcome.semantic_html.clone(),
                content_type: outcome.content_type.clone(),
                attestor: request.attestor.clone(),
                notarize_to_chain: false,
                max_segments: request.max_segments,
                max_links: request.max_links,
            })?;
            let hostile_scan = state
                .hostile_arch
                .scan_page(&render.url, &outcome.semantic_html);
            let epistemic_report = state
                .epistemic
                .analyze_page(&render.url, &render.combined_text());
            let adapter_outcome = outcome.clone();
            let acquisition = state
                .semantic_acquisition
                .finalize_success(&prepared, outcome, render)?;
            let adapter_report = analyze_semantic_adapters(&acquisition, &adapter_outcome);

            let ledger_commitment = if request.notarize_to_chain {
                let attestor = request
                    .attestor
                    .clone()
                    .unwrap_or_else(|| "assistant:semantic-acquisition".into());
                let commitment = state.chain.notarize_resource(
                    &acquisition.provenance.normalized_url,
                    &acquisition.provenance.content_hash,
                    &acquisition.provenance.content_type,
                    "semantic_acquisition",
                    &attestor,
                    serde_json::json!({
                        "artifact": "semantic_acquisition",
                        "acquisition_id": acquisition.acquisition_id,
                        "title": acquisition.render.metadata.title.clone(),
                        "status_code": acquisition.provenance.status_code,
                        "bytes_received": acquisition.provenance.bytes_received,
                        "response_class": acquisition.provenance.response_class.clone(),
                        "segment_count": acquisition.render.summary.segment_count,
                        "link_count": acquisition.render.summary.link_count,
                        "warnings": acquisition.provenance.warnings.clone(),
                    }),
                )?;
                state.semantic_render.record_chain_commit();
                state
                    .semantic_acquisition
                    .record_chain_commit(&acquisition.acquisition_id);
                Some(commitment)
            } else {
                None
            };

            Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
                "acquisition": acquisition,
                "adapter_report": adapter_report,
                "hostile_scan": hostile_scan,
                "epistemic_report": epistemic_report,
                "ledger_commitment": ledger_commitment,
            }))))
        }
        Err(failure) => {
            state
                .semantic_acquisition
                .record_failure(&prepared, &failure);
            Err(failure.into_error())
        }
    }
}

pub async fn semantic_acquisition_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.semantic_acquisition.stats())))
}

pub async fn semantic_acquisition_recent(
    state: web::Data<Arc<RwLock<EngineState>>>,
    query: web::Query<BTreeMap<String, String>>,
) -> Result<HttpResponse, AstraError> {
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10);
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.semantic_acquisition.recent_records(limit),
    )))
}

pub async fn semantic_workflow_execute(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<SemanticWorkflowRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    if request.seed_urls.is_empty() {
        return Err(AstraError::ControlPlaneRejected(
            "semantic workflow requires at least one seed URL".into(),
        ));
    }

    let workflow_id = build_workflow_id(&request.seed_urls);
    let started_at = chrono::Utc::now().timestamp_millis();
    let mut queue = VecDeque::from(
        request
            .seed_urls
            .iter()
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .collect::<Vec<_>>(),
    );
    let mut visited = BTreeSet::new();
    let mut failures = Vec::new();
    let mut pages = Vec::new();

    while let Some(url) = queue.pop_front() {
        if pages.len() >= request.max_pages.max(1) || !visited.insert(url.clone()) {
            continue;
        }

        let page_request = request.page_request(url.clone());
        let prepared = {
            let mut state = state.write().await;
            state.semantic_acquisition.prepare(&page_request)?
        };

        match execute_semantic_acquisition(&prepared).await {
            Ok(outcome) => {
                let mut state = state.write().await;
                let render = state.semantic_render.render(SemanticRenderRequest {
                    url: outcome.final_url.clone(),
                    html: outcome.semantic_html.clone(),
                    content_type: outcome.content_type.clone(),
                    attestor: request.attestor.clone(),
                    notarize_to_chain: false,
                    max_segments: request.max_segments,
                    max_links: request.max_links,
                })?;
                let hostile_scan = state
                    .hostile_arch
                    .scan_page(&render.url, &outcome.semantic_html);
                let epistemic_report = state
                    .epistemic
                    .analyze_page(&render.url, &render.combined_text());
                let adapter_outcome = outcome.clone();
                let acquisition = state
                    .semantic_acquisition
                    .finalize_success(&prepared, outcome, render)?;
                let adapter_report = analyze_semantic_adapters(&acquisition, &adapter_outcome);

                if request.follow_pagination {
                    for candidate in &adapter_report.pagination {
                        if !visited.contains(&candidate.url) {
                            queue.push_back(candidate.url.clone());
                        }
                    }
                }

                pages.push(SemanticWorkflowPage {
                    acquisition_id: acquisition.acquisition_id.clone(),
                    url: acquisition.provenance.normalized_url.clone(),
                    title: acquisition.render.metadata.title.clone(),
                    content_type: acquisition.provenance.content_type.clone(),
                    response_class: acquisition.provenance.response_class.clone(),
                    adapter_report,
                    hostile_signal_count: hostile_scan.total_detected,
                    enshittification_score: hostile_scan.enshittification_score,
                    epistemic_score: epistemic_report.epistemic_score,
                    claim_count: epistemic_report.total_claims,
                    warnings: acquisition.provenance.warnings.clone(),
                });
            }
            Err(failure) => {
                {
                    let mut state = state.write().await;
                    state
                        .semantic_acquisition
                        .record_failure(&prepared, &failure);
                }
                if !request.continue_on_error {
                    return Err(failure.into_error());
                }
                let error = failure.into_error();
                failures.push(SemanticWorkflowFailure {
                    url,
                    error: error.to_string(),
                });
            }
        }
    }

    let manifest =
        build_workflow_manifest(workflow_id.clone(), &request, started_at, pages, failures);
    let ledger_commitment = if request.notarize_manifest_to_chain {
        let payload = workflow_manifest_payload(&manifest);
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:semantic-workflow".into());
        let mut state = state.write().await;
        let commitment = state.chain.notarize_resource(
            &format!("httpa://render/manifest/{workflow_id}"),
            &manifest.manifest_hash,
            "semantic_workflow_manifest",
            "semantic_workflow",
            &attestor,
            payload,
        )?;
        state.semantic_acquisition.record_chain_commit(&workflow_id);
        Some(commitment)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "manifest": manifest,
        "ledger_commitment": ledger_commitment,
    }))))
}

fn abyss_page_request(request: &AbyssMissionRequest, url: String) -> SemanticAcquisitionRequest {
    SemanticAcquisitionRequest {
        url,
        allowed_domains: request.allowed_domains.clone(),
        respect_robots: request.respect_robots,
        timeout_ms: request.timeout_ms,
        max_response_bytes: request.max_response_bytes,
        max_retries: request.max_retries,
        max_segments: request.max_segments,
        max_links: request.max_links,
        requests_per_minute: request.requests_per_minute,
        notarize_to_chain: false,
        attestor: request.attestor.clone(),
        persist_report: request.persist_report,
        user_agent: request.user_agent.clone(),
        force_content_type: None,
    }
}

async fn execute_abyss_mission_internal(
    state: &web::Data<Arc<RwLock<EngineState>>>,
    request: &AbyssMissionRequest,
) -> Result<AbyssMissionReport, AstraError> {
    request.validate()?;

    let mut queue = VecDeque::from(
        request
            .seed_urls
            .iter()
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .collect::<Vec<_>>(),
    );
    let mut visited = BTreeSet::new();
    let mut failures = Vec::new();
    let mut pages = Vec::new();

    while let Some(url) = queue.pop_front() {
        if pages.len() >= request.max_pages.max(1) || !visited.insert(url.clone()) {
            continue;
        }

        let page_request = abyss_page_request(request, url.clone());
        let prepared = {
            let mut state = state.write().await;
            state.semantic_acquisition.prepare(&page_request)?
        };

        match execute_semantic_acquisition(&prepared).await {
            Ok(outcome) => {
                let mut state = state.write().await;
                let render = state.semantic_render.render(SemanticRenderRequest {
                    url: outcome.final_url.clone(),
                    html: outcome.semantic_html.clone(),
                    content_type: outcome.content_type.clone(),
                    attestor: request.attestor.clone(),
                    notarize_to_chain: false,
                    max_segments: request.max_segments,
                    max_links: request.max_links,
                })?;
                let hostile_scan = state
                    .hostile_arch
                    .scan_page(&render.url, &outcome.semantic_html);
                let epistemic_report = state
                    .epistemic
                    .analyze_page(&render.url, &render.combined_text());
                let adapter_outcome = outcome.clone();
                let acquisition = state
                    .semantic_acquisition
                    .finalize_success(&prepared, outcome, render)?;
                let adapter_report = analyze_semantic_adapters(&acquisition, &adapter_outcome);

                if request.follow_pagination {
                    for candidate in &adapter_report.pagination {
                        if !visited.contains(&candidate.url) {
                            queue.push_back(candidate.url.clone());
                        }
                    }
                }

                pages.push(SemanticWorkflowPage {
                    acquisition_id: acquisition.acquisition_id.clone(),
                    url: acquisition.provenance.normalized_url.clone(),
                    title: acquisition.render.metadata.title.clone(),
                    content_type: acquisition.provenance.content_type.clone(),
                    response_class: acquisition.provenance.response_class.clone(),
                    adapter_report,
                    hostile_signal_count: hostile_scan.total_detected,
                    enshittification_score: hostile_scan.enshittification_score,
                    epistemic_score: epistemic_report.epistemic_score,
                    claim_count: epistemic_report.total_claims,
                    warnings: acquisition.provenance.warnings.clone(),
                });
            }
            Err(failure) => {
                {
                    let mut state = state.write().await;
                    state
                        .semantic_acquisition
                        .record_failure(&prepared, &failure);
                }
                if !request.continue_on_error {
                    return Err(failure.into_error());
                }
                let error = failure.into_error();
                failures.push(SemanticWorkflowFailure {
                    url,
                    error: error.to_string(),
                });
            }
        }
    }

    let mut state = state.write().await;
    state.abyss.finalize_mission(request, pages, failures)
}

pub async fn abyss_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let status: AbyssCrawlerStatus = state.abyss.status();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(status)))
}

pub async fn abyss_mission_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<AbyssMissionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let report = execute_abyss_mission_internal(&state, &request).await?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:abyss".into());
        let mut state = state.write().await;
        Some(state.chain.notarize_resource(
            &format!("astra://abyss/mission/{}", report.mission_id),
            &report.manifest_hash,
            "application/json",
            "abyss_mission",
            &attestor,
            serde_json::json!({
                "mission_id": report.mission_id,
                "pages_crawled": report.summary.pages_crawled,
                "truths_confirmed": report.summary.truths_confirmed,
                "documents_excavated": report.summary.documents_excavated,
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

fn personal_domains(
    abyss_report: Option<&AbyssMissionReport>,
    request: &PersonalMissionRequest,
) -> Vec<String> {
    let mut domains = BTreeSet::new();
    for domain in &request.allowed_domains {
        let normalized = domain.trim().to_ascii_lowercase();
        if !normalized.is_empty() {
            domains.insert(normalized);
        }
    }
    if let Some(report) = abyss_report {
        for page in &report.page_digests {
            if let Ok(url) = url::Url::parse(&page.url) {
                if let Some(host) = url.host_str() {
                    domains.insert(host.to_ascii_lowercase());
                }
            }
        }
    }
    domains.into_iter().collect()
}

fn personal_source_documents(
    abyss_report: Option<&AbyssMissionReport>,
    fallback_domain: &str,
) -> Vec<crate::features::ultimate_astra_features::SourceDocumentInput> {
    let mut sources = Vec::new();
    if let Some(report) = abyss_report {
        for page in report.page_digests.iter().take(4) {
            sources.push(
                crate::features::ultimate_astra_features::SourceDocumentInput {
                    title: page.title.clone(),
                    domain: fallback_domain.to_string(),
                    content: format!(
                        "{} | adapters={} | notes={}",
                        page.title,
                        page.adapter_kinds.join(", "),
                        page.notes.join("; ")
                    ),
                    authority_score: page.epistemic_score.clamp(0.2, 0.98),
                },
            );
        }
    }
    sources
}

fn compute_personal_scores(
    abyss_report: Option<&AbyssMissionReport>,
    cognition_score: f64,
    immune_patrol: &ImmunePatrolReport,
    weak_components: usize,
) -> (f64, f64) {
    let abyss_score = abyss_report.map_or(0.62, |report| {
        let coverage = (report.summary.pages_crawled as f64 / 4.0).clamp(0.0, 1.0);
        let truth = if report.summary.pages_crawled == 0 {
            0.4
        } else {
            report.summary.truths_confirmed as f64 / report.summary.pages_crawled as f64
        };
        (0.45 + (coverage * 0.25) + (truth * 0.30)).clamp(0.35, 0.98)
    });
    let immune_total = immune_patrol.healthy_invariants + immune_patrol.degraded_invariants.len();
    let immune_score = if immune_total == 0 {
        0.5
    } else {
        immune_patrol.healthy_invariants as f64 / immune_total as f64
    };
    let readiness_score =
        ((abyss_score + cognition_score.clamp(0.0, 1.0) + immune_score) / 3.0).clamp(0.2, 0.99);
    let fitness = (readiness_score - (weak_components as f64 * 0.08)).clamp(0.05, 0.99);
    (readiness_score, fitness)
}

pub async fn personal_mission_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let status: PersonalMissionStatus = state.personal_mission.status();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(status)))
}

pub async fn personal_mission_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<PersonalMissionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    if request.objective.trim().is_empty() {
        return Err(AstraError::ControlPlaneRejected(
            "personal mission requires a non-empty objective".into(),
        ));
    }

    let abyss_report = if let Some(abyss_request) = request.abyss_request() {
        Some(execute_abyss_mission_internal(&state, &abyss_request).await?)
    } else {
        None
    };
    let domains = personal_domains(abyss_report.as_ref(), &request);
    let primary_domain = domains
        .first()
        .cloned()
        .unwrap_or_else(|| "personal_operations".into());
    let source_documents = personal_source_documents(abyss_report.as_ref(), &primary_domain);
    let mission_text = request
        .current_focus
        .clone()
        .unwrap_or_else(|| request.objective.clone());

    let mut state = state.write().await;
    let omega_context = serde_json::json!({
        "seed_urls": request.seed_urls.clone(),
        "domains": domains.clone(),
        "private_mode": request.private_mode,
        "open_files": request.open_files.clone(),
        "assets": request.local_assets.clone(),
        "threat_signals": request.threat_signals.clone(),
    });
    let (omega_decision, omega_mission) = {
        let mut control_runtime = state
            .control_runtime
            .write()
            .map_err(|_| control_runtime_lock_error())?;
        let omega_decision = control_runtime
            .omega_executive
            .directive_for(&request.objective, &omega_context);
        let omega_mission = control_runtime
            .omega_executive
            .mission_for(&request.objective, &omega_decision);
        (omega_decision, omega_mission)
    };

    let timestamp_ms = chrono::Utc::now().timestamp_millis();
    let sandbox_report = state
        .revolutionary
        .analyze_sandbox(RevolutionarySandboxRequest {
            execution_target: format!(
                "objective={}\nfocus={}\nsteps={}",
                request.objective,
                mission_text,
                omega_mission
                    .steps
                    .iter()
                    .map(|step| step.objective.clone())
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            objective: request.objective.clone(),
            language: None,
            allowed_paths: vec!["C:\\astra browser".into()],
            allowed_hosts: domains.clone(),
            expected_outputs: omega_mission.exit_criteria.clone(),
            environment: BTreeMap::new(),
            require_firebreaks: true,
            notarize_to_chain: false,
            attestor: request.attestor.clone(),
        })?;
    let protocol_report = state.revolutionary.orchestrate_protocol(
        crate::features::revolutionary_features::RevolutionaryProtocolRequest {
            packets: omega_mission
                .steps
                .iter()
                .enumerate()
                .map(
                    |(idx, step)| crate::features::revolutionary_features::SemanticPacketInput {
                        packet_id: format!("packet-{idx}"),
                        content: format!("{} {}", step.stage, step.objective),
                        semantic_type: Some(step.lane.clone()),
                        semantic_dependencies: vec![step.success_signal.clone()],
                    },
                )
                .collect(),
            requests: omega_mission
                .steps
                .iter()
                .enumerate()
                .map(
                    |(idx, step)| crate::features::revolutionary_features::ProtocolRequestInput {
                        request_id: format!("request-{idx}"),
                        intent: step.objective.clone(),
                        trust_profile: omega_mission.autonomy_envelope.clone(),
                        submitted_at_ms: timestamp_ms + idx as i64,
                        dependencies: vec![step.success_signal.clone()],
                        arrival_rank: idx,
                        priority: if idx == 0 {
                            "critical".into()
                        } else {
                            "normal".into()
                        },
                    },
                )
                .collect(),
            system_load: crate::features::revolutionary_features::ProtocolSystemLoad {
                cpu_utilization: (omega_mission.reasoning_budget as f64 / 16.0).clamp(0.1, 0.95),
                memory_pressure: (omega_mission.search_budget as f64 / 16.0).clamp(0.1, 0.85),
                network_congestion: abyss_report.as_ref().map_or(0.12, |report| {
                    (report.summary.pages_crawled as f64 / 16.0).clamp(0.1, 0.8)
                }),
                queue_depth: omega_mission.steps.len() as u32,
            },
            outcome_samples: vec![
                crate::features::revolutionary_features::ProtocolOutcomeSample {
                    success: true,
                    latency_ms: 180,
                    useful_bytes: 1024,
                    total_bytes: 1400,
                    recovered_errors: 0,
                },
            ],
            fold_window_ms: Some(250),
            notarize_to_chain: false,
            attestor: request.attestor.clone(),
        },
    )?;
    let resilience_report = state.revolutionary.orchestrate_resilience(
        crate::features::revolutionary_features::RevolutionaryResilienceRequest {
            transitions: omega_mission
                .steps
                .iter()
                .enumerate()
                .map(
                    |(idx, step)| crate::features::revolutionary_features::CausalTransitionInput {
                        transition_id: format!("transition-{idx}"),
                        parent_id: (idx > 0).then(|| format!("transition-{}", idx - 1)),
                        subsystem: step.lane.clone(),
                        change: step.objective.clone(),
                        trigger: step.success_signal.clone(),
                        abnormality_score: if idx == 0 { 0.08 } else { 0.16 },
                        timestamp_ms: timestamp_ms + idx as i64,
                    },
                )
                .collect(),
            failure: crate::features::revolutionary_features::FailureSymptomInput {
                transition_id: "transition-0".into(),
                symptom: if sandbox_report.verdict.allow_real_execution {
                    "monitoring only".into()
                } else {
                    "execution remains simulation-only".into()
                },
            },
            sonar_probes: vec![crate::features::revolutionary_features::SonarProbeInput {
                probe_id: "personal-loop".into(),
                target_subsystem: "personal_mission".into(),
                baseline_latency_ms: 120.0,
                current_latency_ms: 145.0,
                baseline_size_bytes: 2048.0,
                current_size_bytes: 2200.0,
                baseline_error_rate: 0.01,
                current_error_rate: 0.03,
                sigma: 2.0,
            }],
            assumptions: vec![
                crate::features::revolutionary_features::RealityAssumptionInput {
                    asset_id: "mission_privacy".into(),
                    statement: "private personal mode should avoid unsafe externalization".into(),
                    expected: 1.0,
                    observed: if request.private_mode { 1.0 } else { 0.75 },
                    tolerance: 0.2,
                    compensation_kind: "tighten_scope".into(),
                },
            ],
            sla_statements: vec!["owner-facing missions should stay reviewable".into()],
            sla_windows: vec![
                crate::features::revolutionary_features::SlaObservationWindow {
                    window_id: "owner-review".into(),
                    total_requests: omega_mission.steps.len() as u64 + 1,
                    compliant_requests: omega_mission.steps.len() as u64 + 1,
                    observed_latency_ms: 145.0,
                },
            ],
            components: vec![
                crate::features::revolutionary_features::DependencyComponentInput {
                    component_id: "abyss".into(),
                    depends_on: vec!["semantic_acquisition".into(), "semantic_render".into()],
                    invariants: vec!["permitted_domains_only".into()],
                },
                crate::features::revolutionary_features::DependencyComponentInput {
                    component_id: "personal_mission".into(),
                    depends_on: vec![
                        "revolutionary".into(),
                        "ultimate".into(),
                        "unbreakable".into(),
                    ],
                    invariants: vec!["cpu_only".into(), "reviewable".into()],
                },
            ],
            proposed_changes: vec![
                crate::features::revolutionary_features::ProposedChangeInput {
                    target: "personal_mission".into(),
                    change_type: "adaptive_tuning".into(),
                    summary: "rebalance mission parameters after each run".into(),
                    severity: "moderate".into(),
                },
            ],
            anomaly_events: vec![crate::features::revolutionary_features::AnomalyEventInput {
                event_id: "owner-loop".into(),
                source: "personal_mission".into(),
                structural_score: 0.18,
                temporal_score: 0.12,
                semantic_score: 0.2,
            }],
            speculative_operations: vec![
                crate::features::revolutionary_features::SpeculativeOperationInput {
                    operation_id: "evolve-personal-loop".into(),
                    subsystem: "personal_mission".into(),
                    predicted_error: None,
                    known_repairs: vec![
                        "reduce autonomy envelope".into(),
                        "increase review prompts".into(),
                    ],
                },
            ],
            notarize_to_chain: false,
            attestor: request.attestor.clone(),
        },
    )?;
    let value_report = if let (Some(receiver), Some(amount)) =
        (request.budget_receiver.clone(), request.budget_amount)
    {
        let currency = request
            .budget_currency
            .clone()
            .unwrap_or_else(|| "USD".into());
        let mut revolutionary = std::mem::take(&mut state.revolutionary);
        let result = revolutionary.orchestrate_value(
            RevolutionaryValueRequest {
                sender: "owner".into(),
                receiver,
                amount: amount.max(0.01),
                purpose: format!("personal mission budget for {}", request.objective),
                currency,
                settlement_rail: crate::chain::value_protocol::SettlementRail::InternalLedger,
                compliance: crate::chain::value_protocol::ComplianceMode::TaxAware,
                privacy_mode:
                    crate::chain::value_protocol::PrivacyPreservationMode::AttestedMinimization,
                risk_tier: crate::chain::value_protocol::EnterpriseRiskTier::Standard,
                connector_id: None,
                providers: Vec::new(),
                allow_split_routing: true,
                require_escrow: true,
                autocommit: false,
                fulfillment_window_seconds: 3_600,
                min_quality_score: 0.9,
                evidence: vec![request.objective.clone()],
                cooperative_unlock_parties: vec!["owner".into()],
                metadata: None,
                notarize_to_chain: false,
                attestor: request.attestor.clone(),
            },
            &mut state.chain,
        );
        state.revolutionary = revolutionary;
        Some(result?)
    } else {
        None
    };

    let cognition_report = state
        .ultimate
        .orchestrate_cognition(UltimateCognitionRequest {
            problem: request.objective.clone(),
            candidate_decisions: omega_mission
                .steps
                .iter()
                .take(4)
                .map(
                    |step| crate::features::ultimate_astra_features::CognitiveDecisionInput {
                        content: step.objective.clone(),
                        immediate_score: 0.72,
                    },
                )
                .collect(),
            solved_patterns: vec![
                crate::features::ultimate_astra_features::SolvedPatternInput {
                    label: "personal_research_loop".into(),
                    original_domain: primary_domain.clone(),
                    solution_template: "observe -> verify -> summarize -> adapt".into(),
                    structural_dna: vec![
                        crate::features::ultimate_astra_features::StructuralGene::FeedbackLoop,
                        crate::features::ultimate_astra_features::StructuralGene::Optimization,
                    ],
                    transfer_success_rate: 0.78,
                },
            ],
            contradictory_claims: abyss_report
                .as_ref()
                .map(|report| {
                    report
                        .truth_triangulation
                        .iter()
                        .filter(|finding| finding.requires_review)
                        .take(2)
                        .map(|finding| {
                            crate::features::ultimate_astra_features::ContradictoryClaimInput {
                                claim_a: finding.subject.clone(),
                                score_a: finding.confidence,
                                claim_b: format!(
                                    "manual verification required for {}",
                                    finding.subject
                                ),
                                score_b: 0.5,
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|claims| !claims.is_empty())
                .unwrap_or_else(|| {
                    vec![
                        crate::features::ultimate_astra_features::ContradictoryClaimInput {
                            claim_a: "move fast".into(),
                            score_a: 0.62,
                            claim_b: "preserve auditability".into(),
                            score_b: 0.82,
                        },
                    ]
                }),
            active_tasks: omega_mission
                .steps
                .iter()
                .enumerate()
                .map(
                    |(idx, step)| crate::features::ultimate_astra_features::AttentionTaskInput {
                        task_id: format!("task-{idx}"),
                        description: step.objective.clone(),
                        urgency: 0.7,
                        importance: 0.8,
                        decay_rate: 0.12,
                        user_proximity: 0.95,
                        floor: 0.2,
                        cycles_starved: 0,
                    },
                )
                .collect(),
            behavior_signals: vec![
                crate::features::ultimate_astra_features::BehaviorSignalInput {
                    predicted_intent: mission_text.clone(),
                    confidence: 0.82,
                    typical_hour: Some(9),
                    context_chain: request.local_focus_tags.clone(),
                },
            ],
            concept_clusters: request
                .local_focus_tags
                .iter()
                .take(4)
                .map(
                    |tag| crate::features::ultimate_astra_features::ConceptClusterInput {
                        label: tag.clone(),
                        aliases: vec![format!("{tag}_owner")],
                        adjacent_concepts: domains.clone(),
                        last_activated_days_ago: 0,
                        activation: 0.88,
                    },
                )
                .collect(),
            idle_seconds: 120,
            max_depth: omega_mission.reasoning_budget.max(4).min(10),
            time_horizons_minutes: vec![15, 120, 1_440],
            attestor: request.attestor.clone(),
            notarize_to_chain: false,
        })?;
    let acquisition_report =
        state
            .ultimate
            .orchestrate_acquisition(UltimateAcquisitionRequest {
                targets: request
                    .seed_urls
                    .iter()
                    .map(
                        |url| crate::features::ultimate_astra_features::AcquisitionTargetInput {
                            url: url.clone(),
                            semantic_goal: request.objective.clone(),
                            desired_geo: None,
                            priority: "owner".into(),
                        },
                    )
                    .collect(),
                proxies: Vec::new(),
                cover_domains: domains.clone(),
                scatter_window_ms: 30_000,
                min_mix_batch: 4,
                allowed_profile_families: vec!["verified_httpa".into()],
                respect_robots: request.respect_robots,
                attestor: request.attestor.clone(),
                notarize_to_chain: false,
            })?;
    let assistant_report = state
        .ultimate
        .orchestrate_assistant(UltimateAssistantRequest {
            text: mission_text.clone(),
            typing_speed_wpm: 0.0,
            deletions: 0,
            total_keystrokes: mission_text.len() as u32,
            has_code_open: !request.open_files.is_empty(),
            recent_errors: request.threat_signals.len() as u32,
            is_week_start: false,
            is_morning: true,
            open_files: request.open_files.clone(),
            clipboard_excerpt: None,
            events: Vec::new(),
            metrics: vec![
                crate::features::ultimate_astra_features::MetricSignalInput {
                    metric: "weak_components".into(),
                    value: 0.0,
                    threshold: 2.0,
                    direction: "below".into(),
                },
            ],
            goal: request.objective.clone(),
            seed_cells: request
                .service_offers
                .iter()
                .take(3)
                .map(
                    |offer| crate::features::ultimate_astra_features::GoalCellInput {
                        description: offer.clone(),
                        status: "planned".into(),
                        health_score: 0.72,
                        complexity: 0.45,
                    },
                )
                .collect(),
            schedule_tasks: omega_mission
                .steps
                .iter()
                .enumerate()
                .map(
                    |(idx, step)| crate::features::ultimate_astra_features::ScheduleTaskInput {
                        task_id: format!("schedule-{idx}"),
                        title: step.objective.clone(),
                        duration_minutes: 30,
                        energy_required: 0.7,
                        context: step.lane.clone(),
                        deadline_minutes: Some(240),
                        quick: idx == 0,
                    },
                )
                .collect(),
            energy_windows: vec![
                crate::features::ultimate_astra_features::EnergyWindowInput {
                    start_hour: 8,
                    end_hour: 12,
                    energy_score: 0.9,
                },
            ],
            current_hour: 9,
            attestor: request.attestor.clone(),
            notarize_to_chain: false,
        })?;
    let knowledge_report = state
        .ultimate
        .orchestrate_knowledge(UltimateKnowledgeRequest {
            domain: primary_domain.clone(),
            question: request.objective.clone(),
            sources: source_documents,
            live_feeds: domains.clone(),
            competing_claims: abyss_report
                .as_ref()
                .map(|report| {
                    report
                        .truth_triangulation
                        .iter()
                        .take(4)
                        .map(|finding| {
                            crate::features::ultimate_astra_features::FrontierClaimInput {
                                theory: finding.subject.clone(),
                                support_score: finding.confidence,
                                source_count: finding.corroborating_urls.len() as u32,
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            synthesis_problem: request.objective.clone(),
            cross_domain_matches: request
                .local_focus_tags
                .iter()
                .take(3)
                .map(
                    |tag| crate::features::ultimate_astra_features::CrossDomainMatchInput {
                        label: tag.clone(),
                        source_domain: primary_domain.clone(),
                        structural_similarity: 0.7,
                    },
                )
                .collect(),
            recency_events: vec!["personal mission executed".into()],
            attestor: request.attestor.clone(),
            notarize_to_chain: false,
        })?;
    let cryogenic_checkpoint =
        state
            .unbreakable
            .capture_cryogenic_heartbeat(CryogenicHeartbeatRequest {
                subsystem: "personal_mission".into(),
                active_tasks: omega_mission
                    .steps
                    .iter()
                    .map(|step| step.objective.clone())
                    .collect(),
            })?;
    let network_plan = request.seed_urls.first().cloned().and_then(|destination| {
        state
            .unbreakable
            .plan_network_request(NetworkPrivacyRequest {
                destination: destination.clone(),
                sensitivity: if request.private_mode {
                    PrivacySensitivity::Sensitive
                } else {
                    PrivacySensitivity::Standard
                },
                allow_split_tunnel: true,
            })
            .or_else(|_| {
                state
                    .unbreakable
                    .plan_network_request(NetworkPrivacyRequest {
                        destination,
                        sensitivity: PrivacySensitivity::Standard,
                        allow_split_tunnel: true,
                    })
            })
            .ok()
    });
    let anchored_truths = abyss_report
        .as_ref()
        .map(|report| {
            report
                .truth_triangulation
                .iter()
                .filter(|finding| !finding.requires_review)
                .take(3)
                .filter_map(|finding| {
                    state
                        .unbreakable
                        .anchor_fact(UnbreakableFactRequest {
                            fact_id: format!("truth-{}", finding.subject.replace(' ', "-")),
                            statement: finding.subject.clone(),
                            source_kind: "abyss_research".into(),
                            corroboration_count: finding
                                .corroborating_urls
                                .len()
                                .min(u8::MAX as usize)
                                as u8,
                        })
                        .ok()
                        .filter(|accepted| *accepted)
                        .map(|_| finding.subject.clone())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let immune_patrol = state.unbreakable.run_immune_patrol();

    #[cfg(feature = "experimental")]
    let recovery_report = (!request.local_assets.is_empty() || !request.open_files.is_empty())
        .then(|| {
            state.warfare.orchestrate_recovery(
                crate::features::warfare_edition_features::WarfareRecoveryRequest {
                    case_id: format!("personal-recovery-{}", timestamp_ms),
                    targets: request
                        .local_assets
                        .iter()
                        .map(|asset| {
                            crate::features::warfare_edition_features::RecoveryTargetInput {
                                asset_id: asset.clone(),
                                format_hint: None,
                                corruption_signals: vec!["operator_review".into()],
                                fragment_count: 1,
                                snapshots_available: 1,
                                encrypted: false,
                                directory_hint: None,
                            }
                        })
                        .collect(),
                    memory_artifacts: request
                        .open_files
                        .iter()
                        .take(2)
                        .map(|file| {
                            crate::features::warfare_edition_features::MemoryArtifactInput {
                                artifact_id: file.clone(),
                                content_class: "workspace_context".into(),
                                contains_secrets: false,
                                confidence: 0.6,
                            }
                        })
                        .collect(),
                    protect_outputs: true,
                    attestor: request.attestor.clone(),
                    notarize_to_chain: false,
                },
            )
        })
        .transpose()?;
    #[cfg(not(feature = "experimental"))]
    let recovery_report: Option<serde_json::Value> = None;

    #[cfg(feature = "experimental")]
    let defense_report = (!request.threat_signals.is_empty() || !request.open_files.is_empty())
        .then(|| {
            state.warfare.orchestrate_defense(
                crate::features::warfare_edition_features::WarfareDefenseRequest {
                    samples: request
                        .threat_signals
                        .iter()
                        .enumerate()
                        .map(|(idx, signal)| {
                            crate::features::warfare_edition_features::ThreatSampleInput {
                                sample_id: format!("signal-{idx}"),
                                family_hint: Some("owner_watch".into()),
                                behaviors: vec![signal.clone()],
                                packer_layers: 0,
                                c2_indicators: Vec::new(),
                                privileges: vec!["user".into()],
                                touched_paths: request.open_files.clone(),
                            }
                        })
                        .collect(),
                    ransomware_incidents: Vec::new(),
                    software_inventory: vec![
                        crate::features::warfare_edition_features::SoftwareRiskInput {
                            product: "astra_core_engine".into(),
                            version: env!("CARGO_PKG_VERSION").into(),
                            recent_cve_count: request.threat_signals.len() as u32,
                            days_since_last_patch: 1,
                            attack_surface_score: 0.32,
                            hardening_available: vec!["review suspicious inputs".into()],
                        },
                    ],
                    attestor: request.attestor.clone(),
                    notarize_to_chain: false,
                },
            )
        })
        .transpose()?;
    #[cfg(not(feature = "experimental"))]
    let defense_report: Option<serde_json::Value> = None;

    #[cfg(feature = "experimental")]
    let revenue_report = (!request.owned_assets.is_empty() || !request.service_offers.is_empty())
        .then(|| {
            state.warfare.orchestrate_revenue(
                crate::features::warfare_edition_features::WarfareRevenueRequest {
                    market_quotes: Vec::new(),
                    content_channels: if request.service_offers.is_empty() {
                        vec![
                            crate::features::warfare_edition_features::ContentChannelInput {
                                channel: "owner_briefing".into(),
                                content_type: "research_digest".into(),
                                monetization: "internal_reuse".into(),
                                capacity_score: 0.6,
                            },
                        ]
                    } else {
                        Vec::new()
                    },
                    service_requests: request
                        .service_offers
                        .iter()
                        .map(|offer| {
                            crate::features::warfare_edition_features::ServiceRequestInput {
                                service: offer.clone(),
                                buyer_segment: "owner_network".into(),
                                budget_usd: 250.0,
                                urgency: 0.5,
                                authorized: true,
                            }
                        })
                        .collect(),
                    bounty_programs: Vec::new(),
                    owned_assets: request
                        .owned_assets
                        .iter()
                        .map(
                            |asset| crate::features::warfare_edition_features::OwnedAssetInput {
                                asset_id: asset.clone(),
                                asset_type: "knowledge_asset".into(),
                                utilization: 0.4,
                                maintenance_cost_usd: 5.0,
                                monetizable: true,
                            },
                        )
                        .collect(),
                    yield_options: Vec::new(),
                    market_signals: request
                        .local_focus_tags
                        .iter()
                        .take(2)
                        .map(
                            |tag| crate::features::warfare_edition_features::MarketSignalInput {
                                topic: tag.clone(),
                                sentiment: 0.55,
                                confidence: 0.7,
                            },
                        )
                        .collect(),
                    freelance_leads: Vec::new(),
                    attestor: request.attestor.clone(),
                    notarize_to_chain: false,
                },
            )
        })
        .transpose()?;
    #[cfg(not(feature = "experimental"))]
    let revenue_report: Option<serde_json::Value> = None;

    let (readiness_score, fitness, evolved_params, weak_components) = {
        let mut control_runtime = state
            .control_runtime
            .write()
            .map_err(|_| control_runtime_lock_error())?;
        control_runtime.evolution.record_health(
            "abyss",
            abyss_report.as_ref().map_or(180.0, |report| {
                120.0 + (report.summary.pages_crawled as f64 * 8.0)
            }),
            0.02,
            abyss_report
                .as_ref()
                .map_or(0.5, |report| report.summary.pages_crawled as f64),
            abyss_report.as_ref().map_or(0.62, |_| 0.84),
        );
        control_runtime.evolution.record_health(
            "revolutionary",
            110.0,
            if sandbox_report.verdict.allow_real_execution {
                0.0
            } else {
                0.03
            },
            1.0,
            if sandbox_report.verdict.allow_real_execution {
                0.86
            } else {
                0.74
            },
        );
        control_runtime.evolution.record_health(
            "ultimate",
            95.0,
            0.01,
            1.0,
            cognition_report
                .temporal_reasoning_fabric
                .weighted_temporal_score
                .clamp(0.0, 1.0),
        );
        control_runtime.evolution.record_health(
            "unbreakable",
            70.0,
            immune_patrol.degraded_invariants.len() as f64 * 0.02,
            1.0,
            (immune_patrol.healthy_invariants as f64 / 7.0).clamp(0.0, 1.0),
        );
        #[cfg(feature = "experimental")]
        control_runtime.evolution.record_health(
            "warfare",
            100.0,
            0.01,
            1.0,
            if defense_report.is_some() || recovery_report.is_some() || revenue_report.is_some() {
                0.8
            } else {
                0.62
            },
        );
        let (readiness_score, fitness) = compute_personal_scores(
            abyss_report.as_ref(),
            cognition_report
                .temporal_reasoning_fabric
                .weighted_temporal_score,
            &immune_patrol,
            control_runtime.evolution.get_weak_components().len(),
        );
        let evolved_params = control_runtime.evolution.evolve_step(fitness);
        let weak_components = control_runtime.evolution.get_weak_components();
        (readiness_score, fitness, evolved_params, weak_components)
    };

    let next_actions = omega_mission
        .exit_criteria
        .iter()
        .chain(immune_patrol.healing_actions.iter())
        .take(6)
        .cloned()
        .collect::<Vec<_>>();
    let recommendations = weak_components
        .iter()
        .map(|component| format!("{component} requires a guarded improvement cycle"))
        .collect::<Vec<_>>();
    let digest = state
        .personal_mission
        .record_mission(PersonalMissionRecordInput {
            mission_id: sha3_256_hex(
                format!("personal:{}:{}", request.objective, timestamp_ms).as_bytes(),
            )[..24]
                .to_string(),
            objective: request.objective.clone(),
            domains: domains.clone(),
            readiness_score,
            fitness,
            pages_crawled: abyss_report
                .as_ref()
                .map_or(0, |report| report.summary.pages_crawled),
            next_actions: next_actions.clone(),
            weak_components: weak_components.clone(),
            improvement_recommendations: recommendations.clone(),
            completed_at: timestamp_ms,
        });
    for component in &weak_components {
        state.assistant_autonomy.submit_self_improvement(
            format!("Improve {component} for personal CPU-only missions"),
            "personal_mission".into(),
            vec![
                format!("Profile {component} bottlenecks from the latest mission"),
                format!("Tune {component} using local evolution parameters"),
            ],
            vec![
                "Keep the system CPU-only and local-first".into(),
                "Require review before broadening autonomy".into(),
            ],
        );
    }

    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:personal-mission".into());
        Some(
            state.chain.notarize_resource(
                &format!("astra://personal/mission/{}", digest.mission_id),
                &sha3_256_hex(
                    serde_json::json!({
                        "objective": request.objective.clone(),
                        "readiness_score": readiness_score,
                        "fitness": fitness,
                        "weak_components": weak_components.clone(),
                    })
                    .to_string()
                    .as_bytes(),
                ),
                "application/json",
                "personal_mission",
                &attestor,
                serde_json::json!({
                    "mission_id": digest.mission_id.clone(),
                    "readiness_score": readiness_score,
                    "fitness": fitness,
                    "pages_crawled": digest.pages_crawled,
                }),
            )?,
        )
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "mission": digest,
        "omega_decision": omega_decision,
        "omega_mission": omega_mission,
        "abyss_report": abyss_report,
        "revolutionary": {
            "sandbox": sandbox_report,
            "protocol": protocol_report,
            "resilience": resilience_report,
            "value": value_report,
        },
        "ultimate": {
            "cognition": cognition_report,
            "acquisition": acquisition_report,
            "assistant": assistant_report,
            "knowledge": knowledge_report,
        },
        "unbreakable": {
            "cryogenic_checkpoint": cryogenic_checkpoint,
            "network_plan": network_plan,
            "anchored_truths": anchored_truths,
            "immune_patrol": immune_patrol,
        },
        "warfare": {
            "recovery": recovery_report,
            "defense": defense_report,
            "revenue": revenue_report,
        },
        "evolution": {
            "weak_components": weak_components,
            "evolved_params": evolved_params,
        },
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn revolutionary_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.revolutionary.status())))
}

pub async fn revolutionary_sandbox_analyze(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RevolutionarySandboxRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.revolutionary.analyze_sandbox(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:revolutionary-sandbox".into());
        Some(state.chain.notarize_resource(
            &format!("astra://revolutionary/sandbox/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "revolutionary_sandbox",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "risk_level": report.verdict.risk_level,
                "allow_real_execution": report.verdict.allow_real_execution,
                "heat_score": report.phantom_thermodynamics.heat_score,
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn revolutionary_value_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RevolutionaryValueRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = {
        let mut revolutionary = std::mem::take(&mut state.revolutionary);
        let result = revolutionary.orchestrate_value(request.clone(), &mut state.chain);
        state.revolutionary = revolutionary;
        result?
    };
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:revolutionary-value".into());
        Some(state.chain.notarize_resource(
            &format!("astra://revolutionary/value/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "revolutionary_value",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "guard_contract_id": report.guarded_payment.contract_id,
                "authorized": report.guarded_payment.authorized,
                "selected_provider": report.metabolic_transaction_routing.selected_provider,
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn revolutionary_protocol_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RevolutionaryProtocolRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.revolutionary.orchestrate_protocol(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:revolutionary-protocol".into());
        Some(state.chain.notarize_resource(
            &format!("astra://revolutionary/protocol/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "revolutionary_protocol",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "packet_count": report.semantic_packet_dna.len(),
                "requests_saved": report.temporal_request_folding.requests_saved,
                "shedding_level": report.cognitive_load_shedding.shedding_level,
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn revolutionary_resilience_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<RevolutionaryResilienceRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state
        .revolutionary
        .orchestrate_resilience(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:revolutionary-resilience".into());
        Some(state.chain.notarize_resource(
            &format!("astra://revolutionary/resilience/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "revolutionary_resilience",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "root_cause": report.failure_archaeology_engine.root_cause,
                "anomalies": report.anomaly_triangulation.len(),
                "repairs": report.speculative_self_repair.len(),
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn ultimate_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ultimate.status())))
}

pub async fn ultimate_cognition_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<UltimateCognitionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.ultimate.orchestrate_cognition(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:ultimate-cognition".into());
        Some(state.chain.notarize_resource(
            &format!("astra://ultimate/cognition/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "ultimate_cognition",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "selected_decision": report.temporal_reasoning_fabric.selected_decision,
                "resilience": report.adversarial_imagination_engine.resilience_score,
                "atomic_leaf_count": report.cognitive_depth_charges.atomic_leaf_count,
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn ultimate_acquisition_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<UltimateAcquisitionRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.ultimate.orchestrate_acquisition(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:ultimate-acquisition".into());
        Some(state.chain.notarize_resource(
            &format!("astra://ultimate/acquisition/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "ultimate_acquisition",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "scatter_count": report.temporal_request_scattering.scheduled_requests.len(),
                "proxy_routes": report.semantic_proxy_mesh.selections.len(),
                "mix_batch": report.request_origin_erasure.batch_size,
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn ultimate_assistant_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<UltimateAssistantRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.ultimate.orchestrate_assistant(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:ultimate-assistant".into());
        Some(state.chain.notarize_resource(
            &format!("astra://ultimate/assistant/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "ultimate_assistant",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "response_style": report.ambient_intent_inference.response_style,
                "orchestration_actions": report.proactive_environment_orchestration.actions.len(),
                "schedule_blocks": report.sovereign_schedule_ai.optimized_blocks.len(),
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn ultimate_knowledge_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<UltimateKnowledgeRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.ultimate.orchestrate_knowledge(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:ultimate-knowledge".into());
        Some(state.chain.notarize_resource(
            &format!("astra://ultimate/knowledge/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "ultimate_knowledge",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "domain": report.universal_domain_compiler.domain,
                "frontier": report.epistemic_frontier_detection.location,
                "solutions": report.synthesis_forge.len(),
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

#[cfg(feature = "experimental")]
pub async fn warfare_status(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let status: WarfareEditionStatus = state.warfare.status();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(status)))
}

#[cfg(feature = "experimental")]
pub async fn warfare_recovery_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<WarfareRecoveryRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.warfare.orchestrate_recovery(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:warfare-recovery".into());
        Some(state.chain.notarize_resource(
            &format!("astra://warfare/recovery/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "warfare_recovery",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "targets": report.semantic_structure_inference.len(),
                "protected_assets": report.self_healing_data_organism.len(),
                "memory_artifacts": report.memory_phantom_extraction.len(),
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

#[cfg(feature = "experimental")]
pub async fn warfare_defense_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<WarfareDefenseRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.warfare.orchestrate_defense(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:warfare-defense".into());
        Some(state.chain.notarize_resource(
            &format!("astra://warfare/defense/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "warfare_defense",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "samples": report.behavioral_dna_extraction.len(),
                "families": report.autonomous_immune_memory.broadened_families,
                "blocked_execution": report
                    .malware_vivisection_lab
                    .iter()
                    .all(|item| item.blocked_execution),
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

#[cfg(feature = "experimental")]
pub async fn warfare_revenue_orchestrate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<WarfareRevenueRequest>,
) -> Result<HttpResponse, AstraError> {
    let request = body.into_inner();
    let mut state = state.write().await;
    let report = state.warfare.orchestrate_revenue(request.clone())?;
    let ledger_commitment = if request.notarize_to_chain {
        let attestor = request
            .attestor
            .clone()
            .unwrap_or_else(|| "assistant:warfare-revenue".into());
        Some(state.chain.notarize_resource(
            &format!("astra://warfare/revenue/{}", report.report_id),
            &report.manifest_hash,
            "application/json",
            "warfare_revenue",
            &attestor,
            serde_json::json!({
                "report_id": report.report_id,
                "arbitrage_count": report.arbitrage_radar.len(),
                "service_offers": report.micro_service_mercenary.len(),
                "expected_monthly_usd": report.revenue_portfolio_organism.expected_monthly_usd,
            }),
        )?)
    } else {
        None
    };

    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "report": report,
        "ledger_commitment": ledger_commitment,
    }))))
}

pub async fn hostile_scan(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let html = body.get("html").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let result = state.hostile_arch.scan_page(url, html);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn hostile_set_auto(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mut state = state.write().await;
    state.hostile_arch.set_auto_mode(enabled);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({"auto_mode": enabled}))))
}

pub async fn hostile_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.hostile_arch.get_stats())))
}

// â”€â”€ 3. Epistemic Immune System â”€â”€

pub async fn epistemic_analyze(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let report = state.epistemic.analyze_page(url, content);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

pub async fn epistemic_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.epistemic.get_stats())))
}

// â”€â”€ 4. Predictive Fetch â”€â”€

pub async fn predictive_record(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let _ = (state, url, title);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn predictive_predict(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let count = body.get("count").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let _ = (state, count);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn predictive_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let _ = state;
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

// â”€â”€ 5. Hardware Symbiosis â”€â”€

pub async fn hardware_report(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let mut state = state.write().await;
    let report = state.hardware.get_report();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

pub async fn hardware_tab_usage(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let tab_id = body.get("tab_id").and_then(|v| v.as_str()).unwrap_or("");
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let usage = state.hardware.report_tab_usage(tab_id, title);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(usage)))
}

// â”€â”€ 6. Polymorphic DOM â”€â”€

pub async fn polymorphic_analyze(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let html = body.get("html").and_then(|v| v.as_str()).unwrap_or("");
    let _ = (state, url, html);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn polymorphic_learn(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let zone = body
        .get("zone_selector")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let dwell = body
        .get("dwell_time_sec")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let _ = (state, zone, dwell);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn polymorphic_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let _ = state;
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

// Legacy visual interaction handlers retained only as inert compatibility shims.

pub async fn removed_visual_predict(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let width = body
        .get("page_width")
        .and_then(|v| v.as_f64())
        .unwrap_or(1920.0);
    let height = body
        .get("page_height")
        .and_then(|v| v.as_f64())
        .unwrap_or(1080.0);
    let _ = (state, url, width, height);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn removed_visual_record_click(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let x = body.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = body.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let element = body.get("element").and_then(|v| v.as_str()).unwrap_or("");
    let _ = (state, url, x, y, element);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn removed_visual_form(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let html = body.get("html").and_then(|v| v.as_str()).unwrap_or("");
    let _ = (state, html);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn removed_visual_mode_toggle(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let _ = (state, enabled);
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

pub async fn removed_visual_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let _ = state;
    Err(AstraError::ControlPlaneRejected(
        "visual interaction features were removed from the backend runtime".into(),
    ))
}

// â”€â”€ 8. Dark-Matter Cache â”€â”€

pub async fn cache_page(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let title = body.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let content_type = body
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("text/html");
    let mut state = state.write().await;
    let page = state.dark_cache.cache_page(
        url,
        title,
        content,
        content_type,
        std::collections::HashMap::new(),
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

pub async fn cache_get(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let cached = state.dark_cache.get_cached_page(url).cloned();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "found": cached.is_some(), "page": cached,
    }))))
}

pub async fn cache_download(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let depth = body.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
    let mut state = state.write().await;
    let job = state.dark_cache.queue_download(url, depth);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(job)))
}

pub async fn cache_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.dark_cache.get_stats())))
}

pub async fn cache_predict(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let recent: Vec<String> = body
        .get("recent_urls")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let state = state.read().await;
    let predictions = state.dark_cache.predict_offline_needs(&recent);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        serde_json::json!({"predictions": predictions}),
    )))
}

// â”€â”€ 9. Parasitic Injector â”€â”€

#[cfg(feature = "experimental")]
pub async fn parasitic_inject(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let url = body.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let html = body.get("html").and_then(|v| v.as_str()).unwrap_or("");
    let mut state = state.write().await;
    let report = state.parasitic.analyze_and_inject(url, html);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[cfg(feature = "experimental")]
pub async fn parasitic_set_profile(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let profile = body
        .get("profile")
        .and_then(|v| v.as_str())
        .unwrap_or("balanced");
    let mut state = state.write().await;
    state.parasitic.set_profile(profile);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({"profile": profile}))))
}

#[cfg(feature = "experimental")]
pub async fn parasitic_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.parasitic.get_stats())))
}

// â”€â”€ 10. Enhanced Swarm â”€â”€

pub async fn swarm_deploy(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let intent = body.get("intent").and_then(|v| v.as_str()).unwrap_or("");
    let num_agents = body.get("num_agents").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let debate = body
        .get("enable_debate")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let result = search_runtime
        .enhanced_swarm
        .deploy_swarm(intent, num_agents, debate)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn swarm_debate(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let intent = body.get("intent").and_then(|v| v.as_str()).unwrap_or("");
    let num_agents = body.get("num_agents").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let result = search_runtime
        .enhanced_swarm
        .deploy_swarm(intent, num_agents, true)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub async fn swarm_campaign(
    state: web::Data<Arc<RwLock<EngineState>>>,
    body: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AstraError> {
    let objective = body.get("objective").and_then(|v| v.as_str()).unwrap_or("");
    let num_agents = body.get("num_agents").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let mut search_runtime = search_runtime
        .write()
        .map_err(|_| search_runtime_lock_error())?;
    let campaign = search_runtime
        .enhanced_swarm
        .launch_campaign(objective, num_agents);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(campaign)))
}

pub async fn swarm_enhanced_stats(
    state: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let search_runtime = {
        let state = state.read().await;
        Arc::clone(&state.search_runtime)
    };
    let search_runtime = search_runtime
        .read()
        .map_err(|_| search_runtime_lock_error())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(search_runtime.enhanced_swarm.get_stats())))
}

fn api_request_is_public(method: &actix_web::http::Method, path: &str) -> bool {
    matches!(
        (method.as_str(), path),
        ("GET", "/api/health")
            | ("GET", "/api/features")
            | ("GET", "/api/assistant/runtime/profile")
            | ("POST", "/api/identity/create")
            | ("POST", "/api/identity/challenge")
            | ("POST", "/api/identity/auth")
            | ("POST", "/api/identity/session/auth")
    )
}

fn api_request_requires_admin(method: &actix_web::http::Method, path: &str) -> bool {
    !matches!(method.as_str(), "HEAD" | "OPTIONS") && !api_request_is_public(method, path)
}

/// Configure all feature API routes.
pub fn configure_api_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .wrap_fn(|req: ServiceRequest, srv| {
                let method = req.method().clone();
                let path = req.path().to_string();
                let app_state = req.app_data::<web::Data<Arc<RwLock<AppState>>>>().cloned();

                let auth_result = if api_request_requires_admin(&method, &path) {
                    if let Some(app_state) = app_state {
                        app_state
                            .try_read()
                            .map_err(|_| {
                                AstraError::Internal("api admin middleware state contention".into())
                            })
                            .and_then(|app| authorize_admin_request(req.request(), &app.config))
                    } else {
                        Err(AstraError::Internal(
                            "api admin middleware missing app state".into(),
                        ))
                    }
                } else {
                    Ok(())
                };

                if let Err(error) = auth_result {
                    return Either::Left(ready(Ok(req
                        .into_response(error.error_response())
                        .map_into_right_body())));
                }

                let fut = srv.call(req);
                Either::Right(async move {
                    let response = fut.await?;
                    Ok(response.map_into_left_body())
                })
            })
            // v1 Feature routes
            .route("/memory/commit", web::post().to(memory_commit))
            .route("/memory/search", web::post().to(memory_search))
            .route("/swarm/query", web::post().to(swarm_query))
            .route(
                "/search/intelligence/profile",
                web::post().to(adaptive_search_profile),
            )
            .route("/search/intelligence", web::post().to(adaptive_search))
            .route(
                "/search/intelligence/refresh",
                web::post().to(adaptive_search_refresh),
            )
            .route(
                "/search/intelligence/refresh/plan",
                web::get().to(adaptive_search_refresh_plan),
            )
            .route(
                "/search/intelligence/refresh/sweep",
                web::post().to(adaptive_search_refresh_sweep),
            )
            .route(
                "/search/intelligence/crawl",
                web::get().to(adaptive_search_autonomous_frontier),
            )
            .route(
                "/search/intelligence/crawl/tick",
                web::post().to(adaptive_search_autonomous_tick),
            )
            .route(
                "/search/intelligence/stats",
                web::get().to(adaptive_search_stats),
            )
            .route("/shield/intent", web::post().to(shield_intent))
            .route("/verify/content", web::post().to(verify_content))
            .route("/identity/create", web::post().to(identity_create))
            .route(
                "/identity/challenge",
                web::post().to(identity_issue_challenge),
            )
            .route("/identity/auth", web::post().to(identity_auth))
            .route(
                "/identity/session/auth",
                web::post().to(identity_session_auth),
            )
            .route("/compute/submit", web::post().to(compute_submit))
            .route("/agents/stats", web::get().to(agents_stats))
            .route("/runtime/capabilities", web::get().to(runtime_capabilities))
            .route("/unbreakable/status", web::get().to(unbreakable_status))
            .route(
                "/unbreakable/cryogenic/heartbeat",
                web::post().to(unbreakable_cryogenic_heartbeat),
            )
            .route(
                "/unbreakable/cryogenic/report",
                web::get().to(unbreakable_cryogenic_report),
            )
            .route(
                "/unbreakable/network/plan",
                web::post().to(unbreakable_network_plan),
            )
            .route(
                "/unbreakable/reality-anchor/facts",
                web::post().to(unbreakable_anchor_fact),
            )
            .route(
                "/unbreakable/reality-anchor/records",
                web::get().to(unbreakable_reality_anchor_records),
            )
            .route(
                "/unbreakable/immune/patrol",
                web::post().to(unbreakable_immune_patrol),
            )
            .route(
                "/unbreakable/immune/memory",
                web::get().to(unbreakable_immune_memory),
            )
            .route(
                "/unbreakable/vault/inventory",
                web::get().to(unbreakable_vault_inventory),
            )
            .route(
                "/unbreakable/vault/audit",
                web::get().to(unbreakable_vault_audit),
            )
            .route(
                "/unbreakable/vault/rotate",
                web::post().to(unbreakable_vault_rotate),
            )
            .route(
                "/runtime/device/permissions",
                web::get().to(runtime_list_device_permissions),
            )
            .route(
                "/runtime/device/permissions",
                web::post().to(runtime_grant_device_permission),
            )
            .route(
                "/runtime/device/authorize",
                web::post().to(runtime_authorize_device_action),
            )
            .route(
                "/assistant/runtime/profile",
                web::get().to(assistant_runtime_profile),
            )
            .route("/assistant/execute", web::post().to(assistant_execute))
            .route(
                "/assistant/missions",
                web::get().to(assistant_list_missions),
            )
            .route(
                "/assistant/missions/{mission_id}",
                web::get().to(assistant_get_mission),
            )
            .route(
                "/assistant/missions/{mission_id}/resume",
                web::post().to(assistant_resume_mission),
            )
            .route(
                "/assistant/missions/{mission_id}/cancel",
                web::post().to(assistant_cancel_mission),
            )
            .route(
                "/assistant/missions/{mission_id}/events",
                web::get().to(assistant_mission_events),
            )
            .route(
                "/assistant/missions/{mission_id}/stream",
                web::get().to(assistant_mission_stream),
            )
            .route(
                "/assistant/approvals",
                web::get().to(assistant_list_approvals),
            )
            .route(
                "/assistant/approvals",
                web::post().to(assistant_apply_approval),
            )
            .route(
                "/assistant/connectors",
                web::get().to(assistant_list_connectors),
            )
            .route(
                "/assistant/connectors",
                web::post().to(assistant_upsert_connector),
            )
            .route(
                "/research/policies",
                web::get().to(assistant_list_research_policies),
            )
            .route(
                "/research/policies",
                web::post().to(assistant_upsert_research_policy),
            )
            .route("/assistant/audit", web::get().to(assistant_audit_log))
            .route(
                "/assistant/health/enterprise",
                web::get().to(assistant_enterprise_health),
            )
            .route(
                "/assistant/fleet/snapshot",
                web::get().to(assistant_fleet_snapshot),
            )
            .route(
                "/assistant/fleet/campaigns",
                web::post().to(assistant_create_fleet_campaign),
            )
            .route(
                "/assistant/fleet/campaigns/{campaign_id}",
                web::get().to(assistant_get_fleet_campaign),
            )
            .route(
                "/assistant/fleet/rankings",
                web::get().to(assistant_fleet_rankings),
            )
            .route(
                "/assistant/fleet/league",
                web::get().to(assistant_fleet_league),
            )
            .route(
                "/assistant/fleet/promotions/{candidate_id}",
                web::post().to(assistant_promote_fleet_candidate),
            )
            .route(
                "/assistant/fleet/rollback/{deployment_id}",
                web::post().to(assistant_rollback_fleet_candidate),
            )
            .route(
                "/httpa/execution/policies",
                web::get().to(assistant_list_httpa_execution_policies),
            )
            .route(
                "/httpa/execution/policies",
                web::post().to(assistant_upsert_httpa_execution_policy),
            )
            .route(
                "/acquisition/policies",
                web::get().to(assistant_list_acquisition_policies),
            )
            .route(
                "/acquisition/policies",
                web::post().to(assistant_upsert_acquisition_policy),
            )
            .route(
                "/acquisition/onion/sources",
                web::get().to(assistant_list_onion_sources),
            )
            .route(
                "/acquisition/onion/sources",
                web::post().to(assistant_upsert_onion_source),
            )
            .route(
                "/assistant/chain/receipts",
                web::get().to(assistant_chain_receipts),
            )
            .route(
                "/local/catalog/apps",
                web::get().to(assistant_local_app_catalog),
            )
            .route("/local/open", web::post().to(assistant_local_open))
            .route(
                "/software/sources/resolve",
                web::post().to(assistant_software_sources),
            )
            .route(
                "/software/staged",
                web::get().to(assistant_list_staged_artifacts),
            )
            .route("/software/stage", web::post().to(assistant_stage_software))
            .route(
                "/software/install",
                web::post().to(assistant_install_software),
            )
            .route(
                "/software/update",
                web::post().to(assistant_update_software),
            )
            .route(
                "/software/receipts",
                web::get().to(assistant_install_inventory),
            )
            .route(
                "/security/samples",
                web::post().to(assistant_submit_security_sample),
            )
            .route(
                "/security/reverse-engineer",
                web::post().to(assistant_reverse_engineer_sample),
            )
            .route(
                "/security/cases",
                web::get().to(assistant_list_security_cases),
            )
            .route(
                "/security/cases/{case_id}",
                web::get().to(assistant_get_security_case),
            )
            .route(
                "/security/owned-targets",
                web::get().to(assistant_list_owned_targets),
            )
            .route(
                "/security/owned-targets",
                web::post().to(assistant_register_owned_target),
            )
            .route(
                "/security/lab/sessions",
                web::get().to(assistant_list_lab_environments),
            )
            .route(
                "/security/lab/sessions",
                web::post().to(assistant_launch_lab_environment),
            )
            .route(
                "/assistant/override-sessions",
                web::get().to(assistant_list_override_sessions),
            )
            .route(
                "/assistant/override-sessions",
                web::post().to(assistant_create_override_session),
            )
            .route(
                "/assistant/voice/status",
                web::get().to(assistant_voice_status),
            )
            .route(
                "/assistant/voice/warmup",
                web::post().to(assistant_voice_warmup),
            )
            .route(
                "/assistant/voice/transcribe",
                web::post().to(assistant_voice_transcribe),
            )
            .route(
                "/assistant/voice/listen",
                web::post().to(assistant_voice_listen),
            )
            .route(
                "/assistant/voice/speak",
                web::post().to(assistant_voice_speak),
            )
            .route(
                "/assistant/autonomy",
                web::get().to(assistant_autonomy_snapshot),
            )
            .route(
                "/assistant/autonomy/agents",
                web::post().to(assistant_create_custom_agent),
            )
            .route(
                "/assistant/autonomy/agents/{agent_id}",
                web::delete().to(assistant_delete_custom_agent),
            )
            .route(
                "/assistant/autonomy/agents/{agent_id}/evaluate",
                web::post().to(assistant_evaluate_custom_agent),
            )
            .route(
                "/assistant/autonomy/tools",
                web::post().to(assistant_create_custom_tool),
            )
            .route(
                "/assistant/autonomy/tools/{tool_name}",
                web::delete().to(assistant_delete_custom_tool),
            )
            .route(
                "/assistant/autonomy/tools/{tool_name}/evaluate",
                web::post().to(assistant_evaluate_custom_tool),
            )
            .route(
                "/assistant/self-improvement/proposals",
                web::post().to(assistant_submit_self_improvement_proposal),
            )
            .route(
                "/assistant/brain/execute",
                web::post().to(assistant_brain_execute),
            )
            .route(
                "/assistant/brain/sessions/{session_id}",
                web::get().to(assistant_brain_session),
            )
            .route("/personal/status", web::get().to(personal_mission_status))
            .route(
                "/personal/mission/orchestrate",
                web::post().to(personal_mission_orchestrate),
            )
            .route(
                "/runtime/mission",
                web::post().to(runtime_mission_assignment),
            )
            .route("/chain/info", web::get().to(chain_info))
            .route("/chain/overview", web::get().to(chain_overview))
            .route("/chain/integrity", web::get().to(chain_integrity))
            .route(
                "/chain/quantum-resilience",
                web::get().to(chain_quantum_resilience),
            )
            .route(
                "/chain/acceleration",
                web::get().to(chain_acceleration_report),
            )
            .route("/chain/peers", web::get().to(chain_peers))
            .route("/chain/peers/register", web::post().to(chain_register_peer))
            .route(
                "/chain/peers/handshake/challenge",
                web::post().to(chain_peer_handshake_challenge),
            )
            .route(
                "/chain/peers/handshake/verify",
                web::post().to(chain_peer_handshake_verify),
            )
            .route("/chain/peers/sync", web::post().to(chain_sync_peer))
            .route(
                "/chain/validators",
                web::post().to(chain_register_validator),
            )
            .route(
                "/chain/transactions",
                web::post().to(chain_submit_transaction),
            )
            .route(
                "/chain/value/capabilities",
                web::get().to(chain_value_capabilities),
            )
            .route(
                "/chain/value/connectors",
                web::get().to(chain_list_value_connectors),
            )
            .route(
                "/chain/value/connectors/register",
                web::post().to(chain_register_value_connector),
            )
            .route(
                "/chain/value/compliance",
                web::get().to(chain_list_value_compliance_profiles),
            )
            .route(
                "/chain/value/compliance/register",
                web::post().to(chain_register_value_compliance_profile),
            )
            .route(
                "/chain/value/stream/quote",
                web::post().to(chain_quote_stream_value),
            )
            .route(
                "/chain/value/stream/settle",
                web::post().to(chain_settle_stream_value),
            )
            .route(
                "/chain/value/intents/route",
                web::post().to(chain_route_financial_intent),
            )
            .route(
                "/chain/value/intents/commit",
                web::post().to(chain_commit_financial_intent),
            )
            .route(
                "/chain/value/cognition/reward",
                web::post().to(chain_reward_cognition),
            )
            .route(
                "/chain/value/privacy/attest",
                web::post().to(chain_attest_privacy),
            )
            .route(
                "/chain/value/devices/bind",
                web::post().to(chain_bind_device_wallet),
            )
            .route(
                "/chain/value/payments/guarded",
                web::post().to(chain_guarded_payment),
            )
            .route(
                "/chain/value/treasury",
                web::get().to(chain_treasury_status),
            )
            .route(
                "/chain/value/treasury/receipts",
                web::get().to(chain_treasury_receipts),
            )
            .route(
                "/chain/value/treasury/revenue",
                web::post().to(chain_treasury_capture_revenue),
            )
            .route(
                "/chain/value/reconciliation",
                web::get().to(chain_list_value_reconciliation),
            )
            .route(
                "/chain/value/disputes",
                web::get().to(chain_list_value_disputes),
            )
            .route(
                "/chain/value/disputes/open",
                web::post().to(chain_open_value_dispute),
            )
            .route(
                "/chain/value/disputes/resolve",
                web::post().to(chain_resolve_value_dispute),
            )
            .route("/chain/mine", web::post().to(chain_mine_block))
            .route("/chain/contracts", web::get().to(chain_contracts))
            .route(
                "/chain/contracts/deploy",
                web::post().to(chain_deploy_contract),
            )
            .route(
                "/chain/contracts/invoke",
                web::post().to(chain_invoke_contract),
            )
            .route("/chain/resources", web::get().to(chain_resources))
            .route(
                "/chain/resources/notarize",
                web::post().to(chain_notarize_resource),
            )
            .route(
                "/chain/resources/{commitment_id}/verify",
                web::get().to(chain_resource_verify),
            )
            .route(
                "/chain/resources/{commitment_id}",
                web::get().to(chain_resource_detail),
            )
            .route(
                "/chain/search/proofs/{trace_id}",
                web::get().to(chain_search_proof),
            )
            .route("/chain/audit", web::get().to(chain_audit))
            .route("/chain/replication", web::get().to(chain_replication))
            .route("/chain/export", web::get().to(chain_export))
            .route("/chain/export/audit", web::get().to(chain_export_audit))
            .route(
                "/chain/replicate/block",
                web::post().to(chain_replicate_block),
            )
            .route("/chain/evolution", web::get().to(chain_evolution_analysis))
            .route(
                "/chain/evolution/agents",
                web::post().to(chain_register_evolution_agent),
            )
            .route(
                "/chain/evolution/reports",
                web::post().to(chain_submit_evolution_report),
            )
            .route(
                "/chain/evolution/apply",
                web::post().to(chain_evolution_apply),
            )
            // v2 Intelligence routes
            .route("/reason", web::post().to(reason_brain))
            .route("/plan", web::post().to(plan_brain))
            .route("/plan/ready", web::get().to(plan_ready_tasks))
            .route("/evolve", web::post().to(evolve))
            .route("/evolve/stats", web::get().to(evolution_stats))
            .route("/heal", web::post().to(report_health))
            .route("/heal/status", web::get().to(system_health))
            .route("/graph/store", web::post().to(memory_store))
            .route("/graph/query", web::post().to(memory_query))
            .route("/graph/stats", web::get().to(memory_stats))
            // v2 Network routes
            .route("/dns/resolve", web::post().to(dns_resolve))
            .route("/dns/stats", web::get().to(dns_stats))
            // v2 System
            .route("/engine/status", web::get().to(engine_v2_status))
            // v3 Governance routes
            .route(
                "/governance/propose",
                web::post().to(governance_propose_upgrade),
            )
            .route("/governance/status", web::get().to(governance_status))
            .route("/governance/check", web::post().to(governance_check_action))
            // v3 Brain routes
            .route("/think", web::post().to(think_brain))
            .route("/reflection/stats", web::get().to(reflection_stats))
            .route("/omega/invent", web::post().to(omega_invent))
            .route("/omega/mission", web::post().to(omega_mission))
            // v4 Defender routes
            .route("/defender/scan", web::post().to(defender_scan))
            .route("/defender/status", web::get().to(defender_status))
            .route("/defender/threats", web::get().to(defender_threats))
            .route("/defender/restore", web::post().to(defender_restore))
            .route("/defender/clean", web::post().to(defender_clean))
            .route(
                "/defender/vulnerability",
                web::get().to(defender_vulnerability),
            )
            .route("/defender/processes", web::get().to(defender_processes))
            .route("/defender/sync", web::post().to(defender_sync))
            // v5 Quantum Tab Branching
            .route("/quantum/fork", web::post().to(quantum_fork))
            .route("/quantum/navigate", web::post().to(quantum_navigate))
            .route("/quantum/collapse", web::post().to(quantum_collapse))
            .route("/quantum/merge", web::post().to(quantum_merge))
            .route("/quantum/tree", web::post().to(quantum_tree))
            // v5 Assistant Semantic Render
            .route(
                "/render/semantic/analyze",
                web::post().to(semantic_render_analyze),
            )
            .route(
                "/render/semantic/stats",
                web::get().to(semantic_render_stats),
            )
            .route(
                "/render/semantic/acquire",
                web::post().to(semantic_acquisition_execute),
            )
            .route(
                "/render/semantic/acquisition/stats",
                web::get().to(semantic_acquisition_stats),
            )
            .route(
                "/render/semantic/acquisition/recent",
                web::get().to(semantic_acquisition_recent),
            )
            .route(
                "/render/semantic/workflow",
                web::post().to(semantic_workflow_execute),
            )
            .route("/abyss/status", web::get().to(abyss_status))
            .route(
                "/abyss/mission/orchestrate",
                web::post().to(abyss_mission_orchestrate),
            )
            .route("/revolutionary/status", web::get().to(revolutionary_status))
            .route(
                "/revolutionary/sandbox/analyze",
                web::post().to(revolutionary_sandbox_analyze),
            )
            .route(
                "/revolutionary/value/orchestrate",
                web::post().to(revolutionary_value_orchestrate),
            )
            .route(
                "/revolutionary/protocol/orchestrate",
                web::post().to(revolutionary_protocol_orchestrate),
            )
            .route(
                "/revolutionary/resilience/orchestrate",
                web::post().to(revolutionary_resilience_orchestrate),
            )
            .route("/ultimate/status", web::get().to(ultimate_status))
            .route(
                "/ultimate/cognition/orchestrate",
                web::post().to(ultimate_cognition_orchestrate),
            )
            .route(
                "/ultimate/acquisition/orchestrate",
                web::post().to(ultimate_acquisition_orchestrate),
            )
            .route(
                "/ultimate/assistant/orchestrate",
                web::post().to(ultimate_assistant_orchestrate),
            )
            .route(
                "/ultimate/knowledge/orchestrate",
                web::post().to(ultimate_knowledge_orchestrate),
            )
            // v5 Hostile Architecture Neutralizer
            .route("/hostile/scan", web::post().to(hostile_scan))
            .route("/hostile/auto", web::post().to(hostile_set_auto))
            .route("/hostile/stats", web::get().to(hostile_stats))
            // v5 Epistemic Immune System
            .route("/epistemic/analyze", web::post().to(epistemic_analyze))
            .route("/epistemic/stats", web::get().to(epistemic_stats))
            // v5 Hardware Symbiosis
            .route("/hardware/report", web::get().to(hardware_report))
            .route("/hardware/tab", web::post().to(hardware_tab_usage))
            // v5 Dark-Matter Cache
            .route("/cache/store", web::post().to(cache_page))
            .route("/cache/get", web::post().to(cache_get))
            .route("/cache/download", web::post().to(cache_download))
            .route("/cache/stats", web::get().to(cache_stats))
            .route("/cache/predict", web::post().to(cache_predict))
            // v5 Enhanced Swarm
            .route("/swarm/deploy", web::post().to(swarm_deploy))
            .route("/swarm/debate", web::post().to(swarm_debate))
            .route("/swarm/campaign", web::post().to(swarm_campaign))
            .route("/swarm/enhanced/stats", web::get().to(swarm_enhanced_stats))
            .route("/health", web::get().to(super::health::health_check))
            .route("/features", web::get().to(super::health::feature_status)),
    );

    #[cfg(feature = "experimental")]
    cfg.service(
        web::scope("/api")
            .wrap_fn(|req: ServiceRequest, srv| {
                let method = req.method().clone();
                let path = req.path().to_string();
                let app_state = req.app_data::<web::Data<Arc<RwLock<AppState>>>>().cloned();

                let auth_result = if api_request_requires_admin(&method, &path) {
                    if let Some(app_state) = app_state {
                        app_state
                            .try_read()
                            .map_err(|_| {
                                AstraError::Internal("api admin middleware state contention".into())
                            })
                            .and_then(|app| authorize_admin_request(req.request(), &app.config))
                    } else {
                        Err(AstraError::Internal(
                            "api admin middleware missing app state".into(),
                        ))
                    }
                } else {
                    Ok(())
                };

                if let Err(error) = auth_result {
                    return Either::Left(ready(Ok(req
                        .into_response(error.error_response())
                        .map_into_right_body())));
                }

                let fut = srv.call(req);
                Either::Right(async move {
                    let response = fut.await?;
                    Ok(response.map_into_left_body())
                })
            })
            .route("/warfare/status", web::get().to(warfare_status))
            .route(
                "/warfare/recovery/orchestrate",
                web::post().to(warfare_recovery_orchestrate),
            )
            .route(
                "/warfare/defense/orchestrate",
                web::post().to(warfare_defense_orchestrate),
            )
            .route(
                "/warfare/revenue/orchestrate",
                web::post().to(warfare_revenue_orchestrate),
            )
            .route("/parasitic/inject", web::post().to(parasitic_inject))
            .route("/parasitic/profile", web::post().to(parasitic_set_profile))
            .route("/parasitic/stats", web::get().to(parasitic_stats)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::lightning_loop::LightningLoop;
    use crate::chain::chain::Chain;
    use crate::httpa::bus::HttpaBus;
    use crate::httpa::control_plane::HttpaControlPlane;
    use crate::httpa::session::SessionManager;
    use crate::network::egress::{EgressMode, EgressPolicy};
    use crate::runtime::SessionManager as RuntimeSessionManager;
    use actix_web::{body::to_bytes, test as actix_test, App};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn test_store_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("astra-api-routes-{label}-{nonce}"))
    }

    fn test_state() -> EngineState {
        let mut state = EngineState::new(LightningLoop::new(test_store_dir("identity")));
        state.chain = Chain::new();
        let expires_at = chrono::Utc::now().timestamp_millis() + 86_400_000;
        for party in ["alice", "vendor", "owner"] {
            state
                .chain
                .register_payment_compliance_profile(PaymentComplianceRegistrationRequest {
                    party: party.into(),
                    legal_entity_id: format!("{party}-entity"),
                    jurisdiction: "US".into(),
                    kyc_reference: format!("kyc-{party}"),
                    aml_reference: format!("aml-{party}"),
                    custody_reference: format!("custody-{party}"),
                    sanctions_clear: true,
                    allowed_rails: vec![
                        crate::chain::value_protocol::SettlementRail::InternalLedger,
                    ],
                    allowed_compliance_modes: vec![
                        crate::chain::value_protocol::ComplianceMode::TaxAware,
                    ],
                    max_single_amount: 10_000.0,
                    expires_at,
                    metadata: None,
                })
                .expect("compliance profile registration should succeed");
        }
        state.semantic_acquisition = SemanticAcquisitionEngine::new_ephemeral();
        state.semantic_acquisition.set_egress_policy(EgressPolicy {
            mode: EgressMode::Direct,
            proxy_url: None,
            allow_direct_egress: true,
            scrub_identifying_headers: true,
            verification_required: false,
        });
        state
    }

    #[test]
    fn voice_invocation_accepts_codename_and_command() {
        let decision =
            resolve_voice_invocation("Zuno what is the system status", "zuno", true, None, None);
        assert!(decision.activated);
        assert_eq!(decision.matched_wake_word.as_deref(), Some("zuno"));
        assert_eq!(
            decision.command_text.as_deref(),
            Some("what is the system status")
        );
    }

    #[test]
    fn voice_invocation_accepts_wake_up_phrase_without_command() {
        let decision = resolve_voice_invocation("wake up zuno", "zuno", true, None, None);
        assert!(decision.activated);
        assert_eq!(decision.matched_wake_word.as_deref(), Some("wake up zuno"));
        assert!(decision.command_text.is_none());
    }

    #[test]
    fn voice_invocation_blocks_uninvoked_transcript_when_required() {
        let decision =
            resolve_voice_invocation("what is the weather today", "zuno", true, None, None);
        assert!(!decision.activated);
        assert!(decision.command_text.is_none());
    }

    #[test]
    fn voice_invocation_can_be_bypassed_for_follow_up_turns() {
        let decision = resolve_voice_invocation(
            "tell me the latest health report",
            "zuno",
            true,
            Some(false),
            None,
        );
        assert!(decision.activated);
        assert_eq!(
            decision.command_text.as_deref(),
            Some("tell me the latest health report")
        );
    }

    fn test_app_state(config: AppConfig) -> Arc<RwLock<AppState>> {
        let learning_loop = LightningLoop::new(test_store_dir("app-state"));
        Arc::new(RwLock::new(AppState {
            session_manager: SessionManager::new(
                config.httpa.session_ttl_secs,
                config.httpa.max_sessions,
            ),
            runtime_sessions: RuntimeSessionManager::new(
                config.engine_runtime.max_sessions,
                config.engine_runtime.max_messages_per_session,
                config.engine_runtime.event_buffer,
                config.engine_runtime.default_workspace.clone(),
                config.engine_runtime.session_prefix.clone(),
            ),
            config: config.clone(),
            start_time: chrono::Utc::now(),
            bus: HttpaBus::new(),
            control_plane: HttpaControlPlane::with_intent_retention(
                config.httpa.intent_retention_limit,
            ),
            learning_loop: learning_loop.clone(),
            openclaw_gateway: crate::openclaw::OpenClawGateway::new(
                &config.openclaw,
                learning_loop,
            ),
        }))
    }

    #[actix_web::test]
    async fn api_public_health_route_is_accessible_without_admin_token() {
        let mut config = AppConfig::personal_defaults();
        config.deployment.local_bind_only = false;
        let app_state = test_app_state(config.clone());
        let engine_state = Arc::new(RwLock::new(EngineState::from_config(
            config,
            LightningLoop::new(test_store_dir("engine-state-public")),
        )));

        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(app_state))
                .app_data(web::Data::new(engine_state))
                .configure(configure_api_routes),
        )
        .await;

        let request = actix_test::TestRequest::get()
            .uri("/api/health")
            .to_request();
        let response = actix_test::call_service(&app, request).await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
    }

    #[actix_web::test]
    async fn api_protected_route_requires_admin_when_remote() {
        let mut config = AppConfig::personal_defaults();
        config.deployment.local_bind_only = false;
        let app_state = test_app_state(config.clone());
        let engine_state = Arc::new(RwLock::new(EngineState::from_config(
            config,
            LightningLoop::new(test_store_dir("engine-state-protected")),
        )));

        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(app_state))
                .app_data(web::Data::new(engine_state))
                .configure(configure_api_routes),
        )
        .await;

        let request = actix_test::TestRequest::get()
            .uri("/api/assistant/autonomy")
            .to_request();
        let response = actix_test::call_service(&app, request).await;

        assert_eq!(response.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn api_admin_token_authorizes_protected_route() {
        let mut config = AppConfig::personal_defaults();
        config.deployment.local_bind_only = false;
        config.server.admin_token = Some("top-secret".into());
        let app_state = test_app_state(config.clone());
        let engine_state = Arc::new(RwLock::new(EngineState::from_config(
            config,
            LightningLoop::new(test_store_dir("engine-state-admin")),
        )));

        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(app_state))
                .app_data(web::Data::new(engine_state))
                .configure(configure_api_routes),
        )
        .await;

        let request = actix_test::TestRequest::get()
            .uri("/api/assistant/autonomy")
            .insert_header(("Authorization", "Bearer top-secret"))
            .to_request();
        let response = actix_test::call_service(&app, request).await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
    }

    #[actix_web::test]
    async fn assistant_brain_execute_endpoint_returns_typed_response() {
        let config = AppConfig::personal_defaults();
        let app_state = test_app_state(config.clone());
        let engine_state = Arc::new(RwLock::new(EngineState::from_config(
            config,
            LightningLoop::new(test_store_dir("engine-state-brain")),
        )));

        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(app_state))
                .app_data(web::Data::new(engine_state))
                .configure(configure_api_routes),
        )
        .await;

        let request = actix_test::TestRequest::post()
            .uri("/api/assistant/brain/execute")
            .set_json(serde_json::json!({
                "task": "plan a verified backend migration",
                "context": {"domain": "architecture"},
                "mode_hint": "plan",
                "require_verification": true
            }))
            .to_request();
        let response = actix_test::call_service(&app, request).await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body()).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json payload");
        assert_eq!(payload["success"], true);
        assert_eq!(payload["data"]["stage"], "completed");
        assert_eq!(
            payload["data"]["plan"]["execution_plan"]["goal"],
            "plan a verified backend migration"
        );
    }

    #[actix_web::test]
    async fn legacy_think_route_is_backed_by_one_brain() {
        let config = AppConfig::personal_defaults();
        let app_state = test_app_state(config.clone());
        let engine_state = Arc::new(RwLock::new(EngineState::from_config(
            config,
            LightningLoop::new(test_store_dir("engine-state-think")),
        )));

        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(app_state))
                .app_data(web::Data::new(engine_state))
                .configure(configure_api_routes),
        )
        .await;

        let request = actix_test::TestRequest::post()
            .uri("/api/think")
            .set_json(serde_json::json!({
                "problem": "outline a safe verification workflow",
                "context": {"domain": "architecture"}
            }))
            .to_request();
        let response = actix_test::call_service(&app, request).await;

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body()).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json payload");
        assert_eq!(payload["success"], true);
        assert!(payload["data"]["reasoning_trace"]
            .as_array()
            .expect("reasoning trace array")
            .iter()
            .any(|entry| entry.as_str().unwrap_or_default().contains("[omega]")));
    }

    #[test]
    fn api_public_route_catalog_keeps_identity_auth_surface_open() {
        assert!(api_request_is_public(
            &actix_web::http::Method::POST,
            "/api/identity/create"
        ));
        assert!(api_request_is_public(
            &actix_web::http::Method::POST,
            "/api/identity/challenge"
        ));
        assert!(api_request_is_public(
            &actix_web::http::Method::POST,
            "/api/identity/auth"
        ));
        assert!(api_request_is_public(
            &actix_web::http::Method::POST,
            "/api/identity/session/auth"
        ));
        assert!(!api_request_is_public(
            &actix_web::http::Method::GET,
            "/api/assistant/autonomy"
        ));
    }

    async fn spawn_semantic_acquisition_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener
            .local_addr()
            .expect("listener address should exist");
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 4096];
                    let read = stream
                        .read(&mut buffer)
                        .await
                        .expect("request should be readable");
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/");

                    let (status, content_type, body) = match path {
                        "/robots.txt" => (
                            "200 OK",
                            "text/plain",
                            "User-agent: *\nAllow: /\nDisallow: /private\n".to_string(),
                        ),
                        "/private" => (
                            "200 OK",
                            "text/html",
                            "<html><body><article><p>private</p></article></body></html>"
                                .to_string(),
                        ),
                        "/page1" => (
                            "200 OK",
                            "text/html",
                            "<html><head><title>Catalog Page 1</title></head><body><article><p>Catalog page one is ready for structured extraction.</p><table><tr><th>Name</th><th>Score</th></tr><tr><td>Ada</td><td>99</td></tr><tr><td>Linus</td><td>95</td></tr></table><a rel=\"next\" href=\"/page2\">Next</a></article></body></html>"
                                .to_string(),
                        ),
                        "/page2" => (
                            "200 OK",
                            "text/html",
                            "<html><head><title>Catalog Page 2</title></head><body><article><p>Catalog page two completes the workflow manifest.</p><table><tr><th>Name</th><th>Score</th></tr><tr><td>Grace</td><td>98</td></tr></table></article></body></html>"
                                .to_string(),
                        ),
                        _ => (
                            "200 OK",
                            "application/json",
                            "{\"title\":\"Semantic Pipeline\",\"items\":[\"graph\",\"ledger\"]}"
                                .to_string(),
                        ),
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .await
                        .expect("response should be writable");
                });
            }
        });

        (format!("http://{}", address), handle)
    }

    #[test]
    fn local_identity_can_complete_blockchain_session_authentication() {
        let mut state = test_state();
        let challenge = build_identity_challenge(
            &mut state,
            IdentityChallengeRequest {
                actor: None,
                namespace: Some("acct".into()),
                purpose: Some("assistant_login".into()),
                session_ttl_seconds: Some(1800),
                metadata: None,
            },
        )
        .expect("challenge should be created");

        assert!(challenge.local_identity_can_sign);
        let public_key_hex = challenge
            .local_public_key_hex
            .clone()
            .expect("local public key should be exposed");
        let signature_hex = state
            .identity
            .sign_message_hex(challenge.canonical_message.as_bytes())
            .expect("local identity should sign the canonical challenge");

        let session = complete_identity_session_auth(
            &mut state,
            IdentitySessionAuthRequest {
                actor: challenge.actor.clone(),
                challenge_id: challenge.challenge_id.clone(),
                public_key_hex,
                signature_hex,
                metadata: None,
            },
        )
        .expect("session auth should succeed");

        assert!(session.authenticated);
        assert_eq!(session.actor, challenge.actor);
        assert!(state
            .identity_sessions
            .contains_key(session.session_token.as_str()));
    }

    #[test]
    fn identity_challenges_are_single_use() {
        let mut state = test_state();
        let challenge = build_identity_challenge(
            &mut state,
            IdentityChallengeRequest {
                actor: None,
                namespace: Some("acct".into()),
                purpose: Some("assistant_login".into()),
                session_ttl_seconds: Some(900),
                metadata: None,
            },
        )
        .expect("challenge should be created");
        let signature_hex = state
            .identity
            .sign_message_hex(challenge.canonical_message.as_bytes())
            .expect("local identity should sign the canonical challenge");
        let public_key_hex = challenge
            .local_public_key_hex
            .clone()
            .expect("local public key should exist");

        complete_identity_session_auth(
            &mut state,
            IdentitySessionAuthRequest {
                actor: challenge.actor.clone(),
                challenge_id: challenge.challenge_id.clone(),
                public_key_hex: public_key_hex.clone(),
                signature_hex: signature_hex.clone(),
                metadata: None,
            },
        )
        .expect("first session auth should succeed");

        let replay = complete_identity_session_auth(
            &mut state,
            IdentitySessionAuthRequest {
                actor: challenge.actor,
                challenge_id: challenge.challenge_id,
                public_key_hex,
                signature_hex,
                metadata: None,
            },
        );

        assert!(matches!(replay, Err(AstraError::SessionNotFound(_))));
    }

    #[actix_rt::test]
    async fn semantic_render_can_anchor_results_to_chain() {
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));
        let response = semantic_render_analyze(
            state,
            web::Json(SemanticRenderRequest {
                url: "https://example.com/research/pipeline".into(),
                html: r#"
                    <html lang="en">
                      <head>
                        <title>Pipeline</title>
                        <meta name="description" content="Semantic extraction" />
                        <script type="application/ld+json">{"@type":"Article","headline":"Pipeline"}</script>
                      </head>
                      <body>
                        <article>
                          <p>The semantic engine extracts durable evidence from backend acquisition.</p>
                          <a href="/docs">Docs</a>
                        </article>
                      </body>
                    </html>
                "#
                .into(),
                content_type: "text/html".into(),
                attestor: Some("assistant:semantic-render".into()),
                notarize_to_chain: true,
                max_segments: 12,
                max_links: 8,
            }),
        )
        .await
        .expect("semantic render route should succeed");

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(payload["data"]["render"]["metadata"]["title"], "Pipeline");
        assert!(payload["data"]["ledger_commitment"]["commitment_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }

    #[actix_rt::test]
    async fn semantic_acquisition_can_fetch_render_and_anchor() {
        let (base_url, server) = spawn_semantic_acquisition_server().await;
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));
        let response = semantic_acquisition_execute(
            state.clone(),
            web::Json(SemanticAcquisitionRequest {
                url: format!("{base_url}/dataset"),
                allowed_domains: vec!["127.0.0.1".into()],
                respect_robots: true,
                timeout_ms: 4_000,
                max_response_bytes: 256_000,
                max_retries: 1,
                max_segments: 12,
                max_links: 8,
                requests_per_minute: 12,
                notarize_to_chain: true,
                attestor: Some("assistant:semantic-acquisition".into()),
                persist_report: true,
                user_agent: "AstraAssistant/2.0".into(),
                force_content_type: None,
            }),
        )
        .await
        .expect("semantic acquisition route should succeed");

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["data"]["acquisition"]["render"]["metadata"]["title"],
            "127.0.0.1"
        );
        assert_eq!(
            payload["data"]["acquisition"]["provenance"]["response_class"],
            "json"
        );
        assert_eq!(
            payload["data"]["adapter_report"]["recommended_adapter"],
            "json_collection"
        );
        assert!(payload["data"]["ledger_commitment"]["commitment_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));

        let stats_response = semantic_acquisition_stats(state)
            .await
            .expect("acquisition stats should succeed");
        let stats_body = to_bytes(stats_response.into_body())
            .await
            .expect("stats body should be readable");
        let stats_payload: serde_json::Value =
            serde_json::from_slice(&stats_body).expect("stats body should be valid json");
        assert_eq!(
            stats_payload["data"]["chain_notarizations"],
            serde_json::Value::from(1)
        );

        server.abort();
    }

    #[actix_rt::test]
    async fn semantic_workflow_builds_paginated_manifest() {
        let (base_url, server) = spawn_semantic_acquisition_server().await;
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));
        let response = semantic_workflow_execute(
            state.clone(),
            web::Json(SemanticWorkflowRequest {
                seed_urls: vec![format!("{base_url}/page1")],
                allowed_domains: vec!["127.0.0.1".into()],
                follow_pagination: true,
                max_pages: 3,
                continue_on_error: true,
                respect_robots: true,
                timeout_ms: 4_000,
                max_response_bytes: 256_000,
                max_retries: 1,
                max_segments: 12,
                max_links: 8,
                requests_per_minute: 12,
                persist_report: true,
                user_agent: "AstraAssistant/2.0".into(),
                notarize_manifest_to_chain: true,
                attestor: Some("assistant:semantic-workflow".into()),
            }),
        )
        .await
        .expect("semantic workflow route should succeed");

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["data"]["manifest"]["completed_pages"],
            serde_json::Value::from(2)
        );
        assert_eq!(
            payload["data"]["manifest"]["adapter_summary"]["tabular_html"],
            serde_json::Value::from(2)
        );
        assert!(payload["data"]["manifest"]["manifest_hash"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert!(payload["data"]["ledger_commitment"]["commitment_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));

        server.abort();
    }

    #[actix_rt::test]
    async fn abyss_mission_route_builds_research_graph() {
        let (base_url, server) = spawn_semantic_acquisition_server().await;
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));
        let response = abyss_mission_orchestrate(
            state.clone(),
            web::Json(AbyssMissionRequest {
                objective: "map the catalog workflow".into(),
                seed_urls: vec![format!("{base_url}/page1")],
                allowed_domains: vec!["127.0.0.1".into()],
                follow_pagination: true,
                max_pages: 3,
                continue_on_error: true,
                respect_robots: true,
                timeout_ms: 4_000,
                max_response_bytes: 256_000,
                max_retries: 1,
                max_segments: 12,
                max_links: 8,
                requests_per_minute: 12,
                persist_report: true,
                user_agent: "AstraAssistant/2.0".into(),
                local_focus_tags: vec!["catalog".into(), "procurement".into()],
                enable_truth_triangulation: true,
                enable_archive_guidance: true,
                notarize_to_chain: true,
                attestor: Some("assistant:test".into()),
            }),
        )
        .await
        .expect("abyss route should succeed");

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("payload should be valid json");
        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["data"]["report"]["summary"]["pages_crawled"],
            serde_json::Value::from(2)
        );
        assert!(payload["data"]["report"]["knowledge_graph"]["nodes"]
            .as_array()
            .is_some_and(|nodes| !nodes.is_empty()));
        assert!(payload["data"]["ledger_commitment"]["commitment_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));

        let status_response = abyss_status(state).await.expect("status should succeed");
        let status_body = to_bytes(status_response.into_body())
            .await
            .expect("status body should be readable");
        let status_payload: serde_json::Value =
            serde_json::from_slice(&status_body).expect("status payload should be json");
        assert_eq!(
            status_payload["data"]["total_missions"],
            serde_json::Value::from(1)
        );

        server.abort();
    }

    #[actix_rt::test]
    async fn personal_mission_route_integrates_cpu_only_stack() {
        let (base_url, server) = spawn_semantic_acquisition_server().await;
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));
        let response = personal_mission_orchestrate(
            state.clone(),
            web::Json(PersonalMissionRequest {
                objective: "prepare a private vendor intelligence brief".into(),
                seed_urls: vec![format!("{base_url}/page1")],
                allowed_domains: vec!["127.0.0.1".into()],
                current_focus: Some("summarize supplier resilience".into()),
                open_files: vec!["C:/astra browser/README.md".into()],
                local_focus_tags: vec!["operations".into(), "research".into()],
                local_assets: vec!["notes/vendors.csv".into()],
                threat_signals: vec!["unexpected login prompt".into()],
                owned_assets: vec!["private research notes".into()],
                service_offers: vec!["vendor risk brief".into()],
                budget_receiver: Some("vendor".into()),
                budget_amount: Some(25.0),
                budget_currency: Some("USD".into()),
                private_mode: true,
                max_pages: 3,
                continue_on_error: true,
                respect_robots: true,
                timeout_ms: 4_000,
                max_response_bytes: 256_000,
                max_retries: 1,
                max_segments: 12,
                max_links: 8,
                requests_per_minute: 12,
                persist_report: true,
                user_agent: "AstraAssistant/2.0".into(),
                notarize_to_chain: true,
                attestor: Some("assistant:test".into()),
            }),
        )
        .await
        .expect("personal mission should succeed");

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("payload should be valid json");
        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["data"]["abyss_report"]["summary"]["pages_crawled"],
            serde_json::Value::from(2)
        );
        assert!(
            payload["data"]["ultimate"]["cognition"]["temporal_reasoning_fabric"]
                ["selected_decision"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(payload["data"]["evolution"]["evolved_params"]
            .as_object()
            .is_some_and(|value| !value.is_empty()));
        assert_eq!(
            payload["data"]["mission"]["pages_crawled"],
            serde_json::Value::from(2)
        );
        assert!(payload["data"]["ledger_commitment"]["commitment_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));

        let status_response = personal_mission_status(state)
            .await
            .expect("personal status should succeed");
        let status_body = to_bytes(status_response.into_body())
            .await
            .expect("status body should be readable");
        let status_payload: serde_json::Value =
            serde_json::from_slice(&status_body).expect("status payload should be json");
        assert_eq!(
            status_payload["data"]["total_missions"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            status_payload["data"]["llm_free"],
            serde_json::Value::Bool(true)
        );

        server.abort();
    }

    #[actix_rt::test]
    async fn revolutionary_sandbox_can_notarize_report() {
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));
        let response = revolutionary_sandbox_analyze(
            state,
            web::Json(RevolutionarySandboxRequest {
                execution_target: "sudo rm -rf /".into(),
                objective: "analyze dangerous command".into(),
                language: Some("shell".into()),
                allowed_paths: vec!["/phantom/workspace".into()],
                allowed_hosts: Vec::new(),
                expected_outputs: vec!["risk report".into()],
                environment: BTreeMap::new(),
                require_firebreaks: true,
                notarize_to_chain: true,
                attestor: Some("assistant:test".into()),
            }),
        )
        .await
        .expect("revolutionary sandbox route should succeed");

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["data"]["report"]["verdict"]["risk_level"],
            serde_json::Value::String("critical".into())
        );
        assert!(payload["data"]["ledger_commitment"]["commitment_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }

    #[actix_rt::test]
    async fn revolutionary_value_and_protocol_routes_succeed() {
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));

        let value_response = revolutionary_value_orchestrate(
            state.clone(),
            web::Json(RevolutionaryValueRequest {
                sender: "alice".into(),
                receiver: "vendor".into(),
                amount: 75.0,
                purpose: "pay for verified enterprise crawling".into(),
                currency: "USD".into(),
                settlement_rail: crate::chain::value_protocol::SettlementRail::InternalLedger,
                compliance: crate::chain::value_protocol::ComplianceMode::TaxAware,
                privacy_mode:
                    crate::chain::value_protocol::PrivacyPreservationMode::AttestedMinimization,
                risk_tier: crate::chain::value_protocol::EnterpriseRiskTier::Standard,
                connector_id: None,
                providers: Vec::new(),
                allow_split_routing: true,
                require_escrow: true,
                autocommit: false,
                fulfillment_window_seconds: 1_200,
                min_quality_score: 0.92,
                evidence: vec!["plan-proof".into()],
                cooperative_unlock_parties: vec!["alice".into(), "vendor".into()],
                metadata: None,
                notarize_to_chain: true,
                attestor: Some("assistant:test".into()),
            }),
        )
        .await
        .expect("revolutionary value route should succeed");

        assert_eq!(value_response.status(), actix_web::http::StatusCode::OK);
        let value_body = to_bytes(value_response.into_body())
            .await
            .expect("value response should be readable");
        let value_payload: serde_json::Value =
            serde_json::from_slice(&value_body).expect("value payload should be json");
        assert_eq!(value_payload["success"], serde_json::Value::Bool(true));
        assert!(
            value_payload["data"]["report"]["guarded_payment"]["contract_id"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );

        let protocol_response = revolutionary_protocol_orchestrate(
            state.clone(),
            web::Json(RevolutionaryProtocolRequest {
                packets: vec![
                    crate::features::revolutionary_features::SemanticPacketInput {
                        packet_id: "p1".into(),
                        content: "Fetch the latest verified result".into(),
                        semantic_type: None,
                        semantic_dependencies: Vec::new(),
                    },
                    crate::features::revolutionary_features::SemanticPacketInput {
                        packet_id: "p2".into(),
                        content: "Fetch the latest verified result".into(),
                        semantic_type: None,
                        semantic_dependencies: vec!["p1".into()],
                    },
                ],
                requests: vec![
                    crate::features::revolutionary_features::ProtocolRequestInput {
                        request_id: "r1".into(),
                        intent: "fetch latest result 100".into(),
                        trust_profile: "standard".into(),
                        submitted_at_ms: 10,
                        dependencies: Vec::new(),
                        arrival_rank: 2,
                        priority: "high".into(),
                    },
                    crate::features::revolutionary_features::ProtocolRequestInput {
                        request_id: "r2".into(),
                        intent: "fetch latest result 101".into(),
                        trust_profile: "standard".into(),
                        submitted_at_ms: 40,
                        dependencies: vec!["r1".into()],
                        arrival_rank: 1,
                        priority: "normal".into(),
                    },
                ],
                system_load: crate::features::revolutionary_features::ProtocolSystemLoad {
                    cpu_utilization: 0.72,
                    memory_pressure: 0.66,
                    network_congestion: 0.51,
                    queue_depth: 14,
                },
                outcome_samples: vec![
                    crate::features::revolutionary_features::ProtocolOutcomeSample {
                        success: true,
                        latency_ms: 180,
                        useful_bytes: 900,
                        total_bytes: 1_000,
                        recovered_errors: 1,
                    },
                    crate::features::revolutionary_features::ProtocolOutcomeSample {
                        success: true,
                        latency_ms: 220,
                        useful_bytes: 850,
                        total_bytes: 1_050,
                        recovered_errors: 0,
                    },
                ],
                fold_window_ms: Some(100),
                notarize_to_chain: true,
                attestor: Some("assistant:test".into()),
            }),
        )
        .await
        .expect("revolutionary protocol route should succeed");

        assert_eq!(protocol_response.status(), actix_web::http::StatusCode::OK);
        let protocol_body = to_bytes(protocol_response.into_body())
            .await
            .expect("protocol response should be readable");
        let protocol_payload: serde_json::Value =
            serde_json::from_slice(&protocol_body).expect("protocol payload should be json");
        assert_eq!(protocol_payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            protocol_payload["data"]["report"]["temporal_request_folding"]["requests_saved"],
            serde_json::Value::from(1)
        );
        assert!(
            protocol_payload["data"]["ledger_commitment"]["commitment_id"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
    }

    #[actix_rt::test]
    async fn ultimate_feature_routes_succeed() {
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));

        let cognition_response = ultimate_cognition_orchestrate(
            state.clone(),
            web::Json(UltimateCognitionRequest {
                problem: "choose a durable deployment strategy for a scaling API".into(),
                candidate_decisions: vec![
                    crate::features::ultimate_astra_features::CognitiveDecisionInput {
                        content: "pick the cheapest provider".into(),
                        immediate_score: 0.74,
                    },
                    crate::features::ultimate_astra_features::CognitiveDecisionInput {
                        content: "use a staged multi-region rollout with rollback".into(),
                        immediate_score: 0.71,
                    },
                ],
                solved_patterns: Vec::new(),
                contradictory_claims: vec![
                    crate::features::ultimate_astra_features::ContradictoryClaimInput {
                        claim_a: "centralized systems are efficient".into(),
                        score_a: 0.82,
                        claim_b: "distributed systems are resilient".into(),
                        score_b: 0.8,
                    },
                ],
                active_tasks: Vec::new(),
                behavior_signals: vec![
                    crate::features::ultimate_astra_features::BehaviorSignalInput {
                        predicted_intent: "request a rollout checklist".into(),
                        confidence: 0.83,
                        typical_hour: Some(9),
                        context_chain: vec!["deploy".into(), "verify".into()],
                    },
                ],
                concept_clusters: Vec::new(),
                idle_seconds: 90,
                max_depth: 5,
                time_horizons_minutes: vec![5, 60, 1_440],
                attestor: Some("assistant:test".into()),
                notarize_to_chain: true,
            }),
        )
        .await
        .expect("ultimate cognition route should succeed");
        assert_eq!(cognition_response.status(), actix_web::http::StatusCode::OK);
        let cognition_body = to_bytes(cognition_response.into_body())
            .await
            .expect("cognition body should be readable");
        let cognition_payload: serde_json::Value =
            serde_json::from_slice(&cognition_body).expect("cognition payload should be json");
        assert_eq!(cognition_payload["success"], serde_json::Value::Bool(true));
        assert!(
            cognition_payload["data"]["report"]["temporal_reasoning_fabric"]["selected_decision"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );

        let acquisition_response = ultimate_acquisition_orchestrate(
            state.clone(),
            web::Json(UltimateAcquisitionRequest {
                targets: vec![
                    crate::features::ultimate_astra_features::AcquisitionTargetInput {
                        url: "https://example.com/docs".into(),
                        semantic_goal: "collect reference docs".into(),
                        desired_geo: None,
                        priority: "high".into(),
                    },
                    crate::features::ultimate_astra_features::AcquisitionTargetInput {
                        url: "https://wikipedia.org/wiki/Rust_(programming_language)".into(),
                        semantic_goal: "collect encyclopedic overview".into(),
                        desired_geo: None,
                        priority: "normal".into(),
                    },
                ],
                proxies: Vec::new(),
                cover_domains: vec!["ietf.org".into(), "docs.rs".into(), "mozilla.org".into()],
                scatter_window_ms: 30_000,
                min_mix_batch: 12,
                allowed_profile_families: vec!["chrome_macos".into(), "firefox_linux".into()],
                respect_robots: true,
                attestor: Some("assistant:test".into()),
                notarize_to_chain: true,
            }),
        )
        .await
        .expect("ultimate acquisition route should succeed");
        assert_eq!(
            acquisition_response.status(),
            actix_web::http::StatusCode::OK
        );
        let acquisition_body = to_bytes(acquisition_response.into_body())
            .await
            .expect("acquisition body should be readable");
        let acquisition_payload: serde_json::Value =
            serde_json::from_slice(&acquisition_body).expect("acquisition payload should be json");
        assert_eq!(
            acquisition_payload["success"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            acquisition_payload["data"]["report"]["request_origin_erasure"]["mix_hops"],
            serde_json::Value::from(3)
        );

        let assistant_response = ultimate_assistant_orchestrate(
            state.clone(),
            web::Json(UltimateAssistantRequest {
                text: "fix the failing build quickly".into(),
                typing_speed_wpm: 92.0,
                deletions: 8,
                total_keystrokes: 40,
                has_code_open: true,
                recent_errors: 3,
                is_week_start: true,
                is_morning: true,
                open_files: vec!["src/main.rs".into()],
                clipboard_excerpt: Some("https://docs.rs".into()),
                events: vec![
                    crate::features::ultimate_astra_features::CalendarEventInput {
                        title: "Release review".into(),
                        minutes_until: 10,
                        importance: 0.9,
                        topic: "deployment".into(),
                    },
                ],
                metrics: vec![
                    crate::features::ultimate_astra_features::MetricSignalInput {
                        metric: "build_failures".into(),
                        value: 5.0,
                        threshold: 1.0,
                        direction: "above".into(),
                    },
                ],
                goal: "stabilize release process".into(),
                seed_cells: Vec::new(),
                schedule_tasks: vec![
                    crate::features::ultimate_astra_features::ScheduleTaskInput {
                        task_id: "task1".into(),
                        title: "Repair failing tests".into(),
                        duration_minutes: 90,
                        energy_required: 0.8,
                        context: "debugging".into(),
                        deadline_minutes: Some(120),
                        quick: false,
                    },
                ],
                energy_windows: vec![
                    crate::features::ultimate_astra_features::EnergyWindowInput {
                        start_hour: 8,
                        end_hour: 10,
                        energy_score: 0.9,
                    },
                ],
                current_hour: 9,
                attestor: Some("assistant:test".into()),
                notarize_to_chain: true,
            }),
        )
        .await
        .expect("ultimate assistant route should succeed");
        assert_eq!(assistant_response.status(), actix_web::http::StatusCode::OK);
        let assistant_body = to_bytes(assistant_response.into_body())
            .await
            .expect("assistant body should be readable");
        let assistant_payload: serde_json::Value =
            serde_json::from_slice(&assistant_body).expect("assistant payload should be json");
        assert_eq!(assistant_payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            assistant_payload["data"]["report"]["ambient_intent_inference"]["response_style"],
            serde_json::Value::String("telegraphic".into())
        );

        let knowledge_response = ultimate_knowledge_orchestrate(
            state,
            web::Json(UltimateKnowledgeRequest {
                domain: "soil science".into(),
                question: "how should I interpret a garden soil report".into(),
                sources: vec![
                    crate::features::ultimate_astra_features::SourceDocumentInput {
                        title: "USDA Soil Guide".into(),
                        domain: "soil science".into(),
                        content: "Healthy soil should balance pH, drainage, and nutrient density. Garden plans must account for organic matter and texture.".into(),
                        authority_score: 0.92,
                    },
                    crate::features::ultimate_astra_features::SourceDocumentInput {
                        title: "Extension Bulletin".into(),
                        domain: "soil science".into(),
                        content: "Vegetable soils should stay in a moderate pH range and require careful phosphorus management.".into(),
                        authority_score: 0.88,
                    },
                ],
                live_feeds: vec!["extension_updates".into()],
                competing_claims: vec![
                    crate::features::ultimate_astra_features::FrontierClaimInput {
                        theory: "organic matter is the dominant lever".into(),
                        support_score: 0.7,
                        source_count: 4,
                    },
                    crate::features::ultimate_astra_features::FrontierClaimInput {
                        theory: "micronutrient balance dominates outcomes".into(),
                        support_score: 0.52,
                        source_count: 2,
                    },
                ],
                synthesis_problem: "improve hospital infection rates".into(),
                cross_domain_matches: vec![
                    crate::features::ultimate_astra_features::CrossDomainMatchInput {
                        label: "airline crew checklists".into(),
                        source_domain: "aviation".into(),
                        structural_similarity: 0.63,
                    },
                ],
                recency_events: vec!["new extension note on pH drift".into()],
                attestor: Some("assistant:test".into()),
                notarize_to_chain: true,
            }),
        )
        .await
        .expect("ultimate knowledge route should succeed");
        assert_eq!(knowledge_response.status(), actix_web::http::StatusCode::OK);
        let knowledge_body = to_bytes(knowledge_response.into_body())
            .await
            .expect("knowledge body should be readable");
        let knowledge_payload: serde_json::Value =
            serde_json::from_slice(&knowledge_body).expect("knowledge payload should be json");
        assert_eq!(knowledge_payload["success"], serde_json::Value::Bool(true));
        assert!(
            knowledge_payload["data"]["report"]["universal_domain_compiler"]["ontology"]
                .as_array()
                .is_some_and(|value| !value.is_empty())
        );
    }

    #[cfg(feature = "experimental")]
    #[actix_rt::test]
    async fn warfare_feature_routes_succeed() {
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));

        let recovery_response = warfare_recovery_orchestrate(
            state.clone(),
            web::Json(WarfareRecoveryRequest {
                case_id: "case-001".into(),
                targets: vec![
                    crate::features::warfare_edition_features::RecoveryTargetInput {
                        asset_id: "docs/report.pdf".into(),
                        format_hint: None,
                        corruption_signals: vec!["header_damage".into(), "tail_truncation".into()],
                        fragment_count: 2,
                        snapshots_available: 1,
                        encrypted: false,
                        directory_hint: Some("docs".into()),
                    },
                ],
                memory_artifacts: vec![
                    crate::features::warfare_edition_features::MemoryArtifactInput {
                        artifact_id: "artifact-1".into(),
                        content_class: "session".into(),
                        contains_secrets: true,
                        confidence: 0.8,
                    },
                ],
                protect_outputs: true,
                attestor: Some("assistant:test".into()),
                notarize_to_chain: true,
            }),
        )
        .await
        .expect("warfare recovery route should succeed");
        assert_eq!(recovery_response.status(), actix_web::http::StatusCode::OK);
        let recovery_body = to_bytes(recovery_response.into_body())
            .await
            .expect("recovery body should be readable");
        let recovery_payload: serde_json::Value =
            serde_json::from_slice(&recovery_body).expect("recovery payload should be json");
        assert_eq!(recovery_payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            recovery_payload["data"]["report"]["self_healing_data_organism"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert!(
            recovery_payload["data"]["ledger_commitment"]["commitment_id"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );

        let defense_response = warfare_defense_orchestrate(
            state.clone(),
            web::Json(WarfareDefenseRequest {
                samples: vec![
                    crate::features::warfare_edition_features::ThreatSampleInput {
                        sample_id: "sample-x".into(),
                        family_hint: Some("night-owl".into()),
                        behaviors: vec!["loader encrypt exfil".into()],
                        packer_layers: 2,
                        c2_indicators: vec!["c2.example.test:443".into()],
                        privileges: vec!["kernel".into()],
                        touched_paths: vec!["C:/Windows/System32/drivers/x.sys".into()],
                    },
                ],
                ransomware_incidents: vec![
                    crate::features::warfare_edition_features::RansomwareIncidentInput {
                        incident_id: "incident-x".into(),
                        encrypted_extensions: vec![".locked".into()],
                        recent_restore_points: 2,
                        impacted_paths: 24,
                    },
                ],
                software_inventory: vec![
                    crate::features::warfare_edition_features::SoftwareRiskInput {
                        product: "astra".into(),
                        version: "2.0.0".into(),
                        recent_cve_count: 7,
                        days_since_last_patch: 61,
                        attack_surface_score: 0.79,
                        hardening_available: vec!["enable sandbox".into()],
                    },
                ],
                attestor: Some("assistant:test".into()),
                notarize_to_chain: true,
            }),
        )
        .await
        .expect("warfare defense route should succeed");
        assert_eq!(defense_response.status(), actix_web::http::StatusCode::OK);
        let defense_body = to_bytes(defense_response.into_body())
            .await
            .expect("defense body should be readable");
        let defense_payload: serde_json::Value =
            serde_json::from_slice(&defense_body).expect("defense payload should be json");
        assert_eq!(defense_payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            defense_payload["data"]["report"]["malware_vivisection_lab"][0]["blocked_execution"],
            serde_json::Value::Bool(true)
        );

        let revenue_response = warfare_revenue_orchestrate(
            state.clone(),
            web::Json(WarfareRevenueRequest {
                market_quotes: vec![
                    crate::features::warfare_edition_features::MarketQuoteInput {
                        asset: "AST".into(),
                        venue: "venue-a".into(),
                        price: 100.0,
                        fee_bps: 5.0,
                        liquidity_score: 0.9,
                        compliance_allowed: true,
                    },
                    crate::features::warfare_edition_features::MarketQuoteInput {
                        asset: "AST".into(),
                        venue: "venue-b".into(),
                        price: 102.5,
                        fee_bps: 5.0,
                        liquidity_score: 0.88,
                        compliance_allowed: true,
                    },
                ],
                content_channels: vec![
                    crate::features::warfare_edition_features::ContentChannelInput {
                        channel: "substack".into(),
                        content_type: "technical_article".into(),
                        monetization: "subscription".into(),
                        capacity_score: 0.7,
                    },
                ],
                service_requests: vec![
                    crate::features::warfare_edition_features::ServiceRequestInput {
                        service: "data pipeline review".into(),
                        buyer_segment: "startup".into(),
                        budget_usd: 450.0,
                        urgency: 0.7,
                        authorized: true,
                    },
                ],
                bounty_programs: vec![
                    crate::features::warfare_edition_features::BountyProgramInput {
                        program: "unsafe-program".into(),
                        scope: "public internet".into(),
                        reward_usd: 1500.0,
                        authorized_assets_only: false,
                        exploit_required: true,
                    },
                ],
                owned_assets: vec![crate::features::warfare_edition_features::OwnedAssetInput {
                    asset_id: "dataset-7".into(),
                    asset_type: "dataset".into(),
                    utilization: 0.2,
                    maintenance_cost_usd: 15.0,
                    monetizable: true,
                }],
                yield_options: vec![
                    crate::features::warfare_edition_features::YieldOptionInput {
                        name: "treasury".into(),
                        expected_return: 0.06,
                        lock_days: 30,
                        risk_score: 0.08,
                        compliance_allowed: true,
                    },
                ],
                market_signals: vec![
                    crate::features::warfare_edition_features::MarketSignalInput {
                        topic: "AST".into(),
                        sentiment: 0.55,
                        confidence: 0.75,
                    },
                ],
                freelance_leads: vec![
                    crate::features::warfare_edition_features::FreelanceLeadInput {
                        platform: "Upwork".into(),
                        project_type: "data analysis".into(),
                        budget_usd: 300.0,
                        fit_score: 0.85,
                    },
                ],
                attestor: Some("assistant:test".into()),
                notarize_to_chain: true,
            }),
        )
        .await
        .expect("warfare revenue route should succeed");
        assert_eq!(revenue_response.status(), actix_web::http::StatusCode::OK);
        let revenue_body = to_bytes(revenue_response.into_body())
            .await
            .expect("revenue body should be readable");
        let revenue_payload: serde_json::Value =
            serde_json::from_slice(&revenue_body).expect("revenue payload should be json");
        assert_eq!(revenue_payload["success"], serde_json::Value::Bool(true));
        assert!(
            revenue_payload["data"]["report"]["revenue_portfolio_organism"]["expected_monthly_usd"]
                .as_f64()
                .is_some_and(|value| value > 0.0)
        );

        let status_response = warfare_status(state)
            .await
            .expect("warfare status should succeed");
        let status_body = to_bytes(status_response.into_body())
            .await
            .expect("status body should be readable");
        let status_payload: serde_json::Value =
            serde_json::from_slice(&status_body).expect("status payload should be json");
        assert_eq!(
            status_payload["data"]["recovery_reports"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            status_payload["data"]["defense_reports"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            status_payload["data"]["revenue_reports"],
            serde_json::Value::from(1)
        );
    }

    #[actix_rt::test]
    async fn unbreakable_routes_surface_enterprise_state() {
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));

        let heartbeat_response = unbreakable_cryogenic_heartbeat(
            state.clone(),
            web::Json(CryogenicHeartbeatRequest {
                subsystem: "runtime".into(),
                active_tasks: vec!["semantic-fetch".into()],
            }),
        )
        .await
        .expect("cryogenic heartbeat should succeed");
        assert_eq!(heartbeat_response.status(), actix_web::http::StatusCode::OK);

        let cryo_report_response = unbreakable_cryogenic_report(state.clone())
            .await
            .expect("cryogenic report should succeed");
        let cryo_report_body = to_bytes(cryo_report_response.into_body())
            .await
            .expect("cryogenic report body should be readable");
        let cryo_report_payload: serde_json::Value =
            serde_json::from_slice(&cryo_report_body).expect("cryo report payload should be json");
        assert_eq!(
            cryo_report_payload["success"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            cryo_report_payload["data"].as_array().map(Vec::len),
            Some(1)
        );

        let fact_response = unbreakable_anchor_fact(
            state.clone(),
            web::Json(UnbreakableFactRequest {
                fact_id: "fact-a".into(),
                statement: "Rust ownership rules help prevent memory safety bugs".into(),
                source_kind: "docs".into(),
                corroboration_count: 2,
            }),
        )
        .await
        .expect("fact anchor should succeed");
        assert_eq!(fact_response.status(), actix_web::http::StatusCode::OK);

        let fact_records_response = unbreakable_reality_anchor_records(state.clone())
            .await
            .expect("fact records should succeed");
        let fact_records_body = to_bytes(fact_records_response.into_body())
            .await
            .expect("fact records body should be readable");
        let fact_records_payload: serde_json::Value = serde_json::from_slice(&fact_records_body)
            .expect("fact records payload should be json");
        assert_eq!(
            fact_records_payload["success"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            fact_records_payload["data"].as_array().map(Vec::len),
            Some(1)
        );

        let treasury_response = chain_treasury_capture_revenue(
            state.clone(),
            web::Json(RevenueCaptureRequest {
                actor: "owner".into(),
                strategy: crate::features::unbreakable_backend::RevenueStrategy::Consulting,
                amount_minor: 25_000,
                currency: "USD".into(),
                compliance_mode: crate::chain::value_protocol::ComplianceMode::TaxAware,
                source_reference: "msa-001".into(),
                evidence_refs: vec!["contract".into(), "invoice".into()],
            }),
        )
        .await
        .expect("treasury capture should succeed");
        assert_eq!(treasury_response.status(), actix_web::http::StatusCode::OK);

        let vault_inventory_response = unbreakable_vault_inventory(state.clone())
            .await
            .expect("vault inventory should succeed");
        let vault_inventory_body = to_bytes(vault_inventory_response.into_body())
            .await
            .expect("vault inventory body should be readable");
        let vault_inventory_payload: serde_json::Value =
            serde_json::from_slice(&vault_inventory_body)
                .expect("vault inventory payload should be json");
        assert_eq!(
            vault_inventory_payload["data"][0]["namespace"],
            serde_json::Value::String("treasury".into())
        );

        let vault_audit_response = unbreakable_vault_audit(state.clone())
            .await
            .expect("vault audit should succeed");
        let vault_audit_body = to_bytes(vault_audit_response.into_body())
            .await
            .expect("vault audit body should be readable");
        let vault_audit_payload: serde_json::Value =
            serde_json::from_slice(&vault_audit_body).expect("vault audit payload should be json");
        assert!(vault_audit_payload["data"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));

        let receipt_list_response = chain_treasury_receipts(state.clone())
            .await
            .expect("treasury receipts should succeed");
        let receipt_list_body = to_bytes(receipt_list_response.into_body())
            .await
            .expect("treasury receipts body should be readable");
        let receipt_list_payload: serde_json::Value =
            serde_json::from_slice(&receipt_list_body).expect("receipt payload should be json");
        assert_eq!(
            receipt_list_payload["data"].as_array().map(Vec::len),
            Some(1)
        );

        let rotate_response = unbreakable_vault_rotate(state.clone())
            .await
            .expect("vault rotation should succeed");
        let rotate_body = to_bytes(rotate_response.into_body())
            .await
            .expect("vault rotation body should be readable");
        let rotate_payload: serde_json::Value =
            serde_json::from_slice(&rotate_body).expect("rotate payload should be json");
        assert_eq!(rotate_payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            rotate_payload["data"]["records_rewrapped"],
            serde_json::Value::from(1)
        );

        let patrol_response = unbreakable_immune_patrol(state.clone())
            .await
            .expect("immune patrol should succeed");
        assert_eq!(patrol_response.status(), actix_web::http::StatusCode::OK);

        let immune_memory_response = unbreakable_immune_memory(state)
            .await
            .expect("immune memory should succeed");
        let immune_memory_body = to_bytes(immune_memory_response.into_body())
            .await
            .expect("immune memory body should be readable");
        let immune_memory_payload: serde_json::Value = serde_json::from_slice(&immune_memory_body)
            .expect("immune memory payload should be json");
        assert!(immune_memory_payload["data"]
            .as_array()
            .is_some_and(|items| !items.is_empty()));
    }

    #[actix_rt::test]
    async fn chain_export_audit_reports_bundle_health() {
        let state = web::Data::new(Arc::new(RwLock::new(test_state())));
        let response = chain_export_audit(state)
            .await
            .expect("chain export audit should succeed");

        assert_eq!(response.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(response.into_body())
            .await
            .expect("response body should be readable");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["success"], serde_json::Value::Bool(true));
        assert_eq!(
            payload["data"]["report"]["validation_passed"],
            serde_json::Value::Bool(true)
        );
        assert!(payload["data"]["report"]["bundle_hash"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }
}

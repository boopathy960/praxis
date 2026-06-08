// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
// API â€” Request/Response Models
// â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agents::base::{AgentCapabilityProfile, MissionAssignmentPlan};
use crate::assistant::autonomy::{
    AssistantAutonomySnapshot, CustomAssistantAgent, CustomAssistantTool, LifecycleMode,
    SelfImprovementProposal,
};
pub use crate::assistant::executive::{
    AcquisitionPolicy, AcquisitionPolicyRegistrationRequest, ApprovalAction,
    ApprovalCollectionResponse, ApprovalDecisionRequest, ApprovalStatus, ApprovalTicket,
    AssistantAuditRecord, AssistantRole, AuditCollectionResponse, ConnectorCollectionResponse,
    ConnectorRegistrationRequest, ConnectorSpec, CredentialRef, EnterpriseHealthCheck,
    EnterpriseHealthReport, EvidenceBundle, EvidenceCitation, ExecutionReceipt, Mission,
    MissionCollectionResponse, MissionEvent, MissionEventLogResponse, MissionExecutionRequest,
    MissionMode, MissionRiskDecision, MissionRiskLevel, MissionStatus, MissionStep,
    MissionStepStatus, ResearchPolicyCollectionResponse, RevenueOpportunity,
};
pub use crate::assistant::fleet::{
    AcquisitionSourceScore, AgentManifest, ArmyCampaign, BattalionAssignment,
    ChainAttestationCollectionResponse, ChainAttestationReceipt, FleetAssetKind,
    FleetCampaignRequest, FleetEvaluationRequest, FleetPolicy, FleetPolicyCollectionResponse,
    FleetPolicyRequest, FleetSnapshotResponse, HttpaExecutionPolicy,
    HttpaExecutionPolicyCollectionResponse, HttpaExecutionPolicyRequest, HttpaMissionEnvelope,
    OnionSourcePolicy, OnionSourcePolicyCollectionResponse, OnionSourcePolicyRequest,
    PeerReviewCollectionResponse, PeerReviewScore, PerformanceHistoryPoint, PromotionAction,
    PromotionDecision, PromotionGate, PromotionStatus, RankedLeagueCollectionResponse,
    RankedLeagueEntry, RollbackReceipt, SquadAssignment, ToolManifest,
};
pub use crate::assistant::local_operator::{
    AppLaunchSpec, BinaryTrustReport, DownloadSource, DownloadSourceResolutionResponse,
    InstallReceipt, InstallReceiptCollectionResponse, LabEnvironment,
    LabEnvironmentCollectionResponse, LabSessionRequest, LocalActionKind, LocalAppCatalogResponse,
    LocalApplication, LocalOpenRequest, LocalOperationRequest, LocalResourceRef, OperatorMode,
    OverrideSession, OverrideSessionCollectionResponse, OverrideSessionRequest, OwnedTarget,
    OwnedTargetCollectionResponse, OwnedTargetRegistrationRequest, SampleAnalysisReport,
    SampleSubmissionRequest, SecurityCaseBundle, SecurityCaseCollectionResponse,
    SoftwareInstallRequest, SoftwareSourceResolutionRequest, SoftwareStageRequest, StagedArtifact,
    StagedArtifactCollectionResponse,
};
pub use crate::assistant::one_brain::{
    BrainExecuteRequest, BrainExecuteResponse, BrainModeHint, BrainPlanSummary, BrainSessionState,
    BrainStage, BrainVerificationSummary,
};
use crate::assistant::runtime::PersonalAssistantCapabilitySnapshot;
use crate::chain::accelerator::ChainAccelerationReport;
use crate::chain::ai_contract::ContractVmInstruction;
use crate::chain::block::Block;
use crate::chain::chain::{
    ChainExportAuditReport, ChainInfo, ChainIntegrityReport, LedgerAuditRecord, ResourceCommitment,
    ValidatorBrowsingScore,
};
use crate::chain::evolution::ChainEvolutionFinding;
use crate::chain::value_protocol::GuardedPaymentRequest;
use crate::features::adaptive_search::{AdaptiveSearchStats, SearchResponseMode};
use crate::tools::registry::{ToolDescriptor, ToolRegistrySnapshot};
use crate::tools::search_index::{SearchAcquisitionRefreshPlan, SearchAcquisitionStats};
use crate::tools::web_search::{
    SearchAcquisitionRefreshSweepResult, SearchAutonomousFrontier, SearchAutonomousTickReport,
};
use crate::voice::{VoiceRuntimeStatus, VoiceSpeakReceipt, VoiceTranscription, VoiceWarmupReceipt};

// â”€â”€ Memory â”€â”€
#[derive(Debug, Deserialize)]
pub struct MemoryCommitRequest {
    pub url: String,
    pub intent: String,
    pub context: String,
}

#[derive(Debug, Deserialize)]
pub struct MemorySearchRequest {
    pub query: String,
}

// â”€â”€ Swarm â”€â”€
#[derive(Debug, Deserialize)]
pub struct SwarmQueryRequest {
    pub intent: String,
    pub num_agents: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub min_confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct AdaptiveSearchRequest {
    pub query: String,
    pub preferred_depth: Option<String>,
    pub max_sources: Option<usize>,
    pub include_social: Option<bool>,
    pub include_news: Option<bool>,
    pub include_books: Option<bool>,
    pub include_papers: Option<bool>,
    pub include_docs: Option<bool>,
    pub response_mode: Option<SearchResponseMode>,
    pub freshness_horizon_hours: Option<u64>,
    pub domain_focus: Option<Vec<String>>,
    pub require_citations: Option<bool>,
    pub max_contradictions: Option<usize>,
    pub notarize: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SearchIntelligenceRefreshRequest {
    pub query: String,
    pub preferred_depth: Option<String>,
    pub max_sources: Option<usize>,
    pub include_social: Option<bool>,
    pub include_news: Option<bool>,
    pub include_books: Option<bool>,
    pub include_papers: Option<bool>,
    pub include_docs: Option<bool>,
    pub freshness_horizon_hours: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchIntelligenceStatsResponse {
    pub adaptive: AdaptiveSearchStats,
    pub acquisition: SearchAcquisitionStats,
    pub refresh_plan: SearchAcquisitionRefreshPlan,
    pub autonomous: SearchAutonomousFrontier,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchIntelligenceRefreshResponse {
    pub query: String,
    pub search_results_ingested: usize,
    pub research_sources_ingested: usize,
    pub indexed_documents_total: usize,
    pub indexed_documents_delta: usize,
    pub refreshed_lanes: Vec<String>,
    pub source_mix: BTreeMap<String, usize>,
    pub acquisition: SearchAcquisitionStats,
}

#[derive(Debug, Deserialize)]
pub struct SearchIntelligenceRefreshSweepRequest {
    pub max_tasks: Option<usize>,
    pub max_results_per_task: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchIntelligenceRefreshSweepResponse {
    pub sweep: SearchAcquisitionRefreshSweepResult,
}

#[derive(Debug, Deserialize)]
pub struct SearchIntelligenceCrawlTickRequest {
    pub force: Option<bool>,
    pub max_profiles: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchIntelligenceCrawlTickResponse {
    pub autonomous: SearchAutonomousTickReport,
}

#[derive(Debug, Deserialize)]
pub struct MissionAssignmentRequest {
    pub task: String,
    pub current_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeCapabilitiesResponse {
    pub generated_at: String,
    pub agents: Vec<AgentCapabilityProfile>,
    pub tools: ToolRegistrySnapshot,
    pub assistant: PersonalAssistantCapabilitySnapshot,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MissionAssignmentResponse {
    pub mission: MissionAssignmentPlan,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantRuntimeProfileResponse {
    pub generated_at: String,
    pub assistant: PersonalAssistantCapabilitySnapshot,
}

#[derive(Debug, Deserialize)]
pub struct AssistantCustomAgentRequest {
    pub agent_id: String,
    pub display_name: String,
    pub purpose: String,
    pub capabilities: Vec<String>,
    pub preferred_tools: Vec<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub lifecycle: Option<LifecycleMode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantCustomAgentResponse {
    pub agent: CustomAssistantAgent,
    pub manifest: AgentManifest,
}

#[derive(Debug, Deserialize)]
pub struct AssistantCustomToolRequest {
    pub name: String,
    pub description: String,
    pub category: String,
    pub risk_level: String,
    pub requires_approval: bool,
    pub max_calls_per_minute: Option<u32>,
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub lifecycle: Option<LifecycleMode>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantCustomToolResponse {
    pub tool: CustomAssistantTool,
    pub descriptor: ToolDescriptor,
    pub manifest: ToolManifest,
}

#[derive(Debug, Deserialize)]
pub struct AssistantSelfImprovementProposalRequest {
    pub objective: String,
    pub scope: String,
    pub implementation_outline: Vec<String>,
    pub safeguards: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantSelfImprovementProposalResponse {
    pub proposal: SelfImprovementProposal,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantAutonomySnapshotResponse {
    pub generated_at: String,
    pub autonomy: AssistantAutonomySnapshot,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantVoiceStatusResponse {
    pub generated_at: String,
    pub assistant_codename: String,
    pub invocation_required: bool,
    pub wake_words: Vec<String>,
    pub voice: VoiceRuntimeStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantVoiceWarmupResponse {
    pub generated_at: String,
    pub warmup: VoiceWarmupReceipt,
}

#[derive(Debug, Deserialize)]
pub struct AssistantVoiceTranscribeRequest {
    pub audio_path: String,
    pub language_hint: Option<String>,
    pub initial_prompt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantVoiceTranscriptionResponse {
    pub generated_at: String,
    pub transcription: VoiceTranscription,
}

#[derive(Debug, Deserialize)]
pub struct AssistantVoiceListenRequest {
    pub duration_seconds: Option<u64>,
    pub language_hint: Option<String>,
    pub initial_prompt: Option<String>,
    pub context: Option<serde_json::Value>,
    pub require_invocation: Option<bool>,
    pub wake_words: Option<Vec<String>>,
    pub respond: Option<bool>,
    pub auto_speak: Option<bool>,
    pub keep_audio: Option<bool>,
    pub speech_style: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantVoiceSpeakRequest {
    pub text: String,
    pub voice_hint: Option<String>,
    pub rate: Option<i32>,
    pub volume: Option<u8>,
    pub speech_style: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantVoiceSpeakResponse {
    pub generated_at: String,
    pub spoken_text: String,
    pub speech: VoiceSpeakReceipt,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AssistantVoiceTurnResponse {
    pub generated_at: String,
    pub heard_text: String,
    pub command_text: Option<String>,
    pub activated: bool,
    pub invocation_required: bool,
    pub matched_wake_word: Option<String>,
    pub transcription: VoiceTranscription,
    pub response_text: Option<String>,
    pub speech_text: Option<String>,
    pub route: Option<String>,
    pub agreement_score: Option<f64>,
    pub verification_passed: Option<bool>,
    pub orchestration: serde_json::Value,
    pub speech: Option<VoiceSpeakReceipt>,
}

// â”€â”€ MEV Shield â”€â”€
#[derive(Debug, Deserialize)]
pub struct ShieldIntentRequest {
    pub url: String,
    pub action: String,
    pub max_price: f64,
    pub item_id: String,
}

// â”€â”€ Truth â”€â”€
#[derive(Debug, Deserialize)]
pub struct VerifyContentRequest {
    pub url: String,
    pub content: String,
}

// â”€â”€ Identity â”€â”€
#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    pub challenge: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentityChallengeRequest {
    pub actor: Option<String>,
    pub namespace: Option<String>,
    pub purpose: Option<String>,
    pub session_ttl_seconds: Option<u64>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityChallengeResponse {
    pub challenge_id: String,
    pub actor: String,
    pub namespace: String,
    pub purpose: String,
    pub chain_id: String,
    pub operation: String,
    pub challenge: String,
    pub payload: serde_json::Value,
    pub canonical_message: String,
    pub canonical_message_hash: String,
    pub expires_at: i64,
    pub local_identity_can_sign: bool,
    pub local_public_key_hex: Option<String>,
    pub local_public_key_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentitySessionAuthRequest {
    pub actor: String,
    pub challenge_id: String,
    pub public_key_hex: String,
    pub signature_hex: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentitySessionAuthResponse {
    pub authenticated: bool,
    pub actor: String,
    pub challenge_id: String,
    pub session_token: String,
    pub chain_id: String,
    pub purpose: String,
    pub roles: Vec<String>,
    pub public_key_hex: String,
    pub public_key_hash: String,
    pub algorithm: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubmitGuardedPaymentRequest {
    #[serde(flatten)]
    pub payment: GuardedPaymentRequest,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

// â”€â”€ Compute â”€â”€
#[derive(Debug, Deserialize)]
pub struct ComputeSubmitRequest {
    pub task_type: String,
    pub description: String,
    pub input_data: serde_json::Value,
    pub max_cost: Option<f64>,
    pub timeout_ms: Option<u64>,
}

// â”€â”€ Render â”€â”€
// Ã¢â€â‚¬Ã¢â€â‚¬ Chain Ã¢â€â‚¬Ã¢â€â‚¬
#[derive(Debug, Clone, Serialize)]
pub struct ChainBlockSummary {
    pub height: u64,
    pub block_hash: String,
    pub prev_hash: String,
    pub validator: String,
    pub transaction_count: usize,
    pub timestamp: f64,
    pub certified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainOverviewResponse {
    pub info: ChainInfo,
    pub acceleration: ChainAccelerationReport,
    pub recent_blocks: Vec<ChainBlockSummary>,
    pub recent_resources: Vec<ResourceCommitment>,
    pub recent_audit: Vec<LedgerAuditRecord>,
    pub validator_scores: Vec<ValidatorBrowsingScore>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainIntegrityResponse {
    pub report: ChainIntegrityReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainExportAuditResponse {
    pub report: ChainExportAuditReport,
}

#[derive(Debug, Deserialize)]
pub struct RegisterValidatorRequest {
    pub address: String,
    pub display_name: Option<String>,
    pub trust_score: Option<f64>,
    pub public_key_hex: Option<String>,
    pub public_key_hash: Option<String>,
    pub signature_hex: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub stake: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitChainTransactionRequest {
    pub sender: String,
    pub receiver: String,
    pub amount: f64,
    pub data: serde_json::Value,
    pub intent: Option<String>,
    pub nonce: Option<u64>,
    pub gas_limit: Option<u64>,
    pub fee: Option<f64>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MineBlockRequest {
    pub validator: String,
    pub max_txs: Option<usize>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeployAiContractRequest {
    pub owner: String,
    pub name: String,
    pub purpose: String,
    pub allowed_domains: Vec<String>,
    pub allowed_callers: Option<Vec<String>>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_budget: f64,
    pub max_compute_units: u64,
    pub min_confidence: f64,
    pub max_risk: f64,
    pub review_threshold: Option<f64>,
    pub minimum_evidence: Option<usize>,
    pub require_trace_binding: Option<bool>,
    pub prompt_template: Option<String>,
    pub invariants: Option<Vec<String>>,
    pub vm_program: Option<Vec<ContractVmInstruction>>,
    pub metadata: Option<serde_json::Value>,
    pub active: Option<bool>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InvokeAiContractRequest {
    pub contract_id: String,
    pub caller: String,
    pub action: String,
    pub domain: String,
    pub requested_tools: Option<Vec<String>>,
    pub estimated_cost: Option<f64>,
    pub confidence: Option<f64>,
    pub risk_score: Option<f64>,
    pub input_digest: Option<String>,
    pub trace_id: Option<String>,
    pub session_id: Option<String>,
    pub evidence: Option<Vec<String>>,
    pub context: Option<BTreeMap<String, String>>,
    pub metadata: Option<serde_json::Value>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NotarizeResourceRequest {
    pub url: String,
    pub content_hash: String,
    pub content_type: String,
    pub source: String,
    pub attestor: String,
    pub metadata: Option<serde_json::Value>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterChainPeerRequest {
    pub operator: String,
    pub peer_id: String,
    pub base_url: String,
    pub public_key_hash: Option<String>,
    pub trust_score: Option<f64>,
    pub replication_mode: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SyncChainPeerRequest {
    pub operator: String,
    pub peer_id: String,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RequestPeerHandshakeChallenge {
    pub peer_id: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyPeerHandshakeRequest {
    pub peer_id: String,
    pub challenge_id: String,
    pub public_key_hex: String,
    pub signature_hex: String,
}

#[derive(Debug, Deserialize)]
pub struct ReplicateChainBlockRequest {
    pub source_peer_id: Option<String>,
    pub source_public_key_hex: Option<String>,
    pub source_signature_hex: Option<String>,
    pub block: Block,
}

#[derive(Debug, Deserialize)]
pub struct ApplyChainEvolutionRequest {
    pub operator: String,
    pub rationale: String,
    pub use_recommended: Option<bool>,
    pub max_block_transactions: Option<usize>,
    pub autocommit_batch_size: Option<usize>,
    pub min_peer_trust: Option<f64>,
    pub replication_quorum_floor: Option<usize>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterChainEvolutionAgentRequest {
    pub operator: String,
    pub agent_id: String,
    pub display_name: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub minimum_confidence: Option<f64>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitChainEvolutionReportRequest {
    pub agent_id: String,
    pub role: String,
    pub confidence: f64,
    pub findings: Vec<ChainEvolutionFinding>,
    pub max_block_transactions: Option<usize>,
    pub autocommit_batch_size: Option<usize>,
    pub min_peer_trust: Option<f64>,
    pub replication_quorum_floor: Option<usize>,
    pub context: Option<serde_json::Value>,
    pub public_key_hex: Option<String>,
    pub signature_hex: Option<String>,
}

// â”€â”€ Generic â”€â”€
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: T,
    pub engine: String,
    pub trace_id: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data,
            engine: "astra_core_engine/1.0".into(),
            trace_id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
        }
    }
}

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};

const DEFAULT_TIME_HORIZONS_MINUTES: &[u64] = &[5, 60, 1_440, 10_080];
const DEFAULT_SCATTER_WINDOW_MS: u64 = 90_000;
const DEFAULT_MIX_BATCH_SIZE: usize = 24;
const DEFAULT_TOTAL_COMPUTE_UNITS: u64 = 1_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum StructuralGene {
    System,
    FeedbackLoop,
    Cascade,
    Threshold,
    Competition,
    Cooperation,
    Optimization,
    Constraint,
    Transformation,
    Hierarchy,
    Emergence,
    Oscillation,
    Saturation,
    Bifurcation,
    Conservation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveDecisionInput {
    pub content: String,
    #[serde(default = "default_decision_score")]
    pub immediate_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolvedPatternInput {
    pub label: String,
    pub original_domain: String,
    pub solution_template: String,
    #[serde(default)]
    pub structural_dna: Vec<StructuralGene>,
    #[serde(default = "default_transfer_success_rate")]
    pub transfer_success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictoryClaimInput {
    pub claim_a: String,
    #[serde(default = "default_claim_score")]
    pub score_a: f64,
    pub claim_b: String,
    #[serde(default = "default_claim_score")]
    pub score_b: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionTaskInput {
    pub task_id: String,
    pub description: String,
    #[serde(default = "default_task_weight")]
    pub urgency: f64,
    #[serde(default = "default_task_weight")]
    pub importance: f64,
    #[serde(default = "default_decay_rate")]
    pub decay_rate: f64,
    #[serde(default = "default_user_proximity")]
    pub user_proximity: f64,
    #[serde(default = "default_attention_floor")]
    pub floor: f64,
    #[serde(default)]
    pub cycles_starved: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorSignalInput {
    pub predicted_intent: String,
    #[serde(default = "default_behavior_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub typical_hour: Option<u8>,
    #[serde(default)]
    pub context_chain: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptClusterInput {
    pub label: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub adjacent_concepts: Vec<String>,
    #[serde(default)]
    pub last_activated_days_ago: u32,
    #[serde(default = "default_activation")]
    pub activation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltimateCognitionRequest {
    pub problem: String,
    #[serde(default)]
    pub candidate_decisions: Vec<CognitiveDecisionInput>,
    #[serde(default)]
    pub solved_patterns: Vec<SolvedPatternInput>,
    #[serde(default)]
    pub contradictory_claims: Vec<ContradictoryClaimInput>,
    #[serde(default)]
    pub active_tasks: Vec<AttentionTaskInput>,
    #[serde(default)]
    pub behavior_signals: Vec<BehaviorSignalInput>,
    #[serde(default)]
    pub concept_clusters: Vec<ConceptClusterInput>,
    #[serde(default = "default_idle_seconds")]
    pub idle_seconds: u64,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    #[serde(default)]
    pub time_horizons_minutes: Vec<u64>,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquisitionTargetInput {
    pub url: String,
    #[serde(default)]
    pub semantic_goal: String,
    #[serde(default)]
    pub desired_geo: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyCandidateInput {
    pub proxy_id: String,
    pub address: String,
    #[serde(default)]
    pub geo_location: String,
    #[serde(default = "default_latency_ms")]
    pub latency_ms: u32,
    #[serde(default = "default_reputation")]
    pub ip_reputation: f64,
    #[serde(default)]
    pub domain_scores: BTreeMap<String, f64>,
    #[serde(default)]
    pub rate_limited_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltimateAcquisitionRequest {
    #[serde(default)]
    pub targets: Vec<AcquisitionTargetInput>,
    #[serde(default)]
    pub proxies: Vec<ProxyCandidateInput>,
    #[serde(default)]
    pub cover_domains: Vec<String>,
    #[serde(default = "default_scatter_window_ms")]
    pub scatter_window_ms: u64,
    #[serde(default = "default_mix_batch_size")]
    pub min_mix_batch: usize,
    #[serde(default)]
    pub allowed_profile_families: Vec<String>,
    #[serde(default = "default_true")]
    pub respect_robots: bool,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEventInput {
    pub title: String,
    pub minutes_until: i64,
    #[serde(default = "default_task_weight")]
    pub importance: f64,
    #[serde(default)]
    pub topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSignalInput {
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
    #[serde(default = "default_direction")]
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalCellInput {
    pub description: String,
    #[serde(default = "default_cell_status")]
    pub status: String,
    #[serde(default = "default_task_weight")]
    pub health_score: f64,
    #[serde(default = "default_complexity")]
    pub complexity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleTaskInput {
    pub task_id: String,
    pub title: String,
    #[serde(default = "default_duration_minutes")]
    pub duration_minutes: u32,
    #[serde(default = "default_task_weight")]
    pub energy_required: f64,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub deadline_minutes: Option<u64>,
    #[serde(default)]
    pub quick: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyWindowInput {
    pub start_hour: u8,
    pub end_hour: u8,
    #[serde(default = "default_energy_score")]
    pub energy_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltimateAssistantRequest {
    pub text: String,
    #[serde(default)]
    pub typing_speed_wpm: f64,
    #[serde(default)]
    pub deletions: u32,
    #[serde(default = "default_total_keystrokes")]
    pub total_keystrokes: u32,
    #[serde(default)]
    pub has_code_open: bool,
    #[serde(default)]
    pub recent_errors: u32,
    #[serde(default)]
    pub is_week_start: bool,
    #[serde(default)]
    pub is_morning: bool,
    #[serde(default)]
    pub open_files: Vec<String>,
    #[serde(default)]
    pub clipboard_excerpt: Option<String>,
    #[serde(default)]
    pub events: Vec<CalendarEventInput>,
    #[serde(default)]
    pub metrics: Vec<MetricSignalInput>,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub seed_cells: Vec<GoalCellInput>,
    #[serde(default)]
    pub schedule_tasks: Vec<ScheduleTaskInput>,
    #[serde(default)]
    pub energy_windows: Vec<EnergyWindowInput>,
    #[serde(default = "default_current_hour")]
    pub current_hour: u8,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDocumentInput {
    pub title: String,
    pub domain: String,
    pub content: String,
    #[serde(default = "default_authority")]
    pub authority_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontierClaimInput {
    pub theory: String,
    #[serde(default = "default_claim_score")]
    pub support_score: f64,
    #[serde(default = "default_source_count")]
    pub source_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossDomainMatchInput {
    pub label: String,
    pub source_domain: String,
    #[serde(default = "default_similarity")]
    pub structural_similarity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltimateKnowledgeRequest {
    pub domain: String,
    pub question: String,
    #[serde(default)]
    pub sources: Vec<SourceDocumentInput>,
    #[serde(default)]
    pub live_feeds: Vec<String>,
    #[serde(default)]
    pub competing_claims: Vec<FrontierClaimInput>,
    #[serde(default)]
    pub synthesis_problem: String,
    #[serde(default)]
    pub cross_domain_matches: Vec<CrossDomainMatchInput>,
    #[serde(default)]
    pub recency_events: Vec<String>,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemporalProjectionReport {
    pub horizon_minutes: u64,
    pub projected_score: f64,
    pub risk_factors: Vec<String>,
    pub cascade_effects: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemporalReasoningFabricReport {
    pub selected_decision: String,
    pub temporal_coherence: f64,
    pub weighted_temporal_score: f64,
    pub projections: Vec<TemporalProjectionReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdversarialAttackReport {
    pub inversion: String,
    pub attack_strength: f64,
    pub defended: bool,
    pub defense_summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdversarialImaginationReport {
    pub resilience_score: f64,
    pub recommendation: String,
    pub attacks: Vec<AdversarialAttackReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepthNodeSummary {
    pub problem: String,
    pub depth: usize,
    pub is_atomic: bool,
    pub child_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CognitiveDepthChargesReport {
    pub max_depth_reached: usize,
    pub atomic_leaf_count: usize,
    pub bottom_up_solution: String,
    pub nodes: Vec<DepthNodeSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalogicalGenomeTransferReport {
    pub original_domain: String,
    pub matched_label: String,
    pub structural_similarity: f64,
    pub transferred_genes: Vec<StructuralGene>,
    pub adapted_solution: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrystallizedIntentReport {
    pub predicted_intent: String,
    pub confidence: f64,
    pub preparation_actions: Vec<String>,
    pub predicted_hour: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct PredictiveIntentCrystallizationReport {
    pub behavioral_dna_score: f64,
    pub crystallized_intents: Vec<CrystallizedIntentReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DreamStateConsolidationReport {
    pub compressed_concepts: Vec<String>,
    pub new_heuristics: Vec<String>,
    pub healed_links: Vec<String>,
    pub pruned_concepts: Vec<String>,
    pub mutated_parameters: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttentionAllocationEntry {
    pub task_id: String,
    pub allocation_units: u64,
    pub effective_priority: f64,
    pub starvation_bonus: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FractalAttentionAllocationReport {
    pub total_compute_units: u64,
    pub allocations: Vec<AttentionAllocationEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResonanceDiscoveryReport {
    pub contradiction_axis: String,
    pub deeper_truth: String,
    pub novelty_score: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UltimateCognitionReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub temporal_reasoning_fabric: TemporalReasoningFabricReport,
    pub adversarial_imagination_engine: AdversarialImaginationReport,
    pub cognitive_depth_charges: CognitiveDepthChargesReport,
    pub analogical_genome_transfer: Option<AnalogicalGenomeTransferReport>,
    pub predictive_intent_crystallization: PredictiveIntentCrystallizationReport,
    pub dream_state_consolidation: DreamStateConsolidationReport,
    pub fractal_attention_allocation: FractalAttentionAllocationReport,
    pub contradiction_resonance_mining: Vec<ResonanceDiscoveryReport>,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtocolChameleonConnectionPlan {
    pub url: String,
    pub profile_name: String,
    pub profile_family: String,
    pub ip_ttl: u8,
    pub alpn_protocols: Vec<String>,
    pub header_order: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtocolChameleonEngineReport {
    pub connection_plans: Vec<ProtocolChameleonConnectionPlan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemporalScatterEntry {
    pub url: String,
    pub delay_ms: u64,
    pub browser_profile_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemporalRequestScatteringReport {
    pub scheduled_requests: Vec<TemporalScatterEntry>,
    pub mean_delay_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyRouteDecision {
    pub url: String,
    pub proxy_id: String,
    pub score: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticProxyMeshReport {
    pub selections: Vec<ProxyRouteDecision>,
    pub learned_domain_scores: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsPhantomResolutionEntry {
    pub target_domain: String,
    pub cover_queries: Vec<String>,
    pub inserted_position: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsPhantomResolutionReport {
    pub resolutions: Vec<DnsPhantomResolutionEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TlsFingerprintNebulaReport {
    pub base_family: String,
    pub profile_id: String,
    pub session_id_length: u8,
    pub mutated_cipher_count: usize,
    pub uniqueness_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestOriginErasureReport {
    pub mix_hops: usize,
    pub batch_size: usize,
    pub unlinkability_budget: f64,
    pub routing_plan: Vec<String>,
    pub compliance_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UltimateAcquisitionReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub protocol_chameleon_engine: ProtocolChameleonEngineReport,
    pub temporal_request_scattering: TemporalRequestScatteringReport,
    pub semantic_proxy_mesh: SemanticProxyMeshReport,
    pub dns_phantom_resolution: DnsPhantomResolutionReport,
    pub tls_fingerprint_nebula: TlsFingerprintNebulaReport,
    pub request_origin_erasure: RequestOriginErasureReport,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientIntentInferenceReport {
    pub explicit_intent: String,
    pub inferred_needs: Vec<String>,
    pub urgency: f64,
    pub response_style: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrchestrationActionReport {
    pub trigger: String,
    pub action: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProactiveEnvironmentOrchestrationReport {
    pub readiness_score: f64,
    pub actions: Vec<OrchestrationActionReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskCellReport {
    pub description: String,
    pub status: String,
    pub health: f64,
    pub mutation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskSpeciationReport {
    pub goal: String,
    pub cells: Vec<TaskCellReport>,
    pub adaptation_summary: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleBlockReport {
    pub start_hour: u8,
    pub duration_minutes: u32,
    pub title: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SovereignScheduleReport {
    pub focus_minutes: u32,
    pub optimized_blocks: Vec<ScheduleBlockReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UltimateAssistantReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub ambient_intent_inference: AmbientIntentInferenceReport,
    pub proactive_environment_orchestration: ProactiveEnvironmentOrchestrationReport,
    pub task_speciation: TaskSpeciationReport,
    pub sovereign_schedule_ai: SovereignScheduleReport,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledRuleReport {
    pub statement: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UniversalDomainCompilerReport {
    pub domain: String,
    pub ontology: Vec<String>,
    pub rules: Vec<CompiledRuleReport>,
    pub expert_patterns: Vec<String>,
    pub vocabulary: Vec<String>,
    pub common_mistakes: Vec<String>,
    pub authoritative_sources: Vec<String>,
    pub compilation_confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveKnowledgeMetabolismReport {
    pub ingested_feeds: Vec<String>,
    pub digested_claims: Vec<String>,
    pub absorbed_concepts: Vec<String>,
    pub proactive_alerts: Vec<String>,
    pub pruned_items: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EpistemicFrontierDetectionReport {
    pub location: String,
    pub dominant_theory: Option<String>,
    pub alternative_theories: Vec<String>,
    pub consensus_level: f64,
    pub key_experiments_needed: Vec<String>,
    pub why_unknown: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NovelSolutionReport {
    pub approach: String,
    pub source_domain: String,
    pub structural_similarity: f64,
    pub novelty_score: f64,
    pub feasibility_estimate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UltimateKnowledgeReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub universal_domain_compiler: UniversalDomainCompilerReport,
    pub live_knowledge_metabolism: LiveKnowledgeMetabolismReport,
    pub epistemic_frontier_detection: EpistemicFrontierDetectionReport,
    pub synthesis_forge: Vec<NovelSolutionReport>,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UltimateAstraFeaturesStatus {
    pub total_reports: u64,
    pub cognition_reports: u64,
    pub acquisition_reports: u64,
    pub assistant_reports: u64,
    pub knowledge_reports: u64,
    pub last_reported_at: i64,
    pub learned_patterns: usize,
    pub remembered_domains: usize,
}

pub struct UltimateAstraFeaturesEngine {
    total_reports: u64,
    cognition_reports: u64,
    acquisition_reports: u64,
    assistant_reports: u64,
    knowledge_reports: u64,
    last_reported_at: i64,
    learned_patterns: BTreeMap<String, Vec<StructuralGene>>,
    remembered_domains: BTreeSet<String>,
    recent_predictions: VecDeque<String>,
}

impl UltimateAstraFeaturesEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            total_reports: 0,
            cognition_reports: 0,
            acquisition_reports: 0,
            assistant_reports: 0,
            knowledge_reports: 0,
            last_reported_at: 0,
            learned_patterns: BTreeMap::new(),
            remembered_domains: BTreeSet::new(),
            recent_predictions: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn status(&self) -> UltimateAstraFeaturesStatus {
        UltimateAstraFeaturesStatus {
            total_reports: self.total_reports,
            cognition_reports: self.cognition_reports,
            acquisition_reports: self.acquisition_reports,
            assistant_reports: self.assistant_reports,
            knowledge_reports: self.knowledge_reports,
            last_reported_at: self.last_reported_at,
            learned_patterns: self.learned_patterns.len(),
            remembered_domains: self.remembered_domains.len(),
        }
    }

    pub fn orchestrate_cognition(
        &mut self,
        request: UltimateCognitionRequest,
    ) -> AstraResult<UltimateCognitionReport> {
        validate_cognition_request(&request)?;
        let analyzed_at = now_ms();
        let time_horizons = normalized_time_horizons(&request.time_horizons_minutes);
        let temporal_reasoning_fabric = build_temporal_reasoning(
            &request.problem,
            &request.candidate_decisions,
            &time_horizons,
        );
        let adversarial_imagination_engine = build_adversarial_imagination(
            &request.problem,
            &temporal_reasoning_fabric.selected_decision,
        );
        let cognitive_depth_charges =
            build_depth_charges(&request.problem, request.max_depth.min(10).max(2));
        let analogical_genome_transfer =
            build_analogical_transfer(&request.problem, &request.solved_patterns);
        if let Some(transfer) = &analogical_genome_transfer {
            self.learned_patterns.insert(
                transfer.matched_label.clone(),
                transfer.transferred_genes.clone(),
            );
        }
        let predictive_intent_crystallization = build_predictive_intents(
            &request.behavior_signals,
            &request.problem,
            &mut self.recent_predictions,
        );
        let dream_state_consolidation =
            build_dream_state(&request.concept_clusters, request.idle_seconds);
        let fractal_attention_allocation = build_attention_allocations(&request.active_tasks);
        let contradiction_resonance_mining =
            build_resonance_discoveries(&request.contradictory_claims);

        let report_id =
            hash_prefix(format!("ultimate:cognition:{}:{analyzed_at}", request.problem).as_bytes());
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "selected_decision": temporal_reasoning_fabric.selected_decision,
                "resilience": adversarial_imagination_engine.resilience_score,
                "atomic_leaf_count": cognitive_depth_charges.atomic_leaf_count,
                "prediction_count": predictive_intent_crystallization.crystallized_intents.len(),
            })
            .to_string()
            .as_bytes(),
        );

        self.record_report("cognition", analyzed_at);
        Ok(UltimateCognitionReport {
            report_id,
            manifest_hash,
            temporal_reasoning_fabric,
            adversarial_imagination_engine,
            cognitive_depth_charges,
            analogical_genome_transfer,
            predictive_intent_crystallization,
            dream_state_consolidation,
            fractal_attention_allocation,
            contradiction_resonance_mining,
            analyzed_at,
        })
    }

    pub fn orchestrate_acquisition(
        &mut self,
        request: UltimateAcquisitionRequest,
    ) -> AstraResult<UltimateAcquisitionReport> {
        validate_acquisition_request(&request)?;
        let analyzed_at = now_ms();
        let protocol_chameleon_engine =
            build_protocol_chameleon(&request.targets, &request.allowed_profile_families);
        let temporal_request_scattering =
            build_temporal_scattering(&request.targets, request.scatter_window_ms);
        let semantic_proxy_mesh = build_semantic_proxy_mesh(&request.targets, &request.proxies);
        let dns_phantom_resolution =
            build_dns_phantom_resolution(&request.targets, &request.cover_domains);
        let tls_fingerprint_nebula = build_tls_nebula(&protocol_chameleon_engine.connection_plans);
        let request_origin_erasure =
            build_origin_erasure(request.min_mix_batch.max(4), request.respect_robots);

        let report_id = hash_prefix(
            format!(
                "ultimate:acquisition:{}:{analyzed_at}",
                request.targets.len()
            )
            .as_bytes(),
        );
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "target_count": request.targets.len(),
                "scatter_count": temporal_request_scattering.scheduled_requests.len(),
                "proxy_routes": semantic_proxy_mesh.selections.len(),
                "mix_batch": request_origin_erasure.batch_size,
            })
            .to_string()
            .as_bytes(),
        );

        self.record_report("acquisition", analyzed_at);
        Ok(UltimateAcquisitionReport {
            report_id,
            manifest_hash,
            protocol_chameleon_engine,
            temporal_request_scattering,
            semantic_proxy_mesh,
            dns_phantom_resolution,
            tls_fingerprint_nebula,
            request_origin_erasure,
            analyzed_at,
        })
    }

    pub fn orchestrate_assistant(
        &mut self,
        request: UltimateAssistantRequest,
    ) -> AstraResult<UltimateAssistantReport> {
        if request.text.trim().is_empty() && request.goal.trim().is_empty() {
            return Err(AstraError::ControlPlaneRejected(
                "ultimate assistant orchestration requires user text or a goal".into(),
            ));
        }
        let analyzed_at = now_ms();
        let ambient_intent_inference = build_ambient_intent(&request);
        let proactive_environment_orchestration = build_proactive_orchestration(&request);
        let task_speciation = build_task_speciation(&request.goal, &request.seed_cells);
        let sovereign_schedule_ai =
            build_sovereign_schedule(&request.schedule_tasks, &request.energy_windows);

        let report_id =
            hash_prefix(format!("ultimate:assistant:{}:{analyzed_at}", request.text).as_bytes());
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "response_style": ambient_intent_inference.response_style,
                "orchestration_actions": proactive_environment_orchestration.actions.len(),
                "cells": task_speciation.cells.len(),
                "blocks": sovereign_schedule_ai.optimized_blocks.len(),
            })
            .to_string()
            .as_bytes(),
        );

        self.record_report("assistant", analyzed_at);
        Ok(UltimateAssistantReport {
            report_id,
            manifest_hash,
            ambient_intent_inference,
            proactive_environment_orchestration,
            task_speciation,
            sovereign_schedule_ai,
            analyzed_at,
        })
    }

    pub fn orchestrate_knowledge(
        &mut self,
        request: UltimateKnowledgeRequest,
    ) -> AstraResult<UltimateKnowledgeReport> {
        if request.domain.trim().is_empty() || request.question.trim().is_empty() {
            return Err(AstraError::ControlPlaneRejected(
                "ultimate knowledge orchestration requires domain and question".into(),
            ));
        }
        let analyzed_at = now_ms();
        let universal_domain_compiler = build_universal_domain_compiler(&request);
        let live_knowledge_metabolism = build_knowledge_metabolism(&request);
        let epistemic_frontier_detection = build_epistemic_frontier(&request);
        let synthesis_forge = build_synthesis_forge(&request);

        self.remembered_domains.insert(request.domain.clone());
        let report_id = hash_prefix(
            format!(
                "ultimate:knowledge:{}:{}:{analyzed_at}",
                request.domain, request.question
            )
            .as_bytes(),
        );
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "domain": request.domain,
                "ontology_size": universal_domain_compiler.ontology.len(),
                "frontier": epistemic_frontier_detection.location,
                "solutions": synthesis_forge.len(),
            })
            .to_string()
            .as_bytes(),
        );

        self.record_report("knowledge", analyzed_at);
        Ok(UltimateKnowledgeReport {
            report_id,
            manifest_hash,
            universal_domain_compiler,
            live_knowledge_metabolism,
            epistemic_frontier_detection,
            synthesis_forge,
            analyzed_at,
        })
    }

    fn record_report(&mut self, family: &str, timestamp: i64) {
        self.total_reports = self.total_reports.saturating_add(1);
        self.last_reported_at = timestamp;
        match family {
            "cognition" => self.cognition_reports = self.cognition_reports.saturating_add(1),
            "acquisition" => self.acquisition_reports = self.acquisition_reports.saturating_add(1),
            "assistant" => self.assistant_reports = self.assistant_reports.saturating_add(1),
            "knowledge" => self.knowledge_reports = self.knowledge_reports.saturating_add(1),
            _ => {}
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn hash_prefix(input: &[u8]) -> String {
    sha3_256_hex(input)[..24].to_string()
}

fn default_true() -> bool {
    true
}

fn default_decision_score() -> f64 {
    0.65
}

fn default_transfer_success_rate() -> f64 {
    0.75
}

fn default_claim_score() -> f64 {
    0.72
}

fn default_task_weight() -> f64 {
    0.5
}

fn default_decay_rate() -> f64 {
    0.15
}

fn default_user_proximity() -> f64 {
    0.5
}

fn default_attention_floor() -> f64 {
    0.05
}

fn default_behavior_confidence() -> f64 {
    0.7
}

fn default_activation() -> f64 {
    0.55
}

fn default_idle_seconds() -> u64 {
    45
}

fn default_max_depth() -> usize {
    6
}

fn default_priority() -> String {
    "normal".into()
}

fn default_latency_ms() -> u32 {
    180
}

fn default_reputation() -> f64 {
    0.8
}

fn default_direction() -> String {
    "above".into()
}

fn default_cell_status() -> String {
    "dormant".into()
}

fn default_complexity() -> f64 {
    0.6
}

fn default_duration_minutes() -> u32 {
    45
}

fn default_energy_score() -> f64 {
    0.6
}

fn default_total_keystrokes() -> u32 {
    1
}

fn default_current_hour() -> u8 {
    9
}

fn default_authority() -> f64 {
    0.75
}

fn default_source_count() -> u32 {
    3
}

fn default_similarity() -> f64 {
    0.55
}

fn default_scatter_window_ms() -> u64 {
    DEFAULT_SCATTER_WINDOW_MS
}

fn default_mix_batch_size() -> usize {
    DEFAULT_MIX_BATCH_SIZE
}

fn normalized_time_horizons(input: &[u64]) -> Vec<u64> {
    let mut horizons = if input.is_empty() {
        DEFAULT_TIME_HORIZONS_MINUTES.to_vec()
    } else {
        input.to_vec()
    };
    horizons.sort_unstable();
    horizons.dedup();
    horizons
}

fn validate_cognition_request(request: &UltimateCognitionRequest) -> AstraResult<()> {
    if request.problem.trim().is_empty() {
        return Err(AstraError::ControlPlaneRejected(
            "ultimate cognition requires a problem statement".into(),
        ));
    }
    Ok(())
}

fn validate_acquisition_request(request: &UltimateAcquisitionRequest) -> AstraResult<()> {
    if request.targets.is_empty() {
        return Err(AstraError::ControlPlaneRejected(
            "ultimate acquisition requires at least one target".into(),
        ));
    }
    for target in &request.targets {
        Url::parse(&target.url).map_err(|error| {
            AstraError::ControlPlaneRejected(format!(
                "invalid acquisition target URL '{}': {error}",
                target.url
            ))
        })?;
    }
    Ok(())
}

fn build_temporal_reasoning(
    problem: &str,
    decisions: &[CognitiveDecisionInput],
    horizons: &[u64],
) -> TemporalReasoningFabricReport {
    let candidates = if decisions.is_empty() {
        vec![CognitiveDecisionInput {
            content: format!("address {}", summarize_problem(problem)),
            immediate_score: 0.62,
        }]
    } else {
        decisions.to_vec()
    };

    let mut best = None::<(String, f64, f64, Vec<TemporalProjectionReport>)>;
    for decision in &candidates {
        let projections = horizons
            .iter()
            .map(|horizon| project_decision(problem, decision, *horizon))
            .collect::<Vec<_>>();
        let scores = projections
            .iter()
            .map(|projection| projection.projected_score)
            .collect::<Vec<_>>();
        let min_score = scores.iter().copied().fold(1.0, f64::min);
        let max_score = scores.iter().copied().fold(0.0, f64::max);
        let temporal_coherence = (1.0 - (max_score - min_score)).clamp(0.0, 1.0);
        let avg_score = scores.iter().sum::<f64>() / scores.len().max(1) as f64;
        let weighted_temporal_score = (avg_score * 0.7 + temporal_coherence * 0.3).clamp(0.0, 1.0);
        if best
            .as_ref()
            .is_none_or(|(_, score, _, _)| weighted_temporal_score > *score)
        {
            best = Some((
                decision.content.clone(),
                weighted_temporal_score,
                temporal_coherence,
                projections,
            ));
        }
    }

    let (selected_decision, weighted_temporal_score, temporal_coherence, projections) =
        best.expect("at least one cognition decision should exist");
    TemporalReasoningFabricReport {
        selected_decision,
        temporal_coherence,
        weighted_temporal_score,
        projections,
    }
}

fn summarize_problem(problem: &str) -> String {
    problem
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
}

fn project_decision(
    problem: &str,
    decision: &CognitiveDecisionInput,
    horizon_minutes: u64,
) -> TemporalProjectionReport {
    let content = format!(
        "{} {}",
        problem.to_ascii_lowercase(),
        decision.content.to_ascii_lowercase()
    );
    let horizon_penalty = (horizon_minutes as f64 / 20_000.0).clamp(0.0, 0.25);
    let risk_penalty = keyword_penalty(
        &content,
        &[
            ("cheap", 0.08),
            ("fast", 0.05),
            ("quick", 0.04),
            ("temporary", 0.07),
            ("migrate", 0.06),
            ("scale", 0.09),
        ],
    );
    let robustness_bonus = keyword_bonus(
        &content,
        &[
            ("rollback", 0.05),
            ("attested", 0.04),
            ("tested", 0.04),
            ("staged", 0.03),
            ("redundant", 0.05),
        ],
    );
    let projected_score = (decision.immediate_score - horizon_penalty - risk_penalty
        + robustness_bonus)
        .clamp(0.0, 1.0);
    let mut risk_factors = Vec::new();
    let mut cascade_effects = Vec::new();
    if risk_penalty > 0.05 {
        risk_factors.push("long-horizon fragility".into());
        cascade_effects.push(format!(
            "at {} minutes, operational debt begins to outweigh short-term convenience",
            horizon_minutes
        ));
    }
    if content.contains("scale") {
        cascade_effects.push("capacity constraints shift cost into migration work".into());
    }
    if risk_factors.is_empty() {
        risk_factors.push("bounded execution risk".into());
        cascade_effects.push("decision remains stable across observed horizons".into());
    }
    TemporalProjectionReport {
        horizon_minutes,
        projected_score,
        risk_factors,
        cascade_effects,
    }
}

fn build_adversarial_imagination(
    problem: &str,
    selected_decision: &str,
) -> AdversarialImaginationReport {
    let inversions = vec![
        format!("assume '{}' fails under load", selected_decision),
        format!(
            "assume external dependencies for '{}' become unreliable",
            selected_decision
        ),
        format!(
            "assume the user intent behind '{}' was misread",
            selected_decision
        ),
    ];
    let attacks = inversions
        .into_iter()
        .map(|inversion| {
            let attack_strength = (0.42
                + keyword_penalty(
                    &format!("{problem} {selected_decision} {inversion}"),
                    &[
                        ("load", 0.18),
                        ("unreliable", 0.14),
                        ("misread", 0.12),
                        ("fragile", 0.1),
                    ],
                ))
            .clamp(0.0, 1.0);
            let defense_strength = (0.58
                + keyword_bonus(
                    selected_decision,
                    &[
                        ("staged", 0.12),
                        ("attested", 0.1),
                        ("verified", 0.08),
                        ("fallback", 0.1),
                    ],
                ))
            .clamp(0.0, 1.0);
            let defended = defense_strength >= attack_strength;
            AdversarialAttackReport {
                inversion,
                attack_strength,
                defended,
                defense_summary: if defended {
                    "defended through staged rollout, verification, or fallback controls".into()
                } else {
                    "attack remains undefeated; strengthen rollback and validation boundaries"
                        .into()
                },
            }
        })
        .collect::<Vec<_>>();
    let survived = attacks.iter().filter(|attack| attack.defended).count();
    let resilience_score = survived as f64 / attacks.len().max(1) as f64;
    AdversarialImaginationReport {
        resilience_score,
        recommendation: if resilience_score >= 0.8 {
            "decision is adversarially robust enough for controlled execution".into()
        } else if resilience_score >= 0.5 {
            "decision should be reinforced with stronger guards before execution".into()
        } else {
            "decision is fragile and should be redesigned".into()
        },
        attacks,
    }
}

fn build_depth_charges(problem: &str, max_depth: usize) -> CognitiveDepthChargesReport {
    let mut frontier = vec![(problem.to_string(), 0_usize)];
    let mut nodes = Vec::new();
    let mut atomic_leaf_count = 0;
    let mut max_depth_reached = 0;
    while let Some((current, depth)) = frontier.pop() {
        max_depth_reached = max_depth_reached.max(depth);
        let atomic = is_atomic_problem(&current, depth, max_depth);
        let subproblems = if atomic {
            Vec::new()
        } else {
            decompose_problem(&current)
        };
        if atomic {
            atomic_leaf_count += 1;
        } else {
            for subproblem in subproblems.iter().rev() {
                frontier.push((subproblem.clone(), depth + 1));
            }
        }
        nodes.push(DepthNodeSummary {
            problem: current,
            depth,
            is_atomic: atomic,
            child_count: subproblems.len(),
        });
        if nodes.len() >= 32 {
            break;
        }
    }

    CognitiveDepthChargesReport {
        max_depth_reached,
        atomic_leaf_count,
        bottom_up_solution: format!(
            "solve {} atomic leaves, then recompose upward through {} dependency levels",
            atomic_leaf_count, max_depth_reached
        ),
        nodes,
    }
}

fn is_atomic_problem(problem: &str, depth: usize, max_depth: usize) -> bool {
    depth >= max_depth || problem.split_whitespace().count() <= 5
}

fn decompose_problem(problem: &str) -> Vec<String> {
    let nouns = key_terms(problem);
    let mut parts = vec![
        format!(
            "define the success criteria for {}",
            summarize_problem(problem)
        ),
        format!(
            "identify constraints affecting {}",
            summarize_problem(problem)
        ),
    ];
    if let Some(term) = nouns.first() {
        parts.push(format!("measure the current state of {term}"));
    }
    if let Some(term) = nouns.get(1) {
        parts.push(format!("reduce uncertainty around {term}"));
    }
    parts
}

fn build_analogical_transfer(
    problem: &str,
    patterns: &[SolvedPatternInput],
) -> Option<AnalogicalGenomeTransferReport> {
    let defaults = default_pattern_library();
    let library = if patterns.is_empty() {
        defaults
    } else {
        patterns.to_vec()
    };
    let problem_dna = extract_structural_dna(problem);
    library
        .iter()
        .map(|pattern| {
            let pattern_dna = if pattern.structural_dna.is_empty() {
                extract_structural_dna(&pattern.solution_template)
            } else {
                pattern.structural_dna.clone()
            };
            let similarity = jaccard_genes(&problem_dna, &pattern_dna);
            (pattern, pattern_dna, similarity)
        })
        .filter(|(_, _, similarity)| *similarity >= 0.25)
        .max_by(|left, right| left.2.total_cmp(&right.2))
        .map(
            |(pattern, pattern_dna, similarity)| AnalogicalGenomeTransferReport {
                original_domain: pattern.original_domain.clone(),
                matched_label: pattern.label.clone(),
                structural_similarity: similarity,
                transferred_genes: pattern_dna,
                adapted_solution: format!(
                    "adapt '{}' from {} to {} by preserving the shared structural pattern",
                    pattern.solution_template,
                    pattern.original_domain,
                    summarize_problem(problem)
                ),
            },
        )
}

fn build_predictive_intents(
    signals: &[BehaviorSignalInput],
    problem: &str,
    recent_predictions: &mut VecDeque<String>,
) -> PredictiveIntentCrystallizationReport {
    let mut crystallized_intents = if signals.is_empty() {
        vec![CrystallizedIntentReport {
            predicted_intent: format!("follow-up on {}", summarize_problem(problem)),
            confidence: 0.64,
            preparation_actions: vec![
                "prefetch supporting context".into(),
                "prepare concise next-step summary".into(),
            ],
            predicted_hour: 9,
        }]
    } else {
        signals
            .iter()
            .take(4)
            .map(|signal| CrystallizedIntentReport {
                predicted_intent: signal.predicted_intent.clone(),
                confidence: signal.confidence,
                preparation_actions: vec![
                    format!(
                        "prefetch context chain: {}",
                        signal.context_chain.join(" -> ")
                    ),
                    format!("prepare response draft for '{}'", signal.predicted_intent),
                ],
                predicted_hour: signal.typical_hour.unwrap_or(9),
            })
            .collect::<Vec<_>>()
    };
    crystallized_intents.sort_by(|left, right| right.confidence.total_cmp(&left.confidence));

    for intent in &crystallized_intents {
        recent_predictions.push_back(intent.predicted_intent.clone());
    }
    while recent_predictions.len() > 32 {
        recent_predictions.pop_front();
    }

    let behavioral_dna_score = crystallized_intents
        .iter()
        .map(|intent| intent.confidence)
        .sum::<f64>()
        / crystallized_intents.len().max(1) as f64;
    PredictiveIntentCrystallizationReport {
        behavioral_dna_score,
        crystallized_intents,
    }
}

fn build_dream_state(
    clusters: &[ConceptClusterInput],
    idle_seconds: u64,
) -> DreamStateConsolidationReport {
    let compressed_concepts = clusters
        .iter()
        .filter(|cluster| cluster.aliases.len() >= 2)
        .map(|cluster| format!("{} <= {}", cluster.label, cluster.aliases.join(", ")))
        .collect::<Vec<_>>();
    let new_heuristics = clusters
        .iter()
        .filter(|cluster| cluster.activation >= 0.6 && !cluster.adjacent_concepts.is_empty())
        .take(3)
        .map(|cluster| {
            format!(
                "when '{}' activates, inspect related concepts: {}",
                cluster.label,
                cluster.adjacent_concepts.join(", ")
            )
        })
        .collect::<Vec<_>>();
    let healed_links = clusters
        .iter()
        .flat_map(|cluster| {
            cluster
                .adjacent_concepts
                .iter()
                .take(2)
                .map(move |adjacent| format!("bridge {} <-> {}", cluster.label, adjacent))
        })
        .take(4)
        .collect::<Vec<_>>();
    let pruned_concepts = clusters
        .iter()
        .filter(|cluster| cluster.last_activated_days_ago >= 30 && cluster.activation < 0.35)
        .map(|cluster| cluster.label.clone())
        .collect::<Vec<_>>();
    let mutated_parameters = BTreeMap::from([
        (
            "pattern_crystallization_gain".into(),
            (0.4 + idle_seconds as f64 / 300.0).clamp(0.4, 0.95),
        ),
        (
            "graph_healing_bias".into(),
            (0.3 + clusters.len() as f64 / 20.0).clamp(0.3, 0.9),
        ),
    ]);

    DreamStateConsolidationReport {
        compressed_concepts,
        new_heuristics,
        healed_links,
        pruned_concepts,
        mutated_parameters,
    }
}

fn build_attention_allocations(tasks: &[AttentionTaskInput]) -> FractalAttentionAllocationReport {
    let normalized_tasks = if tasks.is_empty() {
        vec![AttentionTaskInput {
            task_id: "foreground".into(),
            description: "active user request".into(),
            urgency: 0.9,
            importance: 0.9,
            decay_rate: 0.1,
            user_proximity: 1.0,
            floor: 0.1,
            cycles_starved: 0,
        }]
    } else {
        tasks.to_vec()
    };
    let raw = normalized_tasks
        .iter()
        .map(|task| {
            let starvation_bonus = 1.0 + task.cycles_starved as f64 * 0.1;
            let effective_priority =
                (task.urgency * 0.4 + task.importance * 0.35 + task.user_proximity * 0.25)
                    * starvation_bonus
                    * (1.0 + task.decay_rate);
            (task, effective_priority, starvation_bonus)
        })
        .collect::<Vec<_>>();
    let total_priority = raw
        .iter()
        .map(|(_, effective_priority, _)| *effective_priority)
        .sum::<f64>()
        .max(0.001);
    let allocations = raw
        .into_iter()
        .map(
            |(task, effective_priority, starvation_bonus)| AttentionAllocationEntry {
                task_id: task.task_id.clone(),
                allocation_units: (((effective_priority / total_priority).max(task.floor))
                    * DEFAULT_TOTAL_COMPUTE_UNITS as f64) as u64,
                effective_priority,
                starvation_bonus,
            },
        )
        .collect::<Vec<_>>();

    FractalAttentionAllocationReport {
        total_compute_units: DEFAULT_TOTAL_COMPUTE_UNITS,
        allocations,
    }
}

fn build_resonance_discoveries(
    contradictions: &[ContradictoryClaimInput],
) -> Vec<ResonanceDiscoveryReport> {
    contradictions
        .iter()
        .filter(|item| item.score_a >= 0.6 && item.score_b >= 0.6)
        .map(|item| {
            let axis = contradiction_axis(&item.claim_a, &item.claim_b);
            ResonanceDiscoveryReport {
                contradiction_axis: axis.clone(),
                deeper_truth: format!(
                    "both claims can hold when '{}' is reframed as a multi-scale tradeoff instead of a binary choice",
                    axis
                ),
                novelty_score: (0.55 + (item.score_a + item.score_b) / 4.0).clamp(0.0, 1.0),
                confidence: ((item.score_a + item.score_b) / 2.0 * 0.9).clamp(0.0, 1.0),
            }
        })
        .collect()
}

fn build_protocol_chameleon(
    targets: &[AcquisitionTargetInput],
    families: &[String],
) -> ProtocolChameleonEngineReport {
    let allowed = if families.is_empty() {
        vec![
            "chrome_macos".to_string(),
            "firefox_linux".to_string(),
            "safari_macos".to_string(),
            "edge_windows".to_string(),
        ]
    } else {
        families.to_vec()
    };
    let connection_plans = targets
        .iter()
        .enumerate()
        .map(|(index, target)| {
            let family = allowed[index % allowed.len()].clone();
            let (profile_name, ip_ttl, alpn_protocols, header_order) = match family.as_str() {
                "firefox_linux" => (
                    "Firefox 128 on Ubuntu".to_string(),
                    64,
                    vec!["h2".into(), "http/1.1".into()],
                    vec![
                        "host".into(),
                        "user-agent".into(),
                        "accept".into(),
                        "accept-language".into(),
                    ],
                ),
                "safari_macos" => (
                    "Safari 18 on macOS".to_string(),
                    64,
                    vec!["h2".into(), "http/1.1".into()],
                    vec![
                        "host".into(),
                        "accept".into(),
                        "user-agent".into(),
                        "accept-language".into(),
                    ],
                ),
                "edge_windows" => (
                    "Edge 126 on Windows 11".to_string(),
                    128,
                    vec!["h2".into(), "http/1.1".into()],
                    vec![
                        "host".into(),
                        "connection".into(),
                        "user-agent".into(),
                        "accept".into(),
                    ],
                ),
                _ => (
                    "Chrome 126 on macOS".to_string(),
                    64,
                    vec!["h2".into(), "http/1.1".into()],
                    vec![
                        "host".into(),
                        "user-agent".into(),
                        "accept".into(),
                        "accept-encoding".into(),
                    ],
                ),
            };
            ProtocolChameleonConnectionPlan {
                url: target.url.clone(),
                profile_name,
                profile_family: family,
                ip_ttl,
                alpn_protocols,
                header_order,
            }
        })
        .collect();
    ProtocolChameleonEngineReport { connection_plans }
}

fn build_temporal_scattering(
    targets: &[AcquisitionTargetInput],
    scatter_window_ms: u64,
) -> TemporalRequestScatteringReport {
    let scheduled_requests = targets
        .iter()
        .enumerate()
        .map(|(index, target)| {
            let delay_ms = deterministic_delay(&target.url, scatter_window_ms, index + 1);
            TemporalScatterEntry {
                url: target.url.clone(),
                delay_ms,
                browser_profile_name: format!("profile-{}", (index % 4) + 1),
            }
        })
        .collect::<Vec<_>>();
    TemporalRequestScatteringReport {
        mean_delay_ms: scatter_window_ms / targets.len().max(1) as u64,
        scheduled_requests,
    }
}

fn build_semantic_proxy_mesh(
    targets: &[AcquisitionTargetInput],
    proxies: &[ProxyCandidateInput],
) -> SemanticProxyMeshReport {
    let candidates = if proxies.is_empty() {
        default_proxy_pool()
    } else {
        proxies.to_vec()
    };
    let mut learned_domain_scores = BTreeMap::new();
    let selections = targets
        .iter()
        .map(|target| {
            let domain = extract_domain(&target.url);
            let best = candidates
                .iter()
                .filter(|proxy| {
                    !proxy
                        .rate_limited_domains
                        .iter()
                        .any(|blocked| blocked == &domain)
                })
                .map(|proxy| {
                    let domain_score = proxy.domain_scores.get(&domain).copied().unwrap_or(0.5);
                    let latency_score = (1.0 - proxy.latency_ms as f64 / 2_000.0).clamp(0.0, 1.0);
                    let score =
                        domain_score * 0.45 + proxy.ip_reputation * 0.35 + latency_score * 0.2;
                    (proxy, score)
                })
                .max_by(|left, right| left.1.total_cmp(&right.1))
                .expect("proxy candidate pool should not be empty");
            learned_domain_scores.insert(domain.clone(), best.1);
            ProxyRouteDecision {
                url: target.url.clone(),
                proxy_id: best.0.proxy_id.clone(),
                score: best.1,
                rationale: format!(
                    "selected for {} based on domain trust, reputation, and latency",
                    domain
                ),
            }
        })
        .collect::<Vec<_>>();

    SemanticProxyMeshReport {
        selections,
        learned_domain_scores,
    }
}

fn build_dns_phantom_resolution(
    targets: &[AcquisitionTargetInput],
    cover_domains: &[String],
) -> DnsPhantomResolutionReport {
    let cover_pool = if cover_domains.is_empty() {
        vec![
            "example.org".into(),
            "ietf.org".into(),
            "wikipedia.org".into(),
            "rust-lang.org".into(),
            "docs.rs".into(),
            "mozilla.org".into(),
            "opensource.org".into(),
        ]
    } else {
        cover_domains.to_vec()
    };
    let resolutions = targets
        .iter()
        .map(|target| {
            let target_domain = extract_domain(&target.url);
            let mut queries = cover_pool.iter().take(7).cloned().collect::<Vec<_>>();
            let position = deterministic_position(&target_domain, queries.len() + 1);
            queries.insert(position, target_domain.clone());
            DnsPhantomResolutionEntry {
                target_domain,
                cover_queries: queries,
                inserted_position: position,
            }
        })
        .collect::<Vec<_>>();
    DnsPhantomResolutionReport { resolutions }
}

fn build_tls_nebula(plans: &[ProtocolChameleonConnectionPlan]) -> TlsFingerprintNebulaReport {
    let base_family = plans
        .first()
        .map(|plan| plan.profile_family.clone())
        .unwrap_or_else(|| "chrome_macos".into());
    let uniqueness_hash = sha3_256_hex(serde_json::to_string(plans).unwrap_or_default().as_bytes());
    TlsFingerprintNebulaReport {
        base_family,
        profile_id: hash_prefix(uniqueness_hash.as_bytes()),
        session_id_length: (uniqueness_hash.len() % 32) as u8,
        mutated_cipher_count: plans.len().max(1) * 3,
        uniqueness_hash,
    }
}

fn build_origin_erasure(batch_size: usize, respect_robots: bool) -> RequestOriginErasureReport {
    RequestOriginErasureReport {
        mix_hops: 3,
        batch_size,
        unlinkability_budget: (1.0 - 1.0 / batch_size.max(2) as f64).clamp(0.0, 0.99),
        routing_plan: vec![
            "ingress batch mix".into(),
            "policy-aware relay shuffle".into(),
            "egress dispatch through attested lane".into(),
        ],
        compliance_notes: vec![
            "bounded privacy-preserving routing only for approved enterprise acquisition lanes"
                .into(),
            if respect_robots {
                "robots-aware acquisition remains enabled".into()
            } else {
                "robots policy disabled by caller; review before production enablement".into()
            },
        ],
    }
}

fn build_ambient_intent(request: &UltimateAssistantRequest) -> AmbientIntentInferenceReport {
    let text_lower = request.text.to_ascii_lowercase();
    let explicit_intent = if text_lower.contains("fix") || request.recent_errors > 0 {
        "debugging".into()
    } else if text_lower.contains("plan") || request.is_week_start {
        "planning".into()
    } else if text_lower.contains("summar") {
        "summary".into()
    } else {
        "general_assistance".into()
    };
    let urgency = if request.typing_speed_wpm > 80.0 {
        0.9
    } else if request.typing_speed_wpm > 50.0 {
        0.6
    } else {
        0.3
    };
    let uncertainty = request.deletions as f64 / request.total_keystrokes.max(1) as f64;
    let mut inferred_needs = Vec::new();
    if urgency > 0.7 && uncertainty > 0.1 {
        inferred_needs.push("quick_direct_answer".into());
    }
    if request.has_code_open && request.recent_errors > 0 {
        inferred_needs.push("debugging_assistance".into());
    }
    if request.is_week_start && request.is_morning {
        inferred_needs.push("weekly_summary".into());
    }
    if request
        .clipboard_excerpt
        .as_deref()
        .is_some_and(|excerpt| excerpt.contains("http"))
    {
        inferred_needs.push("link_follow_up".into());
    }
    let response_style = if urgency > 0.7 {
        "telegraphic"
    } else if uncertainty > 0.2 {
        "explorative"
    } else {
        "comprehensive"
    };
    AmbientIntentInferenceReport {
        explicit_intent,
        inferred_needs,
        urgency,
        response_style: response_style.into(),
    }
}

fn build_proactive_orchestration(
    request: &UltimateAssistantRequest,
) -> ProactiveEnvironmentOrchestrationReport {
    let mut actions = Vec::new();
    for event in request
        .events
        .iter()
        .filter(|event| event.minutes_until <= 15)
    {
        actions.push(OrchestrationActionReport {
            trigger: format!(
                "meeting '{}' approaches in {} minutes",
                event.title, event.minutes_until
            ),
            action: format!(
                "prepare briefing for {}",
                event.topic.if_empty_then(&event.title)
            ),
            rationale: "time-approaching trigger warrants just-in-time context assembly".into(),
        });
    }
    for metric in request.metrics.iter().filter(|metric| {
        (metric.direction == "above" && metric.value >= metric.threshold)
            || (metric.direction == "below" && metric.value <= metric.threshold)
    }) {
        actions.push(OrchestrationActionReport {
            trigger: format!("metric '{}' crossed threshold", metric.metric),
            action: format!(
                "prefetch diagnostics and mitigation options for {}",
                metric.metric
            ),
            rationale: "threshold crossing suggests proactive intervention".into(),
        });
    }
    if request.has_code_open {
        actions.push(OrchestrationActionReport {
            trigger: "development context entered".into(),
            action: "prepare focused debugging or implementation workspace summary".into(),
            rationale: "code-centric context benefits from preloaded project state".into(),
        });
    }
    let readiness_score = (0.4 + actions.len() as f64 / 10.0).clamp(0.0, 1.0);
    ProactiveEnvironmentOrchestrationReport {
        readiness_score,
        actions,
    }
}

fn build_task_speciation(goal: &str, seed_cells: &[GoalCellInput]) -> TaskSpeciationReport {
    let cells = if seed_cells.is_empty() {
        generate_goal_cells(goal)
    } else {
        seed_cells
            .iter()
            .map(|cell| TaskCellReport {
                description: cell.description.clone(),
                status: cell.status.clone(),
                health: cell.health_score,
                mutation: if cell.complexity > 0.7 {
                    "spawn_subcells".into()
                } else {
                    "stable".into()
                },
            })
            .collect::<Vec<_>>()
    };
    let adaptation_summary = vec![
        "cells with weak health are candidates for mutation or pruning".into(),
        "high-complexity cells can spawn narrower teaching or execution subcells".into(),
        "completed cells feed progress shape back into future scheduling".into(),
    ];
    TaskSpeciationReport {
        goal: if goal.trim().is_empty() {
            "general goal organism".into()
        } else {
            goal.into()
        },
        cells,
        adaptation_summary,
    }
}

fn generate_goal_cells(goal: &str) -> Vec<TaskCellReport> {
    let summary = summarize_problem(goal);
    vec![
        TaskCellReport {
            description: format!("clarify target outcome for {summary}"),
            status: "active".into(),
            health: 0.82,
            mutation: "stable".into(),
        },
        TaskCellReport {
            description: format!("identify blockers for {summary}"),
            status: "dormant".into(),
            health: 0.71,
            mutation: "spawn_subcells".into(),
        },
        TaskCellReport {
            description: format!("measure progress signals for {summary}"),
            status: "dormant".into(),
            health: 0.68,
            mutation: "stable".into(),
        },
    ]
}

fn build_sovereign_schedule(
    tasks: &[ScheduleTaskInput],
    energy_windows: &[EnergyWindowInput],
) -> SovereignScheduleReport {
    let mut windows = if energy_windows.is_empty() {
        vec![
            EnergyWindowInput {
                start_hour: 8,
                end_hour: 10,
                energy_score: 0.9,
            },
            EnergyWindowInput {
                start_hour: 10,
                end_hour: 13,
                energy_score: 0.6,
            },
            EnergyWindowInput {
                start_hour: 14,
                end_hour: 17,
                energy_score: 0.75,
            },
        ]
    } else {
        energy_windows.to_vec()
    };
    windows.sort_by_key(|window| window.start_hour);

    let mut tasks = if tasks.is_empty() {
        vec![ScheduleTaskInput {
            task_id: "default".into(),
            title: "review active priorities".into(),
            duration_minutes: 45,
            energy_required: 0.7,
            context: "planning".into(),
            deadline_minutes: None,
            quick: false,
        }]
    } else {
        tasks.to_vec()
    };
    tasks.sort_by(|left, right| {
        let left_score = left.deadline_minutes.unwrap_or(u64::MAX);
        let right_score = right.deadline_minutes.unwrap_or(u64::MAX);
        left_score.cmp(&right_score)
    });

    let mut optimized_blocks = Vec::new();
    for task in tasks {
        let window = windows
            .iter()
            .find(|window| window.energy_score >= task.energy_required)
            .unwrap_or(&windows[0]);
        optimized_blocks.push(ScheduleBlockReport {
            start_hour: window.start_hour,
            duration_minutes: task.duration_minutes,
            title: task.title,
            rationale: if task.quick {
                "batched into lower-friction schedule window".into()
            } else {
                "aligned with a matching energy window and deadline pressure".into()
            },
        });
    }
    let focus_minutes = optimized_blocks
        .iter()
        .map(|block| block.duration_minutes)
        .sum();
    SovereignScheduleReport {
        focus_minutes,
        optimized_blocks,
    }
}

fn build_universal_domain_compiler(
    request: &UltimateKnowledgeRequest,
) -> UniversalDomainCompilerReport {
    let content = request
        .sources
        .iter()
        .map(|source| source.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let ontology = top_terms(&content, 12);
    let rules = extract_rule_statements(&content);
    let expert_patterns = vec![
        format!(
            "experts in {} prioritize high-authority evidence first",
            request.domain
        ),
        format!(
            "experts in {} validate terminology against domain ontology",
            request.domain
        ),
        "experts compare edge cases before committing to a conclusion".into(),
    ];
    let vocabulary = ontology.iter().take(8).cloned().collect::<Vec<_>>();
    let common_mistakes = vec![
        format!(
            "treating {} as generic instead of domain-specific",
            request.domain
        ),
        "failing to separate observation from mechanism".into(),
        "ignoring boundary conditions when applying rules".into(),
    ];
    let authoritative_sources = request
        .sources
        .iter()
        .map(|source| source.title.clone())
        .collect::<Vec<_>>();
    let compilation_confidence = if request.sources.is_empty() {
        0.45
    } else {
        (request
            .sources
            .iter()
            .map(|source| source.authority_score)
            .sum::<f64>()
            / request.sources.len() as f64)
            .clamp(0.0, 1.0)
    };

    UniversalDomainCompilerReport {
        domain: request.domain.clone(),
        ontology,
        rules,
        expert_patterns,
        vocabulary,
        common_mistakes,
        authoritative_sources,
        compilation_confidence,
    }
}

fn build_knowledge_metabolism(request: &UltimateKnowledgeRequest) -> LiveKnowledgeMetabolismReport {
    let digested_claims = request
        .sources
        .iter()
        .flat_map(|source| source.content.split('.'))
        .map(str::trim)
        .filter(|sentence| sentence.split_whitespace().count() >= 6)
        .take(6)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let absorbed_concepts = request
        .sources
        .iter()
        .flat_map(|source| top_terms(&source.content, 3))
        .take(8)
        .collect::<Vec<_>>();
    let proactive_alerts = request
        .recency_events
        .iter()
        .take(4)
        .map(|event| format!("new signal metabolized: {event}"))
        .collect::<Vec<_>>();
    let pruned_items = request
        .sources
        .iter()
        .filter(|source| source.authority_score < 0.5)
        .map(|source| source.title.clone())
        .collect::<Vec<_>>();
    LiveKnowledgeMetabolismReport {
        ingested_feeds: request.live_feeds.clone(),
        digested_claims,
        absorbed_concepts,
        proactive_alerts,
        pruned_items,
    }
}

fn build_epistemic_frontier(
    request: &UltimateKnowledgeRequest,
) -> EpistemicFrontierDetectionReport {
    if request.competing_claims.is_empty() && !request.sources.is_empty() {
        return EpistemicFrontierDetectionReport {
            location: "well_known".into(),
            dominant_theory: Some(format!("established {} synthesis", request.domain)),
            alternative_theories: Vec::new(),
            consensus_level: 0.9,
            key_experiments_needed: vec!["continue monitoring for contradictory evidence".into()],
            why_unknown: None,
        };
    }
    if request.competing_claims.len() >= 2 {
        let mut claims = request.competing_claims.clone();
        claims.sort_by(|left, right| right.support_score.total_cmp(&left.support_score));
        let consensus_level = claims.iter().map(|claim| claim.support_score).sum::<f64>()
            / claims.len() as f64
            * 0.75;
        return EpistemicFrontierDetectionReport {
            location: "research_frontier".into(),
            dominant_theory: claims.first().map(|claim| claim.theory.clone()),
            alternative_theories: claims
                .iter()
                .skip(1)
                .map(|claim| claim.theory.clone())
                .collect(),
            consensus_level: consensus_level.clamp(0.0, 1.0),
            key_experiments_needed: vec![
                "run discriminating experiment against the top two theories".into(),
                "increase measurement precision around the causal bottleneck".into(),
            ],
            why_unknown: None,
        };
    }
    EpistemicFrontierDetectionReport {
        location: "beyond_frontier".into(),
        dominant_theory: None,
        alternative_theories: Vec::new(),
        consensus_level: 0.2,
        key_experiments_needed: vec![
            "collect first-principles observations".into(),
            "construct a minimal reproducible study design".into(),
        ],
        why_unknown: Some(
            "insufficient authoritative evidence and no stable theory cluster".into(),
        ),
    }
}

fn build_synthesis_forge(request: &UltimateKnowledgeRequest) -> Vec<NovelSolutionReport> {
    let problem = if request.synthesis_problem.trim().is_empty() {
        request.question.as_str()
    } else {
        request.synthesis_problem.as_str()
    };
    let mut matches = if request.cross_domain_matches.is_empty() {
        vec![
            CrossDomainMatchInput {
                label: "immune system triage".into(),
                source_domain: "biology".into(),
                structural_similarity: 0.58,
            },
            CrossDomainMatchInput {
                label: "airline crew checklists".into(),
                source_domain: "aviation".into(),
                structural_similarity: 0.61,
            },
        ]
    } else {
        request.cross_domain_matches.clone()
    };
    matches.sort_by(|left, right| {
        right
            .structural_similarity
            .total_cmp(&left.structural_similarity)
    });
    matches
        .into_iter()
        .take(5)
        .map(|item| NovelSolutionReport {
            approach: format!(
                "apply '{}' from {} to '{}'",
                item.label,
                item.source_domain,
                summarize_problem(problem)
            ),
            source_domain: item.source_domain,
            structural_similarity: item.structural_similarity,
            novelty_score: (1.0 - item.structural_similarity).clamp(0.0, 1.0),
            feasibility_estimate: (item.structural_similarity * 0.85).clamp(0.0, 1.0),
        })
        .collect()
}

fn keyword_penalty(content: &str, rules: &[(&str, f64)]) -> f64 {
    rules
        .iter()
        .filter_map(|(needle, weight)| content.contains(needle).then_some(*weight))
        .sum::<f64>()
        .clamp(0.0, 0.4)
}

fn keyword_bonus(content: &str, rules: &[(&str, f64)]) -> f64 {
    rules
        .iter()
        .filter_map(|(needle, weight)| {
            content
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
                .then_some(*weight)
        })
        .sum::<f64>()
        .clamp(0.0, 0.3)
}

fn key_terms(problem: &str) -> Vec<String> {
    top_terms(problem, 6)
}

fn contradiction_axis(left: &str, right: &str) -> String {
    let left_terms = token_set(left);
    let right_terms = token_set(right);
    let difference = left_terms
        .symmetric_difference(&right_terms)
        .take(3)
        .cloned()
        .collect::<Vec<_>>();
    if difference.is_empty() {
        "scope".into()
    } else {
        difference.join(" vs ")
    }
}

fn default_pattern_library() -> Vec<SolvedPatternInput> {
    vec![
        SolvedPatternInput {
            label: "immune_response".into(),
            original_domain: "biology".into(),
            solution_template: "layered detection, quarantine, and memory-based adaptation".into(),
            structural_dna: vec![
                StructuralGene::System,
                StructuralGene::FeedbackLoop,
                StructuralGene::Threshold,
                StructuralGene::Competition,
            ],
            transfer_success_rate: 0.82,
        },
        SolvedPatternInput {
            label: "supply_chain_buffering".into(),
            original_domain: "logistics".into(),
            solution_template: "introduce safety stock and reroute around brittle dependencies"
                .into(),
            structural_dna: vec![
                StructuralGene::System,
                StructuralGene::Constraint,
                StructuralGene::Optimization,
                StructuralGene::Cascade,
            ],
            transfer_success_rate: 0.76,
        },
        SolvedPatternInput {
            label: "crew_resource_management".into(),
            original_domain: "aviation".into(),
            solution_template: "standardize communication, checklists, and escalation boundaries"
                .into(),
            structural_dna: vec![
                StructuralGene::Cooperation,
                StructuralGene::Hierarchy,
                StructuralGene::Constraint,
                StructuralGene::Conservation,
            ],
            transfer_success_rate: 0.8,
        },
    ]
}

fn extract_structural_dna(text: &str) -> Vec<StructuralGene> {
    let lower = text.to_ascii_lowercase();
    let mapping = [
        (
            StructuralGene::System,
            ["system", "platform", "network", "organization"],
        ),
        (
            StructuralGene::FeedbackLoop,
            ["feedback", "observe", "monitor", "adapt"],
        ),
        (
            StructuralGene::Cascade,
            ["cascade", "chain", "propagate", "domino"],
        ),
        (
            StructuralGene::Threshold,
            ["threshold", "limit", "gate", "trigger"],
        ),
        (
            StructuralGene::Competition,
            ["compete", "adversary", "market", "fraud"],
        ),
        (
            StructuralGene::Cooperation,
            ["collaborate", "team", "handoff", "shared"],
        ),
        (
            StructuralGene::Optimization,
            ["optimize", "best", "efficient", "minimize"],
        ),
        (
            StructuralGene::Constraint,
            ["constraint", "bound", "restricted", "deadline"],
        ),
        (
            StructuralGene::Transformation,
            ["transform", "convert", "compile", "map"],
        ),
        (
            StructuralGene::Hierarchy,
            ["hierarchy", "level", "stack", "tier"],
        ),
        (
            StructuralGene::Emergence,
            ["emerge", "pattern", "collective", "macro"],
        ),
        (
            StructuralGene::Oscillation,
            ["oscillate", "cycle", "periodic", "wave"],
        ),
        (
            StructuralGene::Saturation,
            ["saturate", "plateau", "diminishing", "exhaust"],
        ),
        (
            StructuralGene::Bifurcation,
            ["branch", "fork", "decision", "split"],
        ),
        (
            StructuralGene::Conservation,
            ["preserve", "conserve", "retain", "guard"],
        ),
    ];
    let mut genes = mapping
        .iter()
        .filter_map(|(gene, needles)| {
            needles
                .iter()
                .any(|needle| lower.contains(needle))
                .then_some(gene.clone())
        })
        .collect::<Vec<_>>();
    genes.sort();
    genes.dedup();
    if genes.is_empty() {
        genes.push(StructuralGene::System);
        genes.push(StructuralGene::Constraint);
    }
    genes
}

fn jaccard_genes(left: &[StructuralGene], right: &[StructuralGene]) -> f64 {
    let left = left.iter().cloned().collect::<BTreeSet<_>>();
    let right = right.iter().cloned().collect::<BTreeSet<_>>();
    let intersection = left.intersection(&right).count() as f64;
    let union = left.union(&right).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        (intersection / union).clamp(0.0, 1.0)
    }
}

fn default_proxy_pool() -> Vec<ProxyCandidateInput> {
    vec![
        ProxyCandidateInput {
            proxy_id: "proxy-us-east".into(),
            address: "10.0.0.1:443".into(),
            geo_location: "us-east".into(),
            latency_ms: 120,
            ip_reputation: 0.88,
            domain_scores: BTreeMap::from([("example.com".into(), 0.82), ("docs.rs".into(), 0.9)]),
            rate_limited_domains: Vec::new(),
        },
        ProxyCandidateInput {
            proxy_id: "proxy-eu-west".into(),
            address: "10.0.1.1:443".into(),
            geo_location: "eu-west".into(),
            latency_ms: 160,
            ip_reputation: 0.85,
            domain_scores: BTreeMap::from([
                ("wikipedia.org".into(), 0.86),
                ("mozilla.org".into(), 0.8),
            ]),
            rate_limited_domains: Vec::new(),
        },
    ]
}

fn extract_domain(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".into())
}

fn deterministic_delay(seed: &str, scatter_window_ms: u64, ordinal: usize) -> u64 {
    let hash = sha3_256_hex(format!("{seed}:{ordinal}").as_bytes());
    let sample = u64::from_str_radix(&hash[..8], 16).unwrap_or(0);
    sample % scatter_window_ms.max(1)
}

fn deterministic_position(seed: &str, len: usize) -> usize {
    let hash = sha3_256_hex(seed.as_bytes());
    usize::from_str_radix(&hash[..4], 16).unwrap_or(0) % len.max(1)
}

fn top_terms(content: &str, limit: usize) -> Vec<String> {
    let mut frequencies = BTreeMap::<String, usize>::new();
    for token in token_set(content) {
        *frequencies.entry(token).or_insert(0) += 1;
    }
    let mut ranked = frequencies.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(token, _)| token)
        .collect()
}

fn token_set(content: &str) -> BTreeSet<String> {
    let token_regex = Regex::new(r"[A-Za-z][A-Za-z0-9_-]{2,}").expect("token regex should compile");
    token_regex
        .find_iter(content)
        .map(|value| value.as_str().to_ascii_lowercase())
        .filter(|token| !matches!(token.as_str(), "the" | "and" | "with" | "from" | "that"))
        .collect()
}

fn extract_rule_statements(content: &str) -> Vec<CompiledRuleReport> {
    let mut rules = content
        .split('.')
        .map(str::trim)
        .filter(|sentence| {
            let lower = sentence.to_ascii_lowercase();
            lower.contains("must")
                || lower.contains("should")
                || lower.contains("requires")
                || lower.contains("always")
        })
        .take(6)
        .map(|sentence| CompiledRuleReport {
            statement: sentence.to_string(),
            confidence: 0.72,
        })
        .collect::<Vec<_>>();
    if rules.is_empty() {
        rules.push(CompiledRuleReport {
            statement: "establish authoritative terminology before applying domain logic".into(),
            confidence: 0.6,
        });
    }
    rules
}

trait EmptyFallback {
    fn if_empty_then<'a>(&'a self, fallback: &'a str) -> &'a str;
}

impl EmptyFallback for String {
    fn if_empty_then<'a>(&'a self, fallback: &'a str) -> &'a str {
        if self.trim().is_empty() {
            fallback
        } else {
            self.as_str()
        }
    }
}

impl Default for UltimateAstraFeaturesEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analogical_transfer_finds_structural_match() {
        let transfer = build_analogical_transfer(
            "design a fraud detection system with feedback and thresholds",
            &[],
        )
        .expect("transfer should exist");
        assert!(transfer.structural_similarity >= 0.25);
        assert!(!transfer.adapted_solution.is_empty());
    }

    #[test]
    fn temporal_scattering_spreads_requests() {
        let report = build_temporal_scattering(
            &[
                AcquisitionTargetInput {
                    url: "https://example.com/a".into(),
                    semantic_goal: String::new(),
                    desired_geo: None,
                    priority: "normal".into(),
                },
                AcquisitionTargetInput {
                    url: "https://example.com/b".into(),
                    semantic_goal: String::new(),
                    desired_geo: None,
                    priority: "normal".into(),
                },
            ],
            10_000,
        );
        assert_eq!(report.scheduled_requests.len(), 2);
        assert_ne!(
            report.scheduled_requests[0].delay_ms,
            report.scheduled_requests[1].delay_ms
        );
    }

    #[test]
    fn assistant_schedule_produces_blocks() {
        let report = build_sovereign_schedule(
            &[ScheduleTaskInput {
                task_id: "t1".into(),
                title: "Implement feature".into(),
                duration_minutes: 90,
                energy_required: 0.8,
                context: "coding".into(),
                deadline_minutes: Some(120),
                quick: false,
            }],
            &[EnergyWindowInput {
                start_hour: 8,
                end_hour: 10,
                energy_score: 0.9,
            }],
        );
        assert_eq!(report.optimized_blocks.len(), 1);
        assert_eq!(report.optimized_blocks[0].start_hour, 8);
    }

    #[test]
    fn frontier_detection_identifies_research_frontier() {
        let report = build_epistemic_frontier(&UltimateKnowledgeRequest {
            domain: "consciousness".into(),
            question: "why does anesthesia affect consciousness".into(),
            sources: Vec::new(),
            live_feeds: Vec::new(),
            competing_claims: vec![
                FrontierClaimInput {
                    theory: "integration disruption".into(),
                    support_score: 0.7,
                    source_count: 5,
                },
                FrontierClaimInput {
                    theory: "microtubule coherence".into(),
                    support_score: 0.45,
                    source_count: 2,
                },
            ],
            synthesis_problem: String::new(),
            cross_domain_matches: Vec::new(),
            recency_events: Vec::new(),
            attestor: None,
            notarize_to_chain: false,
        });
        assert_eq!(report.location, "research_frontier");
        assert!(report.dominant_theory.is_some());
    }
}

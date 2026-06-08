use std::collections::{BTreeMap, BTreeSet, VecDeque};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::brain::phantom_sandbox::{PhantomSandbox, RiskLevel};
use crate::chain::chain::Chain;
use crate::chain::value_protocol::{
    ComplianceMode, EnterpriseRiskTier, FinancialIntentRoute, FinancialIntentRouteRequest,
    GuardedPaymentOutcome, GuardedPaymentRequest, MarketProviderQuote, PrivacyPreservationMode,
    SettlementRail, SmartPaymentKind,
};
use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};
use crate::tools::code_executor::{CodeExecutor, ExecutionRequest, Language, ValidationResult};

const MAX_SANDBOX_INPUT_BYTES: usize = 64 * 1024;
const DEFAULT_FOLD_WINDOW_MS: i64 = 250;
const MAX_HEAT_SIGNATURES: usize = 32;

#[derive(Debug, Clone, Deserialize)]
pub struct RevolutionarySandboxRequest {
    pub execution_target: String,
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default)]
    pub expected_outputs: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub require_firebreaks: bool,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntropicExecutionCageReport {
    pub projection_id: String,
    pub synthetic_filesystem_roots: Vec<String>,
    pub synthetic_network_hosts: Vec<String>,
    pub deterministic_clock_epoch_ms: i64,
    pub deterministic_rng_seed_hash: String,
    pub trap_categories: Vec<String>,
    pub environment_projection_count: usize,
    pub allow_real_execution: bool,
    pub isolation_guarantee: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalcifiedIntentNode {
    pub node_id: String,
    pub atomic_intent: String,
    pub scope: String,
    pub evidence: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalcifiedIntentEdge {
    pub from: String,
    pub to: String,
    pub relationship: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalcifiedIntentGraph {
    pub nodes: Vec<CalcifiedIntentNode>,
    pub edges: Vec<CalcifiedIntentEdge>,
    pub canonical_summary: Vec<String>,
    pub dangerous_taxa: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirebreakGate {
    pub gate: String,
    pub rationale: String,
    pub satisfied_by: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CausalFirebreakReport {
    pub required_gates: Vec<FirebreakGate>,
    pub causal_gap_score: f64,
    pub firebreak_ready: bool,
    pub narrative: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhantomThermodynamicsReport {
    pub productive_ratio: f64,
    pub exploratory_ratio: f64,
    pub heat_score: f64,
    pub freeze_execution: bool,
    pub signature: String,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevolutionarySandboxVerdict {
    pub risk_level: String,
    pub allow_simulation: bool,
    pub allow_real_execution: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevolutionarySandboxReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub entropic_execution_cage: EntropicExecutionCageReport,
    pub semantic_opcode_calcification: CalcifiedIntentGraph,
    pub causal_firebreak_architecture: CausalFirebreakReport,
    pub phantom_thermodynamics: PhantomThermodynamicsReport,
    pub verdict: RevolutionarySandboxVerdict,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevolutionaryValueRequest {
    pub sender: String,
    pub receiver: String,
    pub amount: f64,
    pub purpose: String,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default = "default_settlement_rail")]
    pub settlement_rail: SettlementRail,
    #[serde(default = "default_compliance_mode")]
    pub compliance: ComplianceMode,
    #[serde(default = "default_privacy_mode")]
    pub privacy_mode: PrivacyPreservationMode,
    #[serde(default = "default_risk_tier")]
    pub risk_tier: EnterpriseRiskTier,
    #[serde(default)]
    pub connector_id: Option<String>,
    #[serde(default)]
    pub providers: Vec<MarketProviderQuote>,
    #[serde(default = "default_true")]
    pub allow_split_routing: bool,
    #[serde(default = "default_true")]
    pub require_escrow: bool,
    #[serde(default = "default_false")]
    pub autocommit: bool,
    #[serde(default = "default_fulfillment_window_seconds")]
    pub fulfillment_window_seconds: u64,
    #[serde(default = "default_min_quality_score")]
    pub min_quality_score: f64,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub cooperative_unlock_parties: Vec<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CausalValueStep {
    pub stage: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentionStateCurrencyReport {
    pub seal_id: String,
    pub amount: f64,
    pub currency: String,
    pub purpose_hash: String,
    pub purpose_chain: Vec<CausalValueStep>,
    pub reversal_conditions: Vec<String>,
    pub behavioral_proof_hash: String,
    pub manipulation_resistance: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CausalLedgerCrystallographyReport {
    pub sequence: u64,
    pub causal_parents: Vec<String>,
    pub uniqueness_witness: String,
    pub chain_height: u64,
    pub latest_chain_hash: String,
    pub instant_finality_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetabolicRouteLeg {
    pub vessel_id: String,
    pub allocated_amount: f64,
    pub score: f64,
    pub eta_minutes: u32,
    pub fee_estimate: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetabolicTransactionRoutingReport {
    pub selected_provider: String,
    pub route: FinancialIntentRoute,
    pub split_plan: Vec<MetabolicRouteLeg>,
    pub coagulation_strategy: String,
    pub optimization_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EntropicEscrowReport {
    pub escrow_id: String,
    pub puzzle_hash: String,
    pub unlock_after_seconds: u64,
    pub cooperative_unlock_threshold: u8,
    pub cooperative_unlock_parties: Vec<String>,
    pub release_policy: Vec<String>,
    pub early_unlock_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevolutionaryValueReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub intention_state_currency: IntentionStateCurrencyReport,
    pub causal_ledger_crystallography: CausalLedgerCrystallographyReport,
    pub metabolic_transaction_routing: MetabolicTransactionRoutingReport,
    pub entropic_escrow: EntropicEscrowReport,
    pub guarded_payment: GuardedPaymentOutcome,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticPacketInput {
    pub packet_id: String,
    pub content: String,
    #[serde(default)]
    pub semantic_type: Option<String>,
    #[serde(default)]
    pub semantic_dependencies: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProtocolRequestInput {
    pub request_id: String,
    pub intent: String,
    #[serde(default)]
    pub trust_profile: String,
    pub submitted_at_ms: i64,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub arrival_rank: usize,
    #[serde(default = "default_request_priority")]
    pub priority: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProtocolSystemLoad {
    #[serde(default)]
    pub cpu_utilization: f64,
    #[serde(default)]
    pub memory_pressure: f64,
    #[serde(default)]
    pub network_congestion: f64,
    #[serde(default)]
    pub queue_depth: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProtocolOutcomeSample {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub latency_ms: u32,
    #[serde(default)]
    pub useful_bytes: u32,
    #[serde(default)]
    pub total_bytes: u32,
    #[serde(default)]
    pub recovered_errors: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevolutionaryProtocolRequest {
    #[serde(default)]
    pub packets: Vec<SemanticPacketInput>,
    #[serde(default)]
    pub requests: Vec<ProtocolRequestInput>,
    #[serde(default)]
    pub system_load: ProtocolSystemLoad,
    #[serde(default)]
    pub outcome_samples: Vec<ProtocolOutcomeSample>,
    #[serde(default)]
    pub fold_window_ms: Option<i64>,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticPacketDnaReport {
    pub packet_id: String,
    pub semantic_type: String,
    pub novelty_score: f64,
    pub semantic_dependencies: Vec<String>,
    pub reconstruction_hint: String,
    pub semantic_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CognitiveLoadSheddingReport {
    pub stress_level: f64,
    pub shedding_level: String,
    pub disabled_features: Vec<String>,
    pub active_features: Vec<String>,
    pub queue_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FoldedRequestGroup {
    pub canonical_request_id: String,
    pub normalized_intent: String,
    pub folded_request_ids: Vec<String>,
    pub trust_profile: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemporalRequestFoldingReport {
    pub folded_groups: Vec<FoldedRequestGroup>,
    pub actual_requests_required: usize,
    pub requests_saved: usize,
    pub fold_window_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CausalityPreservingMultiplexingReport {
    pub delivery_order: Vec<String>,
    pub buffered_responses: Vec<String>,
    pub release_waves: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProtocolGenome {
    pub connect_timeout_ms: u32,
    pub read_timeout_ms: u32,
    pub retry_count: u8,
    pub retry_backoff_base_ms: u32,
    pub max_concurrent_streams: u16,
    pub initial_window_size: u32,
    pub header_compression_level: u8,
    pub keep_alive_interval_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtocolGeneTherapyReport {
    pub previous_genome: ProtocolGenome,
    pub evolved_genome: ProtocolGenome,
    pub fitness_score: f64,
    pub evolution_cycle: u64,
    pub narrative: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevolutionaryProtocolReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub semantic_packet_dna: Vec<SemanticPacketDnaReport>,
    pub cognitive_load_shedding: CognitiveLoadSheddingReport,
    pub temporal_request_folding: TemporalRequestFoldingReport,
    pub causality_preserving_multiplexing: CausalityPreservingMultiplexingReport,
    pub protocol_gene_therapy: ProtocolGeneTherapyReport,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CausalTransitionInput {
    pub transition_id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub subsystem: String,
    pub change: String,
    pub trigger: String,
    #[serde(default)]
    pub abnormality_score: f64,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FailureSymptomInput {
    pub transition_id: String,
    pub symptom: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SonarProbeInput {
    pub probe_id: String,
    pub target_subsystem: String,
    pub baseline_latency_ms: f64,
    pub current_latency_ms: f64,
    pub baseline_size_bytes: f64,
    pub current_size_bytes: f64,
    pub baseline_error_rate: f64,
    pub current_error_rate: f64,
    #[serde(default = "default_probe_stddev")]
    pub sigma: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RealityAssumptionInput {
    pub asset_id: String,
    pub statement: String,
    pub expected: f64,
    pub observed: f64,
    #[serde(default = "default_drift_tolerance")]
    pub tolerance: f64,
    #[serde(default = "default_compensation_kind")]
    pub compensation_kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlaObservationWindow {
    pub window_id: String,
    pub total_requests: u64,
    pub compliant_requests: u64,
    pub observed_latency_ms: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DependencyComponentInput {
    pub component_id: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub invariants: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProposedChangeInput {
    pub target: String,
    pub change_type: String,
    pub summary: String,
    #[serde(default = "default_change_severity")]
    pub severity: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnomalyEventInput {
    pub event_id: String,
    pub source: String,
    pub structural_score: f64,
    pub temporal_score: f64,
    pub semantic_score: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpeculativeOperationInput {
    pub operation_id: String,
    pub subsystem: String,
    #[serde(default)]
    pub predicted_error: Option<String>,
    #[serde(default)]
    pub known_repairs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevolutionaryResilienceRequest {
    #[serde(default)]
    pub transitions: Vec<CausalTransitionInput>,
    pub failure: FailureSymptomInput,
    #[serde(default)]
    pub sonar_probes: Vec<SonarProbeInput>,
    #[serde(default)]
    pub assumptions: Vec<RealityAssumptionInput>,
    #[serde(default)]
    pub sla_statements: Vec<String>,
    #[serde(default)]
    pub sla_windows: Vec<SlaObservationWindow>,
    #[serde(default)]
    pub components: Vec<DependencyComponentInput>,
    #[serde(default)]
    pub proposed_changes: Vec<ProposedChangeInput>,
    #[serde(default)]
    pub anomaly_events: Vec<AnomalyEventInput>,
    #[serde(default)]
    pub speculative_operations: Vec<SpeculativeOperationInput>,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailureArchaeologyReport {
    pub root_cause: Option<String>,
    pub causal_chain: Vec<String>,
    pub counterfactual: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientRegressionFinding {
    pub probe_id: String,
    pub target_subsystem: String,
    pub drift_score: f64,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmbientRegressionSonarReport {
    pub findings: Vec<AmbientRegressionFinding>,
    pub early_warning: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RealityDriftAction {
    pub asset_id: String,
    pub drift_ratio: f64,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompiledSlaReport {
    pub statement: String,
    pub predicate: String,
    pub current_compliance: f64,
    pub breach_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BreakagePrediction {
    pub component_id: String,
    pub violation: String,
    pub severity: String,
    pub suggested_mitigation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyBlastRadiusReport {
    pub target: String,
    pub total_affected: usize,
    pub safe_to_proceed: bool,
    pub predicted_breakages: Vec<BreakagePrediction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TriangulatedAnomaly {
    pub event_id: String,
    pub source: String,
    pub classification: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeculativeRepairPlan {
    pub operation_id: String,
    pub predicted_error: String,
    pub repair_action: String,
    pub apply_before_execution: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevolutionaryResilienceReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub failure_archaeology_engine: FailureArchaeologyReport,
    pub ambient_regression_sonar: AmbientRegressionSonarReport,
    pub reality_drift_compensator: Vec<RealityDriftAction>,
    pub semantic_sla_compiler: Vec<CompiledSlaReport>,
    pub dependency_phantom_graph: Vec<DependencyBlastRadiusReport>,
    pub anomaly_triangulation: Vec<TriangulatedAnomaly>,
    pub speculative_self_repair: Vec<SpeculativeRepairPlan>,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevolutionaryFeaturesStatus {
    pub total_reports: u64,
    pub sandbox_reports: u64,
    pub value_reports: u64,
    pub protocol_reports: u64,
    pub resilience_reports: u64,
    pub last_reported_at: i64,
    pub causal_sequence: u64,
    pub learned_heat_signatures: usize,
    pub current_protocol_genome: ProtocolGenome,
}

pub struct RevolutionaryFeaturesEngine {
    phantom_sandbox: PhantomSandbox,
    code_executor: CodeExecutor,
    total_reports: u64,
    sandbox_reports: u64,
    value_reports: u64,
    protocol_reports: u64,
    resilience_reports: u64,
    last_reported_at: i64,
    causal_sequence: u64,
    thermal_signatures: VecDeque<String>,
    protocol_genome: ProtocolGenome,
    evolution_cycle: u64,
}

impl Default for ProtocolGenome {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 400,
            read_timeout_ms: 2_000,
            retry_count: 2,
            retry_backoff_base_ms: 200,
            max_concurrent_streams: 32,
            initial_window_size: 256 * 1024,
            header_compression_level: 5,
            keep_alive_interval_ms: 15_000,
        }
    }
}

impl RevolutionaryFeaturesEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            phantom_sandbox: PhantomSandbox::new(),
            code_executor: CodeExecutor::new(),
            total_reports: 0,
            sandbox_reports: 0,
            value_reports: 0,
            protocol_reports: 0,
            resilience_reports: 0,
            last_reported_at: 0,
            causal_sequence: 0,
            thermal_signatures: VecDeque::new(),
            protocol_genome: ProtocolGenome::default(),
            evolution_cycle: 0,
        }
    }

    #[must_use]
    pub fn status(&self) -> RevolutionaryFeaturesStatus {
        RevolutionaryFeaturesStatus {
            total_reports: self.total_reports,
            sandbox_reports: self.sandbox_reports,
            value_reports: self.value_reports,
            protocol_reports: self.protocol_reports,
            resilience_reports: self.resilience_reports,
            last_reported_at: self.last_reported_at,
            causal_sequence: self.causal_sequence,
            learned_heat_signatures: self.thermal_signatures.len(),
            current_protocol_genome: self.protocol_genome,
        }
    }

    pub fn analyze_sandbox(
        &mut self,
        request: RevolutionarySandboxRequest,
    ) -> AstraResult<RevolutionarySandboxReport> {
        validate_sandbox_request(&request)?;
        let analyzed_at = now_ms();
        let language = inferred_language(request.language.as_deref(), &request.execution_target);
        let validation = self.code_executor.validate(&ExecutionRequest {
            code: request.execution_target.clone(),
            language,
            timeout: std::time::Duration::from_secs(5),
            max_memory_mb: 128,
            environment: request.environment.clone().into_iter().collect(),
        });
        let risk = self.phantom_sandbox.assess_risk(&request.execution_target);
        let calcification = calcify_intents(
            &request.execution_target,
            &request.allowed_paths,
            &request.allowed_hosts,
        );
        let thermodynamics = analyze_thermodynamics(
            &request.execution_target,
            &request.expected_outputs,
            &calcification,
            risk.score,
        );
        self.remember_heat_signature(thermodynamics.signature.clone());
        let firebreak = build_firebreak_report(
            &calcification,
            request.require_firebreaks,
            &request.objective,
        );
        let entropic_execution_cage = build_entropic_cage_report(
            &request,
            &validation,
            &risk.level,
            thermodynamics.freeze_execution,
        );

        let mut reasons = vec![risk.recommendation.clone()];
        reasons.extend(calcification.dangerous_taxa.iter().cloned());
        reasons.extend(thermodynamics.findings.iter().cloned());
        reasons.extend(firebreak.narrative.iter().cloned());
        if let ValidationResult::Rejected(message) = &validation {
            reasons.push(message.clone());
        }

        let allow_real_execution = entropic_execution_cage.allow_real_execution
            && risk.allow_execution
            && !thermodynamics.freeze_execution
            && !matches!(validation, ValidationResult::Rejected(_));

        let report_id = hash_prefix(
            format!(
                "revolutionary:sandbox:{}:{}:{}",
                request.execution_target, request.objective, analyzed_at
            )
            .as_bytes(),
        );
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "calcification": calcification,
                "thermodynamics": thermodynamics,
                "risk_level": risk.level.as_str(),
            })
            .to_string()
            .as_bytes(),
        );
        let verdict = RevolutionarySandboxVerdict {
            risk_level: risk.level.as_str().to_string(),
            allow_simulation: true,
            allow_real_execution,
            reasons,
        };

        self.record_report("sandbox", analyzed_at);
        Ok(RevolutionarySandboxReport {
            report_id,
            manifest_hash,
            entropic_execution_cage,
            semantic_opcode_calcification: calcification,
            causal_firebreak_architecture: firebreak,
            phantom_thermodynamics: thermodynamics,
            verdict,
            analyzed_at,
        })
    }

    pub fn orchestrate_value(
        &mut self,
        request: RevolutionaryValueRequest,
        chain: &mut Chain,
    ) -> AstraResult<RevolutionaryValueReport> {
        validate_value_request(&request)?;
        let analyzed_at = now_ms();
        let providers = normalized_value_providers(
            &request.providers,
            request.settlement_rail,
            request.compliance,
        );
        let route_request = FinancialIntentRouteRequest {
            consumer: request.sender.clone(),
            market: "revolutionary_value_fabric".into(),
            objective: request.purpose.clone(),
            max_budget: round_currency(request.amount * 1.05),
            settlement_rail: request.settlement_rail,
            connector_id: request.connector_id.clone(),
            compliance: request.compliance,
            risk_tier: request.risk_tier,
            providers: providers.clone(),
            metadata: request.metadata.clone(),
        };
        let route = chain.route_financial_intent(&route_request)?;
        let purpose_hash = sha3_256_hex(request.purpose.as_bytes());
        self.causal_sequence = self.causal_sequence.saturating_add(1);
        let chain_info = chain.get_chain_info();

        let intention_state_currency = IntentionStateCurrencyReport {
            seal_id: hash_prefix(
                format!(
                    "{}:{}:{:.6}:{}",
                    request.sender, request.receiver, request.amount, request.purpose
                )
                .as_bytes(),
            ),
            amount: round_currency(request.amount),
            currency: request.currency.clone(),
            purpose_hash: purpose_hash.clone(),
            purpose_chain: vec![
                CausalValueStep {
                    stage: "need".into(),
                    detail: request.purpose.clone(),
                },
                CausalValueStep {
                    stage: "authorize".into(),
                    detail: format!(
                        "{} authorizes {} on {:?}",
                        request.sender, request.receiver, request.settlement_rail
                    ),
                },
                CausalValueStep {
                    stage: "fulfill".into(),
                    detail: format!(
                        "quality threshold {:.3} within {}s",
                        request.min_quality_score, request.fulfillment_window_seconds
                    ),
                },
            ],
            reversal_conditions: vec![
                format!(
                    "timeout_unfulfilled: {} seconds",
                    request.fulfillment_window_seconds
                ),
                format!(
                    "quality_breach: minimum score {:.3}",
                    request.min_quality_score
                ),
                "manipulation_detected".into(),
            ],
            behavioral_proof_hash: sha3_256_hex(
                serde_json::json!({
                    "sender": request.sender,
                    "receiver": request.receiver,
                    "purpose": request.purpose,
                    "evidence": request.evidence,
                })
                .to_string()
                .as_bytes(),
            ),
            manipulation_resistance: "dual-sealed intent derived from purpose, route, and evidence"
                .into(),
        };

        let mut causal_parents = vec![chain_info.latest_hash.clone(), purpose_hash];
        causal_parents.extend(
            request
                .evidence
                .iter()
                .map(|item| hash_prefix(item.as_bytes()))
                .take(6),
        );
        let causal_ledger_crystallography = CausalLedgerCrystallographyReport {
            sequence: self.causal_sequence,
            uniqueness_witness: sha3_256_hex(
                format!(
                    "{}:{}:{}",
                    self.causal_sequence, chain_info.latest_hash, intention_state_currency.seal_id
                )
                .as_bytes(),
            ),
            causal_parents,
            chain_height: chain_info.height,
            latest_chain_hash: chain_info.latest_hash,
            instant_finality_path: "single-sovereign monotonic sequence anchored to ledger history"
                .into(),
        };

        let split_plan = build_metabolic_split_plan(
            request.amount,
            &route,
            &providers,
            request.allow_split_routing,
        );
        let metabolic_transaction_routing = MetabolicTransactionRoutingReport {
            selected_provider: route.selected_provider.provider_id.clone(),
            coagulation_strategy:
                "partial failures keep sealed intent and reroute remaining value fragments".into(),
            optimization_score: route.selected_provider.score.clamp(0.0, 1.0),
            route: route.clone(),
            split_plan,
        };

        let unlock_parties = if request.cooperative_unlock_parties.is_empty() {
            vec![request.sender.clone(), request.receiver.clone()]
        } else {
            request.cooperative_unlock_parties.clone()
        };
        let entropic_escrow = EntropicEscrowReport {
            escrow_id: hash_prefix(
                format!(
                    "{}:{}:{}",
                    intention_state_currency.seal_id,
                    request.fulfillment_window_seconds,
                    analyzed_at
                )
                .as_bytes(),
            ),
            puzzle_hash: sha3_256_hex(
                format!(
                    "{}:{}:{}",
                    request.sender, request.receiver, request.fulfillment_window_seconds
                )
                .as_bytes(),
            ),
            unlock_after_seconds: request.fulfillment_window_seconds,
            cooperative_unlock_threshold: unlock_parties.len().clamp(2, 3) as u8,
            cooperative_unlock_parties: unlock_parties.clone(),
            release_policy: vec![
                "release on successful guarded-payment approval".into(),
                "refund on timeout or quality breach".into(),
            ],
            early_unlock_supported: unlock_parties.len() >= 2,
        };

        let guarded_payment = chain.settle_guarded_payment(GuardedPaymentRequest {
            sender: request.sender.clone(),
            receiver: request.receiver.clone(),
            amount: round_currency(request.amount),
            settlement_rail: request.settlement_rail,
            connector_id: request.connector_id.clone(),
            compliance: request.compliance,
            privacy_mode: Some(request.privacy_mode),
            // Let the chain deploy or reuse the correct guarded-payment contract.
            contract_id: None,
            payment_kind: if request.require_escrow {
                SmartPaymentKind::MilestoneRelease
            } else {
                SmartPaymentKind::PeerTransfer
            },
            trace_id: Some(format!("revolutionary-value-{}", self.causal_sequence)),
            session_id: Some(format!("rev-seq-{}", self.causal_sequence)),
            confidence: Some(route.selected_provider.score.clamp(0.0, 1.0)),
            risk_score: Some(risk_tier_score(request.risk_tier)),
            evidence: request.evidence.clone(),
            context: BTreeMap::from([
                ("purpose".into(), request.purpose.clone()),
                ("currency".into(), request.currency.clone()),
                ("escrow_required".into(), request.require_escrow.to_string()),
            ]),
            metadata: Some(serde_json::json!({
                "intention_state_currency": intention_state_currency.clone(),
                "causal_ledger_crystallography": causal_ledger_crystallography.clone(),
                "entropic_escrow": entropic_escrow.clone(),
                "user_metadata": request.metadata,
            })),
            autocommit: request.autocommit,
            fail_closed_on_review: true,
        })?;

        let report_id = hash_prefix(
            format!(
                "revolutionary:value:{}:{}:{}",
                request.sender, request.receiver, analyzed_at
            )
            .as_bytes(),
        );
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "seal_id": intention_state_currency.seal_id,
                "sequence": causal_ledger_crystallography.sequence,
                "selected_provider": metabolic_transaction_routing.selected_provider,
                "guard_receipt": guarded_payment.guard_receipt.receipt_id,
            })
            .to_string()
            .as_bytes(),
        );

        self.record_report("value", analyzed_at);
        Ok(RevolutionaryValueReport {
            report_id,
            manifest_hash,
            intention_state_currency,
            causal_ledger_crystallography,
            metabolic_transaction_routing,
            entropic_escrow,
            guarded_payment,
            analyzed_at,
        })
    }

    pub fn orchestrate_protocol(
        &mut self,
        request: RevolutionaryProtocolRequest,
    ) -> AstraResult<RevolutionaryProtocolReport> {
        let analyzed_at = now_ms();
        let semantic_packet_dna = build_semantic_packet_dna(&request.packets);
        let cognitive_load_shedding =
            build_load_shedding_report(&request.system_load, &request.requests);
        let temporal_request_folding =
            build_temporal_folding(&request.requests, request.fold_window_ms);
        let causality_preserving_multiplexing = build_cpm(&request.requests);
        let protocol_gene_therapy = self.evolve_protocol_gene_therapy(&request.outcome_samples);
        let report_id = hash_prefix(
            format!(
                "revolutionary:protocol:{}:{}",
                request.requests.len(),
                analyzed_at
            )
            .as_bytes(),
        );
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "packet_count": semantic_packet_dna.len(),
                "fold_saved": temporal_request_folding.requests_saved,
                "shedding_level": cognitive_load_shedding.shedding_level,
                "evolution_cycle": protocol_gene_therapy.evolution_cycle,
            })
            .to_string()
            .as_bytes(),
        );

        self.record_report("protocol", analyzed_at);
        Ok(RevolutionaryProtocolReport {
            report_id,
            manifest_hash,
            semantic_packet_dna,
            cognitive_load_shedding,
            temporal_request_folding,
            causality_preserving_multiplexing,
            protocol_gene_therapy,
            analyzed_at,
        })
    }

    pub fn orchestrate_resilience(
        &mut self,
        request: RevolutionaryResilienceRequest,
    ) -> AstraResult<RevolutionaryResilienceReport> {
        if request.failure.transition_id.trim().is_empty() {
            return Err(AstraError::ControlPlaneRejected(
                "resilience analysis requires a failure transition id".into(),
            ));
        }

        let analyzed_at = now_ms();
        let failure_archaeology_engine = excavate_failure(&request.transitions, &request.failure);
        let ambient_regression_sonar = run_sonar(&request.sonar_probes);
        let reality_drift_compensator = compensate_reality_drift(&request.assumptions);
        let semantic_sla_compiler = compile_slas(&request.sla_statements, &request.sla_windows);
        let dependency_phantom_graph =
            simulate_dependency_blast_radius(&request.components, &request.proposed_changes);
        let anomaly_triangulation = triangulate_anomalies(&request.anomaly_events);
        let speculative_self_repair =
            plan_speculative_repairs(&request.speculative_operations, &failure_archaeology_engine);
        let report_id = hash_prefix(
            format!(
                "revolutionary:resilience:{}:{}",
                request.failure.transition_id, analyzed_at
            )
            .as_bytes(),
        );
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "report_id": report_id,
                "root_cause": failure_archaeology_engine.root_cause,
                "sonar_findings": ambient_regression_sonar.findings.len(),
                "anomalies": anomaly_triangulation.len(),
                "repairs": speculative_self_repair.len(),
            })
            .to_string()
            .as_bytes(),
        );

        self.record_report("resilience", analyzed_at);
        Ok(RevolutionaryResilienceReport {
            report_id,
            manifest_hash,
            failure_archaeology_engine,
            ambient_regression_sonar,
            reality_drift_compensator,
            semantic_sla_compiler,
            dependency_phantom_graph,
            anomaly_triangulation,
            speculative_self_repair,
            analyzed_at,
        })
    }

    fn remember_heat_signature(&mut self, signature: String) {
        self.thermal_signatures.push_back(signature);
        while self.thermal_signatures.len() > MAX_HEAT_SIGNATURES {
            self.thermal_signatures.pop_front();
        }
    }

    fn record_report(&mut self, family: &str, timestamp: i64) {
        self.total_reports = self.total_reports.saturating_add(1);
        self.last_reported_at = timestamp;
        match family {
            "sandbox" => self.sandbox_reports = self.sandbox_reports.saturating_add(1),
            "value" => self.value_reports = self.value_reports.saturating_add(1),
            "protocol" => self.protocol_reports = self.protocol_reports.saturating_add(1),
            "resilience" => self.resilience_reports = self.resilience_reports.saturating_add(1),
            _ => {}
        }
    }

    fn evolve_protocol_gene_therapy(
        &mut self,
        samples: &[ProtocolOutcomeSample],
    ) -> ProtocolGeneTherapyReport {
        let previous_genome = self.protocol_genome;
        if samples.is_empty() {
            return ProtocolGeneTherapyReport {
                previous_genome,
                evolved_genome: previous_genome,
                fitness_score: 0.0,
                evolution_cycle: self.evolution_cycle,
                narrative: vec!["no outcome samples provided; baseline genome retained".into()],
            };
        }

        let avg_latency = samples
            .iter()
            .map(|sample| sample.latency_ms as f64)
            .sum::<f64>()
            / samples.len() as f64;
        let success_rate =
            samples.iter().filter(|sample| sample.success).count() as f64 / samples.len() as f64;
        let avg_efficiency = samples
            .iter()
            .map(|sample| {
                if sample.total_bytes == 0 {
                    0.0
                } else {
                    sample.useful_bytes as f64 / sample.total_bytes as f64
                }
            })
            .sum::<f64>()
            / samples.len() as f64;
        let avg_recovery = samples
            .iter()
            .map(|sample| sample.recovered_errors as f64)
            .sum::<f64>()
            / samples.len() as f64;
        let fitness_score = (success_rate * 0.55)
            + (avg_efficiency * 0.2)
            + latency_fitness(avg_latency) * 0.2
            + (avg_recovery / 5.0).clamp(0.0, 1.0) * 0.05;

        let evolved_genome = ProtocolGenome {
            connect_timeout_ms: clamp_u32((avg_latency * 1.2) as u32, 100, 5_000),
            read_timeout_ms: clamp_u32((avg_latency * 3.0) as u32, 300, 30_000),
            retry_count: if success_rate < 0.85 { 3 } else { 1 },
            retry_backoff_base_ms: clamp_u32((avg_latency * 0.6) as u32, 50, 5_000),
            max_concurrent_streams: clamp_u16(if avg_efficiency > 0.8 { 64 } else { 24 }, 1, 256),
            initial_window_size: clamp_u32(
                (samples.iter().map(|sample| sample.total_bytes).sum::<u32>()
                    / samples.len() as u32)
                    .max(16 * 1024),
                16 * 1024,
                16 * 1024 * 1024,
            ),
            header_compression_level: clamp_u8(if avg_efficiency > 0.75 { 7 } else { 4 }, 0, 9),
            keep_alive_interval_ms: clamp_u32((avg_latency * 12.0) as u32, 1_000, 60_000),
        };
        self.protocol_genome = evolved_genome;
        self.evolution_cycle = self.evolution_cycle.saturating_add(samples.len() as u64);

        ProtocolGeneTherapyReport {
            previous_genome,
            evolved_genome,
            fitness_score,
            evolution_cycle: self.evolution_cycle,
            narrative: vec![
                format!("success rate stabilized at {:.2}%", success_rate * 100.0),
                format!("byte efficiency converged to {:.2}", avg_efficiency),
                format!("latency baseline moved to {:.0}ms", avg_latency),
            ],
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn hash_prefix(input: &[u8]) -> String {
    sha3_256_hex(input)[..24].to_string()
}

fn round_currency(value: f64) -> f64 {
    ((value * 100.0).round()) / 100.0
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_currency() -> String {
    "USD".into()
}

fn default_settlement_rail() -> SettlementRail {
    SettlementRail::InternalLedger
}

fn default_compliance_mode() -> ComplianceMode {
    ComplianceMode::TaxAware
}

fn default_privacy_mode() -> PrivacyPreservationMode {
    PrivacyPreservationMode::AttestedMinimization
}

fn default_risk_tier() -> EnterpriseRiskTier {
    EnterpriseRiskTier::Standard
}

fn default_fulfillment_window_seconds() -> u64 {
    24 * 60 * 60
}

fn default_min_quality_score() -> f64 {
    0.9
}

fn default_request_priority() -> String {
    "normal".into()
}

fn default_probe_stddev() -> f64 {
    0.1
}

fn default_drift_tolerance() -> f64 {
    0.15
}

fn default_compensation_kind() -> String {
    "adjust_timeout".into()
}

fn default_change_severity() -> String {
    "medium".into()
}

fn inferred_language(language: Option<&str>, execution_target: &str) -> Language {
    match language
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "python" | "py" => Language::Python,
        "javascript" | "js" | "node" => Language::JavaScript,
        "shell" | "bash" | "powershell" | "pwsh" => Language::Shell,
        "sql" => Language::Sql,
        "rust" | "rs" => Language::Rust,
        _ if execution_target.contains("fn main") || execution_target.contains("cargo") => {
            Language::Rust
        }
        _ if execution_target.contains("import ") || execution_target.contains("def ") => {
            Language::Python
        }
        _ if execution_target.contains("SELECT ") || execution_target.contains("select ") => {
            Language::Sql
        }
        _ => Language::Shell,
    }
}

fn validate_sandbox_request(request: &RevolutionarySandboxRequest) -> AstraResult<()> {
    if request.execution_target.trim().is_empty() {
        return Err(AstraError::ControlPlaneRejected(
            "sandbox analysis requires an execution target".into(),
        ));
    }
    if request.execution_target.len() > MAX_SANDBOX_INPUT_BYTES {
        return Err(AstraError::ControlPlaneRejected(format!(
            "sandbox analysis input exceeds {} bytes",
            MAX_SANDBOX_INPUT_BYTES
        )));
    }
    Ok(())
}

fn validate_value_request(request: &RevolutionaryValueRequest) -> AstraResult<()> {
    if request.sender.trim().is_empty() || request.receiver.trim().is_empty() {
        return Err(AstraError::InvalidTransaction(
            "value orchestration requires sender and receiver".into(),
        ));
    }
    if request.sender == request.receiver {
        return Err(AstraError::InvalidTransaction(
            "sender and receiver must differ".into(),
        ));
    }
    if request.purpose.trim().is_empty() {
        return Err(AstraError::InvalidTransaction(
            "value orchestration requires a purpose".into(),
        ));
    }
    if !request.amount.is_finite() || request.amount <= 0.0 {
        return Err(AstraError::InvalidTransaction(
            "value amount must be a positive finite number".into(),
        ));
    }
    Ok(())
}

fn build_entropic_cage_report(
    request: &RevolutionarySandboxRequest,
    validation: &ValidationResult,
    risk_level: &RiskLevel,
    thermal_freeze: bool,
) -> EntropicExecutionCageReport {
    let projection_id = hash_prefix(
        format!(
            "{}:{}:{}",
            request.execution_target,
            request.objective,
            request.allowed_hosts.join("|")
        )
        .as_bytes(),
    );
    let allow_real_execution = !thermal_freeze
        && !matches!(validation, ValidationResult::Rejected(_))
        && matches!(risk_level, RiskLevel::Safe | RiskLevel::Low);

    EntropicExecutionCageReport {
        projection_id: projection_id.clone(),
        synthetic_filesystem_roots: if request.allowed_paths.is_empty() {
            vec![
                "/phantom/workspace".into(),
                "/phantom/tmp".into(),
                "/phantom/proc".into(),
            ]
        } else {
            request.allowed_paths.clone()
        },
        synthetic_network_hosts: if request.allowed_hosts.is_empty() {
            vec!["loopback-only".into()]
        } else {
            request.allowed_hosts.clone()
        },
        deterministic_clock_epoch_ms: 1_730_000_000_000 + projection_id.len() as i64,
        deterministic_rng_seed_hash: sha3_256_hex(
            format!("rng:{projection_id}:{}", request.objective).as_bytes(),
        ),
        trap_categories: vec![
            "filesystem".into(),
            "network".into(),
            "process".into(),
            "memory".into(),
            "clock".into(),
        ],
        environment_projection_count: request.environment.len(),
        allow_real_execution,
        isolation_guarantee:
            "real side effects are replaced with deterministic synthetic projections".into(),
    }
}

fn calcify_intents(
    execution_target: &str,
    allowed_paths: &[String],
    allowed_hosts: &[String],
) -> CalcifiedIntentGraph {
    let source = execution_target.to_ascii_lowercase();
    let mut nodes = Vec::new();
    let mut dangerous_taxa = BTreeSet::new();

    for (intent, keywords, scope, severity) in [
        (
            "file_delete",
            &[
                "rm -rf",
                "remove-item",
                "unlink",
                "rmtree",
                "del /f /s /q",
                "delete",
            ][..],
            infer_path_scope(&source, allowed_paths),
            infer_severity(
                &source,
                &["rm -rf /", "del /f /s /q", "remove-item -recurse"],
            ),
        ),
        (
            "file_write",
            &[
                "write",
                "touch",
                "tee",
                "copy-item",
                "move-item",
                "set-content",
            ][..],
            infer_path_scope(&source, allowed_paths),
            "medium".into(),
        ),
        (
            "file_read",
            &["cat ", "get-content", "read_to_string", "open("][..],
            infer_path_scope(&source, allowed_paths),
            "low".into(),
        ),
        (
            "net_connect",
            &[
                "http://",
                "https://",
                "curl ",
                "wget ",
                "requests.",
                "invoke-webrequest",
            ][..],
            infer_host_scope(&source, allowed_hosts),
            "medium".into(),
        ),
        (
            "net_listen",
            &["listen(", "bind(", "http.server", "0.0.0.0", "serve("][..],
            "listener".into(),
            "high".into(),
        ),
        (
            "process_spawn",
            &[
                "subprocess",
                "cmd /c",
                "powershell",
                "spawn(",
                "process::command",
            ][..],
            "process".into(),
            "high".into(),
        ),
        (
            "process_signal",
            &["kill(", "stop-process", "taskkill", "terminate("][..],
            "process".into(),
            "high".into(),
        ),
        (
            "memory_map",
            &["mmap", "virtualalloc", "mapview", "shared_memory"][..],
            "memory".into(),
            "medium".into(),
        ),
        (
            "privilege_escalate",
            &["sudo", "runas", "chmod 777", "setuid", "takeown", "icacls"][..],
            "privilege".into(),
            "critical".into(),
        ),
    ] {
        if let Some(evidence) = keywords
            .iter()
            .find(|keyword| source.contains(&keyword.to_ascii_lowercase()))
        {
            let node_id = format!("intent-{}", nodes.len() + 1);
            nodes.push(CalcifiedIntentNode {
                node_id,
                atomic_intent: intent.into(),
                scope,
                evidence: (*evidence).into(),
                severity,
            });
        }
    }

    for node in &nodes {
        if node.atomic_intent == "file_delete" && node.scope == "root" {
            dangerous_taxa.insert("catastrophic_destruction".into());
        }
        if node.atomic_intent == "privilege_escalate" {
            dangerous_taxa.insert("privilege_escalation".into());
        }
        if node.atomic_intent == "net_connect" && node.scope == "external" {
            dangerous_taxa.insert("unbounded_egress".into());
        }
        if node.atomic_intent == "memory_map" {
            dangerous_taxa.insert("memory_surface_expansion".into());
        }
    }

    let edges = nodes
        .windows(2)
        .map(|window| CalcifiedIntentEdge {
            from: window[0].node_id.clone(),
            to: window[1].node_id.clone(),
            relationship: "precedes".into(),
        })
        .collect::<Vec<_>>();

    let canonical_summary = if nodes.is_empty() {
        vec!["no privileged or side-effectful atomic intents detected".into()]
    } else {
        nodes
            .iter()
            .map(|node| format!("{}@{}", node.atomic_intent, node.scope))
            .collect()
    };

    CalcifiedIntentGraph {
        nodes,
        edges,
        canonical_summary,
        dangerous_taxa: dangerous_taxa.into_iter().collect(),
    }
}

fn infer_path_scope(source: &str, allowed_paths: &[String]) -> String {
    if source.contains(" /") || source.contains("c:\\") || source.contains("%systemdrive%") {
        "root".into()
    } else if !allowed_paths.is_empty() {
        "scoped".into()
    } else {
        "workspace".into()
    }
}

fn infer_host_scope(source: &str, allowed_hosts: &[String]) -> String {
    if source.contains("127.0.0.1") || source.contains("localhost") {
        "loopback".into()
    } else if !allowed_hosts.is_empty() {
        "allowlisted".into()
    } else {
        "external".into()
    }
}

fn infer_severity(source: &str, critical_markers: &[&str]) -> String {
    if critical_markers
        .iter()
        .any(|marker| source.contains(marker))
    {
        "critical".into()
    } else {
        "high".into()
    }
}

fn analyze_thermodynamics(
    execution_target: &str,
    expected_outputs: &[String],
    calcification: &CalcifiedIntentGraph,
    base_risk: f64,
) -> PhantomThermodynamicsReport {
    let lower = execution_target.to_ascii_lowercase();
    let exploratory_markers = [
        "while true",
        "for(;;)",
        "sleep(",
        "retry",
        "probe",
        "scan",
        "mmap",
        "chmod 777",
        "sudo",
    ];
    let exploratory_hits = exploratory_markers
        .iter()
        .filter(|marker| lower.contains(**marker))
        .count()
        + calcification
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.atomic_intent.as_str(),
                    "memory_map" | "privilege_escalate" | "net_listen"
                )
            })
            .count();
    let productive_tokens = expected_outputs.len().max(1)
        + calcification
            .nodes
            .iter()
            .filter(|node| matches!(node.atomic_intent.as_str(), "file_read" | "file_write"))
            .count();
    let total = productive_tokens + exploratory_hits;
    let productive_ratio = productive_tokens as f64 / total.max(1) as f64;
    let exploratory_ratio = exploratory_hits as f64 / total.max(1) as f64;
    let heat_score = (exploratory_ratio * 0.7 + base_risk * 0.3).clamp(0.0, 1.0);

    PhantomThermodynamicsReport {
        productive_ratio,
        exploratory_ratio,
        heat_score,
        freeze_execution: heat_score >= 0.58,
        signature: if heat_score >= 0.75 {
            "escape_probe_signature".into()
        } else if heat_score >= 0.4 {
            "unstable_entropy_signature".into()
        } else {
            "productive_signature".into()
        },
        findings: vec![
            format!("productive_ratio={productive_ratio:.3}"),
            format!("exploratory_ratio={exploratory_ratio:.3}"),
            format!("heat_score={heat_score:.3}"),
        ],
    }
}

fn build_firebreak_report(
    calcification: &CalcifiedIntentGraph,
    require_firebreaks: bool,
    objective: &str,
) -> CausalFirebreakReport {
    let mut required_gates = Vec::new();
    if calcification
        .nodes
        .iter()
        .any(|node| node.atomic_intent == "file_delete")
    {
        required_gates.push(FirebreakGate {
            gate: "offline_backup_reconciliation".into(),
            rationale: "destructive file operations require a disconnected recovery path".into(),
            satisfied_by: "backup manifest + operator review".into(),
        });
    }
    if objective.to_ascii_lowercase().contains("pay")
        || calcification
            .dangerous_taxa
            .iter()
            .any(|item| item.contains("egress"))
    {
        required_gates.push(FirebreakGate {
            gate: "hardware_token_gate".into(),
            rationale: "financial or externally propagating actions require a separate trust root"
                .into(),
            satisfied_by: "hardware authenticator".into(),
        });
    }
    if calcification
        .nodes
        .iter()
        .any(|node| node.atomic_intent == "process_spawn" || node.atomic_intent == "net_listen")
    {
        required_gates.push(FirebreakGate {
            gate: "external_network_gate".into(),
            rationale: "propagation-capable actions require an out-of-band network allowlist"
                .into(),
            satisfied_by: "router allowlist and policy bundle".into(),
        });
    }
    if calcification
        .nodes
        .iter()
        .any(|node| node.atomic_intent == "privilege_escalate")
    {
        required_gates.push(FirebreakGate {
            gate: "hsm_signature_gate".into(),
            rationale: "self-modifying or privileged operations require notarized release control"
                .into(),
            satisfied_by: "HSM-backed release signature".into(),
        });
    }

    let causal_gap_score = if required_gates.is_empty() {
        0.2
    } else {
        (required_gates.len() as f64 / 4.0).clamp(0.0, 1.0)
    };
    let firebreak_ready = !require_firebreaks || !required_gates.is_empty();
    let narrative = if required_gates.is_empty() {
        vec!["no independent firebreaks were required for the observed intent surface".into()]
    } else {
        required_gates
            .iter()
            .map(|gate| format!("{} enforces {}", gate.gate, gate.rationale))
            .collect()
    };

    CausalFirebreakReport {
        required_gates,
        causal_gap_score,
        firebreak_ready,
        narrative,
    }
}

fn normalized_value_providers(
    providers: &[MarketProviderQuote],
    rail: SettlementRail,
    compliance: ComplianceMode,
) -> Vec<MarketProviderQuote> {
    if !providers.is_empty() {
        return providers.to_vec();
    }

    vec![
        MarketProviderQuote {
            provider_id: "internal-ledger-primary".into(),
            quoted_price: 0.02,
            eta_minutes: 1,
            reputation_score: 0.99,
            collateral_ratio: 1.0,
            distance_km: 0.0,
            capacity_score: 0.95,
            capabilities: vec![format!("{rail:?}"), format!("{compliance:?}")],
        },
        MarketProviderQuote {
            provider_id: "attested-fallback-rail".into(),
            quoted_price: 0.05,
            eta_minutes: 3,
            reputation_score: 0.96,
            collateral_ratio: 0.9,
            distance_km: 30.0,
            capacity_score: 0.85,
            capabilities: vec!["fallback".into(), format!("{rail:?}")],
        },
    ]
}

fn build_metabolic_split_plan(
    amount: f64,
    route: &FinancialIntentRoute,
    providers: &[MarketProviderQuote],
    allow_split_routing: bool,
) -> Vec<MetabolicRouteLeg> {
    if !allow_split_routing || providers.len() < 2 {
        return vec![MetabolicRouteLeg {
            vessel_id: route.selected_provider.provider_id.clone(),
            allocated_amount: round_currency(amount),
            score: route.selected_provider.score,
            eta_minutes: route.selected_provider.eta_minutes,
            fee_estimate: round_currency(route.selected_provider.quoted_price),
            rationale: "single-vessel route selected by dominant fitness score".into(),
        }];
    }

    let mut ranked = route.alternatives.clone();
    ranked.insert(0, route.selected_provider.clone());
    ranked.sort_by(|left, right| right.score.total_cmp(&left.score));
    let top = ranked.into_iter().take(2).collect::<Vec<_>>();
    let first_amount = round_currency(amount * 0.65);
    let second_amount = round_currency(amount - first_amount);

    top.into_iter()
        .enumerate()
        .map(|(index, provider)| MetabolicRouteLeg {
            vessel_id: provider.provider_id,
            allocated_amount: if index == 0 {
                first_amount
            } else {
                second_amount
            },
            score: provider.score,
            eta_minutes: provider.eta_minutes,
            fee_estimate: round_currency(provider.quoted_price),
            rationale: if index == 0 {
                "primary vessel handles the majority path".into()
            } else {
                "secondary vessel absorbs overflow for resilience".into()
            },
        })
        .collect()
}

fn risk_tier_score(risk_tier: EnterpriseRiskTier) -> f64 {
    match risk_tier {
        EnterpriseRiskTier::Low => 0.2,
        EnterpriseRiskTier::Standard => 0.4,
        EnterpriseRiskTier::High => 0.7,
        EnterpriseRiskTier::Critical => 0.9,
    }
}

fn build_semantic_packet_dna(packets: &[SemanticPacketInput]) -> Vec<SemanticPacketDnaReport> {
    packets
        .iter()
        .enumerate()
        .map(|(index, packet)| {
            let semantic_refs = packets
                .iter()
                .take(index)
                .filter_map(|prior| {
                    let similarity = jaccard_similarity(&packet.content, &prior.content);
                    if similarity >= 0.45 {
                        Some(prior.packet_id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let novelty_score =
                (1.0 - semantic_refs.len() as f64 / (index.max(1) as f64)).clamp(0.1, 1.0);
            SemanticPacketDnaReport {
                packet_id: packet.packet_id.clone(),
                semantic_type: packet
                    .semantic_type
                    .clone()
                    .unwrap_or_else(|| infer_semantic_type(&packet.content)),
                novelty_score,
                semantic_dependencies: packet.semantic_dependencies.clone(),
                reconstruction_hint: if novelty_score < 0.35 && !semantic_refs.is_empty() {
                    format!("exact_derivation_from:{}", semantic_refs.join(","))
                } else if novelty_score < 0.65 && !semantic_refs.is_empty() {
                    format!("approximate_derivation_from:{}", semantic_refs.join(","))
                } else {
                    "irreplaceable".into()
                },
                semantic_refs,
            }
        })
        .collect()
}

fn infer_semantic_type(content: &str) -> String {
    let lower = content.to_ascii_lowercase();
    if lower.contains("step") || lower.contains("how to") {
        "procedural".into()
    } else if lower.contains('?') || lower.contains("where") || lower.contains("go to") {
        "navigational".into()
    } else if lower.contains("feel") || lower.contains("urgent") {
        "emotional".into()
    } else {
        "factual".into()
    }
}

fn jaccard_similarity(left: &str, right: &str) -> f64 {
    let left = token_set(left);
    let right = token_set(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(&right).count() as f64;
    let union = left.union(&right).count() as f64;
    (intersection / union).clamp(0.0, 1.0)
}

fn token_set(value: &str) -> BTreeSet<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.len() > 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn build_load_shedding_report(
    system_load: &ProtocolSystemLoad,
    requests: &[ProtocolRequestInput],
) -> CognitiveLoadSheddingReport {
    let queue_pressure = (system_load.queue_depth as f64 / 100.0).clamp(0.0, 1.0);
    let stress_level = ((system_load.cpu_utilization
        + system_load.memory_pressure
        + system_load.network_congestion
        + queue_pressure)
        / 4.0)
        .clamp(0.0, 1.0);
    let shedding_level = if stress_level < 0.2 {
        "full_cognition"
    } else if stress_level < 0.4 {
        "reduced_cognition"
    } else if stress_level < 0.6 {
        "cached_cognition"
    } else if stress_level < 0.8 {
        "deferred_cognition"
    } else {
        "survival_mode"
    };
    let disabled_features = match shedding_level {
        "full_cognition" => vec![],
        "reduced_cognition" => vec!["explain_plan".into(), "speculative_validation".into()],
        "cached_cognition" => vec!["swarm_consensus".into(), "live_semantic_bootstrap".into()],
        "deferred_cognition" => vec!["background_learning".into(), "noncritical_routing".into()],
        _ => vec![
            "new_pattern_learning".into(),
            "live_reasoning".into(),
            "non_pre_authorized_requests".into(),
        ],
    };
    let active_features = match shedding_level {
        "survival_mode" => vec!["cached_authorized_patterns".into(), "chain_receipts".into()],
        _ => vec![
            "core_intent_processing".into(),
            "chain_receipts".into(),
            "priority_execution".into(),
        ],
    };
    let critical_waiters = requests
        .iter()
        .filter(|request| request.priority == "critical" || request.priority == "high")
        .count();

    CognitiveLoadSheddingReport {
        stress_level,
        shedding_level: shedding_level.into(),
        disabled_features,
        active_features,
        queue_policy: format!(
            "{critical_waiters} high-priority requests remain eligible for realtime dispatch"
        ),
    }
}

fn build_temporal_folding(
    requests: &[ProtocolRequestInput],
    fold_window_ms: Option<i64>,
) -> TemporalRequestFoldingReport {
    let fold_window_ms = fold_window_ms.unwrap_or(DEFAULT_FOLD_WINDOW_MS).max(1);
    let mut sorted = requests.to_vec();
    sorted.sort_by_key(|request| request.submitted_at_ms);

    let mut groups: Vec<(String, String, i64, Vec<String>)> = Vec::new();
    for request in &sorted {
        let normalized = normalize_intent(&request.intent);
        if let Some(group) = groups.iter_mut().find(|group| {
            group.0 == normalized
                && group.1 == request.trust_profile
                && request.submitted_at_ms - group.2 <= fold_window_ms
        }) {
            group.3.push(request.request_id.clone());
        } else {
            groups.push((
                normalized,
                request.trust_profile.clone(),
                request.submitted_at_ms,
                vec![request.request_id.clone()],
            ));
        }
    }

    let actual_requests_required = groups.len();
    let requests_saved = requests.len().saturating_sub(actual_requests_required);
    let folded_groups = groups
        .into_iter()
        .filter(|group| group.3.len() > 1)
        .map(|group| FoldedRequestGroup {
            canonical_request_id: group.3[0].clone(),
            normalized_intent: group.0,
            folded_request_ids: group.3,
            trust_profile: group.1,
        })
        .collect();

    TemporalRequestFoldingReport {
        folded_groups,
        actual_requests_required,
        requests_saved,
        fold_window_ms,
    }
}

fn normalize_intent(intent: &str) -> String {
    Regex::new(r"\d+")
        .expect("regex should compile")
        .replace_all(&intent.to_ascii_lowercase(), ":num")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_cpm(requests: &[ProtocolRequestInput]) -> CausalityPreservingMultiplexingReport {
    let mut pending = requests.to_vec();
    pending.sort_by_key(|request| request.arrival_rank);
    let mut delivered = BTreeSet::new();
    let mut delivery_order = Vec::new();
    let mut release_waves = Vec::new();

    loop {
        let mut wave = Vec::new();
        let mut still_pending = Vec::new();
        for request in pending {
            if request
                .dependencies
                .iter()
                .all(|dependency| delivered.contains(dependency))
            {
                delivered.insert(request.request_id.clone());
                delivery_order.push(request.request_id.clone());
                wave.push(request.request_id);
            } else {
                still_pending.push(request);
            }
        }
        if wave.is_empty() {
            let buffered_responses = still_pending
                .iter()
                .map(|request| request.request_id.clone())
                .collect::<Vec<_>>();
            return CausalityPreservingMultiplexingReport {
                delivery_order,
                buffered_responses,
                release_waves,
            };
        }
        release_waves.push(wave);
        pending = still_pending;
        if pending.is_empty() {
            return CausalityPreservingMultiplexingReport {
                delivery_order,
                buffered_responses: Vec::new(),
                release_waves,
            };
        }
    }
}

fn latency_fitness(latency_ms: f64) -> f64 {
    (1.0 - (latency_ms / 5_000.0)).clamp(0.0, 1.0)
}

fn clamp_u32(value: u32, min: u32, max: u32) -> u32 {
    value.clamp(min, max)
}

fn clamp_u16(value: u16, min: u16, max: u16) -> u16 {
    value.clamp(min, max)
}

fn clamp_u8(value: u8, min: u8, max: u8) -> u8 {
    value.clamp(min, max)
}

fn excavate_failure(
    transitions: &[CausalTransitionInput],
    failure: &FailureSymptomInput,
) -> FailureArchaeologyReport {
    let by_id = transitions
        .iter()
        .map(|transition| (transition.transition_id.clone(), transition))
        .collect::<BTreeMap<_, _>>();
    let mut cursor = failure.transition_id.clone();
    let mut causal_chain = Vec::new();
    let mut root_cause = None;

    while let Some(transition) = by_id.get(&cursor) {
        if transition.abnormality_score < 0.45 && !causal_chain.is_empty() {
            break;
        }
        causal_chain.push(format!(
            "{}:{}:{}",
            transition.transition_id, transition.subsystem, transition.change
        ));
        root_cause = Some(format!(
            "{} triggered by {}",
            transition.transition_id, transition.trigger
        ));
        if let Some(parent) = &transition.parent_id {
            cursor = parent.clone();
        } else {
            break;
        }
    }

    FailureArchaeologyReport {
        counterfactual: if let Some(root) = &root_cause {
            format!(
                "preventing {} or constraining '{}' would likely avoid '{}'",
                root, failure.transition_id, failure.symptom
            )
        } else {
            format!(
                "no abnormal transition chain was found for '{}'",
                failure.symptom
            )
        },
        root_cause,
        causal_chain,
    }
}

fn run_sonar(probes: &[SonarProbeInput]) -> AmbientRegressionSonarReport {
    let findings = probes
        .iter()
        .filter_map(|probe| {
            let latency_drift = normalized_drift(
                probe.baseline_latency_ms,
                probe.current_latency_ms,
                probe.sigma,
            );
            let size_drift = normalized_drift(
                probe.baseline_size_bytes,
                probe.current_size_bytes,
                probe.sigma,
            );
            let error_drift = normalized_drift(
                probe.baseline_error_rate,
                probe.current_error_rate,
                probe.sigma,
            );
            let drift_score = ((latency_drift + size_drift + error_drift) / 3.0).clamp(0.0, 10.0);
            if drift_score >= 1.0 {
                Some(AmbientRegressionFinding {
                    probe_id: probe.probe_id.clone(),
                    target_subsystem: probe.target_subsystem.clone(),
                    drift_score,
                    severity: if drift_score >= 2.0 {
                        "high".into()
                    } else {
                        "medium".into()
                    },
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    AmbientRegressionSonarReport {
        early_warning: !findings.is_empty(),
        findings,
    }
}

fn normalized_drift(baseline: f64, current: f64, sigma: f64) -> f64 {
    if baseline.abs() < f64::EPSILON {
        return 0.0;
    }
    ((current - baseline).abs() / baseline.abs()) / sigma.max(0.01)
}

fn compensate_reality_drift(assumptions: &[RealityAssumptionInput]) -> Vec<RealityDriftAction> {
    assumptions
        .iter()
        .filter_map(|assumption| {
            let drift_ratio = ((assumption.observed - assumption.expected).abs()
                / assumption.expected.abs().max(1.0))
            .clamp(0.0, 10.0);
            if drift_ratio < assumption.tolerance {
                return None;
            }
            let action = match assumption.compensation_kind.as_str() {
                "failover_endpoint" => "switch to attested fallback endpoint",
                "adjust_retry" => "increase bounded retry budget",
                "escalate" => "escalate to operator with drift packet",
                _ => "recalculate timeout envelope",
            };
            Some(RealityDriftAction {
                asset_id: assumption.asset_id.clone(),
                drift_ratio,
                action: action.into(),
            })
        })
        .collect()
}

fn compile_slas(statements: &[String], windows: &[SlaObservationWindow]) -> Vec<CompiledSlaReport> {
    let latency_regex = Regex::new(r"(?i)within\s+(\d+)ms").expect("regex should compile");
    let pct_regex = Regex::new(r"(?i)(\d+(?:\.\d+)?)%").expect("regex should compile");
    statements
        .iter()
        .map(|statement| {
            let latency_target = latency_regex
                .captures(statement)
                .and_then(|captures| captures.get(1))
                .and_then(|value| value.as_str().parse::<f64>().ok())
                .unwrap_or(500.0);
            let compliance_target = pct_regex
                .captures(statement)
                .and_then(|captures| captures.get(1))
                .and_then(|value| value.as_str().parse::<f64>().ok())
                .unwrap_or(99.0)
                / 100.0;
            let compliance = windows
                .first()
                .map(|window| {
                    if window.total_requests == 0 {
                        0.0
                    } else {
                        window.compliant_requests as f64 / window.total_requests as f64
                    }
                })
                .unwrap_or(0.0);
            CompiledSlaReport {
                statement: statement.clone(),
                predicate: format!(
                    "latency <= {:.0}ms and compliance >= {:.3}",
                    latency_target, compliance_target
                ),
                current_compliance: compliance,
                breach_action: if compliance < compliance_target {
                    "raise performance tier and emit compliance breach".into()
                } else {
                    "continue monitored enforcement".into()
                },
            }
        })
        .collect()
}

fn simulate_dependency_blast_radius(
    components: &[DependencyComponentInput],
    proposed_changes: &[ProposedChangeInput],
) -> Vec<DependencyBlastRadiusReport> {
    let adjacency = components
        .iter()
        .map(|component| (component.component_id.clone(), component.depends_on.clone()))
        .collect::<BTreeMap<_, _>>();

    proposed_changes
        .iter()
        .map(|change| {
            let affected = transitive_dependents(&adjacency, &change.target);
            let predicted_breakages = components
                .iter()
                .filter(|component| affected.contains(&component.component_id))
                .filter_map(|component| {
                    let violation = component.invariants.iter().find(|invariant| {
                        invariant
                            .to_ascii_lowercase()
                            .contains(&change.change_type.to_ascii_lowercase())
                            || invariant
                                .to_ascii_lowercase()
                                .contains(&change.summary.to_ascii_lowercase())
                    })?;
                    Some(BreakagePrediction {
                        component_id: component.component_id.clone(),
                        violation: violation.clone(),
                        severity: change.severity.clone(),
                        suggested_mitigation: format!(
                            "introduce compatibility guard before applying '{}' to {}",
                            change.change_type, component.component_id
                        ),
                    })
                })
                .collect::<Vec<_>>();

            DependencyBlastRadiusReport {
                target: change.target.clone(),
                total_affected: affected.len(),
                safe_to_proceed: predicted_breakages.is_empty(),
                predicted_breakages,
            }
        })
        .collect()
}

fn transitive_dependents(
    adjacency: &BTreeMap<String, Vec<String>>,
    target: &str,
) -> BTreeSet<String> {
    let mut affected = BTreeSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for (component, dependencies) in adjacency {
            if dependencies
                .iter()
                .any(|dependency| dependency == target || affected.contains(dependency))
                && affected.insert(component.clone())
            {
                changed = true;
            }
        }
    }
    affected
}

fn triangulate_anomalies(events: &[AnomalyEventInput]) -> Vec<TriangulatedAnomaly> {
    events
        .iter()
        .filter_map(|event| {
            let signals = [
                event.structural_score >= 0.7,
                event.temporal_score >= 0.7,
                event.semantic_score >= 0.7,
            ];
            let hit_count = signals.iter().filter(|signal| **signal).count();
            match hit_count {
                3 => Some(TriangulatedAnomaly {
                    event_id: event.event_id.clone(),
                    source: event.source.clone(),
                    classification: "confirmed".into(),
                    confidence: 0.99,
                }),
                2 => Some(TriangulatedAnomaly {
                    event_id: event.event_id.clone(),
                    source: event.source.clone(),
                    classification: "probable".into(),
                    confidence: 0.75,
                }),
                _ => None,
            }
        })
        .collect()
}

fn plan_speculative_repairs(
    operations: &[SpeculativeOperationInput],
    archaeology: &FailureArchaeologyReport,
) -> Vec<SpeculativeRepairPlan> {
    operations
        .iter()
        .map(|operation| {
            let predicted_error = operation
                .predicted_error
                .clone()
                .unwrap_or_else(|| "unknown_predicted_failure".into());
            let repair_action = operation
                .known_repairs
                .first()
                .cloned()
                .unwrap_or_else(|| infer_repair_action(&predicted_error, archaeology));
            SpeculativeRepairPlan {
                operation_id: operation.operation_id.clone(),
                predicted_error,
                repair_action,
                apply_before_execution: true,
            }
        })
        .collect()
}

fn infer_repair_action(error: &str, archaeology: &FailureArchaeologyReport) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timeout") {
        "raise bounded timeout and enable circuit-breaker fallback".into()
    } else if lower.contains("schema") || lower.contains("contract") {
        "apply compatibility adapter and replay invariant checks".into()
    } else if lower.contains("auth") {
        "refresh credentials and rotate session before real execution".into()
    } else if let Some(root) = &archaeology.root_cause {
        format!("apply remediation derived from failure archaeology root cause: {root}")
    } else {
        "prewarm subsystem and revalidate dependencies".into()
    }
}

impl Default for RevolutionaryFeaturesEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::chain::Chain;
    use crate::chain::value_protocol::PaymentComplianceRegistrationRequest;

    #[test]
    fn calcification_detects_privilege_and_root_delete() {
        let graph = calcify_intents("sudo rm -rf /", &[], &[]);
        assert!(graph
            .dangerous_taxa
            .contains(&"catastrophic_destruction".to_string()));
        assert!(graph
            .dangerous_taxa
            .contains(&"privilege_escalation".to_string()));
    }

    #[test]
    fn temporal_request_folding_collapses_equivalent_intents() {
        let report = build_temporal_folding(
            &[
                ProtocolRequestInput {
                    request_id: "r1".into(),
                    intent: "fetch stock price 100".into(),
                    trust_profile: "standard".into(),
                    submitted_at_ms: 10,
                    dependencies: Vec::new(),
                    arrival_rank: 2,
                    priority: "normal".into(),
                },
                ProtocolRequestInput {
                    request_id: "r2".into(),
                    intent: "fetch stock price 101".into(),
                    trust_profile: "standard".into(),
                    submitted_at_ms: 40,
                    dependencies: Vec::new(),
                    arrival_rank: 1,
                    priority: "normal".into(),
                },
            ],
            Some(100),
        );

        assert_eq!(report.actual_requests_required, 1);
        assert_eq!(report.requests_saved, 1);
        assert_eq!(report.folded_groups.len(), 1);
    }

    #[test]
    fn failure_archaeology_traces_root_cause() {
        let report = excavate_failure(
            &[
                CausalTransitionInput {
                    transition_id: "t1".into(),
                    parent_id: None,
                    subsystem: "cache".into(),
                    change: "allocator drift".into(),
                    trigger: "fragmentation".into(),
                    abnormality_score: 0.9,
                    timestamp_ms: 1,
                },
                CausalTransitionInput {
                    transition_id: "t2".into(),
                    parent_id: Some("t1".into()),
                    subsystem: "search".into(),
                    change: "latency spike".into(),
                    trigger: "slow cache fetch".into(),
                    abnormality_score: 0.8,
                    timestamp_ms: 2,
                },
            ],
            &FailureSymptomInput {
                transition_id: "t2".into(),
                symptom: "search latency breach".into(),
            },
        );

        assert_eq!(report.causal_chain.len(), 2);
        assert!(report.root_cause.is_some());
    }

    #[test]
    fn revolutionary_value_orchestration_anchors_guarded_payment() {
        let mut engine = RevolutionaryFeaturesEngine::new();
        let mut chain = Chain::new();
        let expires_at = chrono::Utc::now().timestamp_millis() + 86_400_000;
        for party in ["alice", "vendor"] {
            chain
                .register_payment_compliance_profile(PaymentComplianceRegistrationRequest {
                    party: party.into(),
                    legal_entity_id: format!("{party}-entity"),
                    jurisdiction: "US".into(),
                    kyc_reference: format!("kyc-{party}"),
                    aml_reference: format!("aml-{party}"),
                    custody_reference: format!("custody-{party}"),
                    sanctions_clear: true,
                    allowed_rails: vec![SettlementRail::InternalLedger],
                    allowed_compliance_modes: vec![ComplianceMode::TaxAware],
                    max_single_amount: 10_000.0,
                    expires_at,
                    metadata: None,
                })
                .expect("compliance profile registration should succeed");
        }
        let report = engine
            .orchestrate_value(
                RevolutionaryValueRequest {
                    sender: "alice".into(),
                    receiver: "vendor".into(),
                    amount: 42.5,
                    purpose: "pay for attested semantic acquisition".into(),
                    currency: "USD".into(),
                    settlement_rail: SettlementRail::InternalLedger,
                    compliance: ComplianceMode::TaxAware,
                    privacy_mode: PrivacyPreservationMode::AttestedMinimization,
                    risk_tier: EnterpriseRiskTier::Standard,
                    connector_id: None,
                    providers: Vec::new(),
                    allow_split_routing: true,
                    require_escrow: true,
                    autocommit: false,
                    fulfillment_window_seconds: 600,
                    min_quality_score: 0.95,
                    evidence: vec!["task-chain".into()],
                    cooperative_unlock_parties: vec!["alice".into(), "vendor".into()],
                    metadata: None,
                    notarize_to_chain: false,
                    attestor: None,
                },
                &mut chain,
            )
            .expect("value orchestration should succeed");

        assert!(!report.report_id.is_empty());
        assert!(!report.guarded_payment.contract_id.is_empty());
        assert!(report.guarded_payment.authorized || report.guarded_payment.blocked);
    }
}

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};

const REPORT_HISTORY_LIMIT: usize = 32;
const MIN_ARBITRAGE_SPREAD: f64 = 0.0015;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryTargetInput {
    pub asset_id: String,
    #[serde(default)]
    pub format_hint: Option<String>,
    #[serde(default)]
    pub corruption_signals: Vec<String>,
    #[serde(default)]
    pub fragment_count: u32,
    #[serde(default)]
    pub snapshots_available: u32,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub directory_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryArtifactInput {
    pub artifact_id: String,
    pub content_class: String,
    #[serde(default)]
    pub contains_secrets: bool,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarfareRecoveryRequest {
    pub case_id: String,
    #[serde(default)]
    pub targets: Vec<RecoveryTargetInput>,
    #[serde(default)]
    pub memory_artifacts: Vec<MemoryArtifactInput>,
    #[serde(default)]
    pub protect_outputs: bool,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatSampleInput {
    pub sample_id: String,
    #[serde(default)]
    pub family_hint: Option<String>,
    #[serde(default)]
    pub behaviors: Vec<String>,
    #[serde(default)]
    pub packer_layers: u8,
    #[serde(default)]
    pub c2_indicators: Vec<String>,
    #[serde(default)]
    pub privileges: Vec<String>,
    #[serde(default)]
    pub touched_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RansomwareIncidentInput {
    pub incident_id: String,
    #[serde(default)]
    pub encrypted_extensions: Vec<String>,
    #[serde(default)]
    pub recent_restore_points: u32,
    #[serde(default)]
    pub impacted_paths: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareRiskInput {
    pub product: String,
    pub version: String,
    #[serde(default)]
    pub recent_cve_count: u32,
    #[serde(default)]
    pub days_since_last_patch: u32,
    #[serde(default = "default_attack_surface_score")]
    pub attack_surface_score: f64,
    #[serde(default)]
    pub hardening_available: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarfareDefenseRequest {
    #[serde(default)]
    pub samples: Vec<ThreatSampleInput>,
    #[serde(default)]
    pub ransomware_incidents: Vec<RansomwareIncidentInput>,
    #[serde(default)]
    pub software_inventory: Vec<SoftwareRiskInput>,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketQuoteInput {
    pub asset: String,
    pub venue: String,
    pub price: f64,
    #[serde(default)]
    pub fee_bps: f64,
    #[serde(default = "default_liquidity_score")]
    pub liquidity_score: f64,
    #[serde(default = "default_true")]
    pub compliance_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentChannelInput {
    pub channel: String,
    pub content_type: String,
    pub monetization: String,
    #[serde(default = "default_capacity_score")]
    pub capacity_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRequestInput {
    pub service: String,
    pub buyer_segment: String,
    pub budget_usd: f64,
    #[serde(default = "default_urgency")]
    pub urgency: f64,
    #[serde(default = "default_true")]
    pub authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyProgramInput {
    pub program: String,
    pub scope: String,
    pub reward_usd: f64,
    #[serde(default)]
    pub authorized_assets_only: bool,
    #[serde(default)]
    pub exploit_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedAssetInput {
    pub asset_id: String,
    pub asset_type: String,
    #[serde(default)]
    pub utilization: f64,
    #[serde(default)]
    pub maintenance_cost_usd: f64,
    #[serde(default = "default_true")]
    pub monetizable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldOptionInput {
    pub name: String,
    pub expected_return: f64,
    #[serde(default)]
    pub lock_days: u32,
    #[serde(default)]
    pub risk_score: f64,
    #[serde(default = "default_true")]
    pub compliance_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSignalInput {
    pub topic: String,
    pub sentiment: f64,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreelanceLeadInput {
    pub platform: String,
    pub project_type: String,
    pub budget_usd: f64,
    #[serde(default = "default_fit_score")]
    pub fit_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarfareRevenueRequest {
    #[serde(default)]
    pub market_quotes: Vec<MarketQuoteInput>,
    #[serde(default)]
    pub content_channels: Vec<ContentChannelInput>,
    #[serde(default)]
    pub service_requests: Vec<ServiceRequestInput>,
    #[serde(default)]
    pub bounty_programs: Vec<BountyProgramInput>,
    #[serde(default)]
    pub owned_assets: Vec<OwnedAssetInput>,
    #[serde(default)]
    pub yield_options: Vec<YieldOptionInput>,
    #[serde(default)]
    pub market_signals: Vec<MarketSignalInput>,
    #[serde(default)]
    pub freelance_leads: Vec<FreelanceLeadInput>,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default)]
    pub notarize_to_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalArchaeologyFinding {
    pub asset_id: String,
    pub recoverable_generations: u8,
    pub confidence: f64,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticRepairPlan {
    pub asset_id: String,
    pub inferred_format: String,
    pub repair_actions: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostTreeNode {
    pub directory: String,
    pub member_assets: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionWeaknessAssessment {
    pub asset_id: String,
    pub weakness_class: String,
    pub mitigation: String,
    pub recovery_feasibility: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentReassemblyPlan {
    pub asset_id: String,
    pub fragments_observed: u32,
    pub contiguous_coverage: f64,
    pub ordering_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryArtifactRecovery {
    pub artifact_id: String,
    pub content_class: String,
    pub recovered_bytes: u64,
    pub redaction_applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfHealingProtection {
    pub asset_id: String,
    pub protection_plan: Vec<String>,
    pub recommended_parity_shards: u8,
    pub max_recoverable_corruption_percent: f64,
    pub header_shadowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarfareRecoveryReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub byte_level_temporal_archaeology: Vec<TemporalArchaeologyFinding>,
    pub semantic_structure_inference: Vec<SemanticRepairPlan>,
    pub filesystem_ghost_graph: Vec<GhostTreeNode>,
    pub encryption_entropy_reversal: Vec<EncryptionWeaknessAssessment>,
    pub cross_fragment_dna_reassembly: Vec<FragmentReassemblyPlan>,
    pub memory_phantom_extraction: Vec<MemoryArtifactRecovery>,
    pub self_healing_data_organism: Vec<SelfHealingProtection>,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralDnaFingerprint {
    pub sample_id: String,
    pub dominant_genes: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymorphicUnpackingAssessment {
    pub sample_id: String,
    pub suspected_layers: u8,
    pub static_safe_unwrap_steps: Vec<String>,
    pub execution_blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootkitDepthSonarFinding {
    pub sample_id: String,
    pub hidden_surface_score: f64,
    pub suspicious_anchors: Vec<String>,
    pub recommended_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RansomwareTimeReversalPlan {
    pub incident_id: String,
    pub restorable_paths: u32,
    pub restore_confidence: f64,
    pub required_artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalwareVivisectionReport {
    pub sample_id: String,
    pub kill_chain: Vec<String>,
    pub iocs: Vec<String>,
    pub blocked_execution: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterIntelligenceContainmentReport {
    pub sample_id: String,
    pub sinkhole_candidates: Vec<String>,
    pub block_rules: Vec<String>,
    pub evidence_bundle_id: String,
    pub active_payloads_disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatLineageMatch {
    pub sample_id: String,
    pub family: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroDayRiskForecast {
    pub product: String,
    pub version: String,
    pub predicted_classes: Vec<String>,
    pub probability: f64,
    pub hardening_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousImmuneMemoryReport {
    pub antibodies_created: usize,
    pub total_antibodies: usize,
    pub broadened_families: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarfareDefenseReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub behavioral_dna_extraction: Vec<BehavioralDnaFingerprint>,
    pub polymorphic_unpacking_engine: Vec<PolymorphicUnpackingAssessment>,
    pub rootkit_depth_sonar: Vec<RootkitDepthSonarFinding>,
    pub ransomware_time_reversal: Vec<RansomwareTimeReversalPlan>,
    pub malware_vivisection_lab: Vec<MalwareVivisectionReport>,
    pub counter_intelligence_containment: Vec<CounterIntelligenceContainmentReport>,
    pub threat_lineage_tracker: Vec<ThreatLineageMatch>,
    pub zero_day_synthesis_predictor: Vec<ZeroDayRiskForecast>,
    pub autonomous_immune_memory: AutonomousImmuneMemoryReport,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunityReport {
    pub asset: String,
    pub buy_venue: String,
    pub sell_venue: String,
    pub net_spread_percent: f64,
    pub risk_score: f64,
    pub executable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentOutputPlan {
    pub channel: String,
    pub content_type: String,
    pub monetization: String,
    pub expected_revenue_30d: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicroServiceOffer {
    pub service: String,
    pub buyer_segment: String,
    pub quoted_price_usd: f64,
    pub turnaround_hours: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyOpportunityAssessment {
    pub program: String,
    pub eligible: bool,
    pub reason: String,
    pub expected_reward_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigitalAssetActivation {
    pub asset_id: String,
    pub action: String,
    pub expected_monthly_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldAllocation {
    pub option: String,
    pub allocation_percent: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSentimentAlchemyReport {
    pub dominant_topic: String,
    pub composite_sentiment: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreelanceDispatchPlan {
    pub platform: String,
    pub project_type: String,
    pub proposal_strength: f64,
    pub expected_value_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenuePortfolioOrganismReport {
    pub expected_monthly_usd: f64,
    pub resilience_score: f64,
    pub diversification_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarfareRevenueReport {
    pub report_id: String,
    pub manifest_hash: String,
    pub arbitrage_radar: Vec<ArbitrageOpportunityReport>,
    pub content_metabolism_engine: Vec<ContentOutputPlan>,
    pub micro_service_mercenary: Vec<MicroServiceOffer>,
    pub autonomous_bounty_hunter: Vec<BountyOpportunityAssessment>,
    pub digital_asset_archaeology: Vec<DigitalAssetActivation>,
    pub yield_optimization_cortex: Vec<YieldAllocation>,
    pub market_sentiment_alchemy: MarketSentimentAlchemyReport,
    pub autonomous_freelance_dispatch: Vec<FreelanceDispatchPlan>,
    pub revenue_portfolio_organism: RevenuePortfolioOrganismReport,
    pub analyzed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarfareEditionStatus {
    pub recovery_reports: u64,
    pub defense_reports: u64,
    pub revenue_reports: u64,
    pub protected_assets: usize,
    pub immune_antibodies: usize,
    pub recent_reports: Vec<String>,
    pub blocked_capabilities: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct ImmuneAntibody {
    family: String,
    dominant_genes: BTreeSet<String>,
    encounters: u64,
}

#[derive(Debug, Clone, Default)]
pub struct WarfareEditionEngine {
    report_sequence: u64,
    recovery_reports: u64,
    defense_reports: u64,
    revenue_reports: u64,
    protected_assets: BTreeMap<String, SelfHealingProtection>,
    immune_antibodies: BTreeMap<String, ImmuneAntibody>,
    recent_reports: VecDeque<String>,
}

impl WarfareEditionEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn status(&self) -> WarfareEditionStatus {
        WarfareEditionStatus {
            recovery_reports: self.recovery_reports,
            defense_reports: self.defense_reports,
            revenue_reports: self.revenue_reports,
            protected_assets: self.protected_assets.len(),
            immune_antibodies: self.immune_antibodies.len(),
            recent_reports: self.recent_reports.iter().cloned().collect(),
            blocked_capabilities: blocked_capabilities(),
            guardrails: guardrails(),
        }
    }

    pub fn orchestrate_recovery(
        &mut self,
        request: WarfareRecoveryRequest,
    ) -> AstraResult<WarfareRecoveryReport> {
        validate_recovery_request(&request)?;

        let analyzed_at = now_ts();
        let report_id = self.next_report_id("recovery");
        let temporal = request
            .targets
            .iter()
            .map(build_temporal_archaeology_finding)
            .collect::<Vec<_>>();
        let semantic = request
            .targets
            .iter()
            .map(build_semantic_repair_plan)
            .collect::<Vec<_>>();
        let ghost = build_ghost_graph(&request.targets);
        let encryption = request
            .targets
            .iter()
            .filter(|target| {
                target.encrypted
                    || target
                        .corruption_signals
                        .iter()
                        .any(|signal| contains_any(signal, &["enc", "cipher", "ransom"]))
            })
            .map(build_encryption_assessment)
            .collect::<Vec<_>>();
        let fragments = request
            .targets
            .iter()
            .map(build_fragment_reassembly_plan)
            .collect::<Vec<_>>();
        let memory = request
            .memory_artifacts
            .iter()
            .map(build_memory_artifact_recovery)
            .collect::<Vec<_>>();
        let self_healing = request
            .targets
            .iter()
            .filter(|target| request.protect_outputs || target.snapshots_available > 0)
            .map(build_self_healing_protection)
            .collect::<Vec<_>>();

        for protection in &self_healing {
            self.protected_assets
                .insert(protection.asset_id.clone(), protection.clone());
        }

        let manifest_hash = manifest_hash(&serde_json::json!({
            "report_id": report_id,
            "case_id": request.case_id,
            "targets": request.targets,
            "memory_artifacts": request.memory_artifacts,
            "temporal": temporal,
            "semantic": semantic,
            "ghost": ghost,
            "encryption": encryption,
            "fragments": fragments,
            "memory": memory,
            "self_healing": self_healing,
            "analyzed_at": analyzed_at,
        }))?;

        let report = WarfareRecoveryReport {
            report_id,
            manifest_hash,
            byte_level_temporal_archaeology: temporal,
            semantic_structure_inference: semantic,
            filesystem_ghost_graph: ghost,
            encryption_entropy_reversal: encryption,
            cross_fragment_dna_reassembly: fragments,
            memory_phantom_extraction: memory,
            self_healing_data_organism: self_healing,
            analyzed_at,
        };

        self.recovery_reports += 1;
        self.push_recent_report(report.report_id.clone());

        Ok(report)
    }

    pub fn orchestrate_defense(
        &mut self,
        request: WarfareDefenseRequest,
    ) -> AstraResult<WarfareDefenseReport> {
        validate_defense_request(&request)?;

        let analyzed_at = now_ts();
        let report_id = self.next_report_id("defense");
        let behavioral = request
            .samples
            .iter()
            .map(build_behavioral_fingerprint)
            .collect::<Vec<_>>();
        let unpacking = request
            .samples
            .iter()
            .map(build_unpacking_assessment)
            .collect::<Vec<_>>();
        let sonar = request
            .samples
            .iter()
            .map(build_rootkit_sonar_finding)
            .collect::<Vec<_>>();
        let ransomware = request
            .ransomware_incidents
            .iter()
            .map(build_ransomware_reversal_plan)
            .collect::<Vec<_>>();
        let vivisection = request
            .samples
            .iter()
            .map(build_malware_vivisection_report)
            .collect::<Vec<_>>();
        let containment = request
            .samples
            .iter()
            .map(build_containment_report)
            .collect::<Vec<_>>();
        let lineage = request
            .samples
            .iter()
            .map(build_threat_lineage_match)
            .collect::<Vec<_>>();
        let forecasts = request
            .software_inventory
            .iter()
            .map(build_zero_day_forecast)
            .collect::<Vec<_>>();

        let prior_antibodies = self.immune_antibodies.len();
        let broadened_families = self.update_immune_memory(&lineage, &behavioral);
        let immune_report = AutonomousImmuneMemoryReport {
            antibodies_created: self
                .immune_antibodies
                .len()
                .saturating_sub(prior_antibodies),
            total_antibodies: self.immune_antibodies.len(),
            broadened_families,
        };

        let manifest_hash = manifest_hash(&serde_json::json!({
            "report_id": report_id,
            "samples": request.samples,
            "ransomware_incidents": request.ransomware_incidents,
            "software_inventory": request.software_inventory,
            "behavioral": behavioral,
            "unpacking": unpacking,
            "sonar": sonar,
            "ransomware": ransomware,
            "vivisection": vivisection,
            "containment": containment,
            "lineage": lineage,
            "forecasts": forecasts,
            "immune_report": immune_report,
            "analyzed_at": analyzed_at,
        }))?;

        let report = WarfareDefenseReport {
            report_id,
            manifest_hash,
            behavioral_dna_extraction: behavioral,
            polymorphic_unpacking_engine: unpacking,
            rootkit_depth_sonar: sonar,
            ransomware_time_reversal: ransomware,
            malware_vivisection_lab: vivisection,
            counter_intelligence_containment: containment,
            threat_lineage_tracker: lineage,
            zero_day_synthesis_predictor: forecasts,
            autonomous_immune_memory: immune_report,
            analyzed_at,
        };

        self.defense_reports += 1;
        self.push_recent_report(report.report_id.clone());

        Ok(report)
    }

    pub fn orchestrate_revenue(
        &mut self,
        request: WarfareRevenueRequest,
    ) -> AstraResult<WarfareRevenueReport> {
        validate_revenue_request(&request)?;

        let analyzed_at = now_ts();
        let report_id = self.next_report_id("revenue");
        let arbitrage = build_arbitrage_opportunities(&request.market_quotes);
        let content = request
            .content_channels
            .iter()
            .map(build_content_output_plan)
            .collect::<Vec<_>>();
        let services = request
            .service_requests
            .iter()
            .filter(|item| item.authorized && item.budget_usd > 0.0)
            .map(build_micro_service_offer)
            .collect::<Vec<_>>();
        let bounties = request
            .bounty_programs
            .iter()
            .map(build_bounty_assessment)
            .collect::<Vec<_>>();
        let assets = request
            .owned_assets
            .iter()
            .filter(|item| item.monetizable)
            .map(build_digital_asset_activation)
            .collect::<Vec<_>>();
        let yield_allocations = build_yield_allocations(&request.yield_options);
        let sentiment = build_market_sentiment_report(&request.market_signals);
        let freelance = request
            .freelance_leads
            .iter()
            .filter(|lead| lead.budget_usd > 0.0)
            .map(build_freelance_dispatch_plan)
            .collect::<Vec<_>>();
        let portfolio = build_revenue_portfolio_report(
            &arbitrage,
            &content,
            &services,
            &bounties,
            &assets,
            &yield_allocations,
            &sentiment,
            &freelance,
        );

        let manifest_hash = manifest_hash(&serde_json::json!({
            "report_id": report_id,
            "market_quotes": request.market_quotes,
            "content_channels": request.content_channels,
            "service_requests": request.service_requests,
            "bounty_programs": request.bounty_programs,
            "owned_assets": request.owned_assets,
            "yield_options": request.yield_options,
            "market_signals": request.market_signals,
            "freelance_leads": request.freelance_leads,
            "arbitrage": arbitrage,
            "content": content,
            "services": services,
            "bounties": bounties,
            "assets": assets,
            "yield_allocations": yield_allocations,
            "sentiment": sentiment,
            "freelance": freelance,
            "portfolio": portfolio,
            "analyzed_at": analyzed_at,
        }))?;

        let report = WarfareRevenueReport {
            report_id,
            manifest_hash,
            arbitrage_radar: arbitrage,
            content_metabolism_engine: content,
            micro_service_mercenary: services,
            autonomous_bounty_hunter: bounties,
            digital_asset_archaeology: assets,
            yield_optimization_cortex: yield_allocations,
            market_sentiment_alchemy: sentiment,
            autonomous_freelance_dispatch: freelance,
            revenue_portfolio_organism: portfolio,
            analyzed_at,
        };

        self.revenue_reports += 1;
        self.push_recent_report(report.report_id.clone());

        Ok(report)
    }

    fn next_report_id(&mut self, category: &str) -> String {
        self.report_sequence += 1;
        format!("warfare-{category}-{:06}", self.report_sequence)
    }

    fn push_recent_report(&mut self, report_id: String) {
        self.recent_reports.push_front(report_id);
        while self.recent_reports.len() > REPORT_HISTORY_LIMIT {
            self.recent_reports.pop_back();
        }
    }

    fn update_immune_memory(
        &mut self,
        lineage: &[ThreatLineageMatch],
        behavioral: &[BehavioralDnaFingerprint],
    ) -> Vec<String> {
        let gene_map = behavioral
            .iter()
            .map(|item| (item.sample_id.clone(), item.dominant_genes.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut broadened = BTreeSet::new();
        for lineage_match in lineage {
            let family_key = lineage_match.family.to_ascii_lowercase();
            let genes = gene_map
                .get(&lineage_match.sample_id)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect::<BTreeSet<_>>();

            let entry = self
                .immune_antibodies
                .entry(family_key.clone())
                .or_insert_with(|| ImmuneAntibody {
                    family: lineage_match.family.clone(),
                    dominant_genes: BTreeSet::new(),
                    encounters: 0,
                });
            let original_gene_count = entry.dominant_genes.len();
            entry.dominant_genes.extend(genes);
            entry.encounters += 1;
            if entry.encounters == 1 || entry.dominant_genes.len() > original_gene_count {
                broadened.insert(entry.family.clone());
            }
        }

        broadened.into_iter().collect()
    }
}

fn default_true() -> bool {
    true
}

fn default_confidence() -> f64 {
    0.72
}

fn default_attack_surface_score() -> f64 {
    0.55
}

fn default_liquidity_score() -> f64 {
    0.75
}

fn default_capacity_score() -> f64 {
    0.65
}

fn default_urgency() -> f64 {
    0.5
}

fn default_fit_score() -> f64 {
    0.7
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn manifest_hash(value: &serde_json::Value) -> AstraResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(sha3_256_hex(&bytes))
}

fn blocked_capabilities() -> Vec<String> {
    vec![
        "offensive_payload_deployment".into(),
        "unauthorized_intrusion".into(),
        "live_exploit_generation".into(),
        "automatic_live_trading".into(),
        "unscoped_bug_bounty_execution".into(),
    ]
}

fn guardrails() -> Vec<String> {
    vec![
        "recovery outputs are snapshot-backed and secret-redacted".into(),
        "malware analysis remains static or replay-only with execution blocked".into(),
        "counter-intelligence features emit containment controls, not payloads".into(),
        "bounty and freelance planning requires explicit authorization and scope".into(),
        "yield and arbitrage outputs remain advisory unless separately approved".into(),
    ]
}

fn validate_recovery_request(request: &WarfareRecoveryRequest) -> AstraResult<()> {
    if request.case_id.trim().is_empty() {
        return Err(AstraError::ControlPlaneRejected(
            "warfare recovery requires a case_id".into(),
        ));
    }
    if request.targets.is_empty() && request.memory_artifacts.is_empty() {
        return Err(AstraError::ControlPlaneRejected(
            "warfare recovery requires at least one target or memory artifact".into(),
        ));
    }
    if request
        .targets
        .iter()
        .any(|target| target.asset_id.trim().is_empty())
    {
        return Err(AstraError::ControlPlaneRejected(
            "warfare recovery target asset_id cannot be empty".into(),
        ));
    }
    if request.memory_artifacts.iter().any(|artifact| {
        artifact.artifact_id.trim().is_empty() || artifact.content_class.trim().is_empty()
    }) {
        return Err(AstraError::ControlPlaneRejected(
            "warfare recovery memory artifacts require ids and content classes".into(),
        ));
    }
    Ok(())
}

fn validate_defense_request(request: &WarfareDefenseRequest) -> AstraResult<()> {
    if request.samples.is_empty()
        && request.ransomware_incidents.is_empty()
        && request.software_inventory.is_empty()
    {
        return Err(AstraError::ControlPlaneRejected(
            "warfare defense requires at least one sample, incident, or software item".into(),
        ));
    }
    if request
        .samples
        .iter()
        .any(|sample| sample.sample_id.trim().is_empty())
    {
        return Err(AstraError::ControlPlaneRejected(
            "warfare defense sample_id cannot be empty".into(),
        ));
    }
    if request
        .ransomware_incidents
        .iter()
        .any(|incident| incident.incident_id.trim().is_empty())
    {
        return Err(AstraError::ControlPlaneRejected(
            "warfare defense incident_id cannot be empty".into(),
        ));
    }
    if request
        .software_inventory
        .iter()
        .any(|item| item.product.trim().is_empty() || item.version.trim().is_empty())
    {
        return Err(AstraError::ControlPlaneRejected(
            "warfare defense software inventory requires product and version".into(),
        ));
    }
    Ok(())
}

fn validate_revenue_request(request: &WarfareRevenueRequest) -> AstraResult<()> {
    if request.market_quotes.is_empty()
        && request.content_channels.is_empty()
        && request.service_requests.is_empty()
        && request.bounty_programs.is_empty()
        && request.owned_assets.is_empty()
        && request.yield_options.is_empty()
        && request.market_signals.is_empty()
        && request.freelance_leads.is_empty()
    {
        return Err(AstraError::ControlPlaneRejected(
            "warfare revenue requires at least one revenue input".into(),
        ));
    }
    if request.market_quotes.iter().any(|quote| {
        quote.asset.trim().is_empty() || quote.venue.trim().is_empty() || quote.price <= 0.0
    }) {
        return Err(AstraError::ControlPlaneRejected(
            "warfare revenue market quotes require asset, venue, and positive price".into(),
        ));
    }
    if request
        .service_requests
        .iter()
        .any(|item| item.service.trim().is_empty() || item.buyer_segment.trim().is_empty())
    {
        return Err(AstraError::ControlPlaneRejected(
            "warfare revenue service requests require service and buyer segment".into(),
        ));
    }
    Ok(())
}

fn build_temporal_archaeology_finding(target: &RecoveryTargetInput) -> TemporalArchaeologyFinding {
    let inferred_format = infer_format(target.format_hint.as_deref(), &target.asset_id);
    let signal_bonus = target.corruption_signals.len().min(3) as u8;
    let recoverable_generations =
        (1 + target.snapshots_available.min(2) as u8 + signal_bonus).min(4);
    let confidence = clamp01(
        0.34 + (target.snapshots_available as f64 * 0.14)
            + (target.fragment_count.min(6) as f64 * 0.04)
            + if target.encrypted { 0.04 } else { 0.1 },
    );
    let mut evidence = vec![
        format!("inferred_format:{inferred_format}"),
        format!("snapshot_count:{}", target.snapshots_available),
        format!("fragment_count:{}", target.fragment_count),
    ];
    evidence.extend(
        target
            .corruption_signals
            .iter()
            .take(4)
            .map(|signal| format!("signal:{signal}")),
    );

    TemporalArchaeologyFinding {
        asset_id: target.asset_id.clone(),
        recoverable_generations,
        confidence: round2(confidence),
        evidence,
    }
}

fn build_semantic_repair_plan(target: &RecoveryTargetInput) -> SemanticRepairPlan {
    let inferred_format = infer_format(target.format_hint.as_deref(), &target.asset_id);
    let repair_actions = repair_actions_for(&inferred_format, &target.corruption_signals);
    let confidence = clamp01(
        0.45 + (target.snapshots_available as f64 * 0.08) + (repair_actions.len() as f64 * 0.03),
    );

    SemanticRepairPlan {
        asset_id: target.asset_id.clone(),
        inferred_format,
        repair_actions,
        confidence: round2(confidence),
    }
}

fn build_ghost_graph(targets: &[RecoveryTargetInput]) -> Vec<GhostTreeNode> {
    let mut directories = BTreeMap::<String, Vec<String>>::new();
    for target in targets {
        let directory = target
            .directory_hint
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("recovered-root")
            .to_string();
        directories
            .entry(directory)
            .or_default()
            .push(target.asset_id.clone());
    }

    directories
        .into_iter()
        .map(|(directory, mut member_assets)| {
            member_assets.sort();
            let confidence = clamp01(0.55 + member_assets.len() as f64 * 0.08);
            GhostTreeNode {
                directory,
                member_assets,
                confidence: round2(confidence),
            }
        })
        .collect()
}

fn build_encryption_assessment(target: &RecoveryTargetInput) -> EncryptionWeaknessAssessment {
    let weakness_class = if target
        .corruption_signals
        .iter()
        .any(|signal| contains_any(signal, &["partial", "header"]))
    {
        "partial_encryption_pattern"
    } else if target.asset_id.to_ascii_lowercase().ends_with(".enc") {
        "predictable_iv_assessment"
    } else {
        "weak_key_management_hygiene"
    };
    let mitigation = if target.snapshots_available > 0 {
        "restore from attested snapshot and rotate to authenticated encryption"
    } else {
        "quarantine ciphertext, validate key lifecycle, and rebuild from known-good structure"
    };
    let recovery_feasibility = if target.snapshots_available > 0 {
        "snapshot-assisted"
    } else if target.fragment_count > 1 {
        "fragment-assisted"
    } else {
        "metadata-assisted"
    };

    EncryptionWeaknessAssessment {
        asset_id: target.asset_id.clone(),
        weakness_class: weakness_class.into(),
        mitigation: mitigation.into(),
        recovery_feasibility: recovery_feasibility.into(),
    }
}

fn build_fragment_reassembly_plan(target: &RecoveryTargetInput) -> FragmentReassemblyPlan {
    let fragments_observed = target.fragment_count.max(1);
    let contiguous_coverage = clamp01(
        0.28 + fragments_observed.min(8) as f64 * 0.08 + target.snapshots_available as f64 * 0.05,
    );
    let ordering_confidence = clamp01(
        0.35 + fragments_observed.min(8) as f64 * 0.06
            + target.corruption_signals.len() as f64 * 0.03,
    );

    FragmentReassemblyPlan {
        asset_id: target.asset_id.clone(),
        fragments_observed,
        contiguous_coverage: round2(contiguous_coverage),
        ordering_confidence: round2(ordering_confidence),
    }
}

fn build_memory_artifact_recovery(artifact: &MemoryArtifactInput) -> MemoryArtifactRecovery {
    let base_bytes = match artifact.content_class.to_ascii_lowercase().as_str() {
        "credentials" => 256,
        "token" => 192,
        "session" => 512,
        "code" => 4_096,
        "text" => 2_048,
        _ => 1_024,
    };
    let recovered_bytes = (base_bytes as f64 * clamp01(artifact.confidence)).round() as u64;

    MemoryArtifactRecovery {
        artifact_id: artifact.artifact_id.clone(),
        content_class: artifact.content_class.clone(),
        recovered_bytes: recovered_bytes.max(64),
        redaction_applied: artifact.contains_secrets,
    }
}

fn build_self_healing_protection(target: &RecoveryTargetInput) -> SelfHealingProtection {
    let inferred_format = infer_format(target.format_hint.as_deref(), &target.asset_id);
    let mut protection_plan = vec![
        "attested restore checkpoint every write window".into(),
        "manifest hash sealing".into(),
        "semantic structure validation before promote".into(),
    ];
    if target.fragment_count > 1 {
        protection_plan.push("fragment parity stripe".into());
    }
    if target.encrypted {
        protection_plan.push("envelope key rotation after restore".into());
    }
    protection_plan.push(format!("format-specific sentinel:{inferred_format}"));

    SelfHealingProtection {
        asset_id: target.asset_id.clone(),
        protection_plan,
        recommended_parity_shards: (2 + target.fragment_count.min(4)) as u8,
        max_recoverable_corruption_percent: round2(clamp01(
            0.4 + target.snapshots_available as f64 * 0.12 + target.fragment_count as f64 * 0.05,
        )),
        header_shadowed: target
            .corruption_signals
            .iter()
            .any(|signal| contains_any(signal, &["header", "trunc", "magic"])),
    }
}

fn build_behavioral_fingerprint(sample: &ThreatSampleInput) -> BehavioralDnaFingerprint {
    let dominant_genes = extract_genes(sample);
    let confidence =
        clamp01(0.46 + dominant_genes.len() as f64 * 0.06 + sample.packer_layers as f64 * 0.03);

    BehavioralDnaFingerprint {
        sample_id: sample.sample_id.clone(),
        dominant_genes,
        confidence: round2(confidence),
    }
}

fn build_unpacking_assessment(sample: &ThreatSampleInput) -> PolymorphicUnpackingAssessment {
    let suspected_layers =
        sample
            .packer_layers
            .max(if sample.behaviors.is_empty() { 0 } else { 1 });
    let mut static_safe_unwrap_steps = vec![
        "capture immutable sample hash".into(),
        "recover import table and string graph without live execution".into(),
        "map section entropy and relocation anomalies".into(),
    ];
    if suspected_layers > 0 {
        static_safe_unwrap_steps.push("perform bounded layer-by-layer static peeling".into());
    }
    if !sample.c2_indicators.is_empty() {
        static_safe_unwrap_steps.push("extract network beacons into IOC bundle".into());
    }

    PolymorphicUnpackingAssessment {
        sample_id: sample.sample_id.clone(),
        suspected_layers,
        static_safe_unwrap_steps,
        execution_blocked: true,
    }
}

fn build_rootkit_sonar_finding(sample: &ThreatSampleInput) -> RootkitDepthSonarFinding {
    let hidden_surface_score = clamp01(
        0.22 + (sample
            .privileges
            .iter()
            .filter(|item| contains_any(item, &["kernel", "driver", "system"]))
            .count() as f64
            * 0.22)
            + (sample
                .touched_paths
                .iter()
                .filter(|item| {
                    contains_any(item, &["system32", "drivers", "/boot", "/lib/modules"])
                })
                .count() as f64
                * 0.1),
    );
    let suspicious_anchors = sample
        .privileges
        .iter()
        .chain(sample.touched_paths.iter())
        .filter(|item| contains_any(item, &["kernel", "driver", "boot", "system32"]))
        .take(6)
        .cloned()
        .collect::<Vec<_>>();

    RootkitDepthSonarFinding {
        sample_id: sample.sample_id.clone(),
        hidden_surface_score: round2(hidden_surface_score),
        suspicious_anchors,
        recommended_checks: vec![
            "verify signed driver inventory".into(),
            "compare kernel module baselines against attested state".into(),
            "inspect early-boot persistence hooks".into(),
        ],
    }
}

fn build_ransomware_reversal_plan(
    incident: &RansomwareIncidentInput,
) -> RansomwareTimeReversalPlan {
    let restorable_paths = if incident.recent_restore_points == 0 {
        incident.impacted_paths / 4
    } else {
        incident
            .impacted_paths
            .min(incident.recent_restore_points * 40)
    };
    let restore_confidence = clamp01(
        0.25 + incident.recent_restore_points.min(5) as f64 * 0.12
            + incident.encrypted_extensions.len().min(4) as f64 * 0.04,
    );

    RansomwareTimeReversalPlan {
        incident_id: incident.incident_id.clone(),
        restorable_paths,
        restore_confidence: round2(restore_confidence),
        required_artifacts: vec![
            "immutable restore point".into(),
            "clean-room diff manifest".into(),
            "post-restore IOC sweep".into(),
        ],
    }
}

fn build_malware_vivisection_report(sample: &ThreatSampleInput) -> MalwareVivisectionReport {
    let mut kill_chain = Vec::new();
    if sample
        .behaviors
        .iter()
        .any(|item| contains_any(item, &["phish", "macro", "loader"]))
    {
        kill_chain.push("initial_access".into());
    }
    if sample
        .privileges
        .iter()
        .any(|item| contains_any(item, &["admin", "system", "kernel"]))
    {
        kill_chain.push("privilege_escalation".into());
    }
    if !sample.c2_indicators.is_empty() {
        kill_chain.push("command_and_control".into());
    }
    if sample
        .behaviors
        .iter()
        .any(|item| contains_any(item, &["encrypt", "exfil", "lateral"]))
    {
        kill_chain.push("impact".into());
    }
    if kill_chain.is_empty() {
        kill_chain.push("discovery".into());
    }

    MalwareVivisectionReport {
        sample_id: sample.sample_id.clone(),
        kill_chain,
        iocs: extract_iocs(sample),
        blocked_execution: true,
    }
}

fn build_containment_report(sample: &ThreatSampleInput) -> CounterIntelligenceContainmentReport {
    let sinkhole_candidates = sample
        .c2_indicators
        .iter()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let mut block_rules = sample
        .c2_indicators
        .iter()
        .map(|indicator| format!("deny outbound to {indicator}"))
        .collect::<Vec<_>>();
    block_rules.extend(
        sample
            .touched_paths
            .iter()
            .filter(|path| contains_any(path, &["startup", "run", "autorun", "system32"]))
            .map(|path| format!("monitor persistence anchor {path}")),
    );
    if block_rules.is_empty() {
        block_rules.push("quarantine sample and freeze egress until triage completes".into());
    }
    let evidence_bundle_id = sha3_256_hex(
        format!(
            "{}:{}:{}",
            sample.sample_id,
            sample.c2_indicators.join("|"),
            sample.touched_paths.join("|")
        )
        .as_bytes(),
    );

    CounterIntelligenceContainmentReport {
        sample_id: sample.sample_id.clone(),
        sinkhole_candidates,
        block_rules,
        evidence_bundle_id,
        active_payloads_disabled: true,
    }
}

fn build_threat_lineage_match(sample: &ThreatSampleInput) -> ThreatLineageMatch {
    let family = infer_family(sample);
    let confidence = clamp01(
        0.42 + if sample.family_hint.is_some() {
            0.25
        } else {
            0.0
        } + sample.behaviors.len().min(5) as f64 * 0.06,
    );

    ThreatLineageMatch {
        sample_id: sample.sample_id.clone(),
        family,
        confidence: round2(confidence),
    }
}

fn build_zero_day_forecast(item: &SoftwareRiskInput) -> ZeroDayRiskForecast {
    let mut predicted_classes = Vec::new();
    if item.attack_surface_score >= 0.7 {
        predicted_classes.push("remote_code_execution".into());
    }
    if item.recent_cve_count >= 6 {
        predicted_classes.push("known_exploit_reuse".into());
    }
    if item.days_since_last_patch >= 45 {
        predicted_classes.push("privilege_escalation".into());
    }
    if predicted_classes.is_empty() {
        predicted_classes.push("configuration_drift".into());
    }

    let mut hardening_actions = if item.hardening_available.is_empty() {
        vec![
            "enforce staged patch rollout".into(),
            "enable memory-safe runtime controls".into(),
            "add exploit-prevention telemetry".into(),
        ]
    } else {
        item.hardening_available.clone()
    };
    hardening_actions.sort();
    hardening_actions.dedup();

    let probability = clamp01(
        0.18 + item.attack_surface_score * 0.45
            + (item.recent_cve_count.min(10) as f64 * 0.03)
            + if item.days_since_last_patch >= 30 {
                0.12
            } else {
                0.0
            },
    );

    ZeroDayRiskForecast {
        product: item.product.clone(),
        version: item.version.clone(),
        predicted_classes,
        probability: round2(probability),
        hardening_actions,
    }
}

fn build_arbitrage_opportunities(quotes: &[MarketQuoteInput]) -> Vec<ArbitrageOpportunityReport> {
    let mut by_asset = BTreeMap::<String, Vec<&MarketQuoteInput>>::new();
    for quote in quotes.iter().filter(|quote| quote.compliance_allowed) {
        by_asset.entry(quote.asset.clone()).or_default().push(quote);
    }

    let mut opportunities = Vec::new();
    for (asset, asset_quotes) in by_asset {
        if asset_quotes.len() < 2 {
            continue;
        }
        let Some(buy) = asset_quotes
            .iter()
            .min_by(|left, right| left.price.total_cmp(&right.price))
        else {
            continue;
        };
        let Some(sell) = asset_quotes
            .iter()
            .max_by(|left, right| left.price.total_cmp(&right.price))
        else {
            continue;
        };
        if buy.venue == sell.venue || buy.price <= 0.0 {
            continue;
        }
        let fee_drag = (buy.fee_bps + sell.fee_bps) / 10_000.0;
        let net_spread = ((sell.price - buy.price) / buy.price) - fee_drag;
        if net_spread < MIN_ARBITRAGE_SPREAD {
            continue;
        }
        let avg_liquidity = (buy.liquidity_score + sell.liquidity_score) / 2.0;
        let risk_score = clamp01(0.85 - avg_liquidity * 0.6 + fee_drag * 5.0);

        opportunities.push(ArbitrageOpportunityReport {
            asset,
            buy_venue: buy.venue.clone(),
            sell_venue: sell.venue.clone(),
            net_spread_percent: round2(net_spread * 100.0),
            risk_score: round2(risk_score),
            executable: false,
        });
    }

    opportunities
}

fn build_content_output_plan(channel: &ContentChannelInput) -> ContentOutputPlan {
    let monetization_base = match channel.monetization.to_ascii_lowercase().as_str() {
        "subscription" => 180.0,
        "sponsorship" => 220.0,
        "affiliate" => 140.0,
        "lead_gen" => 260.0,
        _ => 90.0,
    };
    let content_multiplier = match channel.content_type.to_ascii_lowercase().as_str() {
        "technical_article" => 1.25,
        "dataset" => 1.35,
        "briefing" => 1.1,
        "video" => 1.2,
        _ => 1.0,
    };
    let expected_revenue_30d =
        monetization_base * content_multiplier * channel.capacity_score.max(0.15);

    ContentOutputPlan {
        channel: channel.channel.clone(),
        content_type: channel.content_type.clone(),
        monetization: channel.monetization.clone(),
        expected_revenue_30d: round2(expected_revenue_30d),
    }
}

fn build_micro_service_offer(item: &ServiceRequestInput) -> MicroServiceOffer {
    let urgency = clamp01(item.urgency);
    let quoted_price_usd = item
        .budget_usd
        .min(item.budget_usd * (0.72 + urgency * 0.18));
    let turnaround_hours = (72.0 - urgency * 48.0).round().clamp(8.0, 72.0) as u32;

    MicroServiceOffer {
        service: item.service.clone(),
        buyer_segment: item.buyer_segment.clone(),
        quoted_price_usd: round2(quoted_price_usd.max(25.0)),
        turnaround_hours,
    }
}

fn build_bounty_assessment(item: &BountyProgramInput) -> BountyOpportunityAssessment {
    let safe_scope = item.authorized_assets_only && !item.exploit_required;
    let eligible = safe_scope && !contains_any(&item.scope, &["third-party", "public internet"]);
    let reason = if eligible {
        "scope is authorization-bound; only report drafting and evidence packaging are allowed"
    } else if !item.authorized_assets_only {
        "rejected because authorization is not limited to owned or explicitly approved assets"
    } else if item.exploit_required {
        "rejected because live exploit execution is outside guardrails"
    } else {
        "rejected because scope is too broad for compliance-safe triage"
    };

    BountyOpportunityAssessment {
        program: item.program.clone(),
        eligible,
        reason: reason.into(),
        expected_reward_usd: if eligible {
            round2(item.reward_usd * 0.35)
        } else {
            0.0
        },
    }
}

fn build_digital_asset_activation(item: &OwnedAssetInput) -> DigitalAssetActivation {
    let utilization = clamp01(item.utilization);
    let action = match item.asset_type.to_ascii_lowercase().as_str() {
        "domain" => "list_for_lease",
        "dataset" => "package_for_subscription",
        "api" => "publish_metered_plan",
        "model" => "license_inference_access",
        _ => "reactivate_and_catalog",
    };
    let expected_monthly_usd = ((1.0 - utilization) * 180.0 - item.maintenance_cost_usd).max(0.0);

    DigitalAssetActivation {
        asset_id: item.asset_id.clone(),
        action: action.into(),
        expected_monthly_usd: round2(expected_monthly_usd),
    }
}

fn build_yield_allocations(options: &[YieldOptionInput]) -> Vec<YieldAllocation> {
    let candidates = options
        .iter()
        .filter(|item| item.compliance_allowed && item.expected_return > 0.0)
        .map(|item| {
            let risk = item.risk_score.max(0.05);
            let score = (item.expected_return.max(0.0) + 0.01)
                / (1.0 + risk + item.lock_days as f64 / 365.0);
            (item, score)
        })
        .collect::<Vec<_>>();

    let total_score = candidates.iter().map(|(_, score)| score).sum::<f64>();
    if total_score <= f64::EPSILON {
        return Vec::new();
    }

    candidates
        .into_iter()
        .map(|(item, score)| YieldAllocation {
            option: item.name.clone(),
            allocation_percent: round2(score / total_score * 100.0),
            rationale: format!(
                "risk-adjusted return {:.2}% with {} day lock",
                item.expected_return * 100.0,
                item.lock_days
            ),
        })
        .collect()
}

fn build_market_sentiment_report(signals: &[MarketSignalInput]) -> MarketSentimentAlchemyReport {
    if signals.is_empty() {
        return MarketSentimentAlchemyReport {
            dominant_topic: "none".into(),
            composite_sentiment: 0.0,
            confidence: 0.0,
        };
    }

    let weighted_sum = signals
        .iter()
        .map(|signal| signal.sentiment * clamp01(signal.confidence))
        .sum::<f64>();
    let confidence_sum = signals
        .iter()
        .map(|signal| clamp01(signal.confidence))
        .sum::<f64>()
        .max(f64::EPSILON);
    let dominant_topic = signals
        .iter()
        .max_by(|left, right| {
            (left.sentiment.abs() * left.confidence)
                .total_cmp(&(right.sentiment.abs() * right.confidence))
        })
        .map(|signal| signal.topic.clone())
        .unwrap_or_else(|| "none".into());

    MarketSentimentAlchemyReport {
        dominant_topic,
        composite_sentiment: round2((weighted_sum / confidence_sum).clamp(-1.0, 1.0)),
        confidence: round2((confidence_sum / signals.len() as f64).clamp(0.0, 1.0)),
    }
}

fn build_freelance_dispatch_plan(lead: &FreelanceLeadInput) -> FreelanceDispatchPlan {
    let fit = clamp01(lead.fit_score);
    let proposal_strength = clamp01(0.4 + fit * 0.5 + (lead.budget_usd / 2_000.0).min(0.1));
    let expected_value_usd = round2(lead.budget_usd * (0.45 + fit * 0.4));

    FreelanceDispatchPlan {
        platform: lead.platform.clone(),
        project_type: lead.project_type.clone(),
        proposal_strength: round2(proposal_strength),
        expected_value_usd,
    }
}

fn build_revenue_portfolio_report(
    arbitrage: &[ArbitrageOpportunityReport],
    content: &[ContentOutputPlan],
    services: &[MicroServiceOffer],
    bounties: &[BountyOpportunityAssessment],
    assets: &[DigitalAssetActivation],
    yield_allocations: &[YieldAllocation],
    sentiment: &MarketSentimentAlchemyReport,
    freelance: &[FreelanceDispatchPlan],
) -> RevenuePortfolioOrganismReport {
    let service_total = services
        .iter()
        .map(|item| item.quoted_price_usd)
        .sum::<f64>();
    let content_total = content
        .iter()
        .map(|item| item.expected_revenue_30d)
        .sum::<f64>();
    let bounty_total = bounties
        .iter()
        .filter(|item| item.eligible)
        .map(|item| item.expected_reward_usd)
        .sum::<f64>();
    let asset_total = assets
        .iter()
        .map(|item| item.expected_monthly_usd)
        .sum::<f64>();
    let yield_total = yield_allocations
        .iter()
        .map(|item| item.allocation_percent / 100.0 * 180.0)
        .sum::<f64>();
    let freelance_total = freelance
        .iter()
        .map(|item| item.expected_value_usd)
        .sum::<f64>();
    let arbitrage_total = arbitrage
        .iter()
        .map(|item| item.net_spread_percent.max(0.0) * 5.0)
        .sum::<f64>()
        * 0.1;
    let sentiment_modifier = if sentiment.composite_sentiment > 0.4 {
        1.05
    } else if sentiment.composite_sentiment < -0.4 {
        0.92
    } else {
        1.0
    };

    let expected_monthly_usd = round2(
        (service_total
            + content_total
            + bounty_total
            + asset_total
            + yield_total
            + freelance_total
            + arbitrage_total)
            * sentiment_modifier,
    );

    let active_streams = [
        !arbitrage.is_empty(),
        !content.is_empty(),
        !services.is_empty(),
        bounties.iter().any(|item| item.eligible),
        !assets.is_empty(),
        !yield_allocations.is_empty(),
        !freelance.is_empty(),
    ]
    .into_iter()
    .filter(|active| *active)
    .count();
    let concentration_penalty = if active_streams <= 2 { 0.18 } else { 0.06 };
    let resilience_score = clamp01(0.36 + active_streams as f64 * 0.09 - concentration_penalty);

    let mut diversification_notes = Vec::new();
    diversification_notes.push(format!(
        "{} active advisory revenue streams",
        active_streams
    ));
    if bounties.iter().any(|item| !item.eligible) {
        diversification_notes.push("unsafe bounty scopes were excluded from the portfolio".into());
    }
    if arbitrage.iter().any(|item| !item.executable) {
        diversification_notes
            .push("arbitrage remains advisory-only and is not auto-executed".into());
    }
    if active_streams < 3 {
        diversification_notes
            .push("portfolio concentration is elevated; add more compliant streams".into());
    } else {
        diversification_notes
            .push("portfolio is diversified across multiple compliant channels".into());
    }

    RevenuePortfolioOrganismReport {
        expected_monthly_usd,
        resilience_score: round2(resilience_score),
        diversification_notes,
    }
}

fn infer_format(format_hint: Option<&str>, asset_id: &str) -> String {
    if let Some(format) = format_hint {
        let trimmed = format.trim();
        if !trimmed.is_empty() {
            return trimmed.to_ascii_lowercase();
        }
    }

    let lower = asset_id.to_ascii_lowercase();
    if let Some(extension) = lower.rsplit('.').next() {
        return extension.to_string();
    }

    "binary".into()
}

fn repair_actions_for(inferred_format: &str, corruption_signals: &[String]) -> Vec<String> {
    let mut actions = match inferred_format {
        "pdf" => vec![
            "rebuild xref map".into(),
            "repair page tree pointers".into(),
        ],
        "sqlite" | "db" => {
            vec![
                "reconstruct page map".into(),
                "validate freelist and wal pointers".into(),
            ]
        }
        "jpg" | "jpeg" => vec![
            "repair marker chain".into(),
            "rebuild thumbnail/exif envelope".into(),
        ],
        "zip" | "docx" | "xlsx" => {
            vec![
                "recalculate directory records".into(),
                "repair package manifest".into(),
            ]
        }
        _ => vec![
            "rebuild structural header".into(),
            "recalculate length and checksum fields".into(),
        ],
    };

    if corruption_signals
        .iter()
        .any(|signal| contains_any(signal, &["trunc", "tail", "short"]))
    {
        actions.push("infer missing tail from structural grammar".into());
    }
    if corruption_signals
        .iter()
        .any(|signal| contains_any(signal, &["pointer", "offset"]))
    {
        actions.push("recalculate logical offsets from surviving layout".into());
    }
    if actions.is_empty() {
        actions.push("apply semantic validation sweep".into());
    }
    actions
}

fn extract_genes(sample: &ThreatSampleInput) -> Vec<String> {
    let mut genes = BTreeSet::new();
    for value in sample
        .behaviors
        .iter()
        .chain(sample.privileges.iter())
        .chain(sample.c2_indicators.iter())
    {
        let normalized = value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>();
        for gene in normalized.split('-').filter(|part| part.len() > 2) {
            genes.insert(gene.to_string());
        }
    }
    if genes.is_empty() {
        genes.insert("generic".into());
        genes.insert("loader".into());
    }
    genes.into_iter().take(8).collect()
}

fn extract_iocs(sample: &ThreatSampleInput) -> Vec<String> {
    let mut iocs = BTreeSet::new();
    for indicator in &sample.c2_indicators {
        iocs.insert(indicator.clone());
    }
    for path in &sample.touched_paths {
        if contains_any(
            path,
            &["run", "startup", "temp", "system32", "/tmp", "/boot"],
        ) {
            iocs.insert(path.clone());
        }
    }
    iocs.into_iter().collect()
}

fn infer_family(sample: &ThreatSampleInput) -> String {
    if let Some(family) = &sample.family_hint {
        if !family.trim().is_empty() {
            return family.clone();
        }
    }
    if sample
        .behaviors
        .iter()
        .any(|item| contains_any(item, &["encrypt", "ransom"]))
    {
        return "ransomware".into();
    }
    if sample
        .behaviors
        .iter()
        .any(|item| contains_any(item, &["steal", "credential", "exfil"]))
    {
        return "credential-stealer".into();
    }
    if sample
        .privileges
        .iter()
        .any(|item| contains_any(item, &["kernel", "driver"]))
    {
        return "rootkit".into();
    }
    if sample.packer_layers >= 2 {
        return "packed-loader".into();
    }
    "unknown-family".into()
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    let lower = value.to_ascii_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warfare_recovery_builds_protection_and_redacts_memory() {
        let mut engine = WarfareEditionEngine::new();
        let report = engine
            .orchestrate_recovery(WarfareRecoveryRequest {
                case_id: "case-001".into(),
                targets: vec![RecoveryTargetInput {
                    asset_id: "photos/archive.jpg".into(),
                    format_hint: None,
                    corruption_signals: vec!["header_damage".into(), "partial_overwrite".into()],
                    fragment_count: 3,
                    snapshots_available: 2,
                    encrypted: true,
                    directory_hint: Some("photos".into()),
                }],
                memory_artifacts: vec![MemoryArtifactInput {
                    artifact_id: "ram-1".into(),
                    content_class: "credentials".into(),
                    contains_secrets: true,
                    confidence: 0.84,
                }],
                protect_outputs: true,
                attestor: None,
                notarize_to_chain: false,
            })
            .expect("recovery should succeed");

        assert_eq!(report.self_healing_data_organism.len(), 1);
        assert!(report.memory_phantom_extraction[0].redaction_applied);
        assert_eq!(engine.status().protected_assets, 1);
        assert!(!report.encryption_entropy_reversal.is_empty());
    }

    #[test]
    fn warfare_defense_updates_immune_memory_and_blocks_execution() {
        let mut engine = WarfareEditionEngine::new();
        let report = engine
            .orchestrate_defense(WarfareDefenseRequest {
                samples: vec![ThreatSampleInput {
                    sample_id: "sample-a".into(),
                    family_hint: Some("night-owl".into()),
                    behaviors: vec!["loader encrypt exfil".into()],
                    packer_layers: 3,
                    c2_indicators: vec!["c2.example.test:443".into()],
                    privileges: vec!["kernel".into(), "admin".into()],
                    touched_paths: vec!["C:/Windows/System32/drivers/nx.sys".into()],
                }],
                ransomware_incidents: vec![RansomwareIncidentInput {
                    incident_id: "incident-a".into(),
                    encrypted_extensions: vec![".locked".into()],
                    recent_restore_points: 2,
                    impacted_paths: 40,
                }],
                software_inventory: vec![SoftwareRiskInput {
                    product: "astra-agent".into(),
                    version: "1.2.3".into(),
                    recent_cve_count: 8,
                    days_since_last_patch: 60,
                    attack_surface_score: 0.81,
                    hardening_available: vec!["enable sandbox".into()],
                }],
                attestor: None,
                notarize_to_chain: false,
            })
            .expect("defense should succeed");

        assert!(report.polymorphic_unpacking_engine[0].execution_blocked);
        assert!(report.counter_intelligence_containment[0].active_payloads_disabled);
        assert_eq!(report.autonomous_immune_memory.antibodies_created, 1);
        assert!(!report.zero_day_synthesis_predictor[0]
            .predicted_classes
            .is_empty());
        assert_eq!(engine.status().immune_antibodies, 1);
    }

    #[test]
    fn warfare_revenue_rejects_unsafe_bounty_paths_and_builds_portfolio() {
        let mut engine = WarfareEditionEngine::new();
        let report = engine
            .orchestrate_revenue(WarfareRevenueRequest {
                market_quotes: vec![
                    MarketQuoteInput {
                        asset: "AST".into(),
                        venue: "venue-a".into(),
                        price: 100.0,
                        fee_bps: 5.0,
                        liquidity_score: 0.92,
                        compliance_allowed: true,
                    },
                    MarketQuoteInput {
                        asset: "AST".into(),
                        venue: "venue-b".into(),
                        price: 103.0,
                        fee_bps: 6.0,
                        liquidity_score: 0.88,
                        compliance_allowed: true,
                    },
                ],
                content_channels: vec![ContentChannelInput {
                    channel: "substack".into(),
                    content_type: "technical_article".into(),
                    monetization: "subscription".into(),
                    capacity_score: 0.75,
                }],
                service_requests: vec![ServiceRequestInput {
                    service: "security review".into(),
                    buyer_segment: "enterprise".into(),
                    budget_usd: 750.0,
                    urgency: 0.8,
                    authorized: true,
                }],
                bounty_programs: vec![BountyProgramInput {
                    program: "unsafe-program".into(),
                    scope: "public internet".into(),
                    reward_usd: 2_000.0,
                    authorized_assets_only: false,
                    exploit_required: true,
                }],
                owned_assets: vec![OwnedAssetInput {
                    asset_id: "dataset-1".into(),
                    asset_type: "dataset".into(),
                    utilization: 0.2,
                    maintenance_cost_usd: 20.0,
                    monetizable: true,
                }],
                yield_options: vec![YieldOptionInput {
                    name: "treasury-bills".into(),
                    expected_return: 0.08,
                    lock_days: 30,
                    risk_score: 0.1,
                    compliance_allowed: true,
                }],
                market_signals: vec![MarketSignalInput {
                    topic: "AST".into(),
                    sentiment: 0.6,
                    confidence: 0.8,
                }],
                freelance_leads: vec![FreelanceLeadInput {
                    platform: "Upwork".into(),
                    project_type: "data analysis".into(),
                    budget_usd: 300.0,
                    fit_score: 0.86,
                }],
                attestor: None,
                notarize_to_chain: false,
            })
            .expect("revenue should succeed");

        assert_eq!(report.arbitrage_radar.len(), 1);
        assert!(!report.arbitrage_radar[0].executable);
        assert!(!report.autonomous_bounty_hunter[0].eligible);
        assert!(!report.yield_optimization_cortex.is_empty());
        assert!(report.revenue_portfolio_organism.expected_monthly_usd > 0.0);
    }
}

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::common::{now_ms, sha3_hex};

use super::{Citation, DomainPolicyMode, ResearchSource};

const FORMULA_VERSION: &str = "enterprise_research_calculus_v1";
const EPS: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorMode {
    PublicPage,
    OfficialApi,
    LicensedFeed,
    UserExport,
    ResearchPartner,
    ManualReview,
}

impl Default for ConnectorMode {
    fn default() -> Self {
        Self::PublicPage
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    Surface,
    Deep,
    Dark,
}

impl Default for SourceClass {
    fn default() -> Self {
        Self::Surface
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Draft,
    Review,
    Pilot,
    Production,
    Suspended,
    Retired,
}

impl Default for LifecycleState {
    fn default() -> Self {
        Self::Pilot
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchApproval {
    pub reviewer_id: String,
    #[serde(default = "default_approval_weight")]
    pub weight: f64,
    #[serde(default)]
    pub approved: bool,
}

fn default_approval_weight() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAuthorization {
    pub host: String,
    #[serde(default)]
    pub authorized: Option<bool>,
    #[serde(default)]
    pub terms_ok: Option<bool>,
    #[serde(default)]
    pub robots_ok: Option<bool>,
    #[serde(default)]
    pub privacy_ok: Option<bool>,
    #[serde(default)]
    pub governance_ok: Option<bool>,
    #[serde(default)]
    pub jurisdiction_ok: Option<bool>,
    #[serde(default)]
    pub harm_avoidance_ok: Option<bool>,
    #[serde(default)]
    pub non_interaction_ok: Option<bool>,
    #[serde(default)]
    pub secure_handling_ok: Option<bool>,
    #[serde(default)]
    pub rate_limit_remaining: Option<u32>,
    #[serde(default)]
    pub rate_limit_limit: Option<u32>,
    #[serde(default)]
    pub connector_mode: Option<ConnectorMode>,
    #[serde(default)]
    pub source_class: Option<SourceClass>,
    #[serde(default)]
    pub platform_class: Option<String>,
    #[serde(default)]
    pub relevance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyBudget {
    #[serde(default)]
    pub epsilon_used: f64,
    #[serde(default = "default_epsilon_max")]
    pub epsilon_max: f64,
    #[serde(default)]
    pub delta_used: f64,
    #[serde(default = "default_delta_max")]
    pub delta_max: f64,
}

impl Default for PrivacyBudget {
    fn default() -> Self {
        Self {
            epsilon_used: 0.0,
            epsilon_max: default_epsilon_max(),
            delta_used: 0.0,
            delta_max: default_delta_max(),
        }
    }
}

fn default_epsilon_max() -> f64 {
    1.0
}

fn default_delta_max() -> f64 {
    1e-6
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroTrustTelemetry {
    #[serde(default = "default_high_score")]
    pub auth_strength: f64,
    #[serde(default = "default_high_score")]
    pub endpoint_posture: f64,
    #[serde(default = "default_low_score")]
    pub action_sensitivity: f64,
    #[serde(default = "default_low_score")]
    pub anomaly_score: f64,
    #[serde(default = "default_low_score")]
    pub privilege_breadth: f64,
    #[serde(default = "default_low_score")]
    pub attack_surface: f64,
}

impl Default for ZeroTrustTelemetry {
    fn default() -> Self {
        Self {
            auth_strength: default_high_score(),
            endpoint_posture: default_high_score(),
            action_sensitivity: default_low_score(),
            anomaly_score: default_low_score(),
            privilege_breadth: default_low_score(),
            attack_surface: default_low_score(),
        }
    }
}

fn default_high_score() -> f64 {
    1.0
}

fn default_low_score() -> f64 {
    0.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTelemetry {
    #[serde(default = "default_queue_stable")]
    pub queue_stable: bool,
    #[serde(default)]
    pub slo_risk: f64,
    #[serde(default)]
    pub ops_risk: f64,
    #[serde(default)]
    pub drift: f64,
    #[serde(default)]
    pub anomaly: f64,
    #[serde(default)]
    pub mttd_ms: f64,
    #[serde(default)]
    pub mttr_ms: f64,
}

impl Default for RuntimeTelemetry {
    fn default() -> Self {
        Self {
            queue_stable: true,
            slo_risk: 0.0,
            ops_risk: 0.0,
            drift: 0.0,
            anomaly: 0.0,
            mttd_ms: 0.0,
            mttr_ms: 0.0,
        }
    }
}

fn default_queue_stable() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseThresholds {
    #[serde(default = "default_confidence_threshold")]
    pub confidence: f64,
    #[serde(default = "default_privacy_threshold")]
    pub privacy: f64,
    #[serde(default = "default_harm_threshold")]
    pub harm: f64,
    #[serde(default = "default_zero_trust_threshold")]
    pub zero_trust: f64,
    #[serde(default = "default_attack_threshold")]
    pub attack_surface: f64,
    #[serde(default = "default_incident_threshold")]
    pub incident: f64,
    #[serde(default = "default_sri_threshold")]
    pub service_readiness: f64,
    #[serde(default = "default_deep_dark_budget")]
    pub deep_dark_budget: f64,
    #[serde(default = "default_coverage_threshold")]
    pub coverage: f64,
}

impl Default for ReleaseThresholds {
    fn default() -> Self {
        Self {
            confidence: default_confidence_threshold(),
            privacy: default_privacy_threshold(),
            harm: default_harm_threshold(),
            zero_trust: default_zero_trust_threshold(),
            attack_surface: default_attack_threshold(),
            incident: default_incident_threshold(),
            service_readiness: default_sri_threshold(),
            deep_dark_budget: default_deep_dark_budget(),
            coverage: default_coverage_threshold(),
        }
    }
}

fn default_confidence_threshold() -> f64 {
    0.55
}

fn default_privacy_threshold() -> f64 {
    0.25
}

fn default_harm_threshold() -> f64 {
    0.25
}

fn default_zero_trust_threshold() -> f64 {
    0.55
}

fn default_attack_threshold() -> f64 {
    0.45
}

fn default_incident_threshold() -> f64 {
    1.0
}

fn default_sri_threshold() -> f64 {
    0.5
}

fn default_deep_dark_budget() -> f64 {
    0.1
}

fn default_coverage_threshold() -> f64 {
    0.4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskControl {
    pub risk_id: String,
    #[serde(default)]
    pub probability: f64,
    #[serde(default)]
    pub impact: f64,
    #[serde(default)]
    pub mitigation: f64,
    #[serde(default)]
    pub owner_ready: bool,
    #[serde(default = "default_risk_threshold")]
    pub threshold: f64,
}

fn default_risk_threshold() -> f64 {
    0.25
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseContext {
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub approvals: Vec<ResearchApproval>,
    #[serde(default)]
    pub source_authorizations: Vec<SourceAuthorization>,
    #[serde(default)]
    pub privacy_budget: PrivacyBudget,
    #[serde(default)]
    pub zero_trust: ZeroTrustTelemetry,
    #[serde(default)]
    pub runtime: RuntimeTelemetry,
    #[serde(default)]
    pub release_thresholds: ReleaseThresholds,
    #[serde(default)]
    pub lifecycle_state: LifecycleState,
    #[serde(default)]
    pub risk_register: Vec<RiskControl>,
}

impl Default for EnterpriseContext {
    fn default() -> Self {
        Self {
            actor_id: None,
            roles: Vec::new(),
            approvals: Vec::new(),
            source_authorizations: Vec::new(),
            privacy_budget: PrivacyBudget::default(),
            zero_trust: ZeroTrustTelemetry::default(),
            runtime: RuntimeTelemetry::default(),
            release_thresholds: ReleaseThresholds::default(),
            lifecycle_state: LifecycleState::default(),
            risk_register: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDenial {
    pub url: String,
    pub host: String,
    pub reason: String,
    pub formula: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub host: String,
    pub url: String,
    pub required_mode: ConnectorMode,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAssessment {
    pub url: String,
    pub host: String,
    pub platform_class: String,
    pub source_class: SourceClass,
    pub connector_mode: ConnectorMode,
    pub access_ok: bool,
    pub production_executable: bool,
    pub policy_mode: DomainPolicyMode,
    pub coverage_relevance: f64,
    pub barrier_potential: f64,
    pub barrier_respect_index: f64,
    pub deep_dark_exposure: f64,
    pub governance_residual: f64,
    pub zero_trust_risk: f64,
    pub zero_trust_gate: bool,
    pub authorization_request: Option<AuthorizationRequest>,
    pub denial: Option<PolicyDenial>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceAssessment {
    pub source_id: String,
    pub citation_id: String,
    pub reliability: f64,
    pub trust_score: f64,
    pub discounted_reliability: f64,
    pub freshness: f64,
    pub independence: f64,
    pub relevance: f64,
    pub weight: f64,
    pub signed_support: f64,
    pub privacy_exposure: f64,
    pub contradiction_flag: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceWeight {
    pub source_id: String,
    pub citation_id: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceNode {
    pub source_id: String,
    pub citation_id: String,
    pub artifact_hash: String,
    pub chain_hash: String,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseDecision {
    pub release_allowed: bool,
    pub confidence: f64,
    pub multi_platform_confidence: f64,
    pub privacy_leakage: f64,
    pub expected_harm: f64,
    pub reproducible: bool,
    pub accuracy_lcb: f64,
    pub contradiction_penalty: f64,
    pub stale_penalty: f64,
    pub effective_sample_size: f64,
    pub denied_reasons: Vec<String>,
}

impl Default for ReleaseDecision {
    fn default() -> Self {
        Self {
            release_allowed: false,
            confidence: 0.0,
            multi_platform_confidence: 0.0,
            privacy_leakage: 0.0,
            expected_harm: 0.0,
            reproducible: false,
            accuracy_lcb: 0.0,
            contradiction_penalty: 0.0,
            stale_penalty: 0.0,
            effective_sample_size: 0.0,
            denied_reasons: vec!["no evidence has been assessed".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessReport {
    pub composite_residual_index: f64,
    pub service_readiness_index: f64,
    pub deploy_allowed: bool,
    pub kill_switch: bool,
    pub halt: bool,
    pub risk_gate: bool,
    pub suspend: bool,
    pub incident_severity: f64,
    pub queue_stable: bool,
    pub slo_risk: f64,
    pub ops_risk: f64,
    pub lifecycle_state: LifecycleState,
    pub reasons: Vec<String>,
}

impl Default for ReadinessReport {
    fn default() -> Self {
        Self {
            composite_residual_index: 1.0,
            service_readiness_index: 0.0,
            deploy_allowed: false,
            kill_switch: true,
            halt: true,
            risk_gate: true,
            suspend: true,
            incident_severity: 1.0,
            queue_stable: true,
            slo_risk: 0.0,
            ops_risk: 0.0,
            lifecycle_state: LifecycleState::default(),
            reasons: vec!["no executable research action is available".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculusReport {
    pub formula_version: String,
    pub coverage: f64,
    pub platform_diversity_penalty: f64,
    pub release_decision: ReleaseDecision,
    pub readiness: ReadinessReport,
    pub source_assessments: Vec<SourceAssessment>,
    pub evidence_assessments: Vec<EvidenceAssessment>,
}

impl Default for CalculusReport {
    fn default() -> Self {
        Self {
            formula_version: FORMULA_VERSION.into(),
            coverage: 0.0,
            platform_diversity_penalty: 0.0,
            release_decision: ReleaseDecision::default(),
            readiness: ReadinessReport::default(),
            source_assessments: Vec::new(),
            evidence_assessments: Vec::new(),
        }
    }
}

pub fn evaluate_candidate_source(
    url: &str,
    host: &str,
    policy_mode: DomainPolicyMode,
    context: &EnterpriseContext,
) -> SourceAssessment {
    let auth = authorization_for_host(host, context);
    let source_class = auth
        .and_then(|entry| entry.source_class)
        .unwrap_or_else(|| classify_source(host, policy_mode));
    let connector_mode = auth
        .and_then(|entry| entry.connector_mode)
        .unwrap_or_else(|| default_connector_mode(host, source_class));
    let platform_class = auth
        .and_then(|entry| entry.platform_class.clone())
        .unwrap_or_else(|| classify_platform(host));
    let relevance = auth
        .and_then(|entry| entry.relevance)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);

    let mut notes = Vec::new();
    let mut barrier: f64 = 0.0;
    let mut deep_dark_exposure: f64 = 0.0;
    let mut denial_reason = None;

    if matches!(
        policy_mode,
        DomainPolicyMode::Blocked | DomainPolicyMode::Restricted
    ) {
        barrier += 1.0;
        denial_reason = Some(format!("domain policy is {:?}", policy_mode));
    }

    let surface_defaults_allowed = matches!(source_class, SourceClass::Surface);
    let authorized = bool_predicate(
        auth.and_then(|entry| entry.authorized),
        surface_defaults_allowed,
    );
    let terms_ok = bool_predicate(
        auth.and_then(|entry| entry.terms_ok),
        surface_defaults_allowed,
    );
    let robots_ok = bool_predicate(
        auth.and_then(|entry| entry.robots_ok),
        surface_defaults_allowed,
    );
    let privacy_ok = bool_predicate(
        auth.and_then(|entry| entry.privacy_ok),
        surface_defaults_allowed,
    );
    let rate_ok = rate_limit_ok(auth);
    for (ok, name, weight) in [
        (authorized, "authorization", 1.0),
        (terms_ok, "terms", 0.7),
        (robots_ok, "robots", 0.5),
        (privacy_ok, "privacy", 0.8),
        (rate_ok, "rate_limit", 0.6),
    ] {
        if !ok {
            barrier += weight;
            notes.push(format!("{name} predicate failed or is unknown"));
            if denial_reason.is_none() {
                denial_reason = Some(format!("{name} predicate failed or is unknown"));
            }
        }
    }

    let mut governance_residual = governance_residual(context, source_class);
    if !matches!(source_class, SourceClass::Surface) {
        let governance_ok = bool_predicate(auth.and_then(|entry| entry.governance_ok), false);
        let jurisdiction_ok = bool_predicate(auth.and_then(|entry| entry.jurisdiction_ok), false);
        let harm_ok = bool_predicate(auth.and_then(|entry| entry.harm_avoidance_ok), false);
        let non_interaction_ok =
            bool_predicate(auth.and_then(|entry| entry.non_interaction_ok), false);
        let secure_handling_ok =
            bool_predicate(auth.and_then(|entry| entry.secure_handling_ok), false);
        for (ok, name, weight) in [
            (governance_ok, "deep_dark_governance", 1.0),
            (jurisdiction_ok, "jurisdiction", 1.0),
            (harm_ok, "harm_avoidance", 1.0),
            (non_interaction_ok, "non_interaction", 1.0),
            (secure_handling_ok, "secure_evidence_handling", 1.0),
        ] {
            if !ok {
                deep_dark_exposure += weight;
                governance_residual += 0.2;
                notes.push(format!("{name} predicate failed or is unknown"));
                if denial_reason.is_none() {
                    denial_reason = Some(format!("{name} predicate failed or is unknown"));
                }
            }
        }
    }

    let zero_trust_risk = zero_trust_risk(&context.zero_trust);
    let privacy_residual = privacy_budget_residual(&context.privacy_budget);
    let zero_trust_gate = zero_trust_risk <= context.release_thresholds.zero_trust
        && context.zero_trust.attack_surface <= context.release_thresholds.attack_surface
        && privacy_residual == 0.0;
    if !zero_trust_gate {
        notes.push("zero-trust or privacy-budget gate failed".into());
        if denial_reason.is_none() {
            denial_reason = Some("zero-trust or privacy-budget gate failed".into());
        }
    }

    let barrier_respect_index = (-barrier).exp().clamp(0.0, 1.0);
    let access_ok = denial_reason.is_none();
    let production_executable = access_ok
        && governance_residual == 0.0
        && zero_trust_gate
        && deep_dark_exposure <= context.release_thresholds.deep_dark_budget;
    let authorization_request = if access_ok {
        None
    } else {
        Some(AuthorizationRequest {
            host: host.into(),
            url: url.into(),
            required_mode: connector_mode,
            reason: denial_reason
                .clone()
                .unwrap_or_else(|| "source is outside the production action set".into()),
        })
    };
    let denial = denial_reason.map(|reason| PolicyDenial {
        url: url.into(),
        host: host.into(),
        reason,
        formula: "AccessOK/Aprod".into(),
    });

    SourceAssessment {
        url: url.into(),
        host: host.into(),
        platform_class,
        source_class,
        connector_mode,
        access_ok,
        production_executable,
        policy_mode,
        coverage_relevance: relevance,
        barrier_potential: barrier,
        barrier_respect_index,
        deep_dark_exposure,
        governance_residual,
        zero_trust_risk,
        zero_trust_gate,
        authorization_request,
        denial,
        notes,
    }
}

pub fn finalize_report(
    context: &EnterpriseContext,
    source_assessments: &[SourceAssessment],
    sources: &[ResearchSource],
    citations: &[Citation],
) -> (CalculusReport, Vec<EvidenceWeight>, Vec<ProvenanceNode>) {
    let coverage = authorized_coverage(source_assessments);
    let diversity_penalty = platform_diversity_penalty(source_assessments);
    let mut evidence_assessments = assess_evidence(sources, citations);
    normalize_evidence_weights(&mut evidence_assessments);
    let release_decision =
        release_decision(context, coverage, diversity_penalty, &evidence_assessments);
    let readiness = readiness_report(context, source_assessments, &release_decision);
    let evidence_weights = evidence_assessments
        .iter()
        .map(|evidence| EvidenceWeight {
            source_id: evidence.source_id.clone(),
            citation_id: evidence.citation_id.clone(),
            weight: evidence.weight,
        })
        .collect::<Vec<_>>();
    let provenance_chain = build_provenance_chain(sources, citations);
    (
        CalculusReport {
            formula_version: FORMULA_VERSION.into(),
            coverage,
            platform_diversity_penalty: diversity_penalty,
            release_decision,
            readiness,
            source_assessments: source_assessments.to_vec(),
            evidence_assessments,
        },
        evidence_weights,
        provenance_chain,
    )
}

pub fn enterprise_score(
    credibility: f64,
    relevance: f64,
    freshness: f64,
    independence: f64,
    diversity_penalty: f64,
) -> f64 {
    let raw = credibility * 0.36 + relevance * 0.30 + freshness * 0.18 + independence * 0.16;
    (raw * (-0.35 * diversity_penalty.max(0.0)).exp()).clamp(0.0, 1.0)
}

pub fn calibrated_confidence(evidence: &[(f64, f64, f64, bool)]) -> f64 {
    if evidence.is_empty() {
        return 0.0;
    }
    let mut assessments = evidence
        .iter()
        .enumerate()
        .map(
            |(index, (reliability, freshness, relevance, contradiction))| EvidenceAssessment {
                source_id: format!("source_{index}"),
                citation_id: format!("citation_{index}"),
                reliability: reliability.clamp(0.0, 1.0),
                trust_score: reliability.clamp(0.0, 1.0),
                discounted_reliability: reliability.clamp(0.0, 1.0),
                freshness: freshness.clamp(0.0, 1.0),
                independence: 1.0,
                relevance: relevance.clamp(0.0, 1.0),
                weight: 0.0,
                signed_support: reliability.clamp(0.0, 1.0) * 2.0 - 0.8,
                privacy_exposure: 0.0,
                contradiction_flag: *contradiction,
            },
        )
        .collect::<Vec<_>>();
    normalize_evidence_weights(&mut assessments);
    confidence_from_evidence(&assessments).0
}

fn assess_evidence(sources: &[ResearchSource], citations: &[Citation]) -> Vec<EvidenceAssessment> {
    let mut host_counts = HashMap::<String, usize>::new();
    for source in sources {
        *host_counts.entry(source.host.clone()).or_default() += 1;
    }

    sources
        .iter()
        .map(|source| {
            let citation_id = citations
                .iter()
                .find(|citation| citation.url == source.url)
                .map(|citation| citation.citation_id.clone())
                .unwrap_or_else(|| "uncited".into());
            let host_count = host_counts.get(&source.host).copied().unwrap_or(1).max(1);
            let independence = if source.duplicate {
                0.35
            } else {
                (1.0 / host_count as f64).sqrt().clamp(0.45, 1.0)
            };
            let reliability = (source.extraction_confidence * 0.45
                + source.authority_score * 0.35
                + source.citation_density * 0.20)
                .clamp(0.0, 1.0);
            let trust_score = trust_score_for_source(source);
            let discounted_reliability =
                (reliability * trust_score * (1.0 - privacy_exposure(source))).clamp(0.0, 1.0);
            let contradiction_flag = contradiction_marker(&source.title);
            EvidenceAssessment {
                source_id: source.source_id.clone(),
                citation_id,
                reliability,
                trust_score,
                discounted_reliability,
                freshness: if source.duplicate { 0.72 } else { 0.96 },
                independence,
                relevance: source.citation_density.max(0.35).clamp(0.0, 1.0),
                weight: 0.0,
                signed_support: discounted_reliability * 2.0
                    - if contradiction_flag { 1.2 } else { 0.75 },
                privacy_exposure: privacy_exposure(source),
                contradiction_flag,
            }
        })
        .collect()
}

fn normalize_evidence_weights(evidence: &mut [EvidenceAssessment]) {
    if evidence.is_empty() {
        return;
    }
    let scores = evidence
        .iter()
        .map(|item| {
            (1.45 * item.discounted_reliability
                + 0.65 * item.freshness
                + 0.55 * item.independence
                + 0.85 * item.relevance)
                .exp()
        })
        .collect::<Vec<_>>();
    let denom = scores.iter().sum::<f64>().max(EPS);
    for (item, score) in evidence.iter_mut().zip(scores) {
        item.weight = (score / denom).clamp(0.0, 1.0);
    }
}

fn release_decision(
    context: &EnterpriseContext,
    coverage: f64,
    diversity_penalty: f64,
    evidence: &[EvidenceAssessment],
) -> ReleaseDecision {
    if evidence.is_empty() {
        return ReleaseDecision::default();
    }
    let (confidence, contradiction_penalty, stale_penalty) = confidence_from_evidence(evidence);
    let multi_platform_confidence = (confidence
        * (-1.0 * diversity_penalty
            - positive_part(context.release_thresholds.coverage - coverage))
        .exp())
    .clamp(0.0, 1.0);
    let privacy_leakage = evidence
        .iter()
        .map(|item| item.weight * item.privacy_exposure)
        .sum::<f64>()
        .clamp(0.0, 1.0);
    let expected_harm =
        (privacy_leakage * 0.45 + contradiction_penalty * 0.35 + stale_penalty * 0.20)
            .clamp(0.0, 1.0);
    let effective_sample_size = 1.0
        / evidence
            .iter()
            .map(|item| item.weight.powi(2))
            .sum::<f64>()
            .max(EPS);
    let accuracy_lcb = accuracy_lcb(multi_platform_confidence, effective_sample_size);
    let reproducible = evidence.iter().all(|item| item.citation_id != "uncited");
    let mut denied_reasons = Vec::new();
    if multi_platform_confidence < context.release_thresholds.confidence {
        denied_reasons.push("confidence is below release threshold".into());
    }
    if privacy_leakage > context.release_thresholds.privacy {
        denied_reasons.push("privacy leakage exceeds release threshold".into());
    }
    if expected_harm > context.release_thresholds.harm {
        denied_reasons.push("expected publication harm exceeds release threshold".into());
    }
    if !reproducible {
        denied_reasons.push("evidence provenance is incomplete".into());
    }
    ReleaseDecision {
        release_allowed: denied_reasons.is_empty(),
        confidence,
        multi_platform_confidence,
        privacy_leakage,
        expected_harm,
        reproducible,
        accuracy_lcb,
        contradiction_penalty,
        stale_penalty,
        effective_sample_size,
        denied_reasons,
    }
}

fn readiness_report(
    context: &EnterpriseContext,
    assessments: &[SourceAssessment],
    release: &ReleaseDecision,
) -> ReadinessReport {
    let executable_count = assessments
        .iter()
        .filter(|source| source.production_executable)
        .count();
    let halt = executable_count == 0;
    let risk_gate = risk_gate(context);
    let barrier = assessments
        .iter()
        .map(|source| source.barrier_potential)
        .sum::<f64>();
    let deep_dark = assessments
        .iter()
        .map(|source| {
            positive_part(source.deep_dark_exposure - context.release_thresholds.deep_dark_budget)
        })
        .sum::<f64>();
    let governance = assessments
        .iter()
        .map(|source| source.governance_residual)
        .sum::<f64>();
    let zero_trust_over =
        positive_part(zero_trust_risk(&context.zero_trust) - context.release_thresholds.zero_trust);
    let privacy_over = positive_part(release.privacy_leakage - context.release_thresholds.privacy);
    let harm_over = positive_part(release.expected_harm - context.release_thresholds.harm);
    let composite_residual_index = (0.25 * barrier
        + 0.20 * deep_dark
        + 0.18 * governance
        + 0.14 * zero_trust_over
        + 0.12 * privacy_over
        + 0.11 * harm_over)
        .clamp(0.0, 10.0);
    let incident_severity = (barrier * 0.12
        + deep_dark * 0.18
        + zero_trust_over * 0.32
        + context.runtime.anomaly * 0.18
        + context.runtime.drift * 0.20)
        .clamp(0.0, 10.0);
    let kill_switch = incident_severity > context.release_thresholds.incident
        || !context.runtime.queue_stable
        || halt;
    let service_readiness_index =
        (-composite_residual_index - context.runtime.slo_risk - context.runtime.ops_risk)
            .exp()
            .clamp(0.0, 1.0);
    let lifecycle_blocks_deploy = matches!(
        context.lifecycle_state,
        LifecycleState::Suspended | LifecycleState::Retired
    );
    let deploy_allowed = service_readiness_index >= context.release_thresholds.service_readiness
        && !kill_switch
        && !halt
        && risk_gate
        && !lifecycle_blocks_deploy;
    let suspend = kill_switch || !risk_gate || lifecycle_blocks_deploy;
    let mut reasons = Vec::new();
    if halt {
        reasons.push("production action set is empty".into());
    }
    if kill_switch {
        reasons.push("emergency stop rule is active".into());
    }
    if !risk_gate {
        reasons.push("enterprise risk gate failed".into());
    }
    if lifecycle_blocks_deploy {
        reasons.push("lifecycle state blocks deployment".into());
    }
    if service_readiness_index < context.release_thresholds.service_readiness {
        reasons.push("service readiness is below threshold".into());
    }
    ReadinessReport {
        composite_residual_index,
        service_readiness_index,
        deploy_allowed,
        kill_switch,
        halt,
        risk_gate,
        suspend,
        incident_severity,
        queue_stable: context.runtime.queue_stable,
        slo_risk: context.runtime.slo_risk,
        ops_risk: context.runtime.ops_risk,
        lifecycle_state: context.lifecycle_state,
        reasons,
    }
}

fn confidence_from_evidence(evidence: &[EvidenceAssessment]) -> (f64, f64, f64) {
    let support = evidence
        .iter()
        .map(|item| item.weight * item.signed_support)
        .sum::<f64>();
    let contradiction = contradiction_penalty(evidence);
    let stale = evidence
        .iter()
        .map(|item| item.weight * (1.0 - item.freshness))
        .sum::<f64>()
        .clamp(0.0, 1.0);
    (
        (support - 1.4 * contradiction - 0.9 * stale).sigmoid(),
        contradiction,
        stale,
    )
}

fn contradiction_penalty(evidence: &[EvidenceAssessment]) -> f64 {
    if evidence.len() < 2 {
        return 0.0;
    }
    let mut penalty = 0.0;
    for i in 0..evidence.len() {
        for j in (i + 1)..evidence.len() {
            if evidence[i].contradiction_flag != evidence[j].contradiction_flag {
                penalty += evidence[i].weight * evidence[j].weight;
            }
        }
    }
    (2.0 * penalty).clamp(0.0, 1.0)
}

fn authorized_coverage(assessments: &[SourceAssessment]) -> f64 {
    let denom = assessments
        .iter()
        .map(|source| source.coverage_relevance)
        .sum::<f64>();
    if denom <= EPS {
        return 0.0;
    }
    assessments
        .iter()
        .filter(|source| source.production_executable)
        .map(|source| source.coverage_relevance)
        .sum::<f64>()
        / denom
}

pub fn platform_diversity_penalty(assessments: &[SourceAssessment]) -> f64 {
    let executable = assessments
        .iter()
        .filter(|source| source.production_executable)
        .collect::<Vec<_>>();
    if executable.is_empty() {
        return 1.0;
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for source in executable {
        *counts.entry(source.platform_class.clone()).or_default() += 1;
    }
    let total = counts.values().sum::<usize>().max(1) as f64;
    let max_share = 0.65;
    counts
        .values()
        .map(|count| positive_part(*count as f64 / total - max_share))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

fn build_provenance_chain(
    sources: &[ResearchSource],
    citations: &[Citation],
) -> Vec<ProvenanceNode> {
    let mut previous = "genesis".to_string();
    let mut nodes = Vec::new();
    for source in sources {
        let citation_id = citations
            .iter()
            .find(|citation| citation.url == source.url)
            .map(|citation| citation.citation_id.clone())
            .unwrap_or_else(|| "uncited".into());
        let recorded_at_ms = now_ms();
        let chain_hash = sha3_hex(
            format!(
                "{}:{}:{}:{}",
                previous, source.content_hash, citation_id, recorded_at_ms
            )
            .as_bytes(),
        );
        previous = chain_hash.clone();
        nodes.push(ProvenanceNode {
            source_id: source.source_id.clone(),
            citation_id,
            artifact_hash: source.content_hash.clone(),
            chain_hash,
            recorded_at_ms,
        });
    }
    nodes
}

fn authorization_for_host<'a>(
    host: &str,
    context: &'a EnterpriseContext,
) -> Option<&'a SourceAuthorization> {
    context.source_authorizations.iter().find(|entry| {
        let configured = entry.host.trim().to_ascii_lowercase();
        !configured.is_empty()
            && (configured == host || host.ends_with(configured.trim_start_matches('*')))
    })
}

fn classify_source(host: &str, policy_mode: DomainPolicyMode) -> SourceClass {
    if host.ends_with(".onion") {
        SourceClass::Dark
    } else if matches!(policy_mode, DomainPolicyMode::BrowserRequired) {
        SourceClass::Deep
    } else {
        SourceClass::Surface
    }
}

fn default_connector_mode(host: &str, source_class: SourceClass) -> ConnectorMode {
    if matches!(source_class, SourceClass::Deep | SourceClass::Dark) {
        ConnectorMode::ManualReview
    } else if host.contains("api.") {
        ConnectorMode::OfficialApi
    } else {
        ConnectorMode::PublicPage
    }
}

fn classify_platform(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    if host.contains("reddit") {
        "reddit".into()
    } else if host.contains("twitter") || host == "x.com" || host.ends_with(".x.com") {
        "x".into()
    } else if host.contains("telegram") {
        "telegram".into()
    } else if host.contains("discord") {
        "discord".into()
    } else if host.ends_with(".gov") {
        "government".into()
    } else if host.ends_with(".edu") {
        "academic".into()
    } else {
        "web".into()
    }
}

fn bool_predicate(value: Option<bool>, default_value: bool) -> bool {
    value.unwrap_or(default_value)
}

fn rate_limit_ok(auth: Option<&SourceAuthorization>) -> bool {
    match auth.and_then(|entry| entry.rate_limit_remaining) {
        Some(0) => false,
        _ => true,
    }
}

fn governance_residual(context: &EnterpriseContext, source_class: SourceClass) -> f64 {
    let roles = context
        .roles
        .iter()
        .map(|role| role.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let abac_ok = roles.is_empty()
        || roles.contains("researcher")
        || roles.contains("analyst")
        || roles.contains("admin");
    let sod_ok = !(roles.contains("requester") && roles.contains("approver"));
    let quorum_ok = if matches!(source_class, SourceClass::Surface) {
        true
    } else {
        context
            .approvals
            .iter()
            .filter(|approval| approval.approved)
            .map(|approval| approval.weight)
            .sum::<f64>()
            >= 1.0
    };
    positive_part(1.0 - abac_ok as u8 as f64)
        + positive_part(1.0 - sod_ok as u8 as f64)
        + positive_part(1.0 - quorum_ok as u8 as f64)
}

fn zero_trust_risk(telemetry: &ZeroTrustTelemetry) -> f64 {
    sigmoid(
        -3.0 + 1.4 * telemetry.action_sensitivity
            + 1.2 * telemetry.anomaly_score
            + 1.1 * telemetry.privilege_breadth
            - 1.2 * telemetry.auth_strength
            - 1.0 * telemetry.endpoint_posture,
    )
}

fn privacy_budget_residual(budget: &PrivacyBudget) -> f64 {
    positive_part(budget.epsilon_used - budget.epsilon_max)
        + positive_part(budget.delta_used - budget.delta_max)
}

fn risk_gate(context: &EnterpriseContext) -> bool {
    context.risk_register.iter().all(|risk| {
        let residual = risk.probability
            * risk.impact
            * (1.0 - risk.mitigation)
            * if risk.owner_ready { 0.0 } else { 1.0 };
        residual <= risk.threshold && risk.mitigation >= 0.0 && risk.owner_ready
    })
}

fn trust_score_for_source(source: &ResearchSource) -> f64 {
    let volatility = if source.host.ends_with(".onion") {
        0.35
    } else {
        0.05
    };
    let manipulation = if source.host.contains("social") || source.host.contains("forum") {
        0.18
    } else {
        0.06
    };
    sigmoid(
        -0.35
            + 1.2 * source.authority_score
            + 0.9 * source.citation_density
            + 0.7 * source.extraction_confidence
            - 1.1 * volatility
            - 0.9 * manipulation
            - 0.6 * privacy_exposure(source),
    )
}

fn privacy_exposure(source: &ResearchSource) -> f64 {
    let mut exposure: f64 = 0.03;
    if source.host.contains("social") || source.host.contains("reddit") {
        exposure += 0.08;
    }
    if source.host.ends_with(".onion") {
        exposure += 0.15;
    }
    if source.duplicate {
        exposure += 0.03;
    }
    exposure.clamp(0.0, 1.0)
}

fn contradiction_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        " false",
        " not ",
        " denies ",
        " debunk",
        " refute",
        " contradict",
    ]
    .iter()
    .any(|needle| lower.contains(needle.trim()))
}

fn accuracy_lcb(accuracy: f64, n_eff: f64) -> f64 {
    if n_eff <= 1.0 {
        return (accuracy - 0.25).clamp(0.0, 1.0);
    }
    let z = 1.64;
    let variance = (accuracy * (1.0 - accuracy) / n_eff).max(0.0);
    (accuracy - z * variance.sqrt()).clamp(0.0, 1.0)
}

fn positive_part(value: f64) -> f64 {
    value.max(0.0)
}

fn sigmoid(value: f64) -> f64 {
    1.0 / (1.0 + (-value).exp())
}

trait Sigmoid {
    fn sigmoid(self) -> f64;
}

impl Sigmoid for f64 {
    fn sigmoid(self) -> f64 {
        sigmoid(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> EnterpriseContext {
        EnterpriseContext::default()
    }

    #[test]
    fn blocked_source_fails_access_gate() {
        let assessment = evaluate_candidate_source(
            "http://127.0.0.1/admin",
            "127.0.0.1",
            DomainPolicyMode::Blocked,
            &context(),
        );

        assert!(!assessment.access_ok);
        assert!(!assessment.production_executable);
        assert!(assessment.denial.is_some());
        assert!(assessment.authorization_request.is_some());
    }

    #[test]
    fn deep_dark_source_requires_explicit_controls() {
        let assessment = evaluate_candidate_source(
            "http://example.onion/report",
            "example.onion",
            DomainPolicyMode::BrowserRequired,
            &context(),
        );

        assert!(!assessment.production_executable);
        assert!(assessment.deep_dark_exposure > 0.0);
    }

    #[test]
    fn source_diversity_penalty_detects_dominance() {
        let mut one = evaluate_candidate_source(
            "https://a.example/report",
            "a.example",
            DomainPolicyMode::CrawlAllowed,
            &context(),
        );
        one.production_executable = true;
        one.platform_class = "web".into();
        let mut two = one.clone();
        two.url = "https://b.example/report".into();

        assert!(platform_diversity_penalty(&[one, two]) > 0.0);
    }

    #[test]
    fn calibrated_confidence_penalizes_contradictions() {
        let aligned = calibrated_confidence(&[(0.9, 0.95, 0.9, false), (0.85, 0.9, 0.9, false)]);
        let conflicted = calibrated_confidence(&[(0.9, 0.95, 0.9, false), (0.85, 0.9, 0.9, true)]);

        assert!(aligned > conflicted);
    }

    #[test]
    fn readiness_suspends_on_kill_switch() {
        let mut context = EnterpriseContext::default();
        context.runtime.queue_stable = false;
        let source = evaluate_candidate_source(
            "https://example.com/report",
            "example.com",
            DomainPolicyMode::CrawlAllowed,
            &EnterpriseContext::default(),
        );
        let release = ReleaseDecision {
            release_allowed: true,
            confidence: 0.9,
            multi_platform_confidence: 0.9,
            reproducible: true,
            ..ReleaseDecision::default()
        };
        let readiness = readiness_report(&context, &[source], &release);

        assert!(readiness.kill_switch);
        assert!(readiness.suspend);
        assert!(!readiness.deploy_allowed);
    }
}

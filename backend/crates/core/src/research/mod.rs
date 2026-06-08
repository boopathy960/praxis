use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

use parking_lot::RwLock;
use regex::Regex;
use reqwest::{Client, redirect};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::Shared;
use crate::common::{AppError, TenantScope, new_id, now_ms, sha3_hex};

pub mod calculus;

use calculus::{
    AuthorizationRequest, CalculusReport, EnterpriseContext, EvidenceAssessment, EvidenceWeight,
    LifecycleState, PolicyDenial, PrivacyBudget, ProvenanceNode, ReleaseDecision,
    ReleaseThresholds, ResearchApproval, RiskControl, RuntimeTelemetry, SourceAssessment,
    SourceAuthorization, ZeroTrustTelemetry,
};

const MAX_RESEARCH_URLS: usize = 12;
const MAX_SEED_DOCUMENTS: usize = 8;
const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const FETCH_TIMEOUT_SECS: u64 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainPolicyMode {
    ApiOnly,
    CrawlAllowed,
    BrowserRequired,
    Restricted,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPolicy {
    pub host_pattern: String,
    pub mode: DomainPolicyMode,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub citation_id: String,
    pub url: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub kind: String,
    pub path: String,
    pub content_type: String,
    pub content_hash: String,
    pub byte_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchClaim {
    pub claim: String,
    pub confidence: f64,
    pub citations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSource {
    pub source_id: String,
    pub url: String,
    pub normalized_url: String,
    pub host: String,
    pub title: String,
    pub status: String,
    pub extraction_confidence: f64,
    pub policy_mode: DomainPolicyMode,
    pub source_rank: usize,
    pub authority_score: f64,
    pub citation_density: f64,
    pub content_hash: String,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchJob {
    pub id: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub tenant_scope: TenantScope,
    pub trace_id: String,
    pub summary: String,
    pub errors: Vec<ResearchError>,
    pub artifacts: Vec<ArtifactRef>,
    pub citations: Vec<Citation>,
    pub claims: Vec<ResearchClaim>,
    pub sources: Vec<ResearchSource>,
    pub calculus_report: CalculusReport,
    pub release_decision: ReleaseDecision,
    pub readiness: calculus::ReadinessReport,
    pub policy_denials: Vec<PolicyDenial>,
    pub authorization_requests: Vec<AuthorizationRequest>,
    pub platform_coverage: f64,
    pub evidence_weights: Vec<EvidenceWeight>,
    pub provenance_chain: Vec<ProvenanceNode>,
    pub source_assessments: Vec<SourceAssessment>,
    pub evidence_assessments: Vec<EvidenceAssessment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedDocument {
    pub url: String,
    pub html: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResearchJobRequest {
    pub tenant_scope: TenantScope,
    pub trace_id: Option<String>,
    pub query: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub seed_documents: Vec<SeedDocument>,
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub approvals: Vec<ResearchApproval>,
    #[serde(default)]
    pub source_authorizations: Vec<SourceAuthorization>,
    #[serde(default)]
    pub privacy_budget: Option<PrivacyBudget>,
    #[serde(default)]
    pub zero_trust_telemetry: Option<ZeroTrustTelemetry>,
    #[serde(default)]
    pub runtime_telemetry: Option<RuntimeTelemetry>,
    #[serde(default)]
    pub release_thresholds: Option<ReleaseThresholds>,
    #[serde(default)]
    pub lifecycle_state: Option<LifecycleState>,
    #[serde(default)]
    pub risk_register: Vec<RiskControl>,
}

impl ResearchJobRequest {
    #[must_use]
    pub fn enterprise_context(&self) -> EnterpriseContext {
        EnterpriseContext {
            actor_id: self.actor_id.clone(),
            roles: self.roles.clone(),
            approvals: self.approvals.clone(),
            source_authorizations: self.source_authorizations.clone(),
            privacy_budget: self.privacy_budget.clone().unwrap_or_default(),
            zero_trust: self.zero_trust_telemetry.clone().unwrap_or_default(),
            runtime: self.runtime_telemetry.clone().unwrap_or_default(),
            release_thresholds: self.release_thresholds.clone().unwrap_or_default(),
            lifecycle_state: self.lifecycle_state.unwrap_or_default(),
            risk_register: self.risk_register.clone(),
        }
    }
}

#[derive(Default)]
struct ResearchStore {
    jobs: HashMap<String, ResearchJob>,
    domain_policies: HashMap<String, DomainPolicy>,
}

#[derive(Clone)]
pub struct ResearchService {
    artifact_dir: PathBuf,
    client: Client,
    store: Shared<ResearchStore>,
}

impl ResearchService {
    #[must_use]
    pub fn new(artifact_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&artifact_dir);
        let mut domain_policies = HashMap::new();
        domain_policies.insert(
            "localhost".into(),
            DomainPolicy {
                host_pattern: "localhost".into(),
                mode: DomainPolicyMode::Blocked,
                notes: "loopback access is blocked to avoid SSRF".into(),
            },
        );
        domain_policies.insert(
            "127.0.0.1".into(),
            DomainPolicy {
                host_pattern: "127.0.0.1".into(),
                mode: DomainPolicyMode::Blocked,
                notes: "loopback access is blocked to avoid SSRF".into(),
            },
        );
        domain_policies.insert(
            ".gov".into(),
            DomainPolicy {
                host_pattern: ".gov".into(),
                mode: DomainPolicyMode::ApiOnly,
                notes: "prefer official structured/public sources".into(),
            },
        );
        domain_policies.insert(
            ".edu".into(),
            DomainPolicy {
                host_pattern: ".edu".into(),
                mode: DomainPolicyMode::CrawlAllowed,
                notes: "public academic content is allowed with attribution".into(),
            },
        );

        Self {
            artifact_dir,
            client: Client::builder()
                .user_agent("URAI-HTTPA-Research/1.0")
                .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
                .redirect(redirect::Policy::limited(5))
                .build()
                .expect("reqwest client should build"),
            store: Shared::new(RwLock::new(ResearchStore {
                jobs: HashMap::new(),
                domain_policies,
            })),
        }
    }

    pub async fn create_job(&self, request: ResearchJobRequest) -> Result<ResearchJob, AppError> {
        if request.query.trim().is_empty() {
            return Err(AppError::Validation(
                "research query must not be empty".into(),
            ));
        }
        if request.urls.is_empty() && request.seed_documents.is_empty() {
            return Err(AppError::Validation(
                "research requires at least one URL or seed document".into(),
            ));
        }
        if request.urls.len() > MAX_RESEARCH_URLS {
            return Err(AppError::Validation(format!(
                "research supports at most {MAX_RESEARCH_URLS} URLs per job"
            )));
        }
        if request.seed_documents.len() > MAX_SEED_DOCUMENTS {
            return Err(AppError::Validation(format!(
                "research supports at most {MAX_SEED_DOCUMENTS} seed documents per job"
            )));
        }

        let created_at = now_ms();
        let job_id = new_id("research");
        let enterprise_context = request.enterprise_context();
        let trace_id = request.trace_id.clone().unwrap_or_else(|| new_id("trace"));
        let mut job = ResearchJob {
            id: job_id.clone(),
            status: "running".into(),
            created_at,
            updated_at: created_at,
            tenant_scope: request.tenant_scope,
            trace_id,
            summary: String::new(),
            errors: Vec::new(),
            artifacts: Vec::new(),
            citations: Vec::new(),
            claims: Vec::new(),
            sources: Vec::new(),
            calculus_report: CalculusReport::default(),
            release_decision: ReleaseDecision::default(),
            readiness: calculus::ReadinessReport::default(),
            policy_denials: Vec::new(),
            authorization_requests: Vec::new(),
            platform_coverage: 0.0,
            evidence_weights: Vec::new(),
            provenance_chain: Vec::new(),
            source_assessments: Vec::new(),
            evidence_assessments: Vec::new(),
        };

        let mut seen_urls = HashSet::new();
        for seed in request.seed_documents {
            if seed.html.len() > MAX_DOCUMENT_BYTES {
                job.errors.push(ResearchError {
                    code: "seed_too_large".into(),
                    message: format!("seed document {} exceeds byte limit", seed.url),
                });
                continue;
            }
            self.process_document(
                &request.query,
                &enterprise_context,
                &mut job,
                seed.url,
                seed.html,
                true,
            )?;
        }
        for url in request.urls {
            match normalize_research_url(&url) {
                Ok(normalized) => {
                    if !seen_urls.insert(normalized) {
                        job.errors.push(ResearchError {
                            code: "duplicate_url".into(),
                            message: format!("duplicate research URL skipped: {url}"),
                        });
                        continue;
                    }
                }
                Err(error) => {
                    job.errors.push(ResearchError {
                        code: "invalid_url".into(),
                        message: error.to_string(),
                    });
                    continue;
                }
            }
            match self
                .fetch_and_process(&request.query, &enterprise_context, &mut job, &url)
                .await
            {
                Ok(()) => {}
                Err(error) => job.errors.push(ResearchError {
                    code: "fetch_failed".into(),
                    message: error.to_string(),
                }),
            }
        }

        if job.sources.is_empty() && job.errors.is_empty() {
            job.errors.push(ResearchError {
                code: "no_sources".into(),
                message: "research finished without ingesting any source".into(),
            });
        }
        job.status = if job.errors.is_empty() {
            "completed".into()
        } else if job.sources.is_empty() {
            "failed".into()
        } else {
            "completed_with_errors".into()
        };
        job.updated_at = now_ms();
        job.summary = format!(
            "Processed {} sources for '{}' with {} claims and {} citations.",
            job.sources.len(),
            request.query,
            job.claims.len(),
            job.citations.len()
        );
        rank_sources(&mut job);
        self.finalize_calculus(&enterprise_context, &mut job);
        self.store.write().jobs.insert(job.id.clone(), job.clone());
        Ok(job)
    }

    pub fn get_job(&self, job_id: &str) -> Result<ResearchJob, AppError> {
        self.store
            .read()
            .jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("research job {job_id}")))
    }

    #[must_use]
    pub fn list_domain_policies(&self) -> Vec<DomainPolicy> {
        self.store
            .read()
            .domain_policies
            .values()
            .cloned()
            .collect()
    }

    pub fn readiness(&self, job_id: &str) -> Result<calculus::ReadinessReport, AppError> {
        Ok(self.get_job(job_id)?.readiness)
    }

    async fn fetch_and_process(
        &self,
        query: &str,
        context: &EnterpriseContext,
        job: &mut ResearchJob,
        url: &str,
    ) -> Result<(), AppError> {
        let parsed = Url::parse(url)
            .map_err(|error| AppError::Validation(format!("invalid research URL: {error}")))?;
        let host = parsed.host_str().unwrap_or_default().to_lowercase();
        let policy = self.policy_for_host(&host);
        if matches!(
            policy.mode,
            DomainPolicyMode::Blocked | DomainPolicyMode::Restricted
        ) {
            self.record_policy_assessment(job, url, &host, policy.mode, context);
            return Ok(());
        }
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppError::Validation(
                "only http/https URLs are supported".into(),
            ));
        }
        if is_private_host(&host) {
            self.record_policy_assessment(job, url, &host, DomainPolicyMode::Blocked, context);
            return Ok(());
        }

        let assessment = calculus::evaluate_candidate_source(url, &host, policy.mode, context);
        let allowed = assessment.production_executable;
        self.record_assessment(job, assessment);
        if !allowed {
            return Ok(());
        }

        let response = self
            .client
            .get(parsed.clone())
            .send()
            .await
            .map_err(|error| AppError::Internal(format!("fetch failed: {error}")))?;
        if !response.status().is_success() {
            return Err(AppError::Internal(format!(
                "fetch returned HTTP {}",
                response.status()
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_ascii_lowercase();
        if !is_allowed_content_type(&content_type) {
            return Err(AppError::Validation(format!(
                "unsupported content type: {content_type}"
            )));
        }
        if response
            .content_length()
            .is_some_and(|len| len as usize > MAX_DOCUMENT_BYTES)
        {
            return Err(AppError::Validation(format!(
                "response exceeds {MAX_DOCUMENT_BYTES} bytes"
            )));
        }
        let bytes = response.bytes().await.map_err(|error| {
            AppError::Internal(format!("failed to read response body: {error}"))
        })?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(AppError::Validation(format!(
                "response exceeds {MAX_DOCUMENT_BYTES} bytes"
            )));
        }
        let html = String::from_utf8_lossy(&bytes).to_string();
        self.process_document(query, context, job, parsed.to_string(), html, false)
    }

    fn process_document(
        &self,
        query: &str,
        context: &EnterpriseContext,
        job: &mut ResearchJob,
        url: String,
        html: String,
        seeded: bool,
    ) -> Result<(), AppError> {
        let parsed = Url::parse(&url)
            .map_err(|error| AppError::Validation(format!("invalid document URL: {error}")))?;
        let host = parsed.host_str().unwrap_or_default().to_lowercase();
        let policy = self.policy_for_host(&host);
        if !seeded {
            // Fetched URLs are assessed before the network call. Do not duplicate the assessment.
        } else {
            if is_private_host(&host) {
                self.record_policy_assessment(job, &url, &host, DomainPolicyMode::Blocked, context);
                return Ok(());
            }
            let assessment = calculus::evaluate_candidate_source(&url, &host, policy.mode, context);
            let allowed = assessment.production_executable;
            self.record_assessment(job, assessment);
            if !allowed {
                return Ok(());
            }
        }
        let title =
            extract_title(&html).unwrap_or_else(|| parsed.path().trim_matches('/').to_string());
        let text = strip_html(&html);
        let snippet = first_sentence(&text);
        let content_hash = sha3_hex(html.as_bytes());
        let duplicate = job
            .artifacts
            .iter()
            .any(|artifact| artifact.content_hash == content_hash);
        let artifact_id = new_id("artifact");
        let artifact_path = self.artifact_dir.join(format!("{artifact_id}.html"));
        fs::write(&artifact_path, html.as_bytes()).map_err(|error| {
            AppError::Internal(format!("failed to persist research artifact: {error}"))
        })?;
        let citation_id = new_id("citation");

        let citation = Citation {
            citation_id: citation_id.clone(),
            url: url.clone(),
            title: title.clone(),
            snippet: snippet.clone(),
        };
        let artifact = ArtifactRef {
            artifact_id,
            kind: if seeded { "seed_html" } else { "fetched_html" }.into(),
            path: artifact_path.to_string_lossy().to_string(),
            content_type: "text/html".into(),
            content_hash: content_hash.clone(),
            byte_len: html.len(),
        };
        let extraction_confidence = extraction_confidence(&text, duplicate);
        let authority_score = authority_score(&host, policy.mode);
        let citation_density = citation_density(&text);
        let claim = ResearchClaim {
            claim: if text.is_empty() {
                format!(
                    "Source {} was fetched but did not yield readable body text.",
                    title
                )
            } else {
                format!("{} discusses {}.", title, query.trim())
            },
            confidence: claim_confidence(extraction_confidence, authority_score, duplicate),
            citations: vec![citation_id],
        };
        let source = ResearchSource {
            source_id: new_id("source"),
            url,
            normalized_url: parsed.to_string(),
            host,
            title,
            status: if duplicate {
                "duplicate_ingested".into()
            } else {
                "ingested".into()
            },
            extraction_confidence,
            policy_mode: policy.mode,
            source_rank: 0,
            authority_score,
            citation_density,
            content_hash,
            duplicate,
        };

        job.citations.push(citation);
        job.artifacts.push(artifact);
        job.claims.push(claim);
        job.sources.push(source);
        Ok(())
    }

    fn record_policy_assessment(
        &self,
        job: &mut ResearchJob,
        url: &str,
        host: &str,
        policy_mode: DomainPolicyMode,
        context: &EnterpriseContext,
    ) {
        let assessment = calculus::evaluate_candidate_source(url, host, policy_mode, context);
        self.record_assessment(job, assessment);
    }

    fn record_assessment(&self, job: &mut ResearchJob, assessment: SourceAssessment) {
        if let Some(denial) = assessment.denial.clone() {
            job.policy_denials.push(denial);
        }
        if let Some(request) = assessment.authorization_request.clone() {
            job.authorization_requests.push(request);
        }
        job.source_assessments.push(assessment);
    }

    fn finalize_calculus(&self, context: &EnterpriseContext, job: &mut ResearchJob) {
        let (report, evidence_weights, provenance_chain) = calculus::finalize_report(
            context,
            &job.source_assessments,
            &job.sources,
            &job.citations,
        );
        job.platform_coverage = report.coverage;
        job.release_decision = report.release_decision.clone();
        job.readiness = report.readiness.clone();
        job.evidence_assessments = report.evidence_assessments.clone();
        job.evidence_weights = evidence_weights;
        job.provenance_chain = provenance_chain;
        job.calculus_report = report;
    }

    fn policy_for_host(&self, host: &str) -> DomainPolicy {
        let store = self.store.read();
        if let Some(policy) = store.domain_policies.get(host) {
            return policy.clone();
        }
        if host.ends_with(".gov") {
            return store.domain_policies[".gov"].clone();
        }
        if host.ends_with(".edu") {
            return store.domain_policies[".edu"].clone();
        }
        DomainPolicy {
            host_pattern: host.into(),
            mode: DomainPolicyMode::CrawlAllowed,
            notes: "default public web crawling policy".into(),
        }
    }
}

fn normalize_research_url(url: &str) -> Result<String, AppError> {
    let mut parsed = Url::parse(url)
        .map_err(|error| AppError::Validation(format!("invalid research URL: {error}")))?;
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn is_allowed_content_type(content_type: &str) -> bool {
    let content_type = content_type.to_ascii_lowercase();
    content_type.starts_with("text/html")
        || content_type.starts_with("text/plain")
        || content_type.starts_with("application/xhtml+xml")
        || content_type.starts_with("application/json")
}

fn extraction_confidence(text: &str, duplicate: bool) -> f64 {
    let length_score = if text.len() > 1_200 {
        0.88
    } else if text.len() > 240 {
        0.74
    } else if text.len() > 80 {
        0.58
    } else {
        0.34
    };
    if duplicate {
        (length_score * 0.72_f64).max(0.2_f64)
    } else {
        length_score
    }
}

fn authority_score(host: &str, policy_mode: DomainPolicyMode) -> f64 {
    let base = if host.ends_with(".gov") {
        0.96
    } else if host.ends_with(".edu") {
        0.90
    } else if host.ends_with(".org") {
        0.72
    } else {
        0.62
    };
    match policy_mode {
        DomainPolicyMode::ApiOnly => (base + 0.04_f64).min(1.0_f64),
        DomainPolicyMode::CrawlAllowed => base,
        DomainPolicyMode::BrowserRequired => base * 0.88,
        DomainPolicyMode::Restricted | DomainPolicyMode::Blocked => 0.0,
    }
}

fn citation_density(text: &str) -> f64 {
    let words = text.split_whitespace().count().max(1) as f64;
    let markers = text.matches("http").count()
        + text.matches("doi").count()
        + text.matches("according").count()
        + text.matches("source").count();
    ((markers as f64 + 1.0) / (words / 400.0 + 1.0)).min(1.0)
}

fn claim_confidence(extraction_confidence: f64, authority_score: f64, duplicate: bool) -> f64 {
    let penalty = if duplicate { 0.12 } else { 0.0 };
    (extraction_confidence * 0.58 + authority_score * 0.42 - penalty).clamp(0.0, 1.0)
}

fn rank_sources(job: &mut ResearchJob) {
    job.sources.sort_by(|left, right| {
        source_score(right)
            .total_cmp(&source_score(left))
            .then_with(|| left.normalized_url.cmp(&right.normalized_url))
    });
    for (index, source) in job.sources.iter_mut().enumerate() {
        source.source_rank = index + 1;
    }
}

fn source_score(source: &ResearchSource) -> f64 {
    let duplicate_penalty = if source.duplicate { 0.18 } else { 0.0 };
    (source.extraction_confidence * 0.42
        + source.authority_score * 0.38
        + source.citation_density * 0.20
        - duplicate_penalty)
        .clamp(0.0, 1.0)
}

fn extract_title(html: &str) -> Option<String> {
    let regex = Regex::new("(?is)<title[^>]*>(.*?)</title>").ok()?;
    let captures = regex.captures(html)?;
    captures
        .get(1)
        .map(|value| html_unescape(value.as_str()).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn strip_html(html: &str) -> String {
    let script_regex = Regex::new("(?is)<script.*?</script>").expect("script regex");
    let style_regex = Regex::new("(?is)<style.*?</style>").expect("style regex");
    let tag_regex = Regex::new("(?is)<[^>]+>").expect("tag regex");
    let without_scripts = script_regex.replace_all(html, " ");
    let without_styles = style_regex.replace_all(&without_scripts, " ");
    let without_tags = tag_regex.replace_all(&without_styles, " ");
    html_unescape(&without_tags)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "No snippet available".into();
    }
    trimmed
        .split_terminator(['.', '!', '?'])
        .next()
        .unwrap_or(trimmed)
        .chars()
        .take(240)
        .collect()
}

fn html_unescape(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn is_private_host(host: &str) -> bool {
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    host.parse::<IpAddr>().ok().is_some_and(|ip| match ip {
        IpAddr::V4(addr) => {
            let octets = addr.octets();
            octets[0] == 10
                || (octets[0] == 172 && (16..=31).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 168)
                || octets[0] == 127
        }
        IpAddr::V6(addr) => addr.is_loopback() || addr.is_unique_local(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request(query: &str) -> ResearchJobRequest {
        ResearchJobRequest {
            tenant_scope: TenantScope::Global,
            trace_id: Some("trace_test".into()),
            query: query.into(),
            urls: Vec::new(),
            seed_documents: Vec::new(),
            actor_id: None,
            roles: Vec::new(),
            approvals: Vec::new(),
            source_authorizations: Vec::new(),
            privacy_budget: None,
            zero_trust_telemetry: None,
            runtime_telemetry: None,
            release_thresholds: None,
            lifecycle_state: None,
            risk_register: Vec::new(),
        }
    }

    #[actix_web::test]
    async fn research_job_ingests_seed_document() {
        let service = ResearchService::new(std::env::temp_dir().join("urai_research_test"));
        let mut request = base_request("enterprise httpa");
        request.seed_documents = vec![SeedDocument {
            url: "https://example.com/report".into(),
            html: "<html><head><title>Enterprise HTTPA</title></head><body><main>HTTPA is transported over HTTPS for production reliability and user security.</main></body></html>".into(),
        }];
        let job = service
            .create_job(request)
            .await
            .expect("seeded research job should succeed");

        assert_eq!(job.status, "completed");
        assert_eq!(job.sources.len(), 1);
        assert_eq!(job.citations.len(), 1);
        assert!(!job.claims.is_empty());
        assert_eq!(
            job.calculus_report.formula_version,
            "enterprise_research_calculus_v1"
        );
        assert_eq!(job.source_assessments.len(), 1);
        assert_eq!(job.evidence_weights.len(), 1);
        assert_eq!(job.provenance_chain.len(), 1);
    }

    #[test]
    fn private_hosts_are_blocked() {
        assert!(is_private_host("127.0.0.1"));
        assert!(is_private_host("10.0.0.5"));
        assert!(!is_private_host("example.com"));
    }

    #[actix_web::test]
    async fn unauthorized_dark_source_is_denied_without_ingestion() {
        let service = ResearchService::new(std::env::temp_dir().join("urai_research_dark_test"));
        let mut request = base_request("dark source rumor");
        request.seed_documents = vec![SeedDocument {
            url: "http://example.onion/report".into(),
            html: "<html><head><title>Anonymous report</title></head><body>Unverified claim.</body></html>".into(),
        }];

        let job = service
            .create_job(request)
            .await
            .expect("job should finish");

        assert!(job.sources.is_empty());
        assert!(!job.policy_denials.is_empty());
        assert!(!job.authorization_requests.is_empty());
        assert!(job.readiness.halt);
        assert!(job.calculus_report.readiness.kill_switch);
    }

    #[actix_web::test]
    async fn duplicate_evidence_is_downweighted() {
        let service =
            ResearchService::new(std::env::temp_dir().join("urai_research_duplicate_test"));
        let html = "<html><head><title>Enterprise report</title></head><body>According to source material, HTTPA improves reliable research evidence handling.</body></html>";
        let mut request = base_request("enterprise research evidence");
        request.seed_documents = vec![
            SeedDocument {
                url: "https://example.com/report-a".into(),
                html: html.into(),
            },
            SeedDocument {
                url: "https://example.org/report-b".into(),
                html: html.into(),
            },
        ];

        let job = service
            .create_job(request)
            .await
            .expect("job should finish");
        let duplicate = job
            .sources
            .iter()
            .find(|source| source.duplicate)
            .expect("second identical artifact should be duplicate");
        let duplicate_weight = job
            .evidence_weights
            .iter()
            .find(|weight| weight.source_id == duplicate.source_id)
            .expect("duplicate source should have weight")
            .weight;
        let original_weight = job
            .evidence_weights
            .iter()
            .filter(|weight| weight.source_id != duplicate.source_id)
            .map(|weight| weight.weight)
            .fold(0.0_f64, f64::max);

        assert!(duplicate_weight < original_weight);
    }
}

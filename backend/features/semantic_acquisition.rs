use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::header::{ACCEPT, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use url::Url;

use crate::core_infra::RateLimiter;
use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};
use crate::features::semantic_render::SemanticRenderReport;
use crate::network::egress::{EgressPolicy, EgressStatus};

const DEFAULT_TIMEOUT_MS: u64 = 12_000;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 1_500_000;
const DEFAULT_MAX_RETRIES: u8 = 2;
const DEFAULT_REQUESTS_PER_MINUTE: u32 = 20;
const DEFAULT_SEGMENT_LIMIT: usize = 24;
const DEFAULT_LINK_LIMIT: usize = 24;
const DEFAULT_RECENT_RECORD_LIMIT: usize = 48;
const ROBOTS_MAX_BYTES: usize = 128 * 1024;
const DEFAULT_USER_AGENT: &str = "AstraAssistant/2.0 (+semantic-acquisition)";

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticAcquisitionRequest {
    pub url: String,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default = "default_respect_robots")]
    pub respect_robots: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
    #[serde(default = "default_segment_limit")]
    pub max_segments: usize,
    #[serde(default = "default_link_limit")]
    pub max_links: usize,
    #[serde(default = "default_requests_per_minute")]
    pub requests_per_minute: u32,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
    #[serde(default = "default_persist_report")]
    pub persist_report: bool,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default)]
    pub force_content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAcquisitionPolicy {
    pub allowed_domains: Vec<String>,
    pub respect_robots: bool,
    pub timeout_ms: u64,
    pub max_response_bytes: usize,
    pub max_retries: u8,
    pub max_segments: usize,
    pub max_links: usize,
    pub requests_per_minute: u32,
    pub user_agent: String,
    pub persist_report: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotsPolicyResult {
    pub checked: bool,
    pub robots_url: Option<String>,
    pub allowed: bool,
    pub matched_scope: String,
    pub allow_rules: usize,
    pub disallow_rules: usize,
    pub reason: String,
    pub fetch_status: Option<u16>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticAcquisitionProvenance {
    pub acquisition_id: String,
    pub requested_url: String,
    pub final_url: String,
    pub normalized_url: String,
    pub domain: String,
    pub scheme: String,
    pub status_code: u16,
    pub content_type: String,
    pub response_class: String,
    pub bytes_received: usize,
    pub content_hash: String,
    pub attempt_count: u8,
    pub retried: bool,
    pub duration_ms: u64,
    pub started_at: i64,
    pub fetched_at: i64,
    pub response_headers: BTreeMap<String, String>,
    pub egress: EgressStatus,
    pub policy: SemanticAcquisitionPolicy,
    pub robots: RobotsPolicyResult,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticAcquisitionReport {
    pub acquisition_id: String,
    pub status: String,
    pub provenance: SemanticAcquisitionProvenance,
    pub render: SemanticRenderReport,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAcquisitionRecord {
    pub acquisition_id: String,
    pub requested_url: String,
    pub final_url: String,
    pub title: String,
    pub content_type: String,
    pub status_code: u16,
    pub bytes_received: usize,
    pub attempt_count: u8,
    pub segment_count: usize,
    pub link_count: usize,
    pub external_link_count: usize,
    pub structured_data_blocks: usize,
    pub content_hash: String,
    pub warnings: Vec<String>,
    pub fetched_at: i64,
    pub recorded_at: i64,
    pub notarized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAcquisitionStats {
    pub total_acquisitions: u64,
    pub successful_acquisitions: u64,
    pub failed_acquisitions: u64,
    pub blocked_by_policy: u64,
    pub blocked_by_robots: u64,
    pub total_retries: u64,
    pub total_bytes_received: u64,
    pub persisted_reports: u64,
    pub chain_notarizations: u64,
    pub recent_records: usize,
    pub last_acquired_at: i64,
    pub last_failure_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SemanticAcquisitionSnapshot {
    stats: SemanticAcquisitionStats,
    recent_records: Vec<StoredAcquisitionRecord>,
}

#[derive(Debug, Clone)]
pub struct PreparedSemanticAcquisition {
    acquisition_id: String,
    requested_url: String,
    normalized_url: String,
    domain: String,
    scheme: String,
    started_at: i64,
    policy: SemanticAcquisitionPolicy,
    egress_policy: EgressPolicy,
    egress_status: EgressStatus,
    force_content_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SemanticAcquisitionOutcome {
    pub acquisition_id: String,
    pub requested_url: String,
    pub final_url: String,
    pub normalized_url: String,
    pub domain: String,
    pub scheme: String,
    pub status_code: u16,
    pub content_type: String,
    pub response_class: String,
    pub bytes_received: usize,
    pub content_hash: String,
    pub attempt_count: u8,
    pub duration_ms: u64,
    pub fetched_at: i64,
    pub response_headers: BTreeMap<String, String>,
    pub robots: RobotsPolicyResult,
    pub warnings: Vec<String>,
    pub source_body: String,
    pub semantic_html: String,
}

#[derive(Debug)]
pub struct SemanticAcquisitionFailure {
    error: AstraError,
    attempt_count: u8,
    blocked_by_robots: bool,
    fetched_at: i64,
}

impl SemanticAcquisitionFailure {
    fn new(error: AstraError, attempt_count: u8, blocked_by_robots: bool) -> Self {
        Self {
            error,
            attempt_count,
            blocked_by_robots,
            fetched_at: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub fn into_error(self) -> AstraError {
        self.error
    }
}

pub struct SemanticAcquisitionEngine {
    egress_policy: EgressPolicy,
    rate_limiter: RateLimiter,
    per_domain_windows: BTreeMap<String, VecDeque<i64>>,
    recent_records: VecDeque<StoredAcquisitionRecord>,
    persistence_path: Option<PathBuf>,
    recent_limit: usize,
    total_acquisitions: u64,
    successful_acquisitions: u64,
    failed_acquisitions: u64,
    blocked_by_policy: u64,
    blocked_by_robots: u64,
    total_retries: u64,
    total_bytes_received: u64,
    persisted_reports: u64,
    chain_notarizations: u64,
    last_acquired_at: i64,
    last_failure_at: i64,
}

impl SemanticAcquisitionEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::load_from_path(Some(default_acquisition_path()))
    }

    #[must_use]
    pub fn new_ephemeral() -> Self {
        Self::load_from_path(None)
    }

    pub fn set_egress_policy(&mut self, egress_policy: EgressPolicy) {
        self.egress_policy = egress_policy;
    }

    fn load_from_path(persistence_path: Option<PathBuf>) -> Self {
        if let Some(path) = persistence_path.as_ref() {
            if let Ok(raw) = fs::read_to_string(path) {
                if let Ok(snapshot) = serde_json::from_str::<SemanticAcquisitionSnapshot>(&raw) {
                    let stats = snapshot.stats;
                    return Self {
                        egress_policy: EgressPolicy::from_env(),
                        rate_limiter: RateLimiter::new(4.0, 16),
                        per_domain_windows: BTreeMap::new(),
                        recent_records: snapshot.recent_records.into_iter().collect(),
                        persistence_path,
                        recent_limit: DEFAULT_RECENT_RECORD_LIMIT,
                        total_acquisitions: stats.total_acquisitions,
                        successful_acquisitions: stats.successful_acquisitions,
                        failed_acquisitions: stats.failed_acquisitions,
                        blocked_by_policy: stats.blocked_by_policy,
                        blocked_by_robots: stats.blocked_by_robots,
                        total_retries: stats.total_retries,
                        total_bytes_received: stats.total_bytes_received,
                        persisted_reports: stats.persisted_reports,
                        chain_notarizations: stats.chain_notarizations,
                        last_acquired_at: stats.last_acquired_at,
                        last_failure_at: stats.last_failure_at,
                    };
                }
            }
        }

        Self {
            egress_policy: EgressPolicy::from_env(),
            rate_limiter: RateLimiter::new(4.0, 16),
            per_domain_windows: BTreeMap::new(),
            recent_records: VecDeque::new(),
            persistence_path,
            recent_limit: DEFAULT_RECENT_RECORD_LIMIT,
            total_acquisitions: 0,
            successful_acquisitions: 0,
            failed_acquisitions: 0,
            blocked_by_policy: 0,
            blocked_by_robots: 0,
            total_retries: 0,
            total_bytes_received: 0,
            persisted_reports: 0,
            chain_notarizations: 0,
            last_acquired_at: 0,
            last_failure_at: 0,
        }
    }

    pub fn prepare(
        &mut self,
        request: &SemanticAcquisitionRequest,
    ) -> AstraResult<PreparedSemanticAcquisition> {
        if request.url.trim().is_empty() {
            return Err(AstraError::ControlPlaneRejected(
                "semantic acquisition requires a URL".into(),
            ));
        }

        let normalized = Url::parse(request.url.trim()).map_err(|error| {
            AstraError::ControlPlaneRejected(format!("invalid acquisition URL: {error}"))
        })?;
        if !matches!(normalized.scheme(), "http" | "https") {
            return Err(AstraError::ControlPlaneRejected(
                "semantic acquisition only supports http and https URLs".into(),
            ));
        }

        let domain = normalized.host_str().ok_or_else(|| {
            AstraError::ControlPlaneRejected("semantic acquisition requires a host".into())
        })?;
        let allowed_domains = normalize_allowed_domains(domain, &request.allowed_domains);
        if !host_allowed(domain, &allowed_domains) {
            self.blocked_by_policy += 1;
            return Err(AstraError::ControlPlaneRejected(format!(
                "acquisition domain '{domain}' is outside the allowed domain policy"
            )));
        }

        let requests_per_minute = request.requests_per_minute.max(1);
        if !self.allow_domain_window(domain, requests_per_minute) {
            self.blocked_by_policy += 1;
            return Err(AstraError::ControlPlaneRejected(format!(
                "acquisition rate limit exceeded for '{domain}'"
            )));
        }

        let burst_key = format!("{}:{}", domain.to_ascii_lowercase(), requests_per_minute);
        if !self.rate_limiter.allow(&burst_key) {
            self.blocked_by_policy += 1;
            return Err(AstraError::ControlPlaneRejected(format!(
                "acquisition burst budget exhausted for '{domain}'"
            )));
        }

        let started_at = chrono::Utc::now().timestamp_millis();
        let acquisition_id = sha3_256_hex(
            format!("{}:{}:{}", normalized, started_at, requests_per_minute).as_bytes(),
        )[..24]
            .to_string();
        let policy = SemanticAcquisitionPolicy {
            allowed_domains,
            respect_robots: request.respect_robots,
            timeout_ms: request.timeout_ms.max(1_000),
            max_response_bytes: request.max_response_bytes.max(4_096),
            max_retries: request.max_retries.min(5),
            max_segments: request.max_segments.max(1),
            max_links: request.max_links.max(1),
            requests_per_minute,
            user_agent: normalized_user_agent(&request.user_agent),
            persist_report: request.persist_report,
        };

        Ok(PreparedSemanticAcquisition {
            acquisition_id,
            requested_url: request.url.trim().to_string(),
            normalized_url: normalized.to_string(),
            domain: domain.to_ascii_lowercase(),
            scheme: normalized.scheme().to_string(),
            started_at,
            policy,
            egress_policy: self.egress_policy.clone(),
            egress_status: self.egress_policy.status(),
            force_content_type: request.force_content_type.clone(),
        })
    }

    pub fn finalize_success(
        &mut self,
        prepared: &PreparedSemanticAcquisition,
        outcome: SemanticAcquisitionOutcome,
        render: SemanticRenderReport,
    ) -> AstraResult<SemanticAcquisitionReport> {
        self.total_acquisitions += 1;
        self.successful_acquisitions += 1;
        self.total_retries += outcome.attempt_count.saturating_sub(1) as u64;
        self.total_bytes_received += outcome.bytes_received as u64;
        self.last_acquired_at = outcome.fetched_at;

        let provenance = SemanticAcquisitionProvenance {
            acquisition_id: outcome.acquisition_id.clone(),
            requested_url: outcome.requested_url.clone(),
            final_url: outcome.final_url.clone(),
            normalized_url: outcome.normalized_url.clone(),
            domain: outcome.domain.clone(),
            scheme: outcome.scheme.clone(),
            status_code: outcome.status_code,
            content_type: outcome.content_type.clone(),
            response_class: outcome.response_class.clone(),
            bytes_received: outcome.bytes_received,
            content_hash: outcome.content_hash.clone(),
            attempt_count: outcome.attempt_count,
            retried: outcome.attempt_count > 1,
            duration_ms: outcome.duration_ms,
            started_at: prepared.started_at,
            fetched_at: outcome.fetched_at,
            response_headers: outcome.response_headers.clone(),
            egress: prepared.egress_status.clone(),
            policy: prepared.policy.clone(),
            robots: outcome.robots.clone(),
            warnings: outcome.warnings.clone(),
        };
        let recorded_at = chrono::Utc::now().timestamp_millis();

        if prepared.policy.persist_report {
            self.persisted_reports += 1;
            self.recent_records.push_front(StoredAcquisitionRecord {
                acquisition_id: outcome.acquisition_id.clone(),
                requested_url: outcome.requested_url,
                final_url: outcome.final_url,
                title: render.metadata.title.clone(),
                content_type: outcome.content_type,
                status_code: outcome.status_code,
                bytes_received: outcome.bytes_received,
                attempt_count: outcome.attempt_count,
                segment_count: render.summary.segment_count,
                link_count: render.summary.link_count,
                external_link_count: render.summary.external_link_count,
                structured_data_blocks: render.summary.structured_data_blocks,
                content_hash: outcome.content_hash,
                warnings: provenance.warnings.clone(),
                fetched_at: outcome.fetched_at,
                recorded_at,
                notarized: false,
            });
            self.trim_recent_records();
            self.persist()?;
        }

        Ok(SemanticAcquisitionReport {
            acquisition_id: provenance.acquisition_id.clone(),
            status: "acquired".into(),
            provenance,
            render,
            recorded_at,
        })
    }

    pub fn record_failure(
        &mut self,
        _prepared: &PreparedSemanticAcquisition,
        failure: &SemanticAcquisitionFailure,
    ) {
        self.total_acquisitions += 1;
        self.failed_acquisitions += 1;
        self.total_retries += failure.attempt_count.saturating_sub(1) as u64;
        self.last_failure_at = failure.fetched_at;
        if failure.blocked_by_robots {
            self.blocked_by_robots += 1;
        }
        let _ = self.persist();
    }

    pub fn record_chain_commit(&mut self, acquisition_id: &str) {
        self.chain_notarizations += 1;
        if let Some(record) = self
            .recent_records
            .iter_mut()
            .find(|record| record.acquisition_id == acquisition_id)
        {
            record.notarized = true;
        }
        let _ = self.persist();
    }

    #[must_use]
    pub fn stats(&self) -> SemanticAcquisitionStats {
        SemanticAcquisitionStats {
            total_acquisitions: self.total_acquisitions,
            successful_acquisitions: self.successful_acquisitions,
            failed_acquisitions: self.failed_acquisitions,
            blocked_by_policy: self.blocked_by_policy,
            blocked_by_robots: self.blocked_by_robots,
            total_retries: self.total_retries,
            total_bytes_received: self.total_bytes_received,
            persisted_reports: self.persisted_reports,
            chain_notarizations: self.chain_notarizations,
            recent_records: self.recent_records.len(),
            last_acquired_at: self.last_acquired_at,
            last_failure_at: self.last_failure_at,
        }
    }

    #[must_use]
    pub fn recent_records(&self, limit: usize) -> Vec<StoredAcquisitionRecord> {
        self.recent_records
            .iter()
            .take(limit.max(1))
            .cloned()
            .collect()
    }

    fn allow_domain_window(&mut self, domain: &str, limit_per_minute: u32) -> bool {
        let now = chrono::Utc::now().timestamp_millis();
        let window = self
            .per_domain_windows
            .entry(domain.to_ascii_lowercase())
            .or_default();
        while let Some(oldest) = window.front().copied() {
            if now.saturating_sub(oldest) > 60_000 {
                window.pop_front();
            } else {
                break;
            }
        }
        if window.len() >= limit_per_minute as usize {
            return false;
        }
        window.push_back(now);
        true
    }

    fn trim_recent_records(&mut self) {
        while self.recent_records.len() > self.recent_limit {
            self.recent_records.pop_back();
        }
    }

    fn persist(&self) -> AstraResult<()> {
        let Some(path) = self.persistence_path.as_ref() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AstraError::Persistence(format!(
                    "failed to create semantic acquisition store '{}': {error}",
                    parent.display()
                ))
            })?;
        }
        let snapshot = SemanticAcquisitionSnapshot {
            stats: self.stats(),
            recent_records: self.recent_records.iter().cloned().collect(),
        };
        let payload = serde_json::to_vec_pretty(&snapshot).map_err(|error| {
            AstraError::Persistence(format!(
                "semantic acquisition snapshot encode failed: {error}"
            ))
        })?;
        fs::write(path, payload).map_err(|error| {
            AstraError::Persistence(format!(
                "failed to persist semantic acquisition snapshot '{}': {error}",
                path.display()
            ))
        })
    }
}

pub async fn execute_semantic_acquisition(
    prepared: &PreparedSemanticAcquisition,
) -> Result<SemanticAcquisitionOutcome, SemanticAcquisitionFailure> {
    let client = prepared
        .egress_policy
        .build_client_with_timeout_ms(prepared.policy.timeout_ms)
        .map_err(|error| SemanticAcquisitionFailure::new(error, 0, false))?;

    let robots = if prepared.policy.respect_robots {
        evaluate_robots(
            &client,
            &prepared.normalized_url,
            &prepared.policy.user_agent,
        )
        .await
    } else {
        RobotsPolicyResult {
            checked: false,
            robots_url: None,
            allowed: true,
            matched_scope: "disabled".into(),
            allow_rules: 0,
            disallow_rules: 0,
            reason: "robots enforcement disabled by request".into(),
            fetch_status: None,
            warnings: Vec::new(),
        }
    };

    if !robots.allowed {
        return Err(SemanticAcquisitionFailure::new(
            AstraError::ControlPlaneRejected(format!(
                "robots policy denied acquisition for '{}': {}",
                prepared.normalized_url, robots.reason
            )),
            0,
            true,
        ));
    }

    let started = Instant::now();
    let attempts_max = prepared.policy.max_retries as usize + 1;
    let mut attempt_count = 0_u8;
    let mut last_error: Option<AstraError> = None;
    let mut warnings = robots.warnings.clone();

    for attempt in 1..=attempts_max {
        attempt_count = attempt as u8;
        let response = client
            .get(&prepared.normalized_url)
            .header(
                ACCEPT,
                "text/html,application/xhtml+xml,application/json,text/plain;q=0.8,*/*;q=0.5",
            )
            .header(
                reqwest::header::USER_AGENT,
                prepared.policy.user_agent.clone(),
            )
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                if should_retry_status(status) && attempt < attempts_max {
                    warnings.push(format!(
                        "retrying acquisition after HTTP {} from '{}'",
                        status.as_u16(),
                        prepared.domain
                    ));
                    sleep(backoff_for_attempt(attempt)).await;
                    continue;
                }
                if !status.is_success() {
                    return Err(SemanticAcquisitionFailure::new(
                        AstraError::FetchFailed(format!(
                            "acquisition returned HTTP {} for '{}'",
                            status.as_u16(),
                            prepared.normalized_url
                        )),
                        attempt_count,
                        false,
                    ));
                }

                let final_url = response.url().to_string();
                let normalized_url = normalize_final_url(response.url());
                let declared_content_type = response
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string);
                if response
                    .content_length()
                    .is_some_and(|bytes| bytes as usize > prepared.policy.max_response_bytes)
                {
                    return Err(SemanticAcquisitionFailure::new(
                        AstraError::FetchFailed(format!(
                            "response from '{}' exceeds {} bytes",
                            final_url, prepared.policy.max_response_bytes
                        )),
                        attempt_count,
                        false,
                    ));
                }
                let response_headers = capture_response_headers(response.headers());
                let status_code = status.as_u16();
                let body = response.bytes().await.map_err(|error| {
                    SemanticAcquisitionFailure::new(
                        AstraError::FetchFailed(format!(
                            "failed to read response body from '{}': {error}",
                            prepared.normalized_url
                        )),
                        attempt_count,
                        false,
                    )
                })?;
                if body.len() > prepared.policy.max_response_bytes {
                    return Err(SemanticAcquisitionFailure::new(
                        AstraError::FetchFailed(format!(
                            "response from '{}' exceeds {} bytes",
                            final_url, prepared.policy.max_response_bytes
                        )),
                        attempt_count,
                        false,
                    ));
                }

                let content_hash = sha3_256_hex(body.as_ref());
                let body_text = String::from_utf8_lossy(body.as_ref()).into_owned();
                let response_class = classify_response(
                    declared_content_type.as_deref(),
                    &normalized_url,
                    &body_text,
                );
                let content_type = normalized_content_type(
                    prepared.force_content_type.as_deref(),
                    declared_content_type.as_deref(),
                    &response_class,
                );
                let semantic_html = synthesize_semantic_html(
                    &normalized_url,
                    &content_type,
                    &response_class,
                    &body_text,
                );
                let fetched_at = chrono::Utc::now().timestamp_millis();

                return Ok(SemanticAcquisitionOutcome {
                    acquisition_id: prepared.acquisition_id.clone(),
                    requested_url: prepared.requested_url.clone(),
                    final_url,
                    normalized_url,
                    domain: prepared.domain.clone(),
                    scheme: prepared.scheme.clone(),
                    status_code,
                    content_type,
                    response_class,
                    bytes_received: body.len(),
                    content_hash,
                    attempt_count,
                    duration_ms: started.elapsed().as_millis() as u64,
                    fetched_at,
                    response_headers,
                    robots,
                    warnings,
                    source_body: body_text,
                    semantic_html,
                });
            }
            Err(error) => {
                last_error = Some(AstraError::FetchFailed(format!(
                    "request to '{}' failed: {error}",
                    prepared.normalized_url
                )));
                if attempt < attempts_max {
                    warnings.push(format!(
                        "retrying acquisition after transport error on attempt {} for '{}'",
                        attempt, prepared.domain
                    ));
                    sleep(backoff_for_attempt(attempt)).await;
                    continue;
                }
            }
        }
    }

    Err(SemanticAcquisitionFailure::new(
        last_error.unwrap_or_else(|| {
            AstraError::FetchFailed(format!(
                "acquisition failed for '{}'",
                prepared.normalized_url
            ))
        }),
        attempt_count,
        false,
    ))
}

fn default_respect_robots() -> bool {
    true
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_max_response_bytes() -> usize {
    DEFAULT_MAX_RESPONSE_BYTES
}

fn default_max_retries() -> u8 {
    DEFAULT_MAX_RETRIES
}

fn default_segment_limit() -> usize {
    DEFAULT_SEGMENT_LIMIT
}

fn default_link_limit() -> usize {
    DEFAULT_LINK_LIMIT
}

fn default_requests_per_minute() -> u32 {
    DEFAULT_REQUESTS_PER_MINUTE
}

fn default_persist_report() -> bool {
    true
}

fn default_user_agent() -> String {
    DEFAULT_USER_AGENT.into()
}

fn default_acquisition_path() -> PathBuf {
    if let Some(raw) = std::env::var_os("ASTRA_SEMANTIC_ACQUISITION_PATH") {
        return PathBuf::from(raw);
    }

    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return Path::new(&local_app_data)
            .join("Astra")
            .join("render")
            .join("semantic_acquisition.json");
    }

    std::env::temp_dir()
        .join("astra")
        .join("render")
        .join("semantic_acquisition.json")
}

fn normalized_user_agent(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        DEFAULT_USER_AGENT.into()
    } else {
        trimmed.to_string()
    }
}

fn normalize_allowed_domains(request_host: &str, requested_rules: &[String]) -> Vec<String> {
    let mut normalized = requested_rules
        .iter()
        .filter_map(|value| normalize_domain_rule(value))
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        normalized.push(request_host.to_ascii_lowercase());
    }
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_domain_rule(rule: &str) -> Option<String> {
    let trimmed = rule.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(url) = Url::parse(trimmed) {
        return url.host_str().map(|host| host.to_ascii_lowercase());
    }

    let without_scheme = trimmed.trim_start_matches('.');
    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .trim()
        .trim_start_matches('.');
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn host_allowed(host: &str, allowed_domains: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    allowed_domains
        .iter()
        .any(|rule| host == *rule || host.ends_with(&format!(".{rule}")))
}

fn should_retry_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

fn backoff_for_attempt(attempt: usize) -> Duration {
    Duration::from_millis((attempt as u64).saturating_mul(250))
}

fn capture_response_headers(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    let mut captured = BTreeMap::new();
    for name in [
        CONTENT_TYPE,
        CACHE_CONTROL,
        ETAG,
        LAST_MODIFIED,
        CONTENT_LENGTH,
    ] {
        if let Some(value) = headers.get(&name) {
            if let Ok(value) = value.to_str() {
                captured.insert(name.as_str().to_string(), value.to_string());
            }
        }
    }
    if let Some(value) = headers.get("x-robots-tag") {
        if let Ok(value) = value.to_str() {
            captured.insert("x-robots-tag".into(), value.to_string());
        }
    }
    captured
}

async fn evaluate_robots(
    client: &reqwest::Client,
    target_url: &str,
    user_agent: &str,
) -> RobotsPolicyResult {
    let Ok(target) = Url::parse(target_url) else {
        return RobotsPolicyResult {
            checked: true,
            robots_url: None,
            allowed: true,
            matched_scope: "unknown".into(),
            allow_rules: 0,
            disallow_rules: 0,
            reason: "robots check skipped because target URL was invalid".into(),
            fetch_status: None,
            warnings: vec!["robots policy evaluation skipped invalid URL".into()],
        };
    };

    let Some(host) = target.host_str() else {
        return RobotsPolicyResult {
            checked: true,
            robots_url: None,
            allowed: true,
            matched_scope: "unknown".into(),
            allow_rules: 0,
            disallow_rules: 0,
            reason: "robots check skipped because target URL had no host".into(),
            fetch_status: None,
            warnings: vec!["robots policy evaluation skipped hostless URL".into()],
        };
    };

    let authority = if let Some(port) = target.port() {
        format!("{host}:{port}")
    } else {
        host.to_string()
    };
    let robots_url = format!("{}://{authority}/robots.txt", target.scheme());
    let response = client
        .get(&robots_url)
        .header(reqwest::header::USER_AGENT, user_agent)
        .send()
        .await;

    match response {
        Ok(response) => {
            let status = response.status();
            if status == reqwest::StatusCode::NOT_FOUND {
                return RobotsPolicyResult {
                    checked: true,
                    robots_url: Some(robots_url),
                    allowed: true,
                    matched_scope: "*".into(),
                    allow_rules: 0,
                    disallow_rules: 0,
                    reason: "robots.txt not present".into(),
                    fetch_status: Some(status.as_u16()),
                    warnings: Vec::new(),
                };
            }
            if !status.is_success() {
                return RobotsPolicyResult {
                    checked: true,
                    robots_url: Some(robots_url),
                    allowed: true,
                    matched_scope: "*".into(),
                    allow_rules: 0,
                    disallow_rules: 0,
                    reason: format!("robots.txt returned HTTP {}", status.as_u16()),
                    fetch_status: Some(status.as_u16()),
                    warnings: vec!["robots evaluation failed open on non-success status".into()],
                };
            }
            if response
                .content_length()
                .is_some_and(|value| value as usize > ROBOTS_MAX_BYTES)
            {
                return RobotsPolicyResult {
                    checked: true,
                    robots_url: Some(robots_url),
                    allowed: true,
                    matched_scope: "*".into(),
                    allow_rules: 0,
                    disallow_rules: 0,
                    reason: "robots.txt exceeded parser budget".into(),
                    fetch_status: Some(status.as_u16()),
                    warnings: vec!["robots evaluation failed open on oversized robots.txt".into()],
                };
            }

            match response.bytes().await {
                Ok(body) => {
                    if body.len() > ROBOTS_MAX_BYTES {
                        return RobotsPolicyResult {
                            checked: true,
                            robots_url: Some(robots_url),
                            allowed: true,
                            matched_scope: "*".into(),
                            allow_rules: 0,
                            disallow_rules: 0,
                            reason: "robots.txt exceeded parser budget".into(),
                            fetch_status: Some(status.as_u16()),
                            warnings: vec![
                                "robots evaluation failed open on oversized robots.txt".into()
                            ],
                        };
                    }
                    evaluate_robots_body(
                        &robots_url,
                        user_agent,
                        target.path(),
                        &String::from_utf8_lossy(body.as_ref()),
                        Some(status.as_u16()),
                    )
                }
                Err(error) => RobotsPolicyResult {
                    checked: true,
                    robots_url: Some(robots_url),
                    allowed: true,
                    matched_scope: "*".into(),
                    allow_rules: 0,
                    disallow_rules: 0,
                    reason: format!("robots.txt body read failed: {error}"),
                    fetch_status: Some(status.as_u16()),
                    warnings: vec!["robots evaluation failed open on read error".into()],
                },
            }
        }
        Err(error) => RobotsPolicyResult {
            checked: true,
            robots_url: Some(robots_url),
            allowed: true,
            matched_scope: "*".into(),
            allow_rules: 0,
            disallow_rules: 0,
            reason: format!("robots.txt fetch failed: {error}"),
            fetch_status: None,
            warnings: vec!["robots evaluation failed open on fetch error".into()],
        },
    }
}

fn evaluate_robots_body(
    robots_url: &str,
    user_agent: &str,
    target_path: &str,
    body: &str,
    fetch_status: Option<u16>,
) -> RobotsPolicyResult {
    let rules = parse_robots_rules(body, user_agent);
    let decision = decide_robots_access(target_path, &rules.allow, &rules.disallow);
    RobotsPolicyResult {
        checked: true,
        robots_url: Some(robots_url.to_string()),
        allowed: decision.allowed,
        matched_scope: rules.scope,
        allow_rules: rules.allow.len(),
        disallow_rules: rules.disallow.len(),
        reason: decision.reason,
        fetch_status,
        warnings: Vec::new(),
    }
}

#[derive(Default)]
struct ParsedRobotsRules {
    scope: String,
    allow: Vec<String>,
    disallow: Vec<String>,
}

fn parse_robots_rules(body: &str, user_agent: &str) -> ParsedRobotsRules {
    let user_agent = user_agent.to_ascii_lowercase();
    let mut exact_allow = Vec::new();
    let mut exact_disallow = Vec::new();
    let mut wildcard_allow = Vec::new();
    let mut wildcard_disallow = Vec::new();
    let mut current_targets: Vec<&str> = Vec::new();
    let mut collecting_agents = false;

    for raw_line in body.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            current_targets.clear();
            collecting_agents = false;
            continue;
        }
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let field = field.trim().to_ascii_lowercase();
        let value = value.trim();
        match field.as_str() {
            "user-agent" => {
                if !collecting_agents {
                    current_targets.clear();
                }
                collecting_agents = true;
                let token = value.to_ascii_lowercase();
                if token == "*" {
                    current_targets.push("*");
                } else if user_agent.contains(&token) {
                    current_targets.push("exact");
                }
            }
            "allow" => {
                collecting_agents = false;
                if value.is_empty() {
                    continue;
                }
                for target in &current_targets {
                    if *target == "exact" {
                        exact_allow.push(value.to_string());
                    } else if *target == "*" {
                        wildcard_allow.push(value.to_string());
                    }
                }
            }
            "disallow" => {
                collecting_agents = false;
                if value.is_empty() {
                    continue;
                }
                for target in &current_targets {
                    if *target == "exact" {
                        exact_disallow.push(value.to_string());
                    } else if *target == "*" {
                        wildcard_disallow.push(value.to_string());
                    }
                }
            }
            _ => {
                collecting_agents = false;
            }
        }
    }

    if !exact_allow.is_empty() || !exact_disallow.is_empty() {
        ParsedRobotsRules {
            scope: "exact".into(),
            allow: exact_allow,
            disallow: exact_disallow,
        }
    } else {
        ParsedRobotsRules {
            scope: "*".into(),
            allow: wildcard_allow,
            disallow: wildcard_disallow,
        }
    }
}

struct RobotsDecision {
    allowed: bool,
    reason: String,
}

fn decide_robots_access(
    path: &str,
    allow_rules: &[String],
    disallow_rules: &[String],
) -> RobotsDecision {
    let best_allow = longest_matching_rule(path, allow_rules);
    let best_disallow = longest_matching_rule(path, disallow_rules);

    match (best_allow, best_disallow) {
        (None, None) => RobotsDecision {
            allowed: true,
            reason: "no matching robots rule".into(),
        },
        (Some((allow_rule, allow_len)), Some((disallow_rule, disallow_len))) => {
            if allow_len >= disallow_len {
                RobotsDecision {
                    allowed: true,
                    reason: format!("matched allow rule '{allow_rule}'"),
                }
            } else {
                RobotsDecision {
                    allowed: false,
                    reason: format!("matched disallow rule '{disallow_rule}'"),
                }
            }
        }
        (Some((allow_rule, _)), None) => RobotsDecision {
            allowed: true,
            reason: format!("matched allow rule '{allow_rule}'"),
        },
        (None, Some((disallow_rule, _))) => RobotsDecision {
            allowed: false,
            reason: format!("matched disallow rule '{disallow_rule}'"),
        },
    }
}

fn longest_matching_rule<'a>(path: &str, rules: &'a [String]) -> Option<(&'a str, usize)> {
    rules
        .iter()
        .filter(|rule| !rule.is_empty() && path.starts_with(rule.as_str()))
        .map(|rule| (rule.as_str(), rule.len()))
        .max_by_key(|(_, len)| *len)
}

fn classify_response(
    declared_content_type: Option<&str>,
    normalized_url: &str,
    body: &str,
) -> String {
    if let Some(content_type) = declared_content_type {
        let lower = content_type.to_ascii_lowercase();
        if lower.contains("html") || lower.contains("xhtml") {
            return "html".into();
        }
        if lower.contains("json") {
            return "json".into();
        }
        if lower.contains("xml") || lower.contains("rss") || lower.contains("atom") {
            return "xml".into();
        }
        if lower.contains("text") {
            return "text".into();
        }
    }

    let trimmed = body.trim_start().to_ascii_lowercase();
    if trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || trimmed.contains("<body")
    {
        return "html".into();
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return "json".into();
    }
    if trimmed.starts_with("<?xml") || trimmed.starts_with('<') {
        return "xml".into();
    }
    if normalized_url.ends_with(".json") {
        return "json".into();
    }
    "text".into()
}

fn normalized_content_type(
    forced_content_type: Option<&str>,
    declared_content_type: Option<&str>,
    response_class: &str,
) -> String {
    if let Some(forced) = forced_content_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return forced.to_string();
    }
    if let Some(declared) = declared_content_type {
        return declared
            .split(';')
            .next()
            .unwrap_or(declared)
            .trim()
            .to_string();
    }
    match response_class {
        "json" => "application/json".into(),
        "xml" => "application/xml".into(),
        "text" => "text/plain".into(),
        _ => "text/html".into(),
    }
}

fn synthesize_semantic_html(
    normalized_url: &str,
    content_type: &str,
    response_class: &str,
    body: &str,
) -> String {
    if response_class == "html" {
        return body.to_string();
    }

    let title = Url::parse(normalized_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "resource".into());
    let escaped = escape_html(body);

    format!(
        "<html><head><title>{title}</title><meta name=\"astra:source\" content=\"{content_type}\" /></head><body><article><h1>{title}</h1><p>Normalized {response_class} acquisition for assistant-side extraction.</p><pre>{escaped}</pre></article></body></html>"
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn normalize_final_url(url: &Url) -> String {
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_policy_prefers_more_specific_allow_rule() {
        let rules = parse_robots_rules(
            "User-agent: *\nDisallow: /private\nAllow: /private/docs\n",
            "AstraAssistant/2.0",
        );
        let decision = decide_robots_access("/private/docs/guide", &rules.allow, &rules.disallow);

        assert!(decision.allowed);
        assert!(decision.reason.contains("/private/docs"));
    }

    #[test]
    fn prepare_blocks_domains_outside_policy() {
        let mut engine = SemanticAcquisitionEngine::new_ephemeral();
        let result = engine.prepare(&SemanticAcquisitionRequest {
            url: "https://example.com/data".into(),
            allowed_domains: vec!["docs.example.com".into()],
            respect_robots: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_retries: DEFAULT_MAX_RETRIES,
            max_segments: DEFAULT_SEGMENT_LIMIT,
            max_links: DEFAULT_LINK_LIMIT,
            requests_per_minute: DEFAULT_REQUESTS_PER_MINUTE,
            notarize_to_chain: false,
            attestor: None,
            persist_report: false,
            user_agent: DEFAULT_USER_AGENT.into(),
            force_content_type: None,
        });

        assert!(matches!(result, Err(AstraError::ControlPlaneRejected(_))));
        assert_eq!(engine.stats().blocked_by_policy, 1);
    }

    #[test]
    fn synthetic_html_wraps_json_payloads() {
        let html = synthesize_semantic_html(
            "https://example.com/data.json",
            "application/json",
            "json",
            "{\"answer\":42}",
        );

        assert!(html.contains("Normalized json acquisition"));
        assert!(html.contains("&quot;answer&quot;"));
        assert!(html.contains("<pre>"));
    }
}

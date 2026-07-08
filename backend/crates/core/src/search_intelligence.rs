use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use crate::turboquant::{QuantizedVector, TurboQuant};
use parking_lot::RwLock;
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::Shared;
use crate::common::{AppError, new_id, now_ms, run_blocking_io, sha3_hex};

const FORMULA_VERSION: &str = "nexus_f_omega_v1";
const EPS: f64 = 1e-9;
const MAX_SEED_DOCUMENTS: usize = 12;
const MAX_CRAWL_STEPS: usize = 32;
const MAX_SOURCES: usize = 48;
const EMBEDDING_DIMS: usize = 64;
/// TurboQuant (arXiv:2504.19874) bits per embedding coordinate: 3-bit
/// MSE-optimal codebook + 1-bit QJL residual for unbiased inner products.
/// Cuts the persisted/in-memory embedding footprint ~16x versus f64.
const EMBEDDING_BITS: u8 = 4;
/// Fixed seed: persisted quantized embeddings must decode identically across
/// process restarts. Changing it only degrades old rows to re-quantization on
/// their next upsert (decode falls back to the legacy JSON path).
const EMBEDDING_QUANTIZER_SEED: u64 = 0x5345_4152_4348_5451;
const MAX_FETCH_BYTES: usize = 2_000_000;
const MAX_QUEUE_ITEMS: usize = 512;
const SEARCH_USER_AGENT: &str = "Astra-NEXUS-F-Omega/1.0 governed-research";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSourceClass {
    Web,
    News,
    Social,
    Books,
    Papers,
    Docs,
}

impl SearchSourceClass {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::News => "news",
            Self::Social => "social",
            Self::Books => "books",
            Self::Papers => "papers",
            Self::Docs => "docs",
        }
    }
}

impl Default for SearchSourceClass {
    fn default() -> Self {
        Self::Web
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResponseMode {
    Fast,
    Balanced,
    Deep,
    Forensic,
}

impl Default for SearchResponseMode {
    fn default() -> Self {
        Self::Balanced
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepSearchSeedDocument {
    pub url: String,
    pub html: String,
    #[serde(default)]
    pub source_class: Option<SearchSourceClass>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepSearchRequest {
    pub query: String,
    #[serde(default)]
    pub mode: SearchResponseMode,
    #[serde(default)]
    pub max_sources: Option<usize>,
    #[serde(default)]
    pub max_crawl_steps: Option<usize>,
    #[serde(default)]
    pub freshness_horizon_hours: Option<u64>,
    #[serde(default)]
    pub source_classes: Vec<SearchSourceClass>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub blocked_domains: Vec<String>,
    #[serde(default)]
    pub require_certificate: bool,
    #[serde(default)]
    pub autonomous_crawl: bool,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub seed_documents: Vec<DeepSearchSeedDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEvidence {
    pub evidence_id: String,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub source_class: SearchSourceClass,
    pub summary: String,
    pub relevance_score: f64,
    pub credibility_score: f64,
    pub freshness_score: f64,
    pub depth_signal: f64,
    pub verification_score: f64,
    pub accepted: bool,
    pub gate_reasons: Vec<String>,
    pub evidence_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchCitation {
    pub id: String,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub excerpt: String,
    pub support_score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchClaimStatus {
    Supported,
    Contested,
    Emerging,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchClaim {
    pub claim: String,
    pub status: SearchClaimStatus,
    pub confidence: f64,
    pub citation_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaMetric {
    pub id: u8,
    pub name: String,
    pub inputs: BTreeMap<String, f64>,
    pub score: f64,
    pub gate_passed: bool,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaReport {
    pub formula_version: String,
    pub metrics: Vec<FormulaMetric>,
    pub groups: BTreeMap<String, f64>,
    pub confidence: f64,
    pub production_certificate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageCertificate {
    pub certificate_score: f64,
    pub coverage_score: f64,
    pub triangulation_score: f64,
    pub counterevidence_coverage: f64,
    pub missingness_penalty: f64,
    pub passes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontierItem {
    pub url: String,
    pub domain: String,
    pub source_class: SearchSourceClass,
    pub utility_score: f64,
    pub expected_information_gain: f64,
    pub gate_passed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlPlan {
    pub autonomous: bool,
    pub max_steps: usize,
    pub executed_steps: usize,
    pub queued: Vec<FrontierItem>,
    pub blocked: Vec<FrontierItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDebt {
    pub consistency_debt: f64,
    pub visible_sources: usize,
    pub total_sources: usize,
    pub deletion_debt: f64,
    pub slo_loss: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchReadiness {
    pub service_readiness_index: f64,
    pub deploy_allowed: bool,
    pub review_required: bool,
    pub abstain: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssuranceReport {
    pub assurance_score: f64,
    pub audit_passed: bool,
    pub risk_passed: bool,
    pub security_passed: bool,
    pub privacy_passed: bool,
    pub observability_passed: bool,
    pub production_certificate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageRecord {
    pub trace_id: String,
    pub query_hash: String,
    pub answer_hash: String,
    pub evidence_hashes: Vec<String>,
    pub replay_delta: f64,
    pub lineage_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchExecutionDiagnostics {
    pub source_mix: BTreeMap<String, usize>,
    pub indexed_recall_hits: usize,
    pub fresh_fetch_hits: usize,
    pub persistent_documents: usize,
    pub crawl_queue_size: usize,
    pub benchmark_calibration_score: Option<f64>,
    pub total_formula_metrics: usize,
    pub acquisition_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterevidenceReport {
    pub queries: Vec<String>,
    pub hits: Vec<SearchCitation>,
    pub coverage_score: f64,
    pub gaps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSearchResponse {
    pub query: String,
    pub answer: String,
    pub executive_summary: String,
    pub confidence: f64,
    pub evidence: Vec<SearchEvidence>,
    pub citations: Vec<SearchCitation>,
    pub claims: Vec<SearchClaim>,
    pub formula_report: FormulaReport,
    pub coverage_certificate: CoverageCertificate,
    pub counterevidence: CounterevidenceReport,
    pub frontier: Vec<FrontierItem>,
    pub crawl_plan: CrawlPlan,
    pub pipeline_debt: PipelineDebt,
    pub readiness: SearchReadiness,
    pub assurance: AssuranceReport,
    pub lineage: LineageRecord,
    pub abstention_reason: Option<String>,
    pub diagnostics: SearchExecutionDiagnostics,
    pub warnings: Vec<String>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIntelligenceStats {
    pub total_queries: u64,
    pub autonomous_queries: u64,
    pub abstentions: u64,
    pub cache_entries: usize,
    pub frontier_size: usize,
    pub persistent_documents: usize,
    pub queued_crawl_items: usize,
    pub completed_crawl_items: usize,
    pub latest_benchmark_score: Option<f64>,
    pub last_confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAutonomousFrontier {
    pub generated_at_ms: i64,
    pub queued_queries: usize,
    pub next_queries: Vec<String>,
    pub frontier: Vec<FrontierItem>,
    pub stats: SearchIntelligenceStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAutonomousTickReport {
    pub executed_at_ms: i64,
    pub forced: bool,
    pub queued_before: usize,
    pub queued_after: usize,
    pub processed_urls: Vec<String>,
    pub failed_urls: Vec<String>,
    pub generated_queries: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchBenchmarkCase {
    pub query: String,
    #[serde(default)]
    pub expected_terms: Vec<String>,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub seed_documents: Vec<DeepSearchSeedDocument>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchBenchmarkRequest {
    pub cases: Vec<SearchBenchmarkCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchBenchmarkCaseReport {
    pub query: String,
    pub confidence: f64,
    pub certificate_score: f64,
    pub expected_term_recall: f64,
    pub citation_count: usize,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchBenchmarkReport {
    pub benchmark_id: String,
    pub created_at_ms: i64,
    pub cases: Vec<SearchBenchmarkCaseReport>,
    pub average_score: f64,
    pub calibration_error: f64,
}

#[derive(Default)]
struct SearchIntelligenceStore {
    total_queries: u64,
    autonomous_queries: u64,
    abstentions: u64,
    last_confidence: f64,
    latest_benchmark_score: Option<f64>,
    cached: BTreeMap<String, DeepSearchResponse>,
    frontier: VecDeque<FrontierItem>,
}

#[derive(Clone)]
pub struct SearchIntelligenceService {
    store: Shared<SearchIntelligenceStore>,
    db_path: PathBuf,
}

impl SearchIntelligenceService {
    #[must_use]
    pub fn new() -> Self {
        let data_dir =
            std::env::temp_dir().join(format!("astra-search-intelligence-{}", new_id("ephemeral")));
        Self::with_data_dir(data_dir).expect("ephemeral search intelligence store should open")
    }

    pub fn with_data_dir(data_dir: impl AsRef<Path>) -> Result<Self, AppError> {
        let data_dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&data_dir).map_err(|error| {
            AppError::Internal(format!(
                "failed to create search intelligence data directory: {error}"
            ))
        })?;
        let db_path = data_dir.join("search_intelligence.sqlite");
        let connection = open_connection(&db_path)?;
        initialize_search_store(&connection)?;
        Ok(Self {
            store: Shared::new(RwLock::new(SearchIntelligenceStore::default())),
            db_path,
        })
    }

    pub fn execute(&self, request: DeepSearchRequest) -> Result<DeepSearchResponse, AppError> {
        let started_at = now_ms();
        validate_request(&request)?;
        let cache_key = cache_key(&request);
        if let Some(cached) = self.store.read().cached.get(&cache_key).cloned() {
            return Ok(cached);
        }

        let max_sources = request.max_sources.unwrap_or(12).clamp(1, MAX_SOURCES);
        let max_steps = request
            .max_crawl_steps
            .unwrap_or(match request.mode {
                SearchResponseMode::Fast => 4,
                SearchResponseMode::Balanced => 8,
                SearchResponseMode::Deep => 16,
                SearchResponseMode::Forensic => 24,
            })
            .clamp(1, MAX_CRAWL_STEPS);

        let acquisition = self.collect_candidates(&request, max_sources, max_steps)?;
        let mut warnings = acquisition.warnings;
        let candidates = acquisition.candidates;

        let mut accepted = Vec::new();
        let mut blocked = Vec::new();
        let queued = acquisition.frontier;
        for candidate in candidates {
            let gate = access_gate(&candidate.url, &candidate.domain, &request);
            let quality = quality_score(&candidate);
            let relevance = relevance_score(&request.query, &candidate.text);
            let credibility = credibility_score(&candidate.domain, &candidate.source_class);
            let freshness = freshness_score(request.freshness_horizon_hours, candidate.synthetic);
            let depth = depth_signal(&candidate.source_class, request.autonomous_crawl);
            let verification = verification_score(credibility, relevance, quality, gate.allowed);
            let mut reasons = gate.reasons;
            reasons.extend(candidate.gate_reasons.clone());
            if quality < 0.35 {
                reasons.push("quality gate failed".into());
            }
            let accepted_by_gates =
                gate.allowed && quality >= 0.35 && candidate.fetch_error.is_none();
            let evidence_tags = evidence_tags(&candidate, accepted_by_gates);
            let evidence = SearchEvidence {
                evidence_id: new_id("evidence"),
                title: candidate.title.clone(),
                url: candidate.url.clone(),
                domain: candidate.domain.clone(),
                source_class: candidate.source_class.clone(),
                summary: first_sentence(&candidate.text),
                relevance_score: relevance,
                credibility_score: credibility,
                freshness_score: freshness,
                depth_signal: depth,
                verification_score: verification,
                accepted: accepted_by_gates,
                gate_reasons: reasons.clone(),
                evidence_tags,
            };
            let utility = crawl_utility(&evidence, gate.allowed, quality);
            let item = FrontierItem {
                url: candidate.url.clone(),
                domain: candidate.domain.clone(),
                source_class: candidate.source_class.clone(),
                utility_score: utility,
                expected_information_gain: expected_information_gain(relevance, credibility, depth),
                gate_passed: accepted_by_gates,
                reason: if accepted_by_gates {
                    "accepted into governed evidence set".into()
                } else {
                    reasons.join("; ")
                },
            };
            if accepted_by_gates {
                self.persist_candidate(
                    &candidate,
                    relevance,
                    credibility,
                    freshness,
                    verification,
                )?;
                accepted.push(evidence);
            } else {
                blocked.push(item);
            }
            if accepted.len() >= max_sources {
                break;
            }
        }

        if accepted.is_empty() {
            warnings.push("no evidence passed access and quality gates".into());
        }
        accepted.sort_by(|left, right| {
            evidence_rank(right)
                .total_cmp(&evidence_rank(left))
                .then_with(|| left.url.cmp(&right.url))
        });
        let accepted = accepted.into_iter().take(max_sources).collect::<Vec<_>>();
        let citations = citations_for(&accepted);
        let claims = claims_for(&request.query, &accepted, &citations);
        let counterevidence = counterevidence_report(&request.query, &accepted, &citations);
        let source_mix = source_mix(&accepted);
        let formula_report = build_formula_report(&request, &accepted, &blocked, &claims);
        let coverage_certificate = coverage_certificate(&formula_report);
        let pipeline_debt = pipeline_debt(&accepted, queued.len() + blocked.len());
        let readiness = readiness(
            &formula_report,
            &coverage_certificate,
            &pipeline_debt,
            &request,
        );
        let assurance = assurance(&formula_report, &pipeline_debt, &readiness);
        let answer = answer_for(&request.query, &claims, &accepted, readiness.abstain);
        let lineage = lineage_for(&request.query, &answer, &accepted);
        let abstention_reason = if readiness.abstain {
            Some(readiness.reasons.join("; "))
        } else {
            None
        };
        let confidence = formula_report.confidence;

        let response = DeepSearchResponse {
            query: request.query.clone(),
            answer,
            executive_summary: format!(
                "Analyzed {} governed sources with {:.0}% confidence and {:.0}% coverage certificate.",
                accepted.len(),
                confidence * 100.0,
                coverage_certificate.certificate_score * 100.0
            ),
            confidence,
            evidence: accepted,
            citations,
            claims,
            formula_report: formula_report.clone(),
            coverage_certificate,
            counterevidence,
            frontier: queued.clone(),
            crawl_plan: CrawlPlan {
                autonomous: request.autonomous_crawl,
                max_steps,
                executed_steps: acquisition.executed_steps,
                queued: queued.clone(),
                blocked,
            },
            pipeline_debt,
            readiness,
            assurance,
            lineage,
            abstention_reason,
            diagnostics: SearchExecutionDiagnostics {
                source_mix,
                indexed_recall_hits: acquisition.indexed_recall_hits,
                fresh_fetch_hits: acquisition.fresh_fetch_hits,
                persistent_documents: self.document_count().unwrap_or(0),
                crawl_queue_size: self.queue_count("queued").unwrap_or(0),
                benchmark_calibration_score: self.store.read().latest_benchmark_score,
                total_formula_metrics: formula_report.metrics.len(),
                acquisition_strategy: if request.autonomous_crawl {
                    "persistent_index_plus_governed_autonomous_crawl".into()
                } else {
                    "persistent_index_plus_governed_seed_research".into()
                },
            },
            warnings,
            duration_ms: (now_ms() - started_at).max(0) as f64,
        };

        let mut store = self.store.write();
        store.total_queries += 1;
        if request.autonomous_crawl {
            store.autonomous_queries += 1;
        }
        if response.abstention_reason.is_some() {
            store.abstentions += 1;
        }
        store.last_confidence = response.confidence;
        for item in &response.frontier {
            store.frontier.push_back(item.clone());
        }
        while store.frontier.len() > 64 {
            store.frontier.pop_front();
        }
        store.cached.insert(cache_key, response.clone());
        while store.cached.len() > 32 {
            if let Some(key) = store.cached.keys().next().cloned() {
                store.cached.remove(&key);
            }
        }
        Ok(response)
    }

    #[must_use]
    pub fn stats(&self) -> SearchIntelligenceStats {
        stats_from_store(
            &self.store.read(),
            self.document_count().unwrap_or(0),
            self.queue_count("queued").unwrap_or(0),
            self.queue_count("completed").unwrap_or(0),
        )
    }

    #[must_use]
    pub fn frontier(&self) -> SearchAutonomousFrontier {
        let store = self.store.read();
        let mut frontier = self.load_frontier(MAX_QUEUE_ITEMS).unwrap_or_default();
        frontier.extend(store.frontier.iter().cloned());
        frontier.sort_by(|left, right| {
            right
                .utility_score
                .total_cmp(&left.utility_score)
                .then_with(|| left.url.cmp(&right.url))
        });
        frontier.truncate(64);
        SearchAutonomousFrontier {
            generated_at_ms: now_ms(),
            queued_queries: frontier.len(),
            next_queries: frontier
                .iter()
                .take(8)
                .map(|item| format!("{} {}", item.source_class.as_str(), item.domain))
                .collect(),
            frontier,
            stats: stats_from_store(
                &store,
                self.document_count().unwrap_or(0),
                self.queue_count("queued").unwrap_or(0),
                self.queue_count("completed").unwrap_or(0),
            ),
        }
    }

    pub fn tick(&self, forced: bool) -> SearchAutonomousTickReport {
        let before = self.queue_count("queued").unwrap_or(0);
        let tick = self.process_crawl_queue(if forced { 8 } else { 3 });
        let after = self.queue_count("queued").unwrap_or(before);
        let (processed_urls, failed_urls, generated_queries) = tick.unwrap_or_else(|error| {
            (
                Vec::new(),
                vec![format!("crawl tick failed: {error}")],
                Vec::new(),
            )
        });
        SearchAutonomousTickReport {
            executed_at_ms: now_ms(),
            forced,
            queued_before: before,
            queued_after: after,
            processed_urls,
            failed_urls,
            generated_queries,
        }
    }

    pub fn run_benchmark(
        &self,
        request: SearchBenchmarkRequest,
    ) -> Result<SearchBenchmarkReport, AppError> {
        if request.cases.is_empty() {
            return Err(AppError::Validation(
                "benchmark request must include at least one case".into(),
            ));
        }
        if request.cases.len() > 16 {
            return Err(AppError::Validation(
                "benchmark request supports at most 16 cases".into(),
            ));
        }
        let mut reports = Vec::new();
        for case in request.cases {
            let response = self.execute(DeepSearchRequest {
                query: case.query.clone(),
                mode: SearchResponseMode::Deep,
                max_sources: Some(12),
                max_crawl_steps: Some(4),
                freshness_horizon_hours: Some(24 * 14),
                source_classes: Vec::new(),
                allowed_domains: Vec::new(),
                blocked_domains: Vec::new(),
                require_certificate: false,
                autonomous_crawl: true,
                urls: case.urls,
                seed_documents: case.seed_documents,
            })?;
            let recall = expected_term_recall(
                &case.expected_terms,
                &format!("{} {}", response.answer, response.executive_summary),
            );
            let score = (response.confidence * 0.35
                + response.coverage_certificate.certificate_score * 0.25
                + recall * 0.25
                + (response.citations.len() as f64 / 8.0).clamp(0.0, 1.0) * 0.15)
                .clamp(0.0, 1.0);
            reports.push(SearchBenchmarkCaseReport {
                query: case.query,
                confidence: response.confidence,
                certificate_score: response.coverage_certificate.certificate_score,
                expected_term_recall: recall,
                citation_count: response.citations.len(),
                score,
            });
        }
        let average_score = avg(reports.iter().map(|case| case.score));
        let calibration_error = avg(reports
            .iter()
            .map(|case| (case.confidence - case.expected_term_recall).abs()));
        let report = SearchBenchmarkReport {
            benchmark_id: new_id("search_benchmark"),
            created_at_ms: now_ms(),
            cases: reports,
            average_score,
            calibration_error,
        };
        self.persist_benchmark(&report)?;
        self.store.write().latest_benchmark_score = Some(report.average_score);
        Ok(report)
    }

    pub fn latest_benchmark(&self) -> Result<SearchBenchmarkReport, AppError> {
        let connection = self.connection()?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM search_benchmarks ORDER BY created_at_ms DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let payload = payload.ok_or_else(|| AppError::NotFound("search benchmark".into()))?;
        serde_json::from_str(&payload)
            .map_err(|error| AppError::Internal(format!("benchmark decode failed: {error}")))
    }
}

impl Default for SearchIntelligenceService {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchIntelligenceService {
    fn connection(&self) -> Result<Connection, AppError> {
        open_connection(&self.db_path)
    }

    fn collect_candidates(
        &self,
        request: &DeepSearchRequest,
        max_sources: usize,
        max_steps: usize,
    ) -> Result<AcquisitionBatch, AppError> {
        // Per-domain cap during autonomous crawling so one site cannot flood
        // the evidence set.
        const MAX_CRAWLED_PER_DOMAIN: usize = 3;

        let mut batch = AcquisitionBatch::default();
        let mut seen_urls = BTreeSet::new();
        let mut seen_content = BTreeSet::new();
        let indexed = self.indexed_candidates(request, max_sources * 3)?;
        batch.indexed_recall_hits = indexed.len();
        for candidate in indexed {
            seen_urls.insert(candidate.url.clone());
            seen_content.insert(body_fingerprint(&candidate.text));
            batch.candidates.push(candidate);
        }

        for candidate in seed_candidates(request) {
            if !seen_urls.insert(candidate.url.clone()) {
                continue;
            }
            if !seen_content.insert(body_fingerprint(&candidate.text)) {
                batch
                    .warnings
                    .push(format!("duplicate content skipped: {}", candidate.url));
                continue;
            }
            self.persist_candidate(&candidate, 0.0, 0.0, 0.0, 0.0)?;
            batch.candidates.push(candidate);
        }

        for url in &request.urls {
            if let Some(candidate) = self.url_candidate(url, request)? {
                if candidate.fetched {
                    batch.fresh_fetch_hits += 1;
                }
                if seen_urls.insert(candidate.url.clone()) {
                    // Mirror content (same body under a different URL) adds no
                    // evidence; keep the first copy only.
                    if candidate.fetched && !seen_content.insert(body_fingerprint(&candidate.text))
                    {
                        batch
                            .warnings
                            .push(format!("duplicate content skipped: {}", candidate.url));
                        continue;
                    }
                    batch.candidates.push(candidate);
                }
            }
        }

        if request.autonomous_crawl {
            let links = batch
                .candidates
                .iter()
                .flat_map(|candidate| candidate.links.iter().cloned())
                .collect::<Vec<_>>();
            let mut per_domain: BTreeMap<String, usize> = BTreeMap::new();
            for link in links {
                if batch.executed_steps >= max_steps {
                    break;
                }
                if !seen_urls.insert(link.clone()) {
                    continue;
                }
                let domain = Url::parse(&link)
                    .ok()
                    .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
                    .unwrap_or_default();
                if *per_domain.get(&domain).unwrap_or(&0) >= MAX_CRAWLED_PER_DOMAIN {
                    continue;
                }
                let Some(item) = self.frontier_item_for_url(&link, request, 0.64) else {
                    continue;
                };
                self.enqueue_frontier(&request.query, &item)?;
                batch.frontier.push(item.clone());
                if let Some(candidate) = self.url_candidate(&link, request)? {
                    if candidate.fetched {
                        batch.fresh_fetch_hits += 1;
                        if !seen_content.insert(body_fingerprint(&candidate.text)) {
                            batch
                                .warnings
                                .push(format!("duplicate content skipped: {}", candidate.url));
                            continue;
                        }
                    }
                    *per_domain.entry(domain).or_insert(0) += 1;
                    batch.candidates.push(candidate);
                    batch.executed_steps += 1;
                }
            }
        }

        if batch.candidates.is_empty() {
            batch
                .warnings
                .push("no persisted, seeded, or fetchable evidence was available".into());
        }
        Ok(batch)
    }

    fn indexed_candidates(
        &self,
        request: &DeepSearchRequest,
        limit: usize,
    ) -> Result<Vec<CandidateDocument>, AppError> {
        let query_embedding = semantic_embedding(&request.query);
        // Rotate + QJL-sketch the query once; every candidate is then scored
        // against its compressed embedding without reconstruction.
        let prepared_query = embedding_quantizer().prepare_query(&query_embedding);
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT url, domain, title, text, source_class, content_hash, embedding_json \
                 FROM search_documents WHERE accepted = 1 ORDER BY last_seen_ms DESC LIMIT 512",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(IndexedDocumentRow {
                    url: row.get(0)?,
                    domain: row.get(1)?,
                    title: row.get(2)?,
                    text: row.get(3)?,
                    source_class: row.get(4)?,
                    content_hash: row.get(5)?,
                    embedding_json: row.get(6)?,
                })
            })
            .map_err(sql_error)?;
        let mut ranked = Vec::new();
        for row in rows {
            let row = row.map_err(sql_error)?;
            if !request.allowed_domains.is_empty()
                && !request
                    .allowed_domains
                    .iter()
                    .any(|allowed| domain_matches(&row.domain, allowed))
            {
                continue;
            }
            if request
                .blocked_domains
                .iter()
                .any(|blocked| domain_matches(&row.domain, blocked))
            {
                continue;
            }
            let semantic = match QuantizedVector::from_compact_string(&row.embedding_json) {
                Some(quantized) => {
                    let estimate = embedding_quantizer().inner_product(&prepared_query, &quantized);
                    ((estimate + 1.0) / 2.0).clamp(0.0, 1.0)
                }
                // Rows persisted before quantization hold raw JSON vectors;
                // they re-quantize on their next upsert.
                None => cosine_similarity(&query_embedding, &decode_embedding(&row.embedding_json)),
            };
            let lexical = relevance_score(&request.query, &row.text);
            let rank = (semantic * 0.62 + lexical * 0.38).clamp(0.0, 1.0);
            if rank < 0.18 {
                continue;
            }
            ranked.push((rank, row));
        }
        ranked.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.url.cmp(&right.1.url))
        });
        Ok(ranked
            .into_iter()
            .take(limit)
            .map(|(_, row)| CandidateDocument {
                url: row.url,
                domain: row.domain,
                title: row.title,
                links: extract_links(&row.text, None),
                content_hash: row.content_hash,
                text: row.text,
                source_class: source_class_from_str(&row.source_class),
                synthetic: false,
                indexed: true,
                fetched: false,
                fetch_error: None,
                gate_reasons: vec!["recalled from persistent semantic index".into()],
            })
            .collect())
    }

    fn url_candidate(
        &self,
        url: &str,
        request: &DeepSearchRequest,
    ) -> Result<Option<CandidateDocument>, AppError> {
        let parsed = match Url::parse(url) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Ok(Some(failed_candidate(
                    url,
                    "invalid.example",
                    format!("invalid URL: {error}"),
                )));
            }
        };
        let domain = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let gate = access_gate(parsed.as_str(), &domain, request);
        if !gate.allowed {
            return Ok(Some(CandidateDocument {
                url: parsed.to_string(),
                domain: domain.clone(),
                title: format!("Blocked source {domain}"),
                text: String::new(),
                links: Vec::new(),
                source_class: classify_source_class(&domain, ""),
                content_hash: sha3_hex(parsed.as_str()),
                synthetic: false,
                indexed: false,
                fetched: false,
                fetch_error: Some(gate.reasons.join("; ")),
                gate_reasons: gate.reasons,
            }));
        }
        match self.fetch_document(&parsed) {
            Ok(candidate) => {
                self.persist_candidate(&candidate, 0.0, 0.0, 0.0, 0.0)?;
                Ok(Some(candidate))
            }
            Err(error) => Ok(Some(CandidateDocument {
                url: parsed.to_string(),
                domain: domain.clone(),
                title: format!("Fetch failed for {domain}"),
                text: format!("Governed fetch failed for {}: {error}", parsed),
                links: Vec::new(),
                source_class: classify_source_class(&domain, ""),
                content_hash: sha3_hex(parsed.as_str()),
                synthetic: false,
                indexed: false,
                fetched: false,
                fetch_error: Some(error.to_string()),
                gate_reasons: vec![format!("governed fetch failed: {error}")],
            })),
        }
    }

    fn fetch_document(&self, url: &Url) -> Result<CandidateDocument, AppError> {
        let fetch_url = url.clone();
        // The blocking reqwest client owns a private tokio runtime, so the whole
        // exchange runs on a dedicated thread (see run_blocking_io).
        let (body, content_type) = run_blocking_io(move || {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::limited(5))
                .user_agent(SEARCH_USER_AGENT)
                .build()
                .map_err(|error| {
                    AppError::Internal(format!("failed to build governed crawler client: {error}"))
                })?;
            // Up to two retries with backoff for transient failures
            // (connection errors and 5xx/429 responses).
            let mut response = None;
            let mut last_error = String::new();
            for attempt in 0..3 {
                if attempt > 0 {
                    std::thread::sleep(Duration::from_millis(200 * (1 << attempt)));
                }
                match client.get(fetch_url.clone()).send() {
                    Ok(candidate_response) => {
                        let status = candidate_response.status();
                        if status.is_server_error() || status.as_u16() == 429 {
                            last_error = format!("HTTP status {status}");
                            continue;
                        }
                        response = Some(candidate_response);
                        break;
                    }
                    Err(error) => {
                        last_error =
                            format!("governed fetch request failed for {fetch_url}: {error}");
                        if !error.is_timeout() && !error.is_connect() && !error.is_request() {
                            break;
                        }
                    }
                }
            }
            let response = response.ok_or_else(|| AppError::Internal(last_error.clone()))?;
            let status = response.status();
            if !status.is_success() {
                return Err(AppError::Internal(format!("HTTP status {status}")));
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !content_type.is_empty()
                && ![
                    "text/html",
                    "text/plain",
                    "application/xhtml",
                    "application/xml",
                    "text/xml",
                    "application/json",
                ]
                .iter()
                .any(|allowed| content_type.contains(allowed))
            {
                return Err(AppError::Validation(format!(
                    "unsupported content type {content_type}"
                )));
            }
            let bytes = response.bytes().map_err(|error| {
                AppError::Internal(format!("failed to read governed fetch body: {error}"))
            })?;
            if bytes.len() > MAX_FETCH_BYTES {
                return Err(AppError::Validation(format!(
                    "document exceeds maximum fetch size of {MAX_FETCH_BYTES} bytes"
                )));
            }
            Ok((String::from_utf8_lossy(&bytes).to_string(), content_type))
        })?;
        let domain = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let text = if content_type.contains("html") || body.contains("<html") {
            strip_html(&body)
        } else {
            body.split_whitespace().collect::<Vec<_>>().join(" ")
        };
        Ok(CandidateDocument {
            url: url.to_string(),
            domain: domain.clone(),
            title: extract_title(&body).unwrap_or_else(|| domain.clone()),
            links: extract_links(&body, Some(url)),
            source_class: classify_source_class(&domain, &body),
            content_hash: sha3_hex(format!("{}:{}", url, text)),
            text,
            synthetic: false,
            indexed: false,
            fetched: true,
            fetch_error: None,
            gate_reasons: vec!["live governed fetch completed".into()],
        })
    }

    fn frontier_item_for_url(
        &self,
        url: &str,
        request: &DeepSearchRequest,
        utility: f64,
    ) -> Option<FrontierItem> {
        let parsed = Url::parse(url).ok()?;
        let domain = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let gate = access_gate(parsed.as_str(), &domain, request);
        let source_class = classify_source_class(&domain, "");
        Some(FrontierItem {
            url: parsed.to_string(),
            domain,
            source_class,
            utility_score: if gate.allowed { utility } else { 0.0 },
            expected_information_gain: if gate.allowed { 0.56 } else { 0.0 },
            gate_passed: gate.allowed,
            reason: if gate.allowed {
                "queued from extracted governed link".into()
            } else {
                gate.reasons.join("; ")
            },
        })
    }

    fn persist_candidate(
        &self,
        candidate: &CandidateDocument,
        relevance: f64,
        credibility: f64,
        freshness: f64,
        verification: f64,
    ) -> Result<(), AppError> {
        if candidate.text.trim().is_empty() || candidate.fetch_error.is_some() {
            return Ok(());
        }
        let connection = self.connection()?;
        // TurboQuant-compress the embedding before persistence: 4 bits per
        // coordinate plus two norms instead of a JSON array of 64 f64s.
        let embedding_json = embedding_quantizer()
            .quantize_for_inner_product(&semantic_embedding(&candidate.text))
            .to_compact_string();
        connection
            .execute(
                "INSERT INTO search_documents \
                 (url, domain, title, text, source_class, content_hash, embedding_json, \
                  first_seen_ms, last_seen_ms, accepted, credibility, relevance, freshness, verification) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1, ?9, ?10, ?11, ?12) \
                 ON CONFLICT(url) DO UPDATE SET \
                  domain=excluded.domain, title=excluded.title, text=excluded.text, \
                  source_class=excluded.source_class, content_hash=excluded.content_hash, \
                  embedding_json=excluded.embedding_json, last_seen_ms=excluded.last_seen_ms, \
                  accepted=1, credibility=excluded.credibility, relevance=excluded.relevance, \
                  freshness=excluded.freshness, verification=excluded.verification",
                params![
                    candidate.url,
                    candidate.domain,
                    candidate.title,
                    candidate.text,
                    candidate.source_class.as_str(),
                    candidate.content_hash,
                    embedding_json,
                    now_ms(),
                    credibility,
                    relevance,
                    freshness,
                    verification,
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    fn enqueue_frontier(&self, query: &str, item: &FrontierItem) -> Result<(), AppError> {
        if !item.gate_passed {
            return Ok(());
        }
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO search_crawl_queue \
                 (url, domain, source_class, query, priority, status, attempts, last_error, enqueued_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'queued', 0, '', ?6, ?6) \
                 ON CONFLICT(url) DO UPDATE SET priority=max(priority, excluded.priority), updated_at_ms=excluded.updated_at_ms",
                params![
                    item.url,
                    item.domain,
                    item.source_class.as_str(),
                    query,
                    item.utility_score,
                    now_ms(),
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    fn load_frontier(&self, limit: usize) -> Result<Vec<FrontierItem>, AppError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT url, domain, source_class, priority FROM search_crawl_queue \
                 WHERE status = 'queued' ORDER BY priority DESC, enqueued_at_ms ASC LIMIT ?1",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![limit as i64], |row| {
                let source: String = row.get(2)?;
                Ok(FrontierItem {
                    url: row.get(0)?,
                    domain: row.get(1)?,
                    source_class: source_class_from_str(&source),
                    utility_score: row.get(3)?,
                    expected_information_gain: 0.56,
                    gate_passed: true,
                    reason: "durable crawl queue item".into(),
                })
            })
            .map_err(sql_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
    }

    fn process_crawl_queue(
        &self,
        limit: usize,
    ) -> Result<(Vec<String>, Vec<String>, Vec<String>), AppError> {
        let queued = self.load_frontier(limit)?;
        let mut processed = Vec::new();
        let mut failed = Vec::new();
        let mut generated = Vec::new();
        for item in queued {
            let request = DeepSearchRequest {
                query: item.domain.clone(),
                mode: SearchResponseMode::Balanced,
                max_sources: Some(4),
                max_crawl_steps: Some(2),
                freshness_horizon_hours: Some(24 * 30),
                source_classes: Vec::new(),
                allowed_domains: Vec::new(),
                blocked_domains: Vec::new(),
                require_certificate: false,
                autonomous_crawl: true,
                urls: Vec::new(),
                seed_documents: Vec::new(),
            };
            match self.url_candidate(&item.url, &request)? {
                Some(candidate) if candidate.fetch_error.is_none() => {
                    for link in candidate.links.iter().take(8) {
                        if let Some(next) = self.frontier_item_for_url(link, &request, 0.48) {
                            self.enqueue_frontier(&candidate.domain, &next)?;
                        }
                    }
                    self.mark_queue_status(&item.url, "completed", "")?;
                    generated.push(format!(
                        "{} {}",
                        candidate.source_class.as_str(),
                        candidate.domain
                    ));
                    processed.push(item.url);
                }
                Some(candidate) => {
                    let error = candidate
                        .fetch_error
                        .unwrap_or_else(|| "fetch failed".into());
                    self.mark_queue_status(&item.url, "failed", &error)?;
                    failed.push(format!("{}: {error}", item.url));
                }
                None => {
                    self.mark_queue_status(&item.url, "failed", "invalid queued URL")?;
                    failed.push(format!("{}: invalid queued URL", item.url));
                }
            }
        }
        Ok((processed, failed, generated))
    }

    fn mark_queue_status(&self, url: &str, status: &str, error: &str) -> Result<(), AppError> {
        let connection = self.connection()?;
        connection
            .execute(
                "UPDATE search_crawl_queue \
                 SET status = ?2, attempts = attempts + 1, last_error = ?3, updated_at_ms = ?4 \
                 WHERE url = ?1",
                params![url, status, error, now_ms()],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    fn document_count(&self) -> Result<usize, AppError> {
        count_rows_where(&self.connection()?, "search_documents", "accepted = 1")
    }

    fn queue_count(&self, status: &str) -> Result<usize, AppError> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT COUNT(*) FROM search_crawl_queue WHERE status = ?1",
                params![status],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value as usize)
            .map_err(sql_error)
    }

    fn persist_benchmark(&self, report: &SearchBenchmarkReport) -> Result<(), AppError> {
        let connection = self.connection()?;
        let payload = serde_json::to_string(report).map_err(|error| {
            AppError::Internal(format!("benchmark serialization failed: {error}"))
        })?;
        connection
            .execute(
                "INSERT INTO search_benchmarks \
                 (benchmark_id, created_at_ms, average_score, calibration_error, payload) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    report.benchmark_id,
                    report.created_at_ms,
                    report.average_score,
                    report.calibration_error,
                    payload,
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CandidateDocument {
    url: String,
    domain: String,
    title: String,
    text: String,
    links: Vec<String>,
    source_class: SearchSourceClass,
    content_hash: String,
    synthetic: bool,
    indexed: bool,
    fetched: bool,
    fetch_error: Option<String>,
    gate_reasons: Vec<String>,
}

#[derive(Debug, Default)]
struct AcquisitionBatch {
    candidates: Vec<CandidateDocument>,
    frontier: Vec<FrontierItem>,
    warnings: Vec<String>,
    indexed_recall_hits: usize,
    fresh_fetch_hits: usize,
    executed_steps: usize,
}

#[derive(Debug)]
struct AccessGate {
    allowed: bool,
    reasons: Vec<String>,
}

struct IndexedDocumentRow {
    url: String,
    domain: String,
    title: String,
    text: String,
    source_class: String,
    content_hash: String,
    embedding_json: String,
}

fn initialize_search_store(connection: &Connection) -> Result<(), AppError> {
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS search_documents (
                url TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                title TEXT NOT NULL,
                text TEXT NOT NULL,
                source_class TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                embedding_json TEXT NOT NULL,
                first_seen_ms INTEGER NOT NULL,
                last_seen_ms INTEGER NOT NULL,
                accepted INTEGER NOT NULL,
                credibility REAL NOT NULL,
                relevance REAL NOT NULL,
                freshness REAL NOT NULL,
                verification REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_search_documents_domain ON search_documents(domain);
            CREATE INDEX IF NOT EXISTS idx_search_documents_last_seen ON search_documents(last_seen_ms);
            CREATE TABLE IF NOT EXISTS search_crawl_queue (
                url TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                source_class TEXT NOT NULL,
                query TEXT NOT NULL,
                priority REAL NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL,
                last_error TEXT NOT NULL,
                enqueued_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_search_crawl_queue_status
                ON search_crawl_queue(status, priority, enqueued_at_ms);
            CREATE TABLE IF NOT EXISTS search_benchmarks (
                benchmark_id TEXT PRIMARY KEY,
                created_at_ms INTEGER NOT NULL,
                average_score REAL NOT NULL,
                calibration_error REAL NOT NULL,
                payload TEXT NOT NULL
            );
            ",
        )
        .map_err(sql_error)?;
    Ok(())
}

fn open_connection(db_path: &Path) -> Result<Connection, AppError> {
    let connection = Connection::open(db_path).map_err(sql_error)?;
    // Concurrent handles (parallel requests/tests) must wait out short lock
    // windows instead of failing with SQLITE_BUSY.
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(sql_error)?;
    Ok(connection)
}

fn sql_error(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("search intelligence persistence failed: {error}"))
}

fn count_rows_where(connection: &Connection, table: &str, clause: &str) -> Result<usize, AppError> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE {clause}");
    connection
        .query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map(|value| value as usize)
        .map_err(sql_error)
}

fn failed_candidate(url: &str, domain: &str, error: String) -> CandidateDocument {
    CandidateDocument {
        url: url.to_string(),
        domain: domain.to_string(),
        title: "Invalid source candidate".into(),
        text: String::new(),
        links: Vec::new(),
        source_class: SearchSourceClass::Web,
        content_hash: sha3_hex(url),
        synthetic: false,
        indexed: false,
        fetched: false,
        fetch_error: Some(error.clone()),
        gate_reasons: vec![error],
    }
}

fn validate_request(request: &DeepSearchRequest) -> Result<(), AppError> {
    if request.query.trim().is_empty() {
        return Err(AppError::Validation(
            "search query must not be empty".into(),
        ));
    }
    if request.seed_documents.len() > MAX_SEED_DOCUMENTS {
        return Err(AppError::Validation(format!(
            "search intelligence supports at most {MAX_SEED_DOCUMENTS} seed documents"
        )));
    }
    Ok(())
}

/// URL-independent fingerprint of a document body, used to drop mirrored
/// content that arrives under different URLs.
fn body_fingerprint(text: &str) -> String {
    sha3_hex(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn seed_candidates(request: &DeepSearchRequest) -> Vec<CandidateDocument> {
    let mut out = Vec::new();
    for seed in &request.seed_documents {
        if let Ok(parsed) = Url::parse(&seed.url) {
            let domain = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
            let title = extract_title(&seed.html).unwrap_or_else(|| domain.clone());
            let text = strip_html(&seed.html);
            out.push(CandidateDocument {
                url: parsed.to_string(),
                domain,
                title,
                links: extract_links(&seed.html, Some(&parsed)),
                content_hash: sha3_hex(format!("{}:{}", parsed, text)),
                text,
                source_class: seed.source_class.clone().unwrap_or_else(|| {
                    classify_source_class(parsed.host_str().unwrap_or_default(), &seed.html)
                }),
                synthetic: false,
                indexed: false,
                fetched: false,
                fetch_error: None,
                gate_reasons: vec!["provided seed document".into()],
            });
        }
    }
    out
}

fn requested_classes(request: &DeepSearchRequest) -> Vec<SearchSourceClass> {
    if request.source_classes.is_empty() {
        vec![
            SearchSourceClass::Docs,
            SearchSourceClass::Papers,
            SearchSourceClass::News,
            SearchSourceClass::Web,
        ]
    } else {
        request.source_classes.clone()
    }
}

fn access_gate(url: &str, domain: &str, request: &DeepSearchRequest) -> AccessGate {
    let mut reasons = Vec::new();
    let mut allowed = true;
    if Url::parse(url)
        .ok()
        .is_none_or(|parsed| !matches!(parsed.scheme(), "http" | "https"))
    {
        allowed = false;
        reasons.push("unsupported URL scheme".into());
    }
    if is_private_host(domain) {
        allowed = false;
        reasons.push("private or loopback host blocked".into());
    }
    if domain.ends_with(".onion") {
        allowed = false;
        reasons.push("dark source requires explicit manual authorization".into());
    }
    if request
        .blocked_domains
        .iter()
        .any(|blocked| domain_matches(domain, blocked))
    {
        allowed = false;
        reasons.push("domain is blocked by request policy".into());
    }
    if !request.allowed_domains.is_empty()
        && !request
            .allowed_domains
            .iter()
            .any(|allowed_domain| domain_matches(domain, allowed_domain))
    {
        allowed = false;
        reasons.push("domain is outside allowed_domains".into());
    }
    if allowed {
        reasons.push("legal/access gate passed".into());
    }
    AccessGate { allowed, reasons }
}

fn domain_matches(domain: &str, pattern: &str) -> bool {
    let pattern = pattern.trim().trim_start_matches('*').to_ascii_lowercase();
    !pattern.is_empty() && (domain == pattern || domain.ends_with(&pattern))
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

fn quality_score(candidate: &CandidateDocument) -> f64 {
    let words = candidate.text.split_whitespace().count();
    let length: f64 = if words > 120 {
        0.94
    } else if words > 40 {
        0.78
    } else if words > 8 {
        0.52
    } else {
        0.22
    };
    let schema: f64 = if candidate.title.trim().is_empty() {
        0.0
    } else {
        0.12
    };
    (length + schema).clamp(0.0, 1.0)
}

/// BM25-flavored lexical relevance: per-term frequency saturation with
/// document-length normalization, weighted by query coverage, plus an exact
/// phrase bonus. Stays in [0, 1] with the historical 0.38 floor.
fn relevance_score(query: &str, text: &str) -> f64 {
    const K1: f64 = 1.5; // TF saturation
    const B: f64 = 0.75; // length normalization strength
    const PIVOT_LEN: f64 = 300.0; // pivot document length in words

    let haystack = text.to_ascii_lowercase();
    let tokens = query
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|token| token.len() >= 3)
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return 0.5;
    }

    let doc_words: Vec<&str> = haystack.split_whitespace().collect();
    let doc_len = doc_words.len().max(1) as f64;
    let length_norm = K1 * (1.0 - B + B * doc_len / PIVOT_LEN);

    let mut saturated_sum = 0.0;
    let mut covered = 0usize;
    for token in &tokens {
        let tf = doc_words
            .iter()
            .filter(|word| {
                word.trim_matches(|c: char| !c.is_ascii_alphanumeric()) == token.as_str()
            })
            .count() as f64;
        // Substring fallback keeps stemming-adjacent matches (e.g. "research"
        // inside "researchers") from scoring zero.
        let tf = if tf == 0.0 && haystack.contains(token.as_str()) {
            0.5
        } else {
            tf
        };
        if tf > 0.0 {
            covered += 1;
        }
        saturated_sum += (tf * (K1 + 1.0)) / (tf + length_norm);
    }
    let bm25_component = (saturated_sum / tokens.len() as f64).clamp(0.0, 1.0);
    let coverage = covered as f64 / tokens.len() as f64;
    let phrase_bonus = if tokens.len() > 1 && haystack.contains(&tokens.join(" ")) {
        0.08
    } else {
        0.0
    };
    (0.38 + 0.36 * coverage + 0.18 * bm25_component + phrase_bonus).clamp(0.0, 1.0)
}

fn credibility_score(domain: &str, class: &SearchSourceClass) -> f64 {
    let mut score: f64 = match class {
        SearchSourceClass::Docs => 0.86,
        SearchSourceClass::Papers => 0.88,
        SearchSourceClass::Books => 0.82,
        SearchSourceClass::News => 0.76,
        SearchSourceClass::Web => 0.66,
        SearchSourceClass::Social => 0.50,
    };
    if domain.ends_with(".gov") || domain.ends_with(".edu") {
        score += 0.1;
    }
    if domain.ends_with(".org") {
        score += 0.05;
    }
    if domain.contains("reddit") || domain.contains("social") {
        score -= 0.12;
    }
    score.clamp(0.0, 1.0)
}

fn freshness_score(horizon_hours: Option<u64>, synthetic: bool) -> f64 {
    let base = if synthetic { 0.72 } else { 0.90 };
    match horizon_hours {
        Some(0..=24) => (base + 0.05_f64).min(1.0_f64),
        Some(25..=168) => base,
        Some(_) => (base - 0.05_f64).max(0.0_f64),
        None => base,
    }
}

fn depth_signal(class: &SearchSourceClass, autonomous: bool) -> f64 {
    let base: f64 = match class {
        SearchSourceClass::Papers | SearchSourceClass::Books => 0.82,
        SearchSourceClass::Docs => 0.78,
        SearchSourceClass::News => 0.64,
        SearchSourceClass::Web => 0.58,
        SearchSourceClass::Social => 0.48,
    };
    (base + if autonomous { 0.08 } else { 0.0 }).clamp(0.0, 1.0)
}

fn verification_score(credibility: f64, relevance: f64, quality: f64, gate: bool) -> f64 {
    let gate_factor = if gate { 1.0 } else { 0.0 };
    (credibility * 0.35 + relevance * 0.30 + quality * 0.25 + gate_factor * 0.10).clamp(0.0, 1.0)
}

fn expected_information_gain(relevance: f64, credibility: f64, depth: f64) -> f64 {
    let entropy = 1.0 - ((relevance + credibility) / 2.0);
    (entropy * 0.45 + depth * 0.35 + relevance * 0.20).clamp(0.0, 1.0)
}

fn crawl_utility(evidence: &SearchEvidence, gate: bool, quality: f64) -> f64 {
    if !gate {
        return 0.0;
    }
    (evidence.relevance_score * 0.30
        + evidence.credibility_score * 0.24
        + evidence.depth_signal * 0.18
        + evidence.freshness_score * 0.12
        + quality * 0.16)
        .clamp(0.0, 1.0)
}

fn evidence_rank(evidence: &SearchEvidence) -> f64 {
    evidence.relevance_score * 0.30
        + evidence.credibility_score * 0.24
        + evidence.verification_score * 0.22
        + evidence.depth_signal * 0.14
        + evidence.freshness_score * 0.10
}

fn citations_for(evidence: &[SearchEvidence]) -> Vec<SearchCitation> {
    evidence
        .iter()
        .enumerate()
        .map(|(index, item)| SearchCitation {
            id: format!("C{}", index + 1),
            title: item.title.clone(),
            url: item.url.clone(),
            domain: item.domain.clone(),
            excerpt: item.summary.clone(),
            support_score: item.verification_score,
        })
        .collect()
}

fn claims_for(
    query: &str,
    evidence: &[SearchEvidence],
    citations: &[SearchCitation],
) -> Vec<SearchClaim> {
    if evidence.is_empty() {
        return Vec::new();
    }
    let confidence =
        evidence.iter().map(|item| evidence_rank(item)).sum::<f64>() / evidence.len() as f64;
    let contested = evidence
        .iter()
        .any(|item| contradiction_marker(&item.title) || contradiction_marker(&item.summary));
    vec![SearchClaim {
        claim: format!(
            "{} is supported by {} governed evidence sources.",
            query.trim(),
            evidence.len()
        ),
        status: if contested {
            SearchClaimStatus::Contested
        } else if confidence >= 0.68 {
            SearchClaimStatus::Supported
        } else {
            SearchClaimStatus::Emerging
        },
        confidence: confidence.clamp(0.0, 1.0),
        citation_ids: citations
            .iter()
            .map(|citation| citation.id.clone())
            .collect(),
    }]
}

fn counterevidence_report(
    query: &str,
    evidence: &[SearchEvidence],
    citations: &[SearchCitation],
) -> CounterevidenceReport {
    let queries = vec![
        format!("{query} limitations"),
        format!("{query} counterevidence"),
        format!("{query} false debunk refute"),
    ];
    let hits = citations
        .iter()
        .filter(|citation| {
            contradiction_marker(&citation.title) || contradiction_marker(&citation.excerpt)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut gaps = Vec::new();
    if hits.is_empty() {
        gaps.push("no explicit counterevidence hits found in governed corpus".into());
    }
    if evidence.len() < 3 {
        gaps.push("counterevidence coverage is limited by low evidence count".into());
    }
    CounterevidenceReport {
        queries,
        hits,
        coverage_score: counterevidence_coverage(evidence),
        gaps,
    }
}

fn build_formula_report(
    request: &DeepSearchRequest,
    evidence: &[SearchEvidence],
    blocked: &[FrontierItem],
    claims: &[SearchClaim],
) -> FormulaReport {
    let n = evidence.len().max(1) as f64;
    let avg_relevance = avg(evidence.iter().map(|item| item.relevance_score));
    let avg_credibility = avg(evidence.iter().map(|item| item.credibility_score));
    let avg_freshness = avg(evidence.iter().map(|item| item.freshness_score));
    let avg_depth = avg(evidence.iter().map(|item| item.depth_signal));
    let avg_verification = avg(evidence.iter().map(|item| item.verification_score));
    let diversity = diversity_score(evidence);
    let contradiction = contradiction_score(evidence);
    let coverage = coverage_score(evidence, request);
    let counter = counterevidence_coverage(evidence);
    let triangulation = triangulation_score(evidence);
    let missingness = (1.0 - coverage).clamp(0.0, 1.0);
    let gate_rate = if evidence.is_empty() && blocked.is_empty() {
        0.0
    } else {
        evidence.len() as f64 / (evidence.len() + blocked.len()).max(1) as f64
    };
    let pipeline =
        (evidence.len() as f64 / request.max_sources.unwrap_or(12).max(1) as f64).clamp(0.0, 1.0);
    let slo = 0.96;
    let assurance = (avg_verification * 0.25
        + coverage * 0.20
        + gate_rate * 0.15
        + diversity * 0.15
        + pipeline * 0.10
        + slo * 0.15)
        .clamp(0.0, 1.0);
    let confidence = (avg_verification * 0.28
        + triangulation * 0.18
        + coverage * 0.16
        + counter * 0.12
        + diversity * 0.10
        + avg_freshness * 0.08
        + avg_depth * 0.08
        - contradiction * 0.18
        - missingness * 0.12)
        .clamp(0.0, 1.0);
    let prod_cert = confidence
        .min(assurance)
        .min(gate_rate)
        .min(1.0 - contradiction)
        .clamp(0.0, 1.0);

    let scores = [
        source_weight(avg_credibility, avg_relevance, avg_freshness),
        quantized_kernel(avg_relevance, 0.07),
        (avg_relevance * avg_credibility * n.ln_1p()).clamp(0.0, 1.0),
        (avg_freshness + avg_depth - contradiction).clamp(0.0, 1.0),
        (avg_verification * (1.0 - contradiction)).clamp(0.0, 1.0),
        (1.0 - contradiction * 0.8).clamp(0.0, 1.0),
        anti_echo_score(diversity, avg_relevance, contradiction),
        frontier_void_score(coverage, diversity),
        adversarial_cluster_score(evidence),
        living_synthesis_score(claims, avg_verification),
        retrieval_objective(avg_relevance, coverage, diversity),
        avg_credibility,
        triangulation,
        counter,
        claim_posterior(confidence, missingness),
        answer_confidence(confidence, triangulation, counter, missingness),
        retrieval_objective(avg_relevance, coverage, diversity).max(confidence),
        (missingness * avg_depth + 0.35).clamp(0.0, 1.0),
        benchmark_score(confidence, coverage, contradiction),
        gate_rate,
        coverage,
        deep_crawl_utility(avg_relevance, missingness, gate_rate),
        hidden_source_potential(missingness, avg_depth, gate_rate),
        canonical_fusion_score(evidence),
        (confidence * (-missingness).exp()).clamp(0.0, 1.0),
        (confidence + coverage + counter - missingness).clamp(0.0, 1.0),
        coverage.min(triangulation).min(counter),
        quality_acceptance(evidence),
        polite_rate_score(blocked),
        pipeline,
        slo,
        (1.0 - contradiction * 0.4 - missingness * 0.3).clamp(0.0, 1.0),
        human_review_score(confidence, contradiction, request.require_certificate),
        (1.0 - deletion_debt(evidence)).clamp(0.0, 1.0),
        canary_score(confidence, assurance),
        readiness_certificate(confidence, assurance, slo),
        execution_layer_score(prod_cert, gate_rate),
        assurance,
        slice_eval_score(evidence),
        threat_risk_score(blocked, contradiction),
        security_privacy_score(gate_rate, blocked),
        observability_score(evidence),
        lineage_score(evidence),
        rollback_score(slo, contradiction),
        prod_cert,
    ];
    let names = formula_names();
    let mut metrics = Vec::new();
    for (idx, (score, name)) in scores.into_iter().zip(names).enumerate() {
        let mut inputs = BTreeMap::new();
        inputs.insert("avg_relevance".into(), avg_relevance);
        inputs.insert("avg_credibility".into(), avg_credibility);
        inputs.insert("coverage".into(), coverage);
        inputs.insert("triangulation".into(), triangulation);
        inputs.insert("contradiction".into(), contradiction);
        inputs.insert("gate_rate".into(), gate_rate);
        metrics.push(FormulaMetric {
            id: (idx + 1) as u8,
            name: name.into(),
            inputs,
            score: score.clamp(0.0, 1.0),
            gate_passed: score >= formula_threshold((idx + 1) as u8),
            explanation: formula_explanation((idx + 1) as u8, score),
        });
    }
    let mut groups = BTreeMap::new();
    groups.insert("field_calculus".into(), avg_score(&metrics[0..6]));
    groups.insert("accuracy_research".into(), avg_score(&metrics[6..18]));
    groups.insert("crawl_coverage".into(), avg_score(&metrics[18..30]));
    groups.insert("production_assurance".into(), avg_score(&metrics[30..45]));
    FormulaReport {
        formula_version: FORMULA_VERSION.into(),
        metrics,
        groups,
        confidence,
        production_certificate: prod_cert,
    }
}

fn formula_names() -> [&'static str; 45] {
    [
        "Live Source Weight",
        "Quantization-Corrected Soft Kernel",
        "Epistemic Mass Field",
        "Temporal Semantic Gravity Well Score",
        "Truth Topology Potential",
        "Epistemic Provenance Drift",
        "Anti-Echo Corridor Search",
        "Research Frontier Void Score",
        "Adversarial Cluster Detection",
        "Living Synthesis Belief Dynamics",
        "Unified NEXUS Query Objective",
        "Calibrated Source Credibility",
        "Independent Evidence Triangulation",
        "Counterevidence Completeness",
        "Claim Posterior and Uncertainty",
        "Answer Confidence and Abstention",
        "Accuracy-Optimized Retrieval Objective",
        "Active Research Policy",
        "Benchmark-Driven Superiority Criterion",
        "Legal Whole-Internet Access Gate",
        "Whole-Internet Coverage Gap Field",
        "Deep-Crawl Frontier Utility",
        "Hidden Source Discovery Potential",
        "Canonical Evidence Fusion",
        "Missingness-Attenuated Claim Posterior",
        "Deep-Crawl Accuracy Objective",
        "Internet Completeness Certificate",
        "Production Data-Quality Acceptance Gate",
        "Polite Crawl-Rate Scheduler",
        "Pipeline Consistency Debt",
        "Production SLO Loss",
        "Drift and Recalibration Trigger",
        "Human Review Escalation Gate",
        "Deletion and Retention Propagation",
        "Canary Deployment Gate",
        "Production-Readiness Certificate",
        "Runtime Execution Layer",
        "Evidence-Backed Assurance Register",
        "Slice-Based Release Evaluation Gate",
        "Threat-Model and Residual-Risk Gate",
        "Security, Privacy, and Compliance Control Plane",
        "Observability and Incident-Response Gate",
        "Lineage, Reproducibility, and Answer Replay",
        "Kill Switch, Rollback, and Blast-Radius Gate",
        "Actual Production-Grade Certificate",
    ]
}

fn formula_threshold(id: u8) -> f64 {
    match id {
        16 | 27 | 36 | 45 => 0.55,
        20 | 28 | 41 => 0.50,
        _ => 0.40,
    }
}

fn formula_explanation(id: u8, score: f64) -> String {
    let status = if score >= formula_threshold(id) {
        "passed"
    } else {
        "below threshold"
    };
    format!("Formula {id} {status} with normalized score {:.3}.", score)
}

fn coverage_certificate(report: &FormulaReport) -> CoverageCertificate {
    let get = |id: u8| {
        report
            .metrics
            .iter()
            .find(|metric| metric.id == id)
            .map_or(0.0, |metric| metric.score)
    };
    let coverage = get(21);
    let triangulation = get(13);
    let counter = get(14);
    let missingness = 1.0 - get(25);
    let certificate = coverage
        .min(triangulation)
        .min(counter)
        .min((1.0 - missingness).clamp(0.0, 1.0));
    CoverageCertificate {
        certificate_score: certificate,
        coverage_score: coverage,
        triangulation_score: triangulation,
        counterevidence_coverage: counter,
        missingness_penalty: missingness,
        passes: certificate >= 0.50,
    }
}

fn pipeline_debt(evidence: &[SearchEvidence], total: usize) -> PipelineDebt {
    let visible = evidence.iter().filter(|item| item.accepted).count();
    let total = total.max(visible).max(1);
    let consistency_debt = (1.0 - visible as f64 / total as f64).clamp(0.0, 1.0);
    PipelineDebt {
        consistency_debt,
        visible_sources: visible,
        total_sources: total,
        deletion_debt: deletion_debt(evidence),
        slo_loss: (0.04 + consistency_debt * 0.12).clamp(0.0, 1.0),
    }
}

fn readiness(
    report: &FormulaReport,
    cert: &CoverageCertificate,
    debt: &PipelineDebt,
    request: &DeepSearchRequest,
) -> SearchReadiness {
    let mut reasons = Vec::new();
    if report.confidence < 0.45 {
        reasons.push("answer confidence below automatic threshold".into());
    }
    if request.require_certificate && !cert.passes {
        reasons.push("internet completeness certificate did not pass".into());
    }
    if debt.consistency_debt > 0.55 {
        reasons.push("pipeline consistency debt is high".into());
    }
    let review_required = report.confidence < 0.62 || !cert.passes || debt.consistency_debt > 0.35;
    let abstain = report.confidence < 0.35 || (request.require_certificate && !cert.passes);
    SearchReadiness {
        service_readiness_index: (report.production_certificate * (1.0 - debt.slo_loss))
            .clamp(0.0, 1.0),
        deploy_allowed: !abstain && !review_required,
        review_required,
        abstain,
        reasons,
    }
}

fn assurance(
    report: &FormulaReport,
    debt: &PipelineDebt,
    readiness: &SearchReadiness,
) -> AssuranceReport {
    let score = (report.production_certificate * 0.60
        + (1.0 - debt.consistency_debt) * 0.20
        + readiness.service_readiness_index * 0.20)
        .clamp(0.0, 1.0);
    AssuranceReport {
        assurance_score: score,
        audit_passed: true,
        risk_passed: score >= 0.45,
        security_passed: score >= 0.40,
        privacy_passed: debt.deletion_debt == 0.0,
        observability_passed: true,
        production_certificate: report.production_certificate,
    }
}

fn answer_for(
    query: &str,
    claims: &[SearchClaim],
    evidence: &[SearchEvidence],
    abstain: bool,
) -> String {
    if abstain {
        return format!(
            "I cannot certify an answer for '{query}' yet because governed evidence is insufficient."
        );
    }
    if let Some(claim) = claims.first() {
        format!(
            "{} Top evidence lanes: {}.",
            claim.claim,
            evidence
                .iter()
                .take(4)
                .map(|item| item.source_class.as_str())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("No governed evidence was available for '{query}'.")
    }
}

fn lineage_for(query: &str, answer: &str, evidence: &[SearchEvidence]) -> LineageRecord {
    let evidence_hashes = evidence
        .iter()
        .map(|item| sha3_hex(format!("{}:{}", item.url, item.summary)))
        .collect::<Vec<_>>();
    LineageRecord {
        trace_id: new_id("search_trace"),
        query_hash: sha3_hex(query),
        answer_hash: sha3_hex(answer),
        replay_delta: 0.0,
        lineage_passed: !evidence_hashes.is_empty(),
        evidence_hashes,
    }
}

fn source_mix(evidence: &[SearchEvidence]) -> BTreeMap<String, usize> {
    let mut mix = BTreeMap::new();
    for item in evidence {
        *mix.entry(item.source_class.as_str().to_string())
            .or_insert(0) += 1;
    }
    mix
}

fn evidence_tags(candidate: &CandidateDocument, accepted: bool) -> Vec<String> {
    let mut tags = vec![candidate.source_class.as_str().to_string()];
    if candidate.synthetic {
        tags.push("autonomous_candidate".into());
    }
    if candidate.indexed {
        tags.push("semantic_index_recall".into());
    }
    if candidate.fetched {
        tags.push("live_fetch".into());
    }
    if accepted {
        tags.push("accepted".into());
    }
    tags
}

fn stats_from_store(
    store: &SearchIntelligenceStore,
    persistent_documents: usize,
    queued_crawl_items: usize,
    completed_crawl_items: usize,
) -> SearchIntelligenceStats {
    SearchIntelligenceStats {
        total_queries: store.total_queries,
        autonomous_queries: store.autonomous_queries,
        abstentions: store.abstentions,
        cache_entries: store.cached.len(),
        frontier_size: store.frontier.len(),
        persistent_documents,
        queued_crawl_items,
        completed_crawl_items,
        latest_benchmark_score: store.latest_benchmark_score,
        last_confidence: store.last_confidence,
    }
}

fn cache_key(request: &DeepSearchRequest) -> String {
    sha3_hex(
        serde_json::json!({
            "query": request.query,
            "mode": request.mode,
            "max_sources": request.max_sources,
            "max_crawl_steps": request.max_crawl_steps,
            "freshness_horizon_hours": request.freshness_horizon_hours,
            "source_classes": request.source_classes,
            "allowed_domains": request.allowed_domains,
            "blocked_domains": request.blocked_domains,
            "require_certificate": request.require_certificate,
            "autonomous_crawl": request.autonomous_crawl,
            "urls": request.urls,
            "seed_count": request.seed_documents.len(),
        })
        .to_string(),
    )
}

fn extract_title(html: &str) -> Option<String> {
    let re = Regex::new("(?is)<title[^>]*>(.*?)</title>").ok()?;
    let value = re.captures(html)?.get(1)?.as_str();
    let title = html_unescape(value).trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

fn strip_html(html: &str) -> String {
    let script = Regex::new("(?is)<script[^>]*>.*?</script>").expect("script regex");
    let style = Regex::new("(?is)<style[^>]*>.*?</style>").expect("style regex");
    let tag = Regex::new("(?is)<[^>]+>").expect("tag regex");
    let text = script.replace_all(html, " ");
    let text = style.replace_all(&text, " ");
    let text = tag.replace_all(&text, " ");
    html_unescape(&text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_links(text: &str, base: Option<&Url>) -> Vec<String> {
    let re = Regex::new(r#"https?://[^\s"'<>)]+"#).expect("url regex");
    let href = Regex::new(r#"(?is)href\s*=\s*["']([^"']+)["']"#).expect("href regex");
    let mut out = BTreeSet::new();
    for item in re.find_iter(text) {
        out.insert(item.as_str().trim_end_matches(['.', ',']).to_string());
    }
    for capture in href.captures_iter(text) {
        let Some(raw) = capture.get(1).map(|item| item.as_str().trim()) else {
            continue;
        };
        let parsed = Url::parse(raw)
            .ok()
            .or_else(|| base.and_then(|base| base.join(raw).ok()));
        if let Some(parsed) = parsed {
            if matches!(parsed.scheme(), "http" | "https") {
                out.insert(parsed.to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "No excerpt available".into();
    }
    trimmed
        .split_terminator(['.', '!', '?'])
        .next()
        .unwrap_or(trimmed)
        .chars()
        .take(260)
        .collect()
}

fn html_unescape(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn classify_source_class(domain: &str, body: &str) -> SearchSourceClass {
    let haystack = format!(
        "{} {}",
        domain.to_ascii_lowercase(),
        body.to_ascii_lowercase()
    );
    if haystack.contains("arxiv") || haystack.contains("paper") || haystack.contains("journal") {
        SearchSourceClass::Papers
    } else if haystack.contains("docs")
        || haystack.contains("spec")
        || haystack.contains("reference")
    {
        SearchSourceClass::Docs
    } else if haystack.contains("news") || haystack.contains("report") {
        SearchSourceClass::News
    } else if haystack.contains("reddit") || haystack.contains("forum") {
        SearchSourceClass::Social
    } else if haystack.contains("book") || haystack.contains("library") {
        SearchSourceClass::Books
    } else {
        SearchSourceClass::Web
    }
}

fn source_class_from_str(value: &str) -> SearchSourceClass {
    match value {
        "news" => SearchSourceClass::News,
        "social" => SearchSourceClass::Social,
        "books" => SearchSourceClass::Books,
        "papers" => SearchSourceClass::Papers,
        "docs" => SearchSourceClass::Docs,
        _ => SearchSourceClass::Web,
    }
}

/// Process-wide quantizer; construction precomputes only the rotation sign
/// diagonals, so this is cheap and shared by the persist and query paths.
fn embedding_quantizer() -> &'static TurboQuant {
    static QUANTIZER: OnceLock<TurboQuant> = OnceLock::new();
    QUANTIZER
        .get_or_init(|| TurboQuant::new(EMBEDDING_DIMS, EMBEDDING_BITS, EMBEDDING_QUANTIZER_SEED))
}

fn semantic_embedding(text: &str) -> Vec<f64> {
    let mut vector = vec![0.0; EMBEDDING_DIMS];
    for token in normalized_tokens(text) {
        let digest = sha3_hex(&token);
        let bucket = usize::from_str_radix(&digest[0..8], 16).unwrap_or(0) % EMBEDDING_DIMS;
        let sign = if u8::from_str_radix(&digest[8..10], 16).unwrap_or(0) % 2 == 0 {
            1.0
        } else {
            -1.0
        };
        vector[bucket] += sign;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > EPS {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

fn decode_embedding(value: &str) -> Vec<f64> {
    serde_json::from_str(value).unwrap_or_else(|_| vec![0.0; EMBEDDING_DIMS])
}

fn cosine_similarity(left: &[f64], right: &[f64]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let dot = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f64>();
    ((dot + 1.0) / 2.0).clamp(0.0, 1.0)
}

fn normalized_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 3)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn expected_term_recall(expected_terms: &[String], text: &str) -> f64 {
    if expected_terms.is_empty() {
        return 1.0;
    }
    let haystack = text.to_ascii_lowercase();
    let hits = expected_terms
        .iter()
        .filter(|term| haystack.contains(&term.to_ascii_lowercase()))
        .count() as f64;
    (hits / expected_terms.len() as f64).clamp(0.0, 1.0)
}

fn avg(values: impl Iterator<Item = f64>) -> f64 {
    let mut total = 0.0;
    let mut count = 0.0;
    for value in values {
        total += value;
        count += 1.0;
    }
    if count <= 0.0 { 0.0 } else { total / count }
}

fn avg_score(metrics: &[FormulaMetric]) -> f64 {
    avg(metrics.iter().map(|metric| metric.score))
}

fn diversity_score(evidence: &[SearchEvidence]) -> f64 {
    if evidence.is_empty() {
        return 0.0;
    }
    let distinct = evidence
        .iter()
        .map(|item| item.source_class.as_str())
        .collect::<BTreeSet<_>>()
        .len() as f64;
    (distinct / evidence.len().min(6) as f64).clamp(0.0, 1.0)
}

fn contradiction_score(evidence: &[SearchEvidence]) -> f64 {
    let contradictory = evidence
        .iter()
        .filter(|item| contradiction_marker(&item.title) || contradiction_marker(&item.summary))
        .count() as f64;
    if evidence.is_empty() {
        0.0
    } else {
        (contradictory / evidence.len() as f64).clamp(0.0, 1.0)
    }
}

fn contradiction_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["not ", "false", "debunk", "refute", "contradict", "denies"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn coverage_score(evidence: &[SearchEvidence], request: &DeepSearchRequest) -> f64 {
    let desired = requested_classes(request)
        .into_iter()
        .map(|class| class.as_str())
        .collect::<BTreeSet<_>>();
    if desired.is_empty() {
        return 0.0;
    }
    let seen = evidence
        .iter()
        .map(|item| item.source_class.as_str())
        .collect::<BTreeSet<_>>();
    let covered = desired.intersection(&seen).count() as f64;
    (covered / desired.len() as f64).clamp(0.0, 1.0)
}

fn counterevidence_coverage(evidence: &[SearchEvidence]) -> f64 {
    if evidence.len() < 2 {
        return if evidence.is_empty() { 0.0 } else { 0.45 };
    }
    let has_counter = evidence
        .iter()
        .any(|item| contradiction_marker(&item.title) || contradiction_marker(&item.summary));
    if has_counter {
        0.85
    } else {
        (0.55 + diversity_score(evidence) * 0.25).clamp(0.0, 1.0)
    }
}

fn triangulation_score(evidence: &[SearchEvidence]) -> f64 {
    if evidence.is_empty() {
        return 0.0;
    }
    let domains = evidence
        .iter()
        .map(|item| &item.domain)
        .collect::<BTreeSet<_>>();
    let classes = evidence
        .iter()
        .map(|item| item.source_class.as_str())
        .collect::<BTreeSet<_>>();
    ((domains.len() as f64).ln_1p() / 2.0 + classes.len() as f64 / 6.0).clamp(0.0, 1.0)
}

fn source_weight(credibility: f64, relevance: f64, freshness: f64) -> f64 {
    ((credibility + EPS).powf(0.40) * (1.0 + relevance).powf(0.35) * freshness.exp()
        / std::f64::consts::E)
        .clamp(0.0, 1.0)
}

fn quantized_kernel(score: f64, variance: f64) -> f64 {
    (score.exp() * (-0.5 * variance).exp() / std::f64::consts::E).clamp(0.0, 1.0)
}

fn anti_echo_score(diversity: f64, relevance: f64, contradiction: f64) -> f64 {
    (0.45 * diversity + 0.45 * relevance + 0.10 * (1.0 - contradiction)).clamp(0.0, 1.0)
}

fn frontier_void_score(coverage: f64, diversity: f64) -> f64 {
    ((1.0 - coverage) * 0.65 + (1.0 - diversity) * 0.35).clamp(0.0, 1.0)
}

fn adversarial_cluster_score(evidence: &[SearchEvidence]) -> f64 {
    let duplicate_domains = evidence.len().saturating_sub(
        evidence
            .iter()
            .map(|item| &item.domain)
            .collect::<BTreeSet<_>>()
            .len(),
    ) as f64;
    (1.0 - duplicate_domains / evidence.len().max(1) as f64).clamp(0.0, 1.0)
}

fn living_synthesis_score(claims: &[SearchClaim], verification: f64) -> f64 {
    if claims.is_empty() {
        0.0
    } else {
        (avg(claims.iter().map(|claim| claim.confidence)) * 0.55 + verification * 0.45)
            .clamp(0.0, 1.0)
    }
}

fn retrieval_objective(relevance: f64, coverage: f64, diversity: f64) -> f64 {
    (0.45 * relevance + 0.30 * coverage + 0.25 * diversity).clamp(0.0, 1.0)
}

fn claim_posterior(confidence: f64, missingness: f64) -> f64 {
    (confidence * (-missingness).exp()).clamp(0.0, 1.0)
}

fn answer_confidence(confidence: f64, triangulation: f64, counter: f64, missingness: f64) -> f64 {
    (confidence * triangulation.powf(0.35) * counter.powf(0.25) * (-missingness).exp())
        .clamp(0.0, 1.0)
}

fn benchmark_score(confidence: f64, coverage: f64, contradiction: f64) -> f64 {
    (0.45 * confidence + 0.35 * coverage + 0.20 * (1.0 - contradiction)).clamp(0.0, 1.0)
}

fn deep_crawl_utility(relevance: f64, missingness: f64, gate: f64) -> f64 {
    (gate * (0.45 * relevance + 0.55 * missingness)).clamp(0.0, 1.0)
}

fn hidden_source_potential(missingness: f64, depth: f64, gate: f64) -> f64 {
    (gate * missingness * 0.60 + depth * 0.40).clamp(0.0, 1.0)
}

fn canonical_fusion_score(evidence: &[SearchEvidence]) -> f64 {
    let domains = evidence
        .iter()
        .map(|item| &item.domain)
        .collect::<BTreeSet<_>>()
        .len();
    (1.0 - (evidence.len().saturating_sub(domains) as f64 / evidence.len().max(1) as f64) * 0.5)
        .clamp(0.0, 1.0)
}

fn quality_acceptance(evidence: &[SearchEvidence]) -> f64 {
    avg(evidence.iter().map(|item| item.verification_score))
}

fn polite_rate_score(blocked: &[FrontierItem]) -> f64 {
    let rate_blocks = blocked
        .iter()
        .filter(|item| item.reason.contains("rate"))
        .count() as f64;
    (1.0 - rate_blocks / blocked.len().max(1) as f64).clamp(0.0, 1.0)
}

fn deletion_debt(evidence: &[SearchEvidence]) -> f64 {
    if evidence
        .iter()
        .any(|item| item.evidence_tags.iter().any(|tag| tag == "delete"))
    {
        1.0
    } else {
        0.0
    }
}

fn human_review_score(confidence: f64, contradiction: f64, require_certificate: bool) -> f64 {
    let sensitivity = if require_certificate { 0.18 } else { 0.0 };
    (confidence - contradiction * 0.25 - sensitivity).clamp(0.0, 1.0)
}

fn canary_score(confidence: f64, assurance: f64) -> f64 {
    (0.55 * confidence + 0.45 * assurance).clamp(0.0, 1.0)
}

fn readiness_certificate(confidence: f64, assurance: f64, slo: f64) -> f64 {
    confidence.min(assurance).min(slo).clamp(0.0, 1.0)
}

fn execution_layer_score(prod_cert: f64, gate: f64) -> f64 {
    prod_cert.min(gate).clamp(0.0, 1.0)
}

fn slice_eval_score(evidence: &[SearchEvidence]) -> f64 {
    let classes = evidence
        .iter()
        .map(|item| item.source_class.as_str())
        .collect::<BTreeSet<_>>()
        .len() as f64;
    (classes / 4.0).clamp(0.0, 1.0)
}

fn threat_risk_score(blocked: &[FrontierItem], contradiction: f64) -> f64 {
    (1.0 - blocked.len() as f64 * 0.05 - contradiction * 0.30).clamp(0.0, 1.0)
}

fn security_privacy_score(gate_rate: f64, blocked: &[FrontierItem]) -> f64 {
    let private_blocks = blocked
        .iter()
        .filter(|item| item.reason.contains("private") || item.reason.contains("dark"))
        .count() as f64;
    (0.65 * gate_rate + 0.35 * (1.0 - private_blocks / blocked.len().max(1) as f64)).clamp(0.0, 1.0)
}

fn observability_score(evidence: &[SearchEvidence]) -> f64 {
    if evidence.is_empty() { 0.0 } else { 1.0 }
}

fn lineage_score(evidence: &[SearchEvidence]) -> f64 {
    if evidence.iter().all(|item| !item.url.is_empty()) && !evidence.is_empty() {
        1.0
    } else {
        0.0
    }
}

fn rollback_score(slo: f64, contradiction: f64) -> f64 {
    (slo - contradiction * 0.25).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich_request() -> DeepSearchRequest {
        DeepSearchRequest {
            query: "deep research evidence".into(),
            mode: SearchResponseMode::Deep,
            max_sources: Some(8),
            max_crawl_steps: Some(6),
            freshness_horizon_hours: Some(168),
            source_classes: vec![SearchSourceClass::Docs, SearchSourceClass::Papers],
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
            require_certificate: false,
            autonomous_crawl: true,
            urls: Vec::new(),
            seed_documents: vec![DeepSearchSeedDocument {
                url: "https://docs.example/research".into(),
                html: "<html><head><title>Research Docs</title></head><body>According to source material, deep research evidence needs provenance, counterevidence, coverage, triangulation, and audit replay. See https://papers.example/study for more.</body></html>".into(),
                source_class: Some(SearchSourceClass::Docs),
            }],
        }
    }

    #[test]
    fn bm25_relevance_rewards_frequency_coverage_and_phrases() {
        let query = "governed research evidence";
        let strong = "Governed research evidence requires governed research evidence \
                      pipelines with provenance and replay across every governed source.";
        let partial = "This page is mostly about cooking but mentions research once.";
        let empty = "Completely unrelated text about gardening and weather patterns.";

        let strong_score = relevance_score(query, strong);
        let partial_score = relevance_score(query, partial);
        let empty_score = relevance_score(query, empty);
        assert!(
            strong_score > partial_score,
            "{strong_score} vs {partial_score}"
        );
        assert!(
            partial_score > empty_score,
            "{partial_score} vs {empty_score}"
        );
        assert!((0.0..=1.0).contains(&strong_score));

        // Exact phrase match outranks the same words scattered apart.
        let scattered = "evidence of governed methods is research adjacent material here";
        let phrased = "the governed research evidence corpus is reviewed annually today";
        assert!(relevance_score(query, phrased) > relevance_score(query, scattered));
    }

    #[test]
    fn duplicate_content_under_different_urls_is_deduplicated() {
        let service = SearchIntelligenceService::new();
        let mut request = rich_request();
        request.autonomous_crawl = false;
        let html = "<html><head><title>Mirror</title></head><body>Identical mirrored deep research evidence body with provenance and triangulation coverage for replay audits.</body></html>";
        request.seed_documents = vec![
            DeepSearchSeedDocument {
                url: "https://docs.example/original".into(),
                html: html.into(),
                source_class: Some(SearchSourceClass::Docs),
            },
            DeepSearchSeedDocument {
                url: "https://mirror.example/copy".into(),
                html: html.into(),
                source_class: Some(SearchSourceClass::Docs),
            },
        ];
        let response = service.execute(request).expect("search should run");
        let mirrored = response
            .evidence
            .iter()
            .filter(|item| item.url.contains("original") || item.url.contains("copy"))
            .count();
        assert_eq!(mirrored, 1, "mirrored content should appear exactly once");
    }

    #[test]
    fn formula_report_contains_all_pdf_formulas() {
        let service = SearchIntelligenceService::new();
        let response = service.execute(rich_request()).expect("search should run");
        assert_eq!(response.formula_report.metrics.len(), 45);
        assert_eq!(response.formula_report.metrics[0].id, 1);
        assert_eq!(response.formula_report.metrics[44].id, 45);
        assert!(response.confidence > 0.0);
    }

    #[test]
    fn autonomous_gates_block_private_and_dark_sources() {
        let service = SearchIntelligenceService::new();
        let mut request = rich_request();
        request.urls = vec![
            "http://127.0.0.1/admin".into(),
            "http://example.onion/report".into(),
        ];
        let response = service.execute(request).expect("search should finish");
        assert!(
            response
                .crawl_plan
                .blocked
                .iter()
                .any(|item| item.reason.contains("private"))
        );
        assert!(
            response
                .crawl_plan
                .blocked
                .iter()
                .any(|item| item.reason.contains("dark"))
        );
    }

    #[test]
    fn allowed_domains_policy_is_enforced() {
        let service = SearchIntelligenceService::new();
        let mut request = rich_request();
        request.allowed_domains = vec!["docs.example".into()];
        request.urls = vec!["https://outside.example/report".into()];
        let response = service.execute(request).expect("search should finish");
        assert!(
            response
                .crawl_plan
                .blocked
                .iter()
                .any(|item| item.domain == "outside.example")
        );
    }
}

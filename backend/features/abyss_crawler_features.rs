use std::collections::{BTreeMap, BTreeSet, VecDeque};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};
use crate::features::semantic_workflow::{SemanticWorkflowFailure, SemanticWorkflowPage};

const DEFAULT_ABYSS_MAX_PAGES: usize = 4;
const DEFAULT_TIMEOUT_MS: u64 = 8_000;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 512 * 1024;
const DEFAULT_MAX_RETRIES: u8 = 1;
const DEFAULT_SEGMENT_LIMIT: usize = 24;
const DEFAULT_LINK_LIMIT: usize = 24;
const DEFAULT_REQUESTS_PER_MINUTE: u32 = 18;
const MAX_RECENT_MISSIONS: usize = 24;
const MAX_RECENT_FINDINGS: usize = 32;
const MAX_GRAPH_ITEMS: usize = 192;

#[derive(Debug, Clone, Deserialize)]
pub struct AbyssMissionRequest {
    pub objective: String,
    #[serde(default)]
    pub seed_urls: Vec<String>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default = "default_true")]
    pub follow_pagination: bool,
    #[serde(default = "default_abyss_max_pages")]
    pub max_pages: usize,
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
    #[serde(default = "default_true")]
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
    #[serde(default = "default_true")]
    pub persist_report: bool,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    #[serde(default)]
    pub local_focus_tags: Vec<String>,
    #[serde(default = "default_true")]
    pub enable_truth_triangulation: bool,
    #[serde(default = "default_true")]
    pub enable_archive_guidance: bool,
    #[serde(default)]
    pub notarize_to_chain: bool,
    #[serde(default)]
    pub attestor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssPageDigest {
    pub url: String,
    pub title: String,
    pub content_type: String,
    pub response_class: String,
    pub adapter_kinds: Vec<String>,
    pub entity_count: usize,
    pub hostile_signal_count: usize,
    pub epistemic_score: f64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssDocumentArtifact {
    pub artifact_id: String,
    pub url: String,
    pub content_type: String,
    pub excavation_mode: String,
    pub extracted_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssApiSurface {
    pub surface_id: String,
    pub url: String,
    pub response_class: String,
    pub discovered_fields: Vec<String>,
    pub collection_items: usize,
    pub pagination_targets: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssArchiveCandidate {
    pub url: String,
    pub reason: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssTruthFinding {
    pub subject: String,
    pub corroborating_urls: Vec<String>,
    pub independent_sources: usize,
    pub confidence: f64,
    pub requires_review: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssCounterSurveillanceReport {
    pub suspicious_pages: Vec<String>,
    pub tainted_source_urls: Vec<String>,
    pub honeypot_indicators: Vec<String>,
    pub safe_handling_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssKnowledgeNode {
    pub node_id: String,
    pub label: String,
    pub kind: String,
    pub evidence_count: usize,
    pub source_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssKnowledgeEdge {
    pub edge_id: String,
    pub from: String,
    pub to: String,
    pub relationship: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssKnowledgeGraph {
    pub nodes: Vec<AbyssKnowledgeNode>,
    pub edges: Vec<AbyssKnowledgeEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssMissionSummary {
    pub pages_crawled: usize,
    pub documents_excavated: usize,
    pub api_surfaces: usize,
    pub truths_confirmed: usize,
    pub truths_requiring_review: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub failures: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssMissionReport {
    pub mission_id: String,
    pub manifest_hash: String,
    pub objective: String,
    pub page_digests: Vec<AbyssPageDigest>,
    pub document_excavations: Vec<AbyssDocumentArtifact>,
    pub api_cartography: Vec<AbyssApiSurface>,
    pub archive_candidates: Vec<AbyssArchiveCandidate>,
    pub truth_triangulation: Vec<AbyssTruthFinding>,
    pub counter_surveillance: AbyssCounterSurveillanceReport,
    pub knowledge_graph: AbyssKnowledgeGraph,
    pub summary: AbyssMissionSummary,
    pub guardrails: Vec<String>,
    pub completed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssMissionDigest {
    pub mission_id: String,
    pub objective: String,
    pub pages_crawled: usize,
    pub truths_confirmed: usize,
    pub completed_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbyssCrawlerStatus {
    pub total_missions: u64,
    pub successful_missions: u64,
    pub known_domains: usize,
    pub knowledge_nodes: usize,
    pub knowledge_edges: usize,
    pub archived_candidates: u64,
    pub last_mission_at: i64,
    pub recent_missions: Vec<AbyssMissionDigest>,
    pub recent_findings: Vec<String>,
    pub blocked_capabilities: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AbyssCrawlerEngine {
    total_missions: u64,
    successful_missions: u64,
    archived_candidates: u64,
    last_mission_at: i64,
    known_domains: BTreeMap<String, u64>,
    knowledge_nodes: BTreeMap<String, AbyssKnowledgeNode>,
    knowledge_edges: BTreeMap<String, AbyssKnowledgeEdge>,
    recent_missions: VecDeque<AbyssMissionDigest>,
    recent_findings: VecDeque<String>,
}

impl AbyssMissionRequest {
    pub fn validate(&self) -> AstraResult<()> {
        if self.objective.trim().is_empty() {
            return Err(AstraError::ControlPlaneRejected(
                "abyss mission requires a non-empty objective".into(),
            ));
        }
        if self.seed_urls.is_empty() {
            return Err(AstraError::ControlPlaneRejected(
                "abyss mission requires at least one permitted seed url".into(),
            ));
        }
        if self.max_pages == 0 {
            return Err(AstraError::ControlPlaneRejected(
                "abyss mission max_pages must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

impl AbyssCrawlerEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn status(&self) -> AbyssCrawlerStatus {
        AbyssCrawlerStatus {
            total_missions: self.total_missions,
            successful_missions: self.successful_missions,
            known_domains: self.known_domains.len(),
            knowledge_nodes: self.knowledge_nodes.len(),
            knowledge_edges: self.knowledge_edges.len(),
            archived_candidates: self.archived_candidates,
            last_mission_at: self.last_mission_at,
            recent_missions: self.recent_missions.iter().cloned().collect(),
            recent_findings: self.recent_findings.iter().cloned().collect(),
            blocked_capabilities: blocked_capabilities(),
            guardrails: guardrails(),
        }
    }

    pub fn finalize_mission(
        &mut self,
        request: &AbyssMissionRequest,
        pages: Vec<SemanticWorkflowPage>,
        failures: Vec<SemanticWorkflowFailure>,
    ) -> AstraResult<AbyssMissionReport> {
        request.validate()?;

        let completed_at = Utc::now().timestamp_millis();
        let mission_id = hash_prefix(
            format!(
                "abyss:{}:{}:{}",
                request.objective,
                request.seed_urls.join("|"),
                completed_at
            )
            .as_bytes(),
        );
        let page_digests = pages.iter().map(build_page_digest).collect::<Vec<_>>();
        let document_excavations = build_document_excavations(&pages);
        let api_cartography = build_api_cartography(&pages);
        let archive_candidates = build_archive_candidates(request, &pages, &failures);
        let truth_triangulation = build_truth_triangulation(request, &pages);
        let counter_surveillance = build_counter_surveillance(&pages, &failures);
        let knowledge_graph = build_knowledge_graph(
            &request.objective,
            &request.local_focus_tags,
            &pages,
            &document_excavations,
            &api_cartography,
            &truth_triangulation,
        );
        let summary = AbyssMissionSummary {
            pages_crawled: pages.len(),
            documents_excavated: document_excavations.len(),
            api_surfaces: api_cartography.len(),
            truths_confirmed: truth_triangulation
                .iter()
                .filter(|finding| !finding.requires_review)
                .count(),
            truths_requiring_review: truth_triangulation
                .iter()
                .filter(|finding| finding.requires_review)
                .count(),
            graph_nodes: knowledge_graph.nodes.len(),
            graph_edges: knowledge_graph.edges.len(),
            failures: failures.len(),
        };
        let manifest_hash = sha3_256_hex(
            serde_json::json!({
                "mission_id": mission_id,
                "objective": request.objective,
                "summary": summary,
                "page_digests": page_digests,
                "document_excavations": document_excavations,
                "api_cartography": api_cartography,
                "truth_triangulation": truth_triangulation,
                "archive_candidates": archive_candidates,
                "counter_surveillance": counter_surveillance,
            })
            .to_string()
            .as_bytes(),
        );

        self.total_missions = self.total_missions.saturating_add(1);
        self.successful_missions = self.successful_missions.saturating_add(1);
        self.archived_candidates = self
            .archived_candidates
            .saturating_add(archive_candidates.len() as u64);
        self.last_mission_at = completed_at;
        self.remember_domains(&pages);
        self.merge_graph(&knowledge_graph);
        self.push_recent_mission(AbyssMissionDigest {
            mission_id: mission_id.clone(),
            objective: request.objective.clone(),
            pages_crawled: summary.pages_crawled,
            truths_confirmed: summary.truths_confirmed,
            completed_at,
        });
        for finding in truth_triangulation.iter().take(4) {
            self.push_recent_finding(format!(
                "{} ({})",
                finding.subject,
                if finding.requires_review {
                    "needs review"
                } else {
                    "corroborated"
                }
            ));
        }

        Ok(AbyssMissionReport {
            mission_id,
            manifest_hash,
            objective: request.objective.clone(),
            page_digests,
            document_excavations,
            api_cartography,
            archive_candidates,
            truth_triangulation,
            counter_surveillance,
            knowledge_graph,
            summary,
            guardrails: guardrails(),
            completed_at,
        })
    }

    fn remember_domains(&mut self, pages: &[SemanticWorkflowPage]) {
        for page in pages {
            if let Some(domain) = host_from_url(&page.url) {
                *self.known_domains.entry(domain).or_default() += 1;
            }
        }
    }

    fn merge_graph(&mut self, graph: &AbyssKnowledgeGraph) {
        for node in graph.nodes.iter().take(MAX_GRAPH_ITEMS) {
            self.knowledge_nodes
                .entry(node.node_id.clone())
                .and_modify(|existing| {
                    existing.evidence_count = existing.evidence_count.max(node.evidence_count);
                    for url in &node.source_urls {
                        if !existing.source_urls.contains(url) && existing.source_urls.len() < 8 {
                            existing.source_urls.push(url.clone());
                        }
                    }
                })
                .or_insert_with(|| node.clone());
        }
        for edge in graph.edges.iter().take(MAX_GRAPH_ITEMS) {
            self.knowledge_edges
                .entry(edge.edge_id.clone())
                .or_insert_with(|| edge.clone());
        }
    }

    fn push_recent_mission(&mut self, digest: AbyssMissionDigest) {
        self.recent_missions.push_front(digest);
        while self.recent_missions.len() > MAX_RECENT_MISSIONS {
            self.recent_missions.pop_back();
        }
    }

    fn push_recent_finding(&mut self, finding: String) {
        self.recent_findings.push_front(finding);
        while self.recent_findings.len() > MAX_RECENT_FINDINGS {
            self.recent_findings.pop_back();
        }
    }
}

fn build_page_digest(page: &SemanticWorkflowPage) -> AbyssPageDigest {
    let mut notes = page.warnings.iter().take(3).cloned().collect::<Vec<_>>();
    if page.hostile_signal_count > 0 {
        notes.push(format!(
            "{} hostile patterns detected",
            page.hostile_signal_count
        ));
    }
    if page.epistemic_score < 0.5 {
        notes.push("low epistemic score requires manual verification".into());
    }
    AbyssPageDigest {
        url: page.url.clone(),
        title: page.title.clone(),
        content_type: page.content_type.clone(),
        response_class: page.response_class.clone(),
        adapter_kinds: page.adapter_report.adapter_kinds.clone(),
        entity_count: infer_page_entities(page).len(),
        hostile_signal_count: page.hostile_signal_count,
        epistemic_score: page.epistemic_score,
        notes,
    }
}

fn build_document_excavations(pages: &[SemanticWorkflowPage]) -> Vec<AbyssDocumentArtifact> {
    let mut artifacts = Vec::new();
    for page in pages {
        let excavation_mode = match page.response_class.as_str() {
            "json" => Some("schema_projection"),
            "xml" => Some("feed_projection"),
            _ if page.content_type.contains("pdf") => Some("document_index"),
            _ if page.adapter_report.summary.table_count > 0 => Some("tabular_extraction"),
            _ if page.adapter_report.summary.article_segments > 0 => Some("article_digest"),
            _ => None,
        };
        if let Some(mode) = excavation_mode {
            let mut extracted_signals = page
                .adapter_report
                .schema_targets
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>();
            if extracted_signals.is_empty() {
                extracted_signals.extend(
                    page.adapter_report
                        .tables
                        .iter()
                        .flat_map(|table| table.headers.iter().take(3).cloned())
                        .take(4),
                );
            }
            if extracted_signals.is_empty() {
                extracted_signals.push(page.title.clone());
            }
            artifacts.push(AbyssDocumentArtifact {
                artifact_id: hash_prefix(format!("artifact:{}:{}", page.url, mode).as_bytes()),
                url: page.url.clone(),
                content_type: page.content_type.clone(),
                excavation_mode: mode.into(),
                extracted_signals,
            });
        }
    }
    artifacts
}

fn build_api_cartography(pages: &[SemanticWorkflowPage]) -> Vec<AbyssApiSurface> {
    let mut surfaces = Vec::new();
    for page in pages {
        if !matches!(page.response_class.as_str(), "json" | "xml") {
            continue;
        }
        let mut discovered_fields = page
            .adapter_report
            .schema_targets
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>();
        if discovered_fields.is_empty() {
            discovered_fields.extend(
                page.adapter_report
                    .collection_items
                    .iter()
                    .map(|item| item.title.clone().unwrap_or_else(|| item.item_id.clone()))
                    .take(6),
            );
        }
        surfaces.push(AbyssApiSurface {
            surface_id: hash_prefix(format!("surface:{}", page.url).as_bytes()),
            url: page.url.clone(),
            response_class: page.response_class.clone(),
            discovered_fields,
            collection_items: page.adapter_report.collection_items.len(),
            pagination_targets: page.adapter_report.pagination.len(),
        });
    }
    surfaces
}

fn build_archive_candidates(
    request: &AbyssMissionRequest,
    pages: &[SemanticWorkflowPage],
    failures: &[SemanticWorkflowFailure],
) -> Vec<AbyssArchiveCandidate> {
    if !request.enable_archive_guidance {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for page in pages {
        let priority = if page.epistemic_score < 0.5 || page.hostile_signal_count > 0 {
            "high"
        } else if !page.warnings.is_empty() {
            "medium"
        } else {
            "low"
        };
        let reason = if page.hostile_signal_count > 0 {
            "capture an archive pointer before the source changes again"
        } else if page.epistemic_score < 0.5 {
            "compare against an older snapshot before trusting the current page"
        } else {
            "retain a replayable reference for future missions"
        };
        candidates.push(AbyssArchiveCandidate {
            url: page.url.clone(),
            reason: reason.into(),
            priority: priority.into(),
        });
    }
    for failure in failures.iter().take(6) {
        candidates.push(AbyssArchiveCandidate {
            url: failure.url.clone(),
            reason:
                "unreachable source should be checked against approved archives or local mirrors"
                    .into(),
            priority: "high".into(),
        });
    }
    candidates
}

fn build_truth_triangulation(
    request: &AbyssMissionRequest,
    pages: &[SemanticWorkflowPage],
) -> Vec<AbyssTruthFinding> {
    let mut subject_sources: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut subject_domains: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut subject_scores: BTreeMap<String, Vec<f64>> = BTreeMap::new();

    for page in pages {
        for subject in infer_page_entities(page) {
            subject_sources
                .entry(subject.clone())
                .or_default()
                .insert(page.url.clone());
            if let Some(domain) = host_from_url(&page.url) {
                subject_domains
                    .entry(subject.clone())
                    .or_default()
                    .insert(domain);
            }
            subject_scores
                .entry(subject)
                .or_default()
                .push(page.epistemic_score.clamp(0.0, 1.0));
        }
    }

    let mut findings = subject_sources
        .into_iter()
        .map(|(subject, urls)| {
            let independent_sources = subject_domains
                .get(&subject)
                .map_or(0, BTreeSet::len)
                .max(urls.len().min(3));
            let average_score = subject_scores
                .get(&subject)
                .map(|scores| scores.iter().sum::<f64>() / scores.len() as f64)
                .unwrap_or(0.5);
            let corroborated = independent_sources >= 2 || urls.len() >= 2;
            AbyssTruthFinding {
                subject,
                corroborating_urls: urls.into_iter().collect(),
                independent_sources,
                confidence: (average_score + (independent_sources as f64 * 0.15)).clamp(0.3, 0.98),
                requires_review: request.enable_truth_triangulation && !corroborated,
            }
        })
        .collect::<Vec<_>>();

    findings.sort_by(|left, right| {
        right
            .confidence
            .total_cmp(&left.confidence)
            .then_with(|| left.subject.cmp(&right.subject))
    });
    findings.truncate(24);
    findings
}

fn build_counter_surveillance(
    pages: &[SemanticWorkflowPage],
    failures: &[SemanticWorkflowFailure],
) -> AbyssCounterSurveillanceReport {
    let suspicious_pages = pages
        .iter()
        .filter(|page| {
            page.hostile_signal_count > 0
                || page.enshittification_score >= 0.75
                || page.epistemic_score < 0.45
        })
        .map(|page| page.url.clone())
        .collect::<Vec<_>>();
    let tainted_source_urls = pages
        .iter()
        .filter(|page| page.epistemic_score < 0.45)
        .map(|page| page.url.clone())
        .collect::<Vec<_>>();
    let mut honeypot_indicators = pages
        .iter()
        .flat_map(|page| {
            page.warnings
                .iter()
                .filter(|warning| {
                    let lowered = warning.to_ascii_lowercase();
                    lowered.contains("captcha")
                        || lowered.contains("login")
                        || lowered.contains("blocked")
                        || lowered.contains("tracking")
                })
                .cloned()
        })
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        honeypot_indicators.push(format!(
            "{} sources failed and should be retried only with explicit operator review",
            failures.len()
        ));
    }
    if honeypot_indicators.is_empty() && suspicious_pages.is_empty() {
        honeypot_indicators.push("no active tracking or honeypot indicators detected".into());
    }

    AbyssCounterSurveillanceReport {
        suspicious_pages,
        tainted_source_urls,
        honeypot_indicators,
        safe_handling_actions: vec![
            "pause on suspicious sources instead of escalating evasion".into(),
            "compare low-confidence pages against corroborating sources".into(),
            "keep evidence in the local vault before acting on it".into(),
        ],
    }
}

fn build_knowledge_graph(
    objective: &str,
    local_focus_tags: &[String],
    pages: &[SemanticWorkflowPage],
    documents: &[AbyssDocumentArtifact],
    api_surfaces: &[AbyssApiSurface],
    truths: &[AbyssTruthFinding],
) -> AbyssKnowledgeGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen_nodes = BTreeSet::new();
    let mut seen_edges = BTreeSet::new();

    let objective_id = hash_prefix(format!("objective:{objective}").as_bytes());
    push_node(
        &mut nodes,
        &mut seen_nodes,
        AbyssKnowledgeNode {
            node_id: objective_id.clone(),
            label: objective.trim().to_string(),
            kind: "objective".into(),
            evidence_count: pages.len(),
            source_urls: pages.iter().map(|page| page.url.clone()).take(6).collect(),
        },
    );

    for tag in local_focus_tags.iter().filter(|tag| !tag.trim().is_empty()) {
        let tag_id = hash_prefix(format!("tag:{tag}").as_bytes());
        push_node(
            &mut nodes,
            &mut seen_nodes,
            AbyssKnowledgeNode {
                node_id: tag_id.clone(),
                label: tag.trim().to_string(),
                kind: "focus_tag".into(),
                evidence_count: 1,
                source_urls: Vec::new(),
            },
        );
        push_edge(
            &mut edges,
            &mut seen_edges,
            AbyssKnowledgeEdge {
                edge_id: hash_prefix(format!("edge:{objective_id}:{tag_id}").as_bytes()),
                from: objective_id.clone(),
                to: tag_id,
                relationship: "focuses_on".into(),
                confidence: 0.82,
            },
        );
    }

    for page in pages.iter().take(24) {
        let page_id = hash_prefix(format!("page:{}", page.url).as_bytes());
        push_node(
            &mut nodes,
            &mut seen_nodes,
            AbyssKnowledgeNode {
                node_id: page_id.clone(),
                label: page.title.clone(),
                kind: "page".into(),
                evidence_count: 1 + page.adapter_report.summary.schema_targets,
                source_urls: vec![page.url.clone()],
            },
        );
        push_edge(
            &mut edges,
            &mut seen_edges,
            AbyssKnowledgeEdge {
                edge_id: hash_prefix(format!("edge:{objective_id}:{page_id}").as_bytes()),
                from: objective_id.clone(),
                to: page_id.clone(),
                relationship: "investigates".into(),
                confidence: page.epistemic_score.clamp(0.35, 0.95),
            },
        );
        if let Some(domain) = host_from_url(&page.url) {
            let domain_id = hash_prefix(format!("domain:{domain}").as_bytes());
            push_node(
                &mut nodes,
                &mut seen_nodes,
                AbyssKnowledgeNode {
                    node_id: domain_id.clone(),
                    label: domain,
                    kind: "domain".into(),
                    evidence_count: 1,
                    source_urls: vec![page.url.clone()],
                },
            );
            push_edge(
                &mut edges,
                &mut seen_edges,
                AbyssKnowledgeEdge {
                    edge_id: hash_prefix(format!("edge:{page_id}:{domain_id}").as_bytes()),
                    from: page_id,
                    to: domain_id,
                    relationship: "hosted_on".into(),
                    confidence: 0.99,
                },
            );
        }
    }

    for document in documents.iter().take(24) {
        let page_id = hash_prefix(format!("page:{}", document.url).as_bytes());
        push_node(
            &mut nodes,
            &mut seen_nodes,
            AbyssKnowledgeNode {
                node_id: document.artifact_id.clone(),
                label: document.excavation_mode.clone(),
                kind: "document_artifact".into(),
                evidence_count: document.extracted_signals.len(),
                source_urls: vec![document.url.clone()],
            },
        );
        push_edge(
            &mut edges,
            &mut seen_edges,
            AbyssKnowledgeEdge {
                edge_id: hash_prefix(
                    format!("edge:{}:{}", page_id, document.artifact_id).as_bytes(),
                ),
                from: page_id,
                to: document.artifact_id.clone(),
                relationship: "yields".into(),
                confidence: 0.8,
            },
        );
    }

    for surface in api_surfaces.iter().take(16) {
        let page_id = hash_prefix(format!("page:{}", surface.url).as_bytes());
        push_node(
            &mut nodes,
            &mut seen_nodes,
            AbyssKnowledgeNode {
                node_id: surface.surface_id.clone(),
                label: surface.response_class.clone(),
                kind: "api_surface".into(),
                evidence_count: surface.discovered_fields.len(),
                source_urls: vec![surface.url.clone()],
            },
        );
        push_edge(
            &mut edges,
            &mut seen_edges,
            AbyssKnowledgeEdge {
                edge_id: hash_prefix(format!("edge:{}:{}", page_id, surface.surface_id).as_bytes()),
                from: page_id,
                to: surface.surface_id.clone(),
                relationship: "exposes".into(),
                confidence: 0.78,
            },
        );
    }

    for truth in truths.iter().take(16) {
        let truth_id = hash_prefix(format!("truth:{}", truth.subject).as_bytes());
        push_node(
            &mut nodes,
            &mut seen_nodes,
            AbyssKnowledgeNode {
                node_id: truth_id.clone(),
                label: truth.subject.clone(),
                kind: if truth.requires_review {
                    "hypothesis".into()
                } else {
                    "validated_claim".into()
                },
                evidence_count: truth.corroborating_urls.len(),
                source_urls: truth.corroborating_urls.clone(),
            },
        );
        push_edge(
            &mut edges,
            &mut seen_edges,
            AbyssKnowledgeEdge {
                edge_id: hash_prefix(format!("edge:{objective_id}:{truth_id}").as_bytes()),
                from: objective_id.clone(),
                to: truth_id,
                relationship: if truth.requires_review {
                    "monitor".into()
                } else {
                    "learns".into()
                },
                confidence: truth.confidence,
            },
        );
    }

    nodes.truncate(MAX_GRAPH_ITEMS);
    edges.truncate(MAX_GRAPH_ITEMS);
    AbyssKnowledgeGraph { nodes, edges }
}

fn infer_page_entities(page: &SemanticWorkflowPage) -> Vec<String> {
    let mut entities = BTreeSet::new();
    for candidate in normalize_entity(&page.title) {
        entities.insert(candidate);
    }
    for target in page.adapter_report.schema_targets.iter().take(6) {
        for candidate in normalize_entity(target) {
            entities.insert(candidate);
        }
    }
    for item in page.adapter_report.collection_items.iter().take(6) {
        if let Some(title) = &item.title {
            for candidate in normalize_entity(title) {
                entities.insert(candidate);
            }
        }
    }
    for table in page.adapter_report.tables.iter().take(4) {
        for header in table.headers.iter().take(4) {
            for candidate in normalize_entity(header) {
                entities.insert(candidate);
            }
        }
    }
    entities.into_iter().take(12).collect()
}

fn normalize_entity(input: &str) -> Vec<String> {
    let sanitized = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '_' || ch == '-' {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>();
    sanitized
        .split_whitespace()
        .collect::<Vec<_>>()
        .chunks(3)
        .map(|chunk| chunk.join(" ").trim().to_lowercase())
        .filter(|candidate| candidate.len() >= 4)
        .collect()
}

fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))
}

fn push_node(
    nodes: &mut Vec<AbyssKnowledgeNode>,
    seen_nodes: &mut BTreeSet<String>,
    node: AbyssKnowledgeNode,
) {
    if seen_nodes.insert(node.node_id.clone()) {
        nodes.push(node);
    }
}

fn push_edge(
    edges: &mut Vec<AbyssKnowledgeEdge>,
    seen_edges: &mut BTreeSet<String>,
    edge: AbyssKnowledgeEdge,
) {
    if seen_edges.insert(edge.edge_id.clone()) {
        edges.push(edge);
    }
}

fn hash_prefix(bytes: &[u8]) -> String {
    sha3_256_hex(bytes)[..24].to_string()
}

fn default_true() -> bool {
    true
}

fn default_abyss_max_pages() -> usize {
    DEFAULT_ABYSS_MAX_PAGES
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

fn default_user_agent() -> String {
    "AstraAbyss/1.0".into()
}

fn blocked_capabilities() -> Vec<String> {
    vec![
        "Tor, I2P, Lokinet, and dark-web routing are disabled.".into(),
        "CAPTCHA solving and anti-bot bypass are disabled.".into(),
        "Autonomous login-wall takeover is disabled.".into(),
        "Rate-limit evasion and identity rotation are disabled.".into(),
        "Invite-only infiltration and persona forgery are disabled.".into(),
    ]
}

fn guardrails() -> Vec<String> {
    vec![
        "Abyss missions only crawl permitted public URLs and approved domains.".into(),
        "Robots guidance, rate limits, and provenance are preserved.".into(),
        "Suspicious sources trigger review instead of stealth escalation.".into(),
        "The engine builds evidence graphs and archive guidance, not covert access paths.".into(),
        "All research output is designed for local CPU-only personal use.".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::semantic_workflow::{
        SemanticAdapterReport, SemanticAdapterSummary, SemanticWorkflowPage,
    };

    fn sample_page(url: &str, title: &str, response_class: &str) -> SemanticWorkflowPage {
        SemanticWorkflowPage {
            acquisition_id: hash_prefix(url.as_bytes()),
            url: url.into(),
            title: title.into(),
            content_type: if response_class == "json" {
                "application/json".into()
            } else {
                "text/html".into()
            },
            response_class: response_class.into(),
            adapter_report: SemanticAdapterReport {
                adapter_kinds: vec![if response_class == "json" {
                    "json_collection".into()
                } else {
                    "tabular_html".into()
                }],
                recommended_adapter: if response_class == "json" {
                    "json_collection".into()
                } else {
                    "tabular_html".into()
                },
                tables: Vec::new(),
                collection_items: Vec::new(),
                pagination: Vec::new(),
                schema_targets: vec!["supply chain resilience".into()],
                summary: SemanticAdapterSummary {
                    table_count: 0,
                    table_rows: 0,
                    collection_items: 0,
                    pagination_candidates: 0,
                    schema_targets: 1,
                    form_surfaces: 0,
                    article_segments: 3,
                },
                warnings: Vec::new(),
            },
            hostile_signal_count: 0,
            enshittification_score: 0.18,
            epistemic_score: 0.88,
            claim_count: 4,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn abyss_engine_builds_truth_and_graph_state() {
        let mut engine = AbyssCrawlerEngine::new();
        let report = engine
            .finalize_mission(
                &AbyssMissionRequest {
                    objective: "track resilient suppliers".into(),
                    seed_urls: vec!["https://example.com/a".into()],
                    allowed_domains: vec!["example.com".into()],
                    follow_pagination: true,
                    max_pages: 4,
                    continue_on_error: true,
                    respect_robots: true,
                    timeout_ms: 4_000,
                    max_response_bytes: 128_000,
                    max_retries: 1,
                    max_segments: 8,
                    max_links: 8,
                    requests_per_minute: 10,
                    persist_report: true,
                    user_agent: "AstraTest/1.0".into(),
                    local_focus_tags: vec!["procurement".into()],
                    enable_truth_triangulation: true,
                    enable_archive_guidance: true,
                    notarize_to_chain: false,
                    attestor: None,
                },
                vec![
                    sample_page("https://example.com/a", "Supply Chain Resilience", "html"),
                    sample_page(
                        "https://mirror.example.com/b",
                        "Supply Chain Resilience",
                        "json",
                    ),
                ],
                Vec::new(),
            )
            .expect("mission should succeed");

        assert_eq!(report.summary.pages_crawled, 2);
        assert!(!report.truth_triangulation.is_empty());
        assert!(!report.knowledge_graph.nodes.is_empty());
        assert_eq!(engine.status().total_missions, 1);
    }
}

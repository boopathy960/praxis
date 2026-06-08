use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::chain::chain::{Chain, ResourceCommitmentVerification, SearchProofBundle};
use crate::core_infra::cache_hierarchy::CacheTier;
use crate::crypto::hash::sha3_256_hex;
use crate::error::AstraResult;
use crate::features::enhanced_swarm::{EnhancedSwarmEngine, EnhancedSwarmResult};
use crate::features::swarm_browsing::{SwarmBrowser, SwarmQuery};
use crate::features::truth_verification::TruthEngine;
use crate::intelligence::omega_executive::{InventionBlueprint, OmegaDecision, OmegaMission};
use crate::tools::deep_research::{DeepResearch, ResearchDepth, ResearchQuery, ResearchReport};
use crate::tools::web_search::{
    SearchProvider, SearchQuery, SearchResponse, SearchSourceClass, TimeRange, WebSearch,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchDifficulty {
    Easy,
    Standard,
    Hard,
    Expert,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchClaimStatus {
    Supported,
    Contested,
    Emerging,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIntentProfile {
    pub difficulty: SearchDifficulty,
    pub research_depth: ResearchDepth,
    pub source_budget: usize,
    pub source_classes: Vec<SearchSourceClass>,
    pub reasoning_mode: String,
    pub freshness_sensitive: bool,
    pub rationale: Vec<String>,
    pub use_enhanced_swarm: bool,
    pub fast_path: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchExecutionContract {
    pub response_mode: SearchResponseMode,
    pub citation_required: bool,
    pub freshness_horizon_hours: Option<u64>,
    pub max_contradictions: usize,
    pub domain_focus: Vec<String>,
    pub source_quotas: BTreeMap<String, usize>,
    pub reasoning_layers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveSearchPlan {
    pub profile: SearchIntentProfile,
    pub execution_contract: SearchExecutionContract,
    pub follow_up_queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEvidence {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub source_class: SearchSourceClass,
    pub summary: String,
    pub relevance_score: f64,
    pub credibility_score: f64,
    pub freshness_score: f64,
    pub verification_score: f64,
    pub evidence_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchCitation {
    pub id: String,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub source_class: SearchSourceClass,
    pub excerpt: String,
    pub support_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchClaim {
    pub claim: String,
    pub status: SearchClaimStatus,
    pub confidence: f64,
    pub citation_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchVerificationSummary {
    pub truth_score: f64,
    pub cross_source_agreement: f64,
    pub contradictions: usize,
    pub suspicious_segments: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchLedgerProof {
    pub trace_id: String,
    pub answer_hash: String,
    pub manifest_commitment: ResourceCommitmentVerification,
    pub answer_commitment: ResourceCommitmentVerification,
    pub evidence_commitments: Vec<ResourceCommitmentVerification>,
    pub anchored_commitments: usize,
    pub certified_commitments: usize,
    pub manifest_consistent: bool,
    pub verified: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAgentRank {
    pub label: String,
    pub specialization: String,
    pub rank: usize,
    pub final_score: f64,
    pub peer_review_score: f64,
    pub consensus_weight: f64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAgentRankingSummary {
    pub strategy: String,
    pub speed_profile: String,
    pub champion_label: String,
    pub agreement_score: f64,
    pub cohort_grade: String,
    pub consensus_summary: String,
    pub peer_review_count: usize,
    pub top_agents: Vec<SearchAgentRank>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchExactAnswer {
    pub answer: String,
    pub answer_type: String,
    pub confidence: f64,
    pub citation_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchLaneExecution {
    pub lane: String,
    pub planned_quota: usize,
    pub search_results: usize,
    pub vetted_sources: usize,
    pub avg_credibility: f64,
    pub avg_relevance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchExecutionDiagnostics {
    pub speed_class: String,
    pub freshness_sensitive: bool,
    pub contradiction_pressure: String,
    pub acquisition_strategy: String,
    pub indexed_recall_hits: usize,
    pub fresh_fetch_hits: usize,
    pub total_lanes: usize,
    pub lane_execution: Vec<SearchLaneExecution>,
    pub exact_answer: SearchExactAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveSearchResponse {
    pub query: String,
    pub answer: String,
    pub executive_summary: String,
    pub confidence: f64,
    pub profile: SearchIntentProfile,
    pub execution_contract: SearchExecutionContract,
    pub evidence: Vec<SearchEvidence>,
    pub citations: Vec<SearchCitation>,
    pub claims: Vec<SearchClaim>,
    pub source_mix: BTreeMap<String, usize>,
    pub verification: SearchVerificationSummary,
    pub agent_ranking: SearchAgentRankingSummary,
    pub diagnostics: SearchExecutionDiagnostics,
    pub follow_up_queries: Vec<String>,
    pub warnings: Vec<String>,
    pub subsystems_used: Vec<String>,
    pub ledger_proof: Option<SearchLedgerProof>,
    pub omega_decision: Option<OmegaDecision>,
    pub omega_mission: Option<OmegaMission>,
    pub inventions: Vec<InventionBlueprint>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveSearchStats {
    pub total_queries: u64,
    pub fast_path_queries: u64,
    pub deep_queries: u64,
    pub notarized_queries: u64,
    pub cache_hits: u64,
    pub cache_entries: usize,
}

pub struct AdaptiveSearchDeps<'a> {
    pub web_search: &'a mut WebSearch,
    pub deep_research: &'a mut DeepResearch,
    pub swarm: &'a mut SwarmBrowser,
    pub truth: &'a mut TruthEngine,
    pub enhanced_swarm: &'a mut EnhancedSwarmEngine,
}

pub struct AdaptiveSearchEngine {
    cache: CacheTier<AdaptiveSearchResponse>,
    total_queries: u64,
    fast_path_queries: u64,
    deep_queries: u64,
    notarized_queries: u64,
    cache_hits: u64,
}

impl AdaptiveSearchEngine {
    pub fn new() -> Self {
        Self {
            cache: CacheTier::new(256, Duration::from_secs(600)),
            total_queries: 0,
            fast_path_queries: 0,
            deep_queries: 0,
            notarized_queries: 0,
            cache_hits: 0,
        }
    }

    pub fn new_compact() -> Self {
        Self {
            cache: CacheTier::new(64, Duration::from_secs(180)),
            total_queries: 0,
            fast_path_queries: 0,
            deep_queries: 0,
            notarized_queries: 0,
            cache_hits: 0,
        }
    }

    pub fn preview(&self, request: &AdaptiveSearchRequest) -> AdaptiveSearchPlan {
        let profile = self.profile_query(request);
        let execution_contract = build_execution_contract(request, &profile);
        let follow_up_queries =
            recommend_follow_up_queries(&request.query, &profile, &execution_contract, &[]);

        AdaptiveSearchPlan {
            profile,
            execution_contract,
            follow_up_queries,
        }
    }

    pub fn execute(
        &mut self,
        request: AdaptiveSearchRequest,
        deps: AdaptiveSearchDeps<'_>,
    ) -> AstraResult<AdaptiveSearchResponse> {
        self.total_queries += 1;
        let cache_key = cache_key(&request);
        if let Some(cached) = self.cache.get(&cache_key) {
            self.cache_hits += 1;
            if request.notarize.unwrap_or(!cached.profile.fast_path) {
                self.notarized_queries += 1;
            }
            return Ok(cached);
        }

        let start = Instant::now();
        let plan = self.preview(&request);
        let profile = plan.profile.clone();
        let execution_contract = plan.execution_contract.clone();
        if profile.fast_path {
            self.fast_path_queries += 1;
        } else {
            self.deep_queries += 1;
        }
        if request.notarize.unwrap_or(!profile.fast_path) {
            self.notarized_queries += 1;
        }

        let search_response = deps.web_search.search(SearchQuery {
            query: request.query.clone(),
            provider: SearchProvider::Aggregated,
            max_results: request
                .max_sources
                .unwrap_or(profile.source_budget)
                .max(profile.source_budget),
            language: "en".into(),
            region: None,
            safe_search: true,
            time_range: horizon_to_time_range(
                execution_contract.freshness_horizon_hours,
                profile.freshness_sensitive,
            ),
            source_classes: profile.source_classes.clone(),
            deep_reasoning: !profile.fast_path,
        });

        let research_report = deps.deep_research.research(ResearchQuery {
            topic: request.query.clone(),
            depth: profile.research_depth.clone(),
            max_sources: request.max_sources.unwrap_or(profile.source_budget),
            focus_areas: derive_focus_areas(&request.query, &execution_contract.domain_focus),
            exclude_domains: vec![],
            source_classes: profile.source_classes.clone(),
        });
        deps.web_search
            .ingest_research_sources(&research_report.sources);

        let swarm_result = deps.swarm.query(&SwarmQuery {
            intent: request.query.clone(),
            num_agents: swarm_size_for(&profile),
            timeout_ms: if profile.fast_path { 5000 } else { 15000 },
            min_confidence: if profile.fast_path { 0.35 } else { 0.5 },
        })?;

        let enhanced_swarm = if profile.use_enhanced_swarm {
            Some(deps.enhanced_swarm.deploy_swarm(
                &request.query,
                swarm_size_for(&profile) + 2,
                true,
            )?)
        } else {
            None
        };

        let evidence = build_evidence(&search_response, &research_report);
        let agent_ranking = build_agent_ranking(
            &profile,
            &execution_contract,
            &swarm_result,
            enhanced_swarm.as_ref(),
        );
        let verification = verify_evidence(
            deps.truth,
            &request.query,
            &evidence,
            &swarm_result,
            agent_ranking.agreement_score,
        );
        let confidence = compute_confidence(
            &search_response,
            &research_report,
            verification.truth_score,
            swarm_result.confidence,
            enhanced_swarm.as_ref(),
            agent_ranking.agreement_score,
        );
        let citations = build_citations(&evidence, &execution_contract);
        let claims = build_claims(
            &research_report,
            &citations,
            &verification,
            confidence,
            &execution_contract,
        );
        let diagnostics = build_execution_diagnostics(
            &profile,
            &execution_contract,
            &search_response,
            &research_report,
            &claims,
            &citations,
            confidence,
            verification.contradictions,
        );
        let warnings = build_warnings(
            &verification,
            &research_report,
            &evidence,
            &execution_contract,
            &agent_ranking,
        );
        let follow_up_queries = recommend_follow_up_queries(
            &request.query,
            &profile,
            &execution_contract,
            &research_report.gaps,
        );
        let answer = synthesize_answer(
            &request.query,
            &profile,
            &execution_contract,
            &search_response,
            &research_report,
            enhanced_swarm.as_ref(),
            &evidence,
            confidence,
            &claims,
            &warnings,
            &agent_ranking,
        );
        let executive_summary = format!(
            "{} sources analyzed across {} lanes with {} citations prepared. Ranked {} agents with {:.0}% agreement. Exact answer ready.",
            evidence.len(),
            profile.source_classes.len(),
            citations.len(),
            agent_ranking.top_agents.len(),
            agent_ranking.agreement_score * 100.0
        );
        let subsystems_used = build_subsystems_used(
            &profile,
            &execution_contract,
            enhanced_swarm.is_some(),
            true,
        );
        let source_mix = build_source_mix(&evidence);

        let response = AdaptiveSearchResponse {
            query: request.query,
            answer,
            executive_summary,
            confidence,
            profile,
            execution_contract,
            evidence,
            citations,
            claims,
            source_mix,
            verification,
            agent_ranking,
            diagnostics,
            follow_up_queries,
            warnings,
            subsystems_used,
            ledger_proof: None,
            omega_decision: None,
            omega_mission: None,
            inventions: Vec::new(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        };

        self.cache.put(cache_key, response.clone());

        Ok(response)
    }

    pub fn get_stats(&self) -> AdaptiveSearchStats {
        AdaptiveSearchStats {
            total_queries: self.total_queries,
            fast_path_queries: self.fast_path_queries,
            deep_queries: self.deep_queries,
            notarized_queries: self.notarized_queries,
            cache_hits: self.cache_hits,
            cache_entries: self.cache.size(),
        }
    }

    fn profile_query(&self, request: &AdaptiveSearchRequest) -> SearchIntentProfile {
        let query = request.query.to_lowercase();
        let word_count = request.query.split_whitespace().count();
        let complexity_tokens = [
            "why",
            "how",
            "compare",
            "versus",
            "tradeoff",
            "impact",
            "trend",
            "strategy",
            "evidence",
            "history",
            "research",
            "study",
            "paper",
            "book",
            "social",
            "news",
            "exact",
            "deep",
            "analyze",
            "investigate",
        ];
        let complexity_hits = complexity_tokens
            .iter()
            .filter(|token| query.contains(**token))
            .count();
        let freshness_sensitive = request.freshness_horizon_hours.is_some()
            || ["latest", "today", "current", "breaking", "news"]
                .iter()
                .any(|token| query.contains(token));

        let difficulty = if word_count <= 5 && complexity_hits == 0 {
            SearchDifficulty::Easy
        } else if complexity_hits <= 2 && word_count <= 10 {
            SearchDifficulty::Standard
        } else if complexity_hits <= 5 {
            SearchDifficulty::Hard
        } else {
            SearchDifficulty::Expert
        };

        let requested_depth = request
            .preferred_depth
            .as_deref()
            .map(|value| value.to_lowercase());

        let research_depth = match requested_depth.as_deref() {
            Some("quick") => ResearchDepth::Quick,
            Some("standard") => ResearchDepth::Standard,
            Some("deep") => ResearchDepth::Deep,
            Some("ultra") => ResearchDepth::Ultra,
            _ => match difficulty {
                SearchDifficulty::Easy => ResearchDepth::Quick,
                SearchDifficulty::Standard => ResearchDepth::Standard,
                SearchDifficulty::Hard => ResearchDepth::Deep,
                SearchDifficulty::Expert => ResearchDepth::Ultra,
            },
        };

        let source_budget = match research_depth {
            ResearchDepth::Quick => 4,
            ResearchDepth::Standard => 8,
            ResearchDepth::Deep => 16,
            ResearchDepth::Ultra => 24,
        };

        let mut source_classes = vec![SearchSourceClass::Web, SearchSourceClass::Docs];
        if freshness_sensitive || request.include_news.unwrap_or(false) {
            source_classes.push(SearchSourceClass::News);
        }
        if query.contains("social")
            || query.contains("reddit")
            || query.contains("community")
            || request.include_social.unwrap_or(false)
        {
            source_classes.push(SearchSourceClass::Social);
        }
        if query.contains("book")
            || query.contains("history")
            || request
                .include_books
                .unwrap_or(matches!(difficulty, SearchDifficulty::Expert))
        {
            source_classes.push(SearchSourceClass::Books);
        }
        if query.contains("paper")
            || query.contains("study")
            || query.contains("research")
            || request.include_papers.unwrap_or(matches!(
                difficulty,
                SearchDifficulty::Hard | SearchDifficulty::Expert
            ))
        {
            source_classes.push(SearchSourceClass::Papers);
        }
        if request.include_docs.unwrap_or(true)
            && !source_classes.contains(&SearchSourceClass::Docs)
        {
            source_classes.push(SearchSourceClass::Docs);
        }

        if let Some(domain_focus) = request.domain_focus.as_ref() {
            for focus in domain_focus {
                let focus_lower = focus.to_lowercase();
                if focus_lower.contains("news")
                    && !source_classes.contains(&SearchSourceClass::News)
                {
                    source_classes.push(SearchSourceClass::News);
                }
                if focus_lower.contains("social")
                    && !source_classes.contains(&SearchSourceClass::Social)
                {
                    source_classes.push(SearchSourceClass::Social);
                }
                if focus_lower.contains("paper")
                    && !source_classes.contains(&SearchSourceClass::Papers)
                {
                    source_classes.push(SearchSourceClass::Papers);
                }
                if focus_lower.contains("book")
                    && !source_classes.contains(&SearchSourceClass::Books)
                {
                    source_classes.push(SearchSourceClass::Books);
                }
            }
        }

        source_classes.sort_by_key(|class| class.as_str().to_string());
        source_classes.dedup();

        let rationale = vec![
            format!("complexity_hits={complexity_hits}"),
            format!("word_count={word_count}"),
            format!("freshness_sensitive={freshness_sensitive}"),
            format!(
                "domain_focus={}",
                request.domain_focus.as_ref().map_or(0, Vec::len)
            ),
        ];

        SearchIntentProfile {
            difficulty,
            research_depth,
            source_budget,
            source_classes,
            reasoning_mode: match difficulty {
                SearchDifficulty::Easy => "fast_path".into(),
                SearchDifficulty::Standard => "evidence_blend".into(),
                SearchDifficulty::Hard => "deep_research".into(),
                SearchDifficulty::Expert => "research_council".into(),
            },
            freshness_sensitive,
            rationale,
            use_enhanced_swarm: matches!(
                difficulty,
                SearchDifficulty::Hard | SearchDifficulty::Expert
            ),
            fast_path: matches!(difficulty, SearchDifficulty::Easy),
        }
    }
}

impl Default for AdaptiveSearchEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub fn attach_ledger_proof(
    chain: &mut Chain,
    response: &mut AdaptiveSearchResponse,
) -> AstraResult<()> {
    if response.ledger_proof.is_none() {
        response.ledger_proof = Some(notarize_search_result(
            chain,
            &response.query,
            &response.answer,
            &response.evidence,
        )?);
    }
    Ok(())
}

fn build_execution_contract(
    request: &AdaptiveSearchRequest,
    profile: &SearchIntentProfile,
) -> SearchExecutionContract {
    let response_mode = request.response_mode.unwrap_or(match profile.difficulty {
        SearchDifficulty::Easy => SearchResponseMode::Fast,
        SearchDifficulty::Standard => SearchResponseMode::Balanced,
        SearchDifficulty::Hard => SearchResponseMode::Deep,
        SearchDifficulty::Expert => SearchResponseMode::Forensic,
    });
    let domain_focus = normalize_domain_focus(request.domain_focus.as_ref());
    let freshness_horizon_hours =
        request
            .freshness_horizon_hours
            .or(if profile.freshness_sensitive {
                Some(match response_mode {
                    SearchResponseMode::Fast => 24,
                    SearchResponseMode::Balanced => 72,
                    SearchResponseMode::Deep => 168,
                    SearchResponseMode::Forensic => 336,
                })
            } else {
                None
            });
    let max_contradictions = request.max_contradictions.unwrap_or(match response_mode {
        SearchResponseMode::Fast => 1,
        SearchResponseMode::Balanced => 2,
        SearchResponseMode::Deep => 3,
        SearchResponseMode::Forensic => 5,
    });

    SearchExecutionContract {
        response_mode,
        citation_required: request.require_citations.unwrap_or(!profile.fast_path),
        freshness_horizon_hours,
        max_contradictions,
        domain_focus,
        source_quotas: allocate_source_quotas(
            &profile.source_classes,
            request.max_sources.unwrap_or(profile.source_budget),
            response_mode,
            profile.freshness_sensitive,
        ),
        reasoning_layers: reasoning_layers_for(profile, response_mode),
    }
}

fn build_evidence(search: &SearchResponse, research: &ResearchReport) -> Vec<SearchEvidence> {
    let mut seen_urls = std::collections::BTreeSet::new();
    let mut evidence = search
        .results
        .iter()
        .map(|result| SearchEvidence {
            title: result.title.clone(),
            url: result.url.clone(),
            domain: result.domain.clone(),
            source_class: result.source_class.clone(),
            summary: result.snippet.clone(),
            relevance_score: result.relevance_score,
            credibility_score: result.credibility_score,
            freshness_score: result.freshness_score,
            verification_score: (result.credibility_score * 0.6 + result.depth_signal * 0.4)
                .clamp(0.0, 1.0),
            evidence_tags: result.evidence_tags.clone(),
        })
        .filter(|item| seen_urls.insert(item.url.clone()))
        .collect::<Vec<_>>();

    for source in research.sources.iter().take(8) {
        let item = SearchEvidence {
            title: source.title.clone(),
            url: source.url.clone(),
            domain: source.domain.clone(),
            source_class: source.source_class.clone(),
            summary: source.content_summary.clone(),
            relevance_score: source.relevance_score,
            credibility_score: source.credibility_score,
            freshness_score: if matches!(source.source_class, SearchSourceClass::News) {
                0.84
            } else {
                0.56
            },
            verification_score: (source.credibility_score * 0.65 + source.relevance_score * 0.35)
                .clamp(0.0, 1.0),
            evidence_tags: source.evidence_tags.clone(),
        };
        if seen_urls.insert(item.url.clone()) {
            evidence.push(item);
        }
    }

    evidence.sort_by(|left, right| {
        let right_score = evidence_score(right);
        let left_score = evidence_score(left);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    evidence.truncate(12);
    evidence
}

fn build_citations(
    evidence: &[SearchEvidence],
    contract: &SearchExecutionContract,
) -> Vec<SearchCitation> {
    let limit = match contract.response_mode {
        SearchResponseMode::Fast => 3,
        SearchResponseMode::Balanced => 5,
        SearchResponseMode::Deep => 6,
        SearchResponseMode::Forensic => 8,
    };

    evidence
        .iter()
        .take(limit)
        .enumerate()
        .map(|(index, item)| SearchCitation {
            id: format!("C{}", index + 1),
            title: item.title.clone(),
            url: item.url.clone(),
            domain: item.domain.clone(),
            source_class: item.source_class.clone(),
            excerpt: trim_excerpt(&item.summary, 180),
            support_score: evidence_score(item),
        })
        .collect()
}

fn build_claims(
    research: &ResearchReport,
    citations: &[SearchCitation],
    verification: &SearchVerificationSummary,
    confidence: f64,
    contract: &SearchExecutionContract,
) -> Vec<SearchClaim> {
    let claim_limit = match contract.response_mode {
        SearchResponseMode::Fast => 1,
        SearchResponseMode::Balanced => 2,
        SearchResponseMode::Deep => 3,
        SearchResponseMode::Forensic => 4,
    };
    let claim_texts = if research.key_findings.is_empty() {
        citations
            .iter()
            .map(|citation| citation.excerpt.clone())
            .collect::<Vec<_>>()
    } else {
        research.key_findings.clone()
    };

    claim_texts
        .into_iter()
        .take(claim_limit)
        .enumerate()
        .map(|(index, claim)| {
            let citation_ids = citations
                .iter()
                .skip(index.min(citations.len()))
                .take(
                    if matches!(contract.response_mode, SearchResponseMode::Forensic) {
                        2
                    } else {
                        1
                    },
                )
                .map(|citation| citation.id.clone())
                .collect::<Vec<_>>();
            let support_score = citations
                .iter()
                .filter(|citation| citation_ids.contains(&citation.id))
                .map(|citation| citation.support_score)
                .sum::<f64>()
                / citation_ids.len().max(1) as f64;
            let status = if verification.contradictions > contract.max_contradictions {
                SearchClaimStatus::Contested
            } else if support_score >= 0.72 && confidence >= 0.65 {
                SearchClaimStatus::Supported
            } else {
                SearchClaimStatus::Emerging
            };

            SearchClaim {
                claim: trim_excerpt(&claim, 160),
                status,
                confidence: (confidence * 0.7 + support_score * 0.3).clamp(0.0, 1.0),
                citation_ids,
            }
        })
        .collect()
}

fn verify_evidence(
    truth: &mut TruthEngine,
    query: &str,
    evidence: &[SearchEvidence],
    swarm: &crate::features::swarm_browsing::SwarmResult,
    ranking_agreement: f64,
) -> SearchVerificationSummary {
    let pseudo_html = evidence
        .iter()
        .map(|item| format!("<p>{}</p>", item.summary))
        .collect::<String>();
    let report = truth.verify_content(
        &format!(
            "search://{}",
            sha3_256_hex(query.as_bytes())[..24].to_string()
        ),
        &pseudo_html,
    );

    SearchVerificationSummary {
        truth_score: report.overall_score,
        cross_source_agreement: (swarm.confidence * 0.65 + ranking_agreement * 0.35)
            .clamp(0.0, 1.0),
        contradictions: evidence
            .iter()
            .filter(|item| item.credibility_score < 0.6 && item.freshness_score > 0.8)
            .count(),
        suspicious_segments: report.suspicious_count,
    }
}

fn compute_confidence(
    search: &SearchResponse,
    research: &ResearchReport,
    truth_score: f64,
    swarm_confidence: f64,
    enhanced_swarm: Option<&EnhancedSwarmResult>,
    ranking_agreement: f64,
) -> f64 {
    let search_confidence = search
        .results
        .iter()
        .take(5)
        .map(|result| result.credibility_score * 0.5 + result.relevance_score * 0.5)
        .sum::<f64>()
        / search.results.len().max(1).min(5) as f64;
    let enhanced_confidence = enhanced_swarm
        .map(|result| result.consensus_confidence)
        .unwrap_or(swarm_confidence);

    (search_confidence * 0.25
        + research.confidence * 0.3
        + truth_score * 0.2
        + swarm_confidence * 0.1
        + enhanced_confidence * 0.1
        + ranking_agreement * 0.05)
        .clamp(0.0, 1.0)
}

fn synthesize_answer(
    query: &str,
    profile: &SearchIntentProfile,
    contract: &SearchExecutionContract,
    search: &SearchResponse,
    research: &ResearchReport,
    enhanced_swarm: Option<&EnhancedSwarmResult>,
    evidence: &[SearchEvidence],
    confidence: f64,
    claims: &[SearchClaim],
    warnings: &[String],
    agent_ranking: &SearchAgentRankingSummary,
) -> String {
    let top_evidence = evidence
        .first()
        .map(|item| item.summary.as_str())
        .unwrap_or("No evidence was synthesized.");
    let consensus = enhanced_swarm
        .map(|result| result.consensus_answer.as_str())
        .unwrap_or(research.summary.as_str());
    let provider_mix = search
        .source_mix
        .iter()
        .map(|(kind, count)| format!("{kind}:{count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let claim_line = claims
        .iter()
        .map(|claim| format!("{} [{:?}]", claim.claim, claim.status))
        .collect::<Vec<_>>()
        .join(" | ");
    let warning_line = if warnings.is_empty() {
        "No major warning flags.".to_string()
    } else {
        warnings.join("; ")
    };

    match contract.response_mode {
        SearchResponseMode::Fast => format!(
            "Quick answer for '{}': {} Confidence {:.0}% from {}. Champion lane: {}.",
            query,
            top_evidence,
            confidence * 100.0,
            provider_mix,
            agent_ranking.champion_label
        ),
        SearchResponseMode::Balanced => format!(
            "Grounded answer for '{}': {} Consensus signal: {} Confidence {:.0}%. Ranked champion: {} ({:.0}% agreement). Claims: {}.",
            query,
            top_evidence,
            consensus,
            confidence * 100.0,
            agent_ranking.champion_label,
            agent_ranking.agreement_score * 100.0,
            if claim_line.is_empty() {
                "none".into()
            } else {
                claim_line
            }
        ),
        SearchResponseMode::Deep => format!(
            "Deep answer for '{}': {} Primary evidence: {} Confidence {:.0}%. Ranked council: {} ({:.0}% agreement). Layers: {}. Warnings: {}.",
            query,
            consensus,
            top_evidence,
            confidence * 100.0,
            agent_ranking.champion_label,
            agent_ranking.agreement_score * 100.0,
            contract.reasoning_layers.join(" -> "),
            warning_line
        ),
        SearchResponseMode::Forensic => format!(
            "Forensic answer for '{}': {} Research synthesis: {} Confidence {:.0}%. Ranked council: {} with cohort grade {}. Difficulty {:?}. Claim map: {}. Warning surface: {}.",
            query,
            consensus,
            top_evidence,
            confidence * 100.0,
            agent_ranking.champion_label,
            agent_ranking.cohort_grade,
            profile.difficulty,
            if claim_line.is_empty() {
                "none".into()
            } else {
                claim_line
            },
            warning_line
        ),
    }
}

fn build_subsystems_used(
    profile: &SearchIntentProfile,
    contract: &SearchExecutionContract,
    used_enhanced_swarm: bool,
    used_agent_ranking: bool,
) -> Vec<String> {
    let mut subsystems = vec![
        "adaptive_query_profiler".into(),
        "web_search".into(),
        "deep_research".into(),
        "truth_verification".into(),
        "chain_notary".into(),
        "swarm_consensus".into(),
        "execution_contract_resolver".into(),
        "citation_bundle_builder".into(),
        "claim_map_generator".into(),
    ];
    if profile.freshness_sensitive {
        subsystems.push("freshness_router".into());
    }
    if used_enhanced_swarm {
        subsystems.push("enhanced_swarm".into());
    }
    if used_agent_ranking {
        subsystems.push("agent_peer_ranking".into());
    }
    if contract.citation_required {
        subsystems.push("evidence_citations".into());
    }
    if matches!(contract.response_mode, SearchResponseMode::Forensic) {
        subsystems.push("forensic_trace_pack".into());
    }
    if profile
        .source_classes
        .iter()
        .any(|class| matches!(class, SearchSourceClass::Social))
    {
        subsystems.push("social_signal_sampler".into());
    }
    if profile
        .source_classes
        .iter()
        .any(|class| matches!(class, SearchSourceClass::Books))
    {
        subsystems.push("longform_contextualizer".into());
    }
    if profile
        .source_classes
        .iter()
        .any(|class| matches!(class, SearchSourceClass::Papers))
    {
        subsystems.push("paper_verifier".into());
    }
    subsystems
}

fn build_source_mix(evidence: &[SearchEvidence]) -> BTreeMap<String, usize> {
    let mut mix = BTreeMap::new();
    for item in evidence {
        *mix.entry(item.source_class.as_str().to_string())
            .or_insert(0) += 1;
    }
    mix
}

fn build_warnings(
    verification: &SearchVerificationSummary,
    research: &ResearchReport,
    evidence: &[SearchEvidence],
    contract: &SearchExecutionContract,
    agent_ranking: &SearchAgentRankingSummary,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if verification.contradictions > contract.max_contradictions {
        warnings.push(format!(
            "contradictions_exceed_threshold:{}>{}",
            verification.contradictions, contract.max_contradictions
        ));
    }
    if verification.truth_score < 0.55 {
        warnings.push(format!("low_truth_score:{:.2}", verification.truth_score));
    }
    if evidence.len() < 3 {
        warnings.push("thin_evidence_bundle".into());
    }
    if contract.freshness_horizon_hours.is_some()
        && !evidence
            .iter()
            .any(|item| matches!(item.source_class, SearchSourceClass::News))
    {
        warnings.push("freshness_sensitive_query_without_news_lane".into());
    }
    if let Some(gap) = research.gaps.first() {
        warnings.push(format!("coverage_gap:{gap}"));
    }
    if agent_ranking.agreement_score < 0.58 {
        warnings.push(format!(
            "low_agent_agreement:{:.2}",
            agent_ranking.agreement_score
        ));
    }
    if agent_ranking.cohort_grade == "C" {
        warnings.push("ranking_cohort_requires_review".into());
    }

    warnings
}

fn build_agent_ranking(
    profile: &SearchIntentProfile,
    contract: &SearchExecutionContract,
    swarm: &crate::features::swarm_browsing::SwarmResult,
    enhanced_swarm: Option<&EnhancedSwarmResult>,
) -> SearchAgentRankingSummary {
    let ranked = enhanced_swarm
        .map(|result| &result.ranking_consensus)
        .unwrap_or(&swarm.ranking_consensus);
    let strategy = if profile.fast_path {
        "speed_lane".to_string()
    } else if enhanced_swarm.is_some() {
        "ranked_council".to_string()
    } else {
        "peer_ranked_swarm".to_string()
    };
    let speed_profile = match contract.response_mode {
        SearchResponseMode::Fast => "latency_first",
        SearchResponseMode::Balanced => "balanced_accuracy",
        SearchResponseMode::Deep => "depth_first",
        SearchResponseMode::Forensic => "deliberative_accuracy",
    }
    .to_string();

    SearchAgentRankingSummary {
        strategy,
        speed_profile,
        champion_label: ranked.champion_label.clone(),
        agreement_score: ranked.agreement_score,
        cohort_grade: ranked.cohort_grade.clone(),
        consensus_summary: ranked.consensus_summary.clone(),
        peer_review_count: ranked.peer_reviews.len(),
        top_agents: ranked
            .ranked_agents
            .iter()
            .take(if profile.fast_path { 2 } else { 4 })
            .map(|agent| SearchAgentRank {
                label: agent.label.clone(),
                specialization: agent.specialization.clone(),
                rank: agent.rank,
                final_score: agent.final_score,
                peer_review_score: agent.peer_review_score,
                consensus_weight: agent.consensus_weight,
                summary: trim_excerpt(&agent.summary, 180),
            })
            .collect(),
    }
}

fn build_execution_diagnostics(
    profile: &SearchIntentProfile,
    contract: &SearchExecutionContract,
    search: &SearchResponse,
    research: &ResearchReport,
    claims: &[SearchClaim],
    citations: &[SearchCitation],
    confidence: f64,
    contradictions: usize,
) -> SearchExecutionDiagnostics {
    let lane_execution = contract
        .source_quotas
        .iter()
        .map(|(lane, quota)| {
            let lane_search_results = search
                .results
                .iter()
                .filter(|result| result.source_class.as_str() == lane.as_str())
                .collect::<Vec<_>>();
            let lane_sources = research
                .sources
                .iter()
                .filter(|source| source.source_class.as_str() == lane.as_str())
                .collect::<Vec<_>>();
            let avg_credibility = if lane_sources.is_empty() {
                lane_search_results
                    .iter()
                    .map(|result| result.credibility_score)
                    .sum::<f64>()
                    / lane_search_results.len().max(1) as f64
            } else {
                lane_sources
                    .iter()
                    .map(|source| source.credibility_score)
                    .sum::<f64>()
                    / lane_sources.len() as f64
            };
            let avg_relevance = if lane_sources.is_empty() {
                lane_search_results
                    .iter()
                    .map(|result| result.relevance_score)
                    .sum::<f64>()
                    / lane_search_results.len().max(1) as f64
            } else {
                lane_sources
                    .iter()
                    .map(|source| source.relevance_score)
                    .sum::<f64>()
                    / lane_sources.len() as f64
            };

            SearchLaneExecution {
                lane: lane.clone(),
                planned_quota: *quota,
                search_results: lane_search_results.len(),
                vetted_sources: lane_sources.len(),
                avg_credibility: avg_credibility.clamp(0.0, 1.0),
                avg_relevance: avg_relevance.clamp(0.0, 1.0),
            }
        })
        .collect::<Vec<_>>();

    SearchExecutionDiagnostics {
        speed_class: if profile.fast_path {
            "latency_first".into()
        } else if matches!(contract.response_mode, SearchResponseMode::Forensic) {
            "deliberative".into()
        } else {
            "balanced_depth".into()
        },
        freshness_sensitive: profile.freshness_sensitive,
        contradiction_pressure: if contradictions == 0 {
            "low".into()
        } else if contradictions <= contract.max_contradictions {
            "moderate".into()
        } else {
            "high".into()
        },
        acquisition_strategy: search.acquisition_strategy.clone(),
        indexed_recall_hits: search.indexed_hits,
        fresh_fetch_hits: search.fresh_hits,
        total_lanes: lane_execution.len(),
        exact_answer: build_exact_answer(claims, citations, confidence),
        lane_execution,
    }
}

fn build_exact_answer(
    claims: &[SearchClaim],
    citations: &[SearchCitation],
    confidence: f64,
) -> SearchExactAnswer {
    if let Some(claim) = claims.first() {
        return SearchExactAnswer {
            answer: claim.claim.clone(),
            answer_type: match claim.status {
                SearchClaimStatus::Supported => "supported_claim".into(),
                SearchClaimStatus::Contested => "contested_claim".into(),
                SearchClaimStatus::Emerging => "emerging_claim".into(),
            },
            confidence: claim.confidence,
            citation_ids: claim.citation_ids.clone(),
        };
    }

    let citation_ids = citations
        .first()
        .map(|citation| vec![citation.id.clone()])
        .unwrap_or_default();
    let answer = citations
        .first()
        .map(|citation| trim_excerpt(&citation.excerpt, 140))
        .unwrap_or_else(|| "No exact answer could be isolated.".into());

    SearchExactAnswer {
        answer,
        answer_type: "evidence_excerpt".into(),
        confidence,
        citation_ids,
    }
}

fn notarize_search_result(
    chain: &mut Chain,
    query: &str,
    answer: &str,
    evidence: &[SearchEvidence],
) -> AstraResult<SearchLedgerProof> {
    let trace_id = sha3_256_hex(format!("trace:{query}:{answer}").as_bytes())[..24].to_string();
    let answer_hash = sha3_256_hex(answer.as_bytes());
    let answer_commitment = chain.notarize_resource(
        &format!("httpa://search/answer/{trace_id}"),
        &answer_hash,
        "search_answer",
        "adaptive_search",
        "agent:adaptive_search",
        serde_json::json!({
            "trace_id": trace_id.clone(),
            "query": query,
            "artifact": "answer",
            "answer_hash": answer_hash.clone(),
            "bundle_version": 1,
            "evidence_count": evidence.len(),
        }),
    )?;

    let mut evidence_commitments = Vec::new();
    for (index, item) in evidence.iter().take(3).enumerate() {
        let evidence_hash = sha3_256_hex(item.summary.as_bytes());
        evidence_commitments.push(chain.notarize_resource(
            &item.url,
            &evidence_hash,
            "search_evidence",
            item.source_class.as_str(),
            "agent:adaptive_search",
            serde_json::json!({
                "trace_id": trace_id.clone(),
                "artifact": "evidence",
                "query": query,
                "answer_hash": answer_hash.clone(),
                "evidence_hash": evidence_hash.clone(),
                "evidence_rank": index,
                "title": item.title.clone(),
                "domain": item.domain.clone(),
                "source_class": item.source_class.as_str(),
                "credibility": item.credibility_score,
                "verification_score": item.verification_score,
            }),
        )?);
    }

    let manifest_payload = serde_json::json!({
        "trace_id": trace_id.clone(),
        "artifact": "manifest",
        "bundle_version": 1,
        "query": query,
        "answer_hash": answer_hash.clone(),
        "answer_commitment_id": answer_commitment.commitment_id.clone(),
        "evidence_commitment_ids": evidence_commitments
            .iter()
            .map(|commitment| commitment.commitment_id.clone())
            .collect::<Vec<_>>(),
    });
    let manifest_hash = sha3_256_hex(manifest_payload.to_string().as_bytes());
    chain.notarize_resource(
        &format!("httpa://search/manifest/{trace_id}"),
        &manifest_hash,
        "search_manifest",
        "adaptive_search",
        "agent:adaptive_search",
        manifest_payload,
    )?;

    let proof_bundle = chain.find_search_proof(&trace_id)?;
    build_search_ledger_proof(proof_bundle, answer_hash)
}

fn build_search_ledger_proof(
    proof_bundle: SearchProofBundle,
    fallback_answer_hash: String,
) -> AstraResult<SearchLedgerProof> {
    let manifest_commitment = proof_bundle.manifest_commitment.ok_or_else(|| {
        crate::error::AstraError::SearchFailed(
            "search proof is missing its manifest commitment".into(),
        )
    })?;
    let answer_commitment = proof_bundle.answer_commitment.ok_or_else(|| {
        crate::error::AstraError::SearchFailed(
            "search proof is missing its answer commitment".into(),
        )
    })?;

    Ok(SearchLedgerProof {
        trace_id: proof_bundle.trace_id,
        answer_hash: proof_bundle.answer_hash.unwrap_or(fallback_answer_hash),
        manifest_commitment,
        answer_commitment,
        evidence_commitments: proof_bundle.evidence_commitments,
        anchored_commitments: proof_bundle.anchored_commitments,
        certified_commitments: proof_bundle.certified_commitments,
        manifest_consistent: proof_bundle.manifest_consistent,
        verified: proof_bundle.verified,
        warnings: proof_bundle.warnings,
    })
}

fn derive_focus_areas(query: &str, domain_focus: &[String]) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut focus = Vec::new();
    if lower.contains("security") {
        focus.push("security".into());
    }
    if lower.contains("performance") || lower.contains("fast") {
        focus.push("performance".into());
    }
    if lower.contains("history") || lower.contains("book") {
        focus.push("historical context".into());
    }
    if lower.contains("social") || lower.contains("community") {
        focus.push("community sentiment".into());
    }
    if lower.contains("news") || lower.contains("latest") {
        focus.push("recent developments".into());
    }
    focus.extend(domain_focus.iter().cloned());
    focus.sort();
    focus.dedup();
    if focus.is_empty() {
        focus.push("core answer".into());
    }
    focus
}

fn normalize_domain_focus(domain_focus: Option<&Vec<String>>) -> Vec<String> {
    let mut normalized = domain_focus
        .map(|values| {
            values
                .iter()
                .map(|value| value.trim().to_lowercase())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn allocate_source_quotas(
    classes: &[SearchSourceClass],
    total: usize,
    response_mode: SearchResponseMode,
    freshness_sensitive: bool,
) -> BTreeMap<String, usize> {
    let mut weighted = classes
        .iter()
        .map(|class| {
            let base = match class {
                SearchSourceClass::Web => 3usize,
                SearchSourceClass::Docs => 2,
                SearchSourceClass::News => {
                    if freshness_sensitive {
                        3
                    } else {
                        1
                    }
                }
                SearchSourceClass::Social => 1,
                SearchSourceClass::Books => 2,
                SearchSourceClass::Papers => 2,
            };
            let mode_bonus = match response_mode {
                SearchResponseMode::Fast => {
                    if matches!(class, SearchSourceClass::Web | SearchSourceClass::Docs) {
                        1
                    } else {
                        0
                    }
                }
                SearchResponseMode::Balanced => 0,
                SearchResponseMode::Deep => {
                    if matches!(
                        class,
                        SearchSourceClass::Docs
                            | SearchSourceClass::Papers
                            | SearchSourceClass::Books
                    ) {
                        1
                    } else {
                        0
                    }
                }
                SearchResponseMode::Forensic => {
                    if matches!(
                        class,
                        SearchSourceClass::Docs
                            | SearchSourceClass::Papers
                            | SearchSourceClass::News
                    ) {
                        2
                    } else {
                        1
                    }
                }
            };
            (class.as_str().to_string(), base + mode_bonus)
        })
        .collect::<Vec<_>>();
    weighted.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    let mut quotas = weighted
        .iter()
        .map(|(label, _)| (label.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    let total = total.max(1);

    if total >= weighted.len() {
        for (label, _) in &weighted {
            quotas.insert(label.clone(), 1);
        }
        let mut remaining = total.saturating_sub(weighted.len());
        let total_weight = weighted
            .iter()
            .map(|(_, weight)| *weight)
            .sum::<usize>()
            .max(1);
        for (label, weight) in &weighted {
            if remaining == 0 {
                break;
            }
            let share = (remaining * *weight) / total_weight;
            if share > 0 {
                *quotas.entry(label.clone()).or_insert(0) += share;
                remaining = remaining.saturating_sub(share);
            }
        }
        let mut index = 0usize;
        while remaining > 0 {
            let label = &weighted[index % weighted.len()].0;
            *quotas.entry(label.clone()).or_insert(0) += 1;
            remaining -= 1;
            index += 1;
        }
    } else {
        for (label, _) in weighted.into_iter().take(total) {
            quotas.insert(label, 1);
        }
    }

    quotas.retain(|_, value| *value > 0);
    quotas
}

fn reasoning_layers_for(
    profile: &SearchIntentProfile,
    response_mode: SearchResponseMode,
) -> Vec<String> {
    let mut layers = vec!["query_profile".into(), "source_routing".into()];
    if !profile.fast_path {
        layers.push("deep_research".into());
    }
    layers.push("truth_verification".into());
    if profile.use_enhanced_swarm {
        layers.push("swarm_debate".into());
    } else {
        layers.push("swarm_consensus".into());
    }
    layers.push("agent_peer_ranking".into());
    if matches!(
        response_mode,
        SearchResponseMode::Deep | SearchResponseMode::Forensic
    ) {
        layers.push("citation_packaging".into());
    }
    if matches!(response_mode, SearchResponseMode::Forensic) {
        layers.push("claim_map".into());
    }
    layers
}

fn recommend_follow_up_queries(
    query: &str,
    profile: &SearchIntentProfile,
    contract: &SearchExecutionContract,
    gaps: &[String],
) -> Vec<String> {
    let mut follow_ups = Vec::new();

    if profile.freshness_sensitive {
        follow_ups.push(format!("What changed in the last 24 hours about {query}?"));
    }
    if profile
        .source_classes
        .iter()
        .any(|class| matches!(class, SearchSourceClass::Papers))
    {
        follow_ups.push(format!("Find primary papers or studies behind {query}."));
    }
    if profile
        .source_classes
        .iter()
        .any(|class| matches!(class, SearchSourceClass::Social))
    {
        follow_ups.push(format!(
            "Compare institutional claims and community sentiment for {query}."
        ));
    }
    if let Some(focus) = contract.domain_focus.first() {
        follow_ups.push(format!("Re-run {query} with deeper focus on {focus}."));
    }
    if let Some(gap) = gaps.first() {
        follow_ups.push(format!("Resolve the remaining evidence gap: {gap}."));
    }
    if follow_ups.is_empty() {
        follow_ups.push(format!("Collect primary-source verification for {query}."));
    }

    follow_ups.truncate(4);
    follow_ups
}

fn horizon_to_time_range(
    freshness_horizon_hours: Option<u64>,
    freshness_sensitive: bool,
) -> Option<TimeRange> {
    match freshness_horizon_hours {
        Some(hours) if hours <= 24 => Some(TimeRange::Day),
        Some(hours) if hours <= 24 * 7 => Some(TimeRange::Week),
        Some(hours) if hours <= 24 * 30 => Some(TimeRange::Month),
        Some(_) => Some(TimeRange::Year),
        None if freshness_sensitive => Some(TimeRange::Week),
        None => None,
    }
}

fn trim_excerpt(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len {
        return input.to_string();
    }

    let trimmed = input
        .chars()
        .take(max_len.saturating_sub(3))
        .collect::<String>();
    format!("{trimmed}...")
}

fn evidence_score(item: &SearchEvidence) -> f64 {
    item.relevance_score * 0.35
        + item.credibility_score * 0.35
        + item.verification_score * 0.2
        + item.freshness_score * 0.1
}

fn cache_key(request: &AdaptiveSearchRequest) -> String {
    format!(
        "{}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
        request.query,
        request.preferred_depth,
        request.max_sources,
        request.include_social,
        request.include_news,
        request.include_books,
        request.include_papers,
        request.include_docs,
        request.response_mode,
        request.freshness_horizon_hours,
        request.domain_focus,
        request.require_citations,
        request.max_contradictions,
        request.notarize
    )
}

fn swarm_size_for(profile: &SearchIntentProfile) -> usize {
    match profile.difficulty {
        SearchDifficulty::Easy => 3,
        SearchDifficulty::Standard => 5,
        SearchDifficulty::Hard => 7,
        SearchDifficulty::Expert => 9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiler_detects_deep_research_queries() {
        let engine = AdaptiveSearchEngine::new();
        let profile = engine.profile_query(&AdaptiveSearchRequest {
            query:
                "compare social media, news, books, and research papers on remote work productivity"
                    .into(),
            preferred_depth: None,
            max_sources: None,
            include_social: None,
            include_news: None,
            include_books: None,
            include_papers: None,
            include_docs: None,
            response_mode: None,
            freshness_horizon_hours: None,
            domain_focus: None,
            require_citations: None,
            max_contradictions: None,
            notarize: None,
        });

        assert!(matches!(
            profile.difficulty,
            SearchDifficulty::Hard | SearchDifficulty::Expert
        ));
        assert!(profile
            .source_classes
            .iter()
            .any(|class| matches!(class, SearchSourceClass::Social)));
        assert!(profile
            .source_classes
            .iter()
            .any(|class| matches!(class, SearchSourceClass::Papers)));
    }

    #[test]
    fn preview_builds_forensic_execution_contract_for_expert_queries() {
        let engine = AdaptiveSearchEngine::new();
        let plan = engine.preview(&AdaptiveSearchRequest {
            query:
                "analyze latest research papers, books, news, and social reactions to sovereign browser protocols"
                    .into(),
            preferred_depth: Some("ultra".into()),
            max_sources: Some(18),
            include_social: Some(true),
            include_news: Some(true),
            include_books: Some(true),
            include_papers: Some(true),
            include_docs: Some(true),
            response_mode: None,
            freshness_horizon_hours: Some(48),
            domain_focus: Some(vec!["governance".into(), "security".into()]),
            require_citations: Some(true),
            max_contradictions: Some(4),
            notarize: None,
        });

        assert!(matches!(
            plan.execution_contract.response_mode,
            SearchResponseMode::Forensic
        ));
        assert!(plan.execution_contract.citation_required);
        assert!(plan.execution_contract.source_quotas.contains_key("papers"));
        assert!(plan
            .execution_contract
            .reasoning_layers
            .iter()
            .any(|layer| layer == "agent_peer_ranking"));
        assert!(plan
            .follow_up_queries
            .iter()
            .any(|query| query.contains("primary papers")));
    }

    #[test]
    fn adaptive_search_can_notarize_results() {
        let mut engine = AdaptiveSearchEngine::new();
        let mut web_search = WebSearch::new_ephemeral();
        let mut deep_research = DeepResearch::new();
        let mut swarm = SwarmBrowser::new();
        let mut truth = TruthEngine::new();
        let mut enhanced_swarm = EnhancedSwarmEngine::new();
        let mut chain = Chain::new();

        let mut response = engine
            .execute(
                AdaptiveSearchRequest {
                    query: "latest protocol governance research".into(),
                    preferred_depth: Some("deep".into()),
                    max_sources: Some(10),
                    include_social: Some(true),
                    include_news: Some(true),
                    include_books: Some(false),
                    include_papers: Some(true),
                    include_docs: Some(true),
                    response_mode: Some(SearchResponseMode::Deep),
                    freshness_horizon_hours: Some(72),
                    domain_focus: Some(vec!["governance".into(), "security".into()]),
                    require_citations: Some(true),
                    max_contradictions: Some(2),
                    notarize: Some(true),
                },
                AdaptiveSearchDeps {
                    web_search: &mut web_search,
                    deep_research: &mut deep_research,
                    swarm: &mut swarm,
                    truth: &mut truth,
                    enhanced_swarm: &mut enhanced_swarm,
                },
            )
            .expect("adaptive search should succeed");
        attach_ledger_proof(&mut chain, &mut response).expect("ledger proof should be attached");

        assert!(response.ledger_proof.is_some());
        assert!(!response.evidence.is_empty());
        assert!(!response.citations.is_empty());
        assert!(!response.claims.is_empty());
        let proof = response
            .ledger_proof
            .expect("search response should expose a ledger proof");
        assert!(proof.manifest_consistent);
        assert!(proof.verified);
        assert_eq!(proof.anchored_commitments, 5);
        assert!(!proof.evidence_commitments.is_empty());
        assert!(!response.agent_ranking.top_agents.is_empty());
        assert!(response.agent_ranking.agreement_score > 0.0);
        assert!(!response.agent_ranking.champion_label.is_empty());
        assert_eq!(response.diagnostics.acquisition_strategy, "fresh_only");
        assert_eq!(response.diagnostics.indexed_recall_hits, 0);
        assert!(response.diagnostics.fresh_fetch_hits > 0);
    }
}

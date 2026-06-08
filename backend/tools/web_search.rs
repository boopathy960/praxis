use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::core_infra::cache_hierarchy::CacheTier;
use crate::orchestrator::scheduler::JobScheduler;

use super::deep_research::ResearchSource;
use super::live_web::{bing_search, credibility_for_domain};
use super::search_index::{
    IndexedSearchDocument, SearchAcquisitionIndex, SearchAcquisitionRefreshPlan,
    SearchAcquisitionStats,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchProvider {
    DuckDuckGo,
    Brave,
    Google,
    Bing,
    Aggregated,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub provider: SearchProvider,
    pub max_results: usize,
    pub language: String,
    pub region: Option<String>,
    pub safe_search: bool,
    pub time_range: Option<TimeRange>,
    #[serde(default)]
    pub source_classes: Vec<SearchSourceClass>,
    #[serde(default)]
    pub deep_reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    pub relevance_score: f64,
    pub credibility_score: f64,
    pub freshness_score: f64,
    pub depth_signal: f64,
    pub source: SearchProvider,
    pub source_class: SearchSourceClass,
    pub evidence_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub total_found: usize,
    pub duration_ms: f64,
    pub provider_used: SearchProvider,
    pub source_mix: BTreeMap<String, usize>,
    pub indexed_hits: usize,
    pub fresh_hits: usize,
    pub acquisition_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAcquisitionRefreshSweepResult {
    pub plan: SearchAcquisitionRefreshPlan,
    pub executed_tasks: usize,
    pub search_results_ingested: usize,
    pub refreshed_queries: Vec<String>,
    pub refreshed_lanes: Vec<String>,
    pub source_mix: BTreeMap<String, usize>,
    pub acquisition: SearchAcquisitionStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAutonomousProfileStatus {
    pub profile_id: String,
    pub name: String,
    pub interval_minutes: u64,
    pub enabled: bool,
    pub due: bool,
    pub seconds_until_due: u64,
    pub run_count: u64,
    pub max_tasks_per_run: usize,
    pub max_results_per_task: usize,
    pub source_classes: Vec<String>,
    pub default_seeds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAutonomousProfileRun {
    pub profile_id: String,
    pub profile_name: String,
    pub refreshed_queries: Vec<String>,
    pub search_results_ingested: usize,
    pub source_mix: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAutonomousTickReport {
    pub executed_at: u64,
    pub forced: bool,
    pub due_profiles: usize,
    pub executed_profiles: usize,
    pub queued_queries: usize,
    pub refreshed_queries: Vec<String>,
    pub source_mix: BTreeMap<String, usize>,
    pub profiles: Vec<SearchAutonomousProfileRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAutonomousFrontier {
    pub generated_at: u64,
    pub queued_queries: usize,
    pub next_queries: Vec<String>,
    pub profiles: Vec<SearchAutonomousProfileStatus>,
    pub total_runs: u64,
    pub last_run: Option<SearchAutonomousTickReport>,
}

#[derive(Debug, Clone)]
struct SearchAutonomousProfile {
    profile_id: String,
    name: String,
    interval: Duration,
    max_tasks_per_run: usize,
    max_results_per_task: usize,
    source_classes: Vec<SearchSourceClass>,
    default_seeds: Vec<String>,
    deep_reasoning: bool,
}

pub struct WebSearch {
    preferred_provider: SearchProvider,
    fallback_providers: Vec<SearchProvider>,
    acquisition_index: SearchAcquisitionIndex,
    autonomous_profiles: Vec<SearchAutonomousProfile>,
    autonomous_job_ids: HashMap<String, String>,
    autonomous_scheduler: JobScheduler,
    autonomous_total_runs: u64,
    autonomous_last_run: Option<SearchAutonomousTickReport>,
    cache: CacheTier<SearchResponse>,
    total_searches: u64,
}

impl WebSearch {
    pub fn new() -> Self {
        let (autonomous_profiles, autonomous_scheduler, autonomous_job_ids) =
            build_autonomous_scheduler();
        Self {
            preferred_provider: SearchProvider::Brave,
            fallback_providers: vec![SearchProvider::DuckDuckGo, SearchProvider::Bing],
            acquisition_index: SearchAcquisitionIndex::new(),
            autonomous_profiles,
            autonomous_job_ids,
            autonomous_scheduler,
            autonomous_total_runs: 0,
            autonomous_last_run: None,
            cache: CacheTier::new(1024, Duration::from_secs(300)),
            total_searches: 0,
        }
    }

    pub fn new_compact() -> Self {
        let autonomous_profiles = Vec::new();
        let (autonomous_scheduler, autonomous_job_ids) =
            build_scheduler_for_profiles(&autonomous_profiles);
        Self {
            preferred_provider: SearchProvider::Brave,
            fallback_providers: vec![SearchProvider::DuckDuckGo, SearchProvider::Bing],
            acquisition_index: SearchAcquisitionIndex::new_ephemeral(),
            autonomous_profiles,
            autonomous_job_ids,
            autonomous_scheduler,
            autonomous_total_runs: 0,
            autonomous_last_run: None,
            cache: CacheTier::new(128, Duration::from_secs(180)),
            total_searches: 0,
        }
    }

    pub fn new_ephemeral() -> Self {
        let (autonomous_profiles, autonomous_scheduler, autonomous_job_ids) =
            build_autonomous_scheduler();
        Self {
            preferred_provider: SearchProvider::Brave,
            fallback_providers: vec![SearchProvider::DuckDuckGo, SearchProvider::Bing],
            acquisition_index: SearchAcquisitionIndex::new_ephemeral(),
            autonomous_profiles,
            autonomous_job_ids,
            autonomous_scheduler,
            autonomous_total_runs: 0,
            autonomous_last_run: None,
            cache: CacheTier::new(1024, Duration::from_secs(300)),
            total_searches: 0,
        }
    }

    pub fn search(&mut self, query: SearchQuery) -> SearchResponse {
        self.total_searches += 1;
        let start = Instant::now();
        let cache_key = format!(
            "{}:{:?}:{}:{:?}:{:?}:{}",
            query.query,
            query.provider,
            query.max_results,
            query.time_range,
            query.source_classes,
            query.deep_reasoning
        );
        if let Some(cached) = self.cache.get(&cache_key) {
            return cached;
        }

        let indexed_recall = self.acquisition_index.query(
            &query.query,
            &query.source_classes,
            query.max_results.max(1) * 2,
        );
        let fresh_results = self.execute_search(&query);
        self.acquisition_index
            .ingest_search_results(&query, &fresh_results);
        let indexed_hits = indexed_recall.len();
        let fresh_hits = fresh_results.len();
        let merged = self.merge_with_indexed_recall(indexed_recall, fresh_results);
        let total_found = merged.len();
        let ranked = self.rank_results(&query, merged);
        let truncated: Vec<SearchResult> =
            ranked.into_iter().take(query.max_results.max(1)).collect();
        let source_mix = build_source_mix(&truncated);

        let response = SearchResponse {
            query: query.query.clone(),
            total_found,
            results: truncated,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            provider_used: query.provider.clone(),
            source_mix,
            indexed_hits,
            fresh_hits,
            acquisition_strategy: if indexed_hits > 0 {
                "fresh_plus_index".into()
            } else {
                "fresh_only".into()
            },
        };

        self.cache.put(cache_key, response.clone());

        response
    }

    pub fn refresh_acquisition(&mut self, query: SearchQuery) -> SearchResponse {
        self.total_searches += 1;
        let start = Instant::now();
        let fresh_results = self.execute_search(&query);
        let fresh_hits = fresh_results.len();
        self.acquisition_index
            .ingest_search_results(&query, &fresh_results);
        let ranked = self.rank_results(&query, fresh_results);
        let truncated: Vec<SearchResult> =
            ranked.into_iter().take(query.max_results.max(1)).collect();
        let source_mix = build_source_mix(&truncated);

        SearchResponse {
            query: query.query,
            total_found: truncated.len(),
            results: truncated,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            provider_used: query.provider,
            source_mix,
            indexed_hits: 0,
            fresh_hits,
            acquisition_strategy: "forced_refresh".into(),
        }
    }

    fn execute_search(&self, query: &SearchQuery) -> Vec<SearchResult> {
        let classes = if query.source_classes.is_empty() {
            vec![SearchSourceClass::Web, SearchSourceClass::Docs]
        } else {
            query.source_classes.clone()
        };
        let mut results = Vec::new();
        let providers = self.providers_for(&query.provider);
        for class in &classes {
            let live_results = self.fetch_live_results(query, class);
            if !live_results.is_empty() {
                results.extend(live_results);
            } else {
                for provider in &providers {
                    results.extend(self.generate_results_for_class(query, provider, class));
                }
            }
        }
        results
    }

    fn providers_for(&self, provider: &SearchProvider) -> Vec<SearchProvider> {
        if matches!(provider, SearchProvider::Aggregated) {
            let mut providers = vec![self.preferred_provider.clone()];
            providers.extend(self.fallback_providers.clone());
            providers.push(SearchProvider::Google);
            providers
        } else {
            vec![provider.clone()]
        }
    }

    fn generate_results_for_class(
        &self,
        query: &SearchQuery,
        provider: &SearchProvider,
        class: &SearchSourceClass,
    ) -> Vec<SearchResult> {
        let intent_tags = classify_query_tags(&query.query);
        let templates = source_templates(class);
        let freshness_boost = freshness_bias(query.time_range.as_ref());
        let reasoning_bonus = if query.deep_reasoning { 0.08 } else { 0.0 };

        templates
            .iter()
            .enumerate()
            .map(|(idx, template)| {
                let domain = format!(
                    "{}.{}.example",
                    template.0,
                    provider_label(provider).replace(' ', "-")
                );
                let title = format!("{} on {}", template.1, query.query);
                let snippet = format!(
                    "{} perspective on {}. Key signals: {}.",
                    template.2,
                    query.query,
                    intent_tags.join(", ")
                );
                let position_penalty = idx as f64 * 0.035;
                let relevance_score =
                    (0.92 - position_penalty + class_bonus(class, &intent_tags) + reasoning_bonus)
                        .clamp(0.0, 1.0);
                let credibility_score = (template.3 + provider_bonus(provider)).clamp(0.0, 1.0);
                let freshness_score =
                    (template.4 + freshness_boost - position_penalty * 0.5).clamp(0.0, 1.0);
                let depth_signal = (0.58
                    + if query.deep_reasoning { 0.18 } else { 0.0 }
                    + if matches!(class, SearchSourceClass::Papers | SearchSourceClass::Books) {
                        0.16
                    } else {
                        0.0
                    }
                    - position_penalty)
                    .clamp(0.0, 1.0);
                let evidence_tags = build_evidence_tags(class, &intent_tags, idx);

                SearchResult {
                    title,
                    url: format!(
                        "https://{}/{}",
                        domain,
                        query
                            .query
                            .to_lowercase()
                            .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
                    ),
                    domain,
                    snippet,
                    relevance_score,
                    credibility_score,
                    freshness_score,
                    depth_signal,
                    source: provider.clone(),
                    source_class: class.clone(),
                    evidence_tags,
                }
            })
            .collect()
    }

    fn fetch_live_results(
        &self,
        query: &SearchQuery,
        class: &SearchSourceClass,
    ) -> Vec<SearchResult> {
        let target_results = query.max_results.max(3);
        let freshness_boost = freshness_bias(query.time_range.as_ref());
        let reasoning_bonus = if query.deep_reasoning { 0.08 } else { 0.0 };

        bing_search(&query.query, class, target_results)
            .into_iter()
            .enumerate()
            .map(|(idx, hit)| {
                let position_penalty = idx as f64 * 0.06;
                let credibility_score = credibility_for_domain(&hit.domain, class);
                let depth_signal = (0.58
                    + if query.deep_reasoning { 0.16 } else { 0.0 }
                    + if matches!(class, SearchSourceClass::Papers | SearchSourceClass::Books) {
                        0.14
                    } else if matches!(class, SearchSourceClass::Docs) {
                        0.1
                    } else {
                        0.0
                    }
                    - position_penalty)
                    .clamp(0.0, 1.0);
                let relevance_score = (0.9
                    + class_bonus(class, &classify_query_tags(&query.query))
                    + reasoning_bonus
                    - position_penalty)
                    .clamp(0.0, 1.0);
                let freshness_score = live_freshness_score(class, query.time_range.as_ref(), idx);
                let mut evidence_tags =
                    build_evidence_tags(class, &classify_query_tags(&query.query), idx);
                evidence_tags.push("live_web".into());
                evidence_tags.push("bing_rss".into());
                if query.deep_reasoning {
                    evidence_tags.push("deep_reasoning".into());
                }

                SearchResult {
                    title: hit.title,
                    url: hit.url,
                    domain: hit.domain,
                    snippet: hit.snippet,
                    relevance_score,
                    credibility_score,
                    freshness_score: (freshness_score + freshness_boost * 0.5).clamp(0.0, 1.0),
                    depth_signal,
                    source: SearchProvider::Bing,
                    source_class: class.clone(),
                    evidence_tags,
                }
            })
            .collect()
    }

    fn rank_results(
        &self,
        query: &SearchQuery,
        mut results: Vec<SearchResult>,
    ) -> Vec<SearchResult> {
        let time_bias = freshness_bias(query.time_range.as_ref());
        results.sort_by(|left, right| {
            let right_score = ranking_score(right, time_bias);
            let left_score = ranking_score(left, time_bias);
            right_score
                .partial_cmp(&left_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    pub fn total_searches(&self) -> u64 {
        self.total_searches
    }

    pub fn acquisition_stats(&self) -> SearchAcquisitionStats {
        self.acquisition_index.stats()
    }

    pub fn acquisition_refresh_plan(&self, limit: usize) -> SearchAcquisitionRefreshPlan {
        self.acquisition_index.refresh_plan(limit)
    }

    pub fn execute_refresh_plan(
        &mut self,
        limit: usize,
        max_results_per_task: usize,
    ) -> SearchAcquisitionRefreshSweepResult {
        let plan = self.acquisition_index.refresh_plan(limit);
        let mut source_mix = BTreeMap::<String, usize>::new();
        let mut refreshed_queries = Vec::new();
        let mut refreshed_lanes = Vec::new();
        let mut search_results_ingested = 0_usize;

        for task in &plan.tasks {
            let response = self.refresh_acquisition(SearchQuery {
                query: task.query.clone(),
                provider: SearchProvider::Aggregated,
                max_results: max_results_per_task.max(1),
                language: "en".into(),
                region: None,
                safe_search: true,
                time_range: refresh_time_range(task.target_refresh_hours),
                source_classes: vec![task.source_class.clone()],
                deep_reasoning: matches!(
                    task.source_class,
                    SearchSourceClass::Docs | SearchSourceClass::Papers | SearchSourceClass::Books
                ),
            });
            search_results_ingested += response.fresh_hits;
            refreshed_queries.push(task.query.clone());
            refreshed_lanes.push(task.lane.clone());

            for (lane, count) in response.source_mix {
                *source_mix.entry(lane).or_insert(0) += count;
            }
        }

        SearchAcquisitionRefreshSweepResult {
            executed_tasks: plan.tasks.len(),
            plan,
            search_results_ingested,
            refreshed_queries,
            refreshed_lanes,
            source_mix,
            acquisition: self.acquisition_index.stats(),
        }
    }

    pub fn autonomous_frontier(&self) -> SearchAutonomousFrontier {
        let refresh_plan = self.acquisition_index.refresh_plan(16);
        let profile_statuses = self.autonomous_profile_statuses();
        let mut next_queries = Vec::new();

        for profile in &self.autonomous_profiles {
            for query in self.autonomous_candidate_queries(profile, &refresh_plan) {
                if !next_queries.contains(&query) {
                    next_queries.push(query);
                }
                if next_queries.len() >= 12 {
                    break;
                }
            }
            if next_queries.len() >= 12 {
                break;
            }
        }

        SearchAutonomousFrontier {
            generated_at: now_epoch_secs(),
            queued_queries: next_queries.len(),
            next_queries,
            profiles: profile_statuses,
            total_runs: self.autonomous_total_runs,
            last_run: self.autonomous_last_run.clone(),
        }
    }

    pub fn execute_autonomous_tick(
        &mut self,
        force: bool,
        max_profiles: Option<usize>,
    ) -> SearchAutonomousTickReport {
        let due_profiles = self.autonomous_scheduler.due_job_ids().len();
        let refresh_plan = self.acquisition_index.refresh_plan(24);
        let mut profile_ids = if force {
            self.autonomous_profiles
                .iter()
                .map(|profile| profile.profile_id.clone())
                .collect::<Vec<_>>()
        } else {
            self.autonomous_scheduler.due_job_ids()
        };
        if let Some(limit) = max_profiles {
            profile_ids.truncate(limit.max(1));
        }

        let profiles_to_run = profile_ids
            .into_iter()
            .filter_map(|profile_id| self.autonomous_profile(&profile_id).cloned())
            .collect::<Vec<_>>();
        let mut refreshed_queries = Vec::new();
        let mut source_mix = BTreeMap::<String, usize>::new();
        let mut profile_reports = Vec::new();

        for profile in profiles_to_run {
            let queries = self
                .autonomous_candidate_queries(&profile, &refresh_plan)
                .into_iter()
                .take(profile.max_tasks_per_run)
                .collect::<Vec<_>>();
            if queries.is_empty() {
                if let Some(job_id) = self.autonomous_job_ids.get(&profile.profile_id) {
                    self.autonomous_scheduler.mark_run(job_id);
                }
                continue;
            }

            let mut profile_mix = BTreeMap::<String, usize>::new();
            let mut search_results_ingested = 0_usize;
            for query in &queries {
                let response = self.refresh_acquisition(SearchQuery {
                    query: query.clone(),
                    provider: SearchProvider::Aggregated,
                    max_results: profile.max_results_per_task.max(1),
                    language: "en".into(),
                    region: None,
                    safe_search: true,
                    time_range: profile_time_range(&profile),
                    source_classes: profile.source_classes.clone(),
                    deep_reasoning: profile.deep_reasoning,
                });
                search_results_ingested += response.fresh_hits;
                for (lane, count) in response.source_mix {
                    *profile_mix.entry(lane.clone()).or_insert(0) += count;
                    *source_mix.entry(lane).or_insert(0) += count;
                }
            }

            refreshed_queries.extend(queries.iter().cloned());
            if let Some(job_id) = self.autonomous_job_ids.get(&profile.profile_id) {
                self.autonomous_scheduler.mark_run(job_id);
            }
            profile_reports.push(SearchAutonomousProfileRun {
                profile_id: profile.profile_id.clone(),
                profile_name: profile.name.clone(),
                refreshed_queries: queries,
                search_results_ingested,
                source_mix: profile_mix,
            });
        }

        let report = SearchAutonomousTickReport {
            executed_at: now_epoch_secs(),
            forced: force,
            due_profiles,
            executed_profiles: profile_reports.len(),
            queued_queries: refreshed_queries.len(),
            refreshed_queries,
            source_mix,
            profiles: profile_reports,
        };
        self.autonomous_total_runs += 1;
        self.autonomous_last_run = Some(report.clone());
        report
    }

    pub fn ingest_research_sources(&mut self, sources: &[ResearchSource]) {
        let documents = sources.iter().map(|source| IndexedSearchDocument {
            url: source.url.clone(),
            canonical_url: String::new(),
            title: source.title.clone(),
            domain: source.domain.clone(),
            snippet: source.content_summary.clone(),
            primary_query: source.title.clone(),
            source_class: source.source_class.clone(),
            relevance_score: source.relevance_score,
            credibility_score: source.credibility_score,
            freshness_score: if matches!(source.source_class, SearchSourceClass::News) {
                0.84
            } else {
                0.6
            },
            depth_signal: if matches!(
                source.source_class,
                SearchSourceClass::Papers | SearchSourceClass::Books | SearchSourceClass::Docs
            ) {
                0.9
            } else {
                0.72
            },
            evidence_tags: source.evidence_tags.clone(),
            queries_seen: 1,
            recall_count: 0,
            first_seen_at: 0,
            last_seen_at: 0,
            search_haystack: String::new(),
        });
        self.acquisition_index.ingest_external_documents(documents);
    }

    fn merge_with_indexed_recall(
        &self,
        indexed_hits: Vec<super::search_index::IndexedSearchHit>,
        fresh_results: Vec<SearchResult>,
    ) -> Vec<SearchResult> {
        let mut merged = HashMap::<String, SearchResult>::new();
        for hit in indexed_hits {
            let mut evidence_tags = hit.document.evidence_tags.clone();
            evidence_tags.push("indexed_recall".into());
            evidence_tags.push("historical_recall".into());
            merged.insert(
                hit.document.url.clone(),
                SearchResult {
                    title: hit.document.title.clone(),
                    url: hit.document.url.clone(),
                    domain: hit.document.domain.clone(),
                    snippet: hit.document.snippet.clone(),
                    relevance_score: ((hit.document.relevance_score * 0.6)
                        + (hit.recall_score * 0.4))
                        .clamp(0.0, 1.0),
                    credibility_score: hit.document.credibility_score,
                    freshness_score: hit.document.freshness_score,
                    depth_signal: (hit.document.depth_signal + 0.08).clamp(0.0, 1.0),
                    source: SearchProvider::Aggregated,
                    source_class: hit.document.source_class.clone(),
                    evidence_tags,
                },
            );
        }

        for result in fresh_results {
            merged
                .entry(result.url.clone())
                .and_modify(|existing| {
                    existing.relevance_score = existing.relevance_score.max(result.relevance_score);
                    existing.credibility_score =
                        existing.credibility_score.max(result.credibility_score);
                    existing.freshness_score = existing.freshness_score.max(result.freshness_score);
                    existing.depth_signal = existing.depth_signal.max(result.depth_signal);
                    for tag in &result.evidence_tags {
                        if !existing.evidence_tags.contains(tag) {
                            existing.evidence_tags.push(tag.clone());
                        }
                    }
                    existing.source = result.source.clone();
                    existing.source_class = result.source_class.clone();
                })
                .or_insert(result);
        }

        merged.into_values().collect()
    }

    fn autonomous_profile(&self, profile_id: &str) -> Option<&SearchAutonomousProfile> {
        self.autonomous_profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
    }

    fn autonomous_profile_statuses(&self) -> Vec<SearchAutonomousProfileStatus> {
        let job_statuses = self
            .autonomous_scheduler
            .job_statuses()
            .into_iter()
            .map(|status| (status.name.clone(), status))
            .collect::<HashMap<_, _>>();

        self.autonomous_profiles
            .iter()
            .map(|profile| {
                let job_status = job_statuses.get(&profile.profile_id);
                SearchAutonomousProfileStatus {
                    profile_id: profile.profile_id.clone(),
                    name: profile.name.clone(),
                    interval_minutes: profile.interval.as_secs() / 60,
                    enabled: job_status.map_or(true, |status| status.enabled),
                    due: job_status.map_or(true, |status| status.due),
                    seconds_until_due: job_status.map_or(0, |status| status.seconds_until_due),
                    run_count: job_status.map_or(0, |status| status.run_count),
                    max_tasks_per_run: profile.max_tasks_per_run,
                    max_results_per_task: profile.max_results_per_task,
                    source_classes: profile
                        .source_classes
                        .iter()
                        .map(|class| class.as_str().to_string())
                        .collect(),
                    default_seeds: profile.default_seeds.clone(),
                }
            })
            .collect()
    }

    fn autonomous_candidate_queries(
        &self,
        profile: &SearchAutonomousProfile,
        refresh_plan: &SearchAcquisitionRefreshPlan,
    ) -> Vec<String> {
        let mut queries = Vec::new();
        for task in &refresh_plan.tasks {
            if profile.source_classes.contains(&task.source_class) && !queries.contains(&task.query)
            {
                queries.push(task.query.clone());
            }
        }
        for query in &refresh_plan.suggested_queries {
            if !queries.contains(query) {
                queries.push(query.clone());
            }
        }
        for seed in &profile.default_seeds {
            if !queries.contains(seed) {
                queries.push(seed.clone());
            }
        }
        queries
    }
}

impl Default for WebSearch {
    fn default() -> Self {
        Self::new()
    }
}

fn ranking_score(result: &SearchResult, time_bias: f64) -> f64 {
    result.relevance_score * 0.4
        + result.credibility_score * 0.25
        + result.freshness_score * (0.1 + time_bias * 0.1)
        + result.depth_signal * 0.15
        + provider_bonus(&result.source) * 0.1
}

fn build_source_mix(results: &[SearchResult]) -> BTreeMap<String, usize> {
    let mut mix = BTreeMap::new();
    for result in results {
        *mix.entry(result.source_class.as_str().to_string())
            .or_insert(0) += 1;
    }
    mix
}

fn classify_query_tags(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    let mut tags = Vec::new();

    if ["why", "how", "compare", "versus", "difference"]
        .iter()
        .any(|token| lower.contains(token))
    {
        tags.push("analysis".into());
    }
    if ["latest", "today", "current", "news", "trend"]
        .iter()
        .any(|token| lower.contains(token))
    {
        tags.push("freshness".into());
    }
    if ["paper", "study", "research", "journal", "evidence"]
        .iter()
        .any(|token| lower.contains(token))
    {
        tags.push("evidence".into());
    }
    if ["book", "history", "author", "chapter", "biography"]
        .iter()
        .any(|token| lower.contains(token))
    {
        tags.push("longform".into());
    }
    if ["social", "reddit", "twitter", "x ", "community", "people"]
        .iter()
        .any(|token| lower.contains(token))
    {
        tags.push("social".into());
    }
    if tags.is_empty() {
        tags.push("general".into());
    }

    tags
}

fn class_bonus(class: &SearchSourceClass, tags: &[String]) -> f64 {
    match class {
        SearchSourceClass::News if tags.iter().any(|tag| tag == "freshness") => 0.09,
        SearchSourceClass::Papers if tags.iter().any(|tag| tag == "evidence") => 0.1,
        SearchSourceClass::Books if tags.iter().any(|tag| tag == "longform") => 0.08,
        SearchSourceClass::Social if tags.iter().any(|tag| tag == "social") => 0.08,
        SearchSourceClass::Docs if tags.iter().any(|tag| tag == "analysis") => 0.04,
        _ => 0.02,
    }
}

fn provider_bonus(provider: &SearchProvider) -> f64 {
    match provider {
        SearchProvider::Brave => 0.08,
        SearchProvider::DuckDuckGo => 0.06,
        SearchProvider::Google => 0.07,
        SearchProvider::Bing => 0.05,
        SearchProvider::Aggregated => 0.09,
    }
}

fn freshness_bias(time_range: Option<&TimeRange>) -> f64 {
    match time_range {
        Some(TimeRange::Day) => 0.18,
        Some(TimeRange::Week) => 0.12,
        Some(TimeRange::Month) => 0.08,
        Some(TimeRange::Year) => 0.04,
        None => 0.02,
    }
}

fn live_freshness_score(
    class: &SearchSourceClass,
    time_range: Option<&TimeRange>,
    idx: usize,
) -> f64 {
    let class_bias = match class {
        SearchSourceClass::News => 0.9,
        SearchSourceClass::Social => 0.82,
        SearchSourceClass::Web => 0.7,
        SearchSourceClass::Docs => 0.62,
        SearchSourceClass::Papers => 0.56,
        SearchSourceClass::Books => 0.38,
    };
    let time_bias = freshness_bias(time_range);
    (class_bias + time_bias - idx as f64 * 0.04).clamp(0.0, 1.0)
}

fn build_autonomous_scheduler() -> (
    Vec<SearchAutonomousProfile>,
    JobScheduler,
    HashMap<String, String>,
) {
    let profiles = default_autonomous_profiles();
    let (scheduler, job_ids) = build_scheduler_for_profiles(&profiles);
    (profiles, scheduler, job_ids)
}

fn build_scheduler_for_profiles(
    profiles: &[SearchAutonomousProfile],
) -> (JobScheduler, HashMap<String, String>) {
    let mut scheduler = JobScheduler::new();
    let mut job_ids = HashMap::new();
    for profile in profiles {
        let job_id = scheduler.schedule(&profile.profile_id, profile.interval);
        job_ids.insert(profile.profile_id.clone(), job_id);
    }
    (scheduler, job_ids)
}

fn default_autonomous_profiles() -> Vec<SearchAutonomousProfile> {
    vec![
        SearchAutonomousProfile {
            profile_id: "breaking_news".into(),
            name: "Breaking News".into(),
            interval: Duration::from_secs(30 * 60),
            max_tasks_per_run: 3,
            max_results_per_task: 4,
            source_classes: vec![SearchSourceClass::News, SearchSourceClass::Social],
            default_seeds: vec![
                "latest browser protocol market update".into(),
                "latest AI browser security news".into(),
            ],
            deep_reasoning: false,
        },
        SearchAutonomousProfile {
            profile_id: "canonical_docs".into(),
            name: "Canonical Docs".into(),
            interval: Duration::from_secs(6 * 60 * 60),
            max_tasks_per_run: 3,
            max_results_per_task: 4,
            source_classes: vec![SearchSourceClass::Docs, SearchSourceClass::Web],
            default_seeds: vec![
                "browser protocol spec".into(),
                "HTTPA execution contract docs".into(),
            ],
            deep_reasoning: true,
        },
        SearchAutonomousProfile {
            profile_id: "research_watch".into(),
            name: "Research Watch".into(),
            interval: Duration::from_secs(12 * 60 * 60),
            max_tasks_per_run: 2,
            max_results_per_task: 5,
            source_classes: vec![SearchSourceClass::Papers, SearchSourceClass::Books],
            default_seeds: vec![
                "multi agent search ranking research".into(),
                "browser retrieval system papers".into(),
            ],
            deep_reasoning: true,
        },
    ]
}

fn profile_time_range(profile: &SearchAutonomousProfile) -> Option<TimeRange> {
    if profile
        .source_classes
        .iter()
        .any(|class| matches!(class, SearchSourceClass::News | SearchSourceClass::Social))
    {
        Some(TimeRange::Day)
    } else if profile
        .source_classes
        .iter()
        .any(|class| matches!(class, SearchSourceClass::Docs | SearchSourceClass::Web))
    {
        Some(TimeRange::Month)
    } else {
        Some(TimeRange::Year)
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn refresh_time_range(hours: u64) -> Option<TimeRange> {
    match hours {
        0..=24 => Some(TimeRange::Day),
        25..=168 => Some(TimeRange::Week),
        169..=720 => Some(TimeRange::Month),
        _ => Some(TimeRange::Year),
    }
}

fn provider_label(provider: &SearchProvider) -> &'static str {
    match provider {
        SearchProvider::DuckDuckGo => "duckduckgo",
        SearchProvider::Brave => "brave",
        SearchProvider::Google => "google",
        SearchProvider::Bing => "bing",
        SearchProvider::Aggregated => "aggregated",
    }
}

fn build_evidence_tags(
    class: &SearchSourceClass,
    query_tags: &[String],
    idx: usize,
) -> Vec<String> {
    let mut tags = vec![class.as_str().to_string()];
    tags.extend(query_tags.iter().cloned());
    if idx == 0 {
        tags.push("top_ranked".into());
    }
    if matches!(class, SearchSourceClass::Papers | SearchSourceClass::Books) {
        tags.push("longform_context".into());
    }
    tags
}

fn source_templates(
    class: &SearchSourceClass,
) -> Vec<(&'static str, &'static str, &'static str, f64, f64)> {
    match class {
        SearchSourceClass::Web => vec![
            (
                "index",
                "Reference overview",
                "Structured overview",
                0.76,
                0.62,
            ),
            (
                "insights",
                "Analyst explainer",
                "Interpretive breakdown",
                0.72,
                0.58,
            ),
            (
                "guide",
                "Technical guide",
                "How-it-works explanation",
                0.78,
                0.54,
            ),
        ],
        SearchSourceClass::News => vec![
            (
                "newsdesk",
                "Newsroom dispatch",
                "Recent reporting",
                0.81,
                0.92,
            ),
            ("dailybrief", "Daily brief", "Timely coverage", 0.76, 0.88),
            (
                "investigations",
                "Investigation desk",
                "Verified reporting",
                0.84,
                0.83,
            ),
        ],
        SearchSourceClass::Social => vec![
            (
                "community",
                "Community thread",
                "Crowd-sourced reaction",
                0.56,
                0.78,
            ),
            (
                "creator",
                "Creator commentary",
                "First-person perspective",
                0.52,
                0.74,
            ),
            ("forum", "Forum synthesis", "Experience reports", 0.59, 0.7),
        ],
        SearchSourceClass::Books => vec![
            (
                "library",
                "Book chapter extract",
                "Long-form framing",
                0.88,
                0.45,
            ),
            (
                "archive",
                "Reference volume",
                "Historical synthesis",
                0.9,
                0.38,
            ),
            (
                "reader",
                "Scholarly book review",
                "Contextual interpretation",
                0.82,
                0.4,
            ),
        ],
        SearchSourceClass::Papers => vec![
            ("journal", "Peer-reviewed paper", "Formal study", 0.93, 0.64),
            (
                "preprint",
                "Research preprint",
                "Early findings",
                0.79,
                0.71,
            ),
            (
                "labnotes",
                "Research lab summary",
                "Method-oriented analysis",
                0.86,
                0.6,
            ),
        ],
        SearchSourceClass::Docs => vec![
            (
                "docs",
                "Official documentation",
                "Canonical reference",
                0.95,
                0.68,
            ),
            (
                "spec",
                "Technical specification",
                "Normative behavior",
                0.97,
                0.52,
            ),
            (
                "manual",
                "Implementation manual",
                "Operational detail",
                0.89,
                0.57,
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregated_search_returns_mixed_sources() {
        let mut search = WebSearch::new_ephemeral();
        let response = search.search(SearchQuery {
            query: "latest ai agent research".into(),
            provider: SearchProvider::Aggregated,
            max_results: 6,
            language: "en".into(),
            region: None,
            safe_search: true,
            time_range: Some(TimeRange::Week),
            source_classes: vec![SearchSourceClass::News, SearchSourceClass::Papers],
            deep_reasoning: true,
        });

        assert_eq!(response.results.len(), 6);
        assert!(response.source_mix.contains_key("news"));
        assert!(response.source_mix.contains_key("papers"));
        assert_eq!(response.acquisition_strategy, "fresh_only");
    }

    #[test]
    fn indexed_recall_enriches_related_queries() {
        let mut search = WebSearch::new_ephemeral();
        let first = search.search(SearchQuery {
            query: "sovereign browser search protocol".into(),
            provider: SearchProvider::Aggregated,
            max_results: 5,
            language: "en".into(),
            region: None,
            safe_search: true,
            time_range: None,
            source_classes: vec![SearchSourceClass::Docs, SearchSourceClass::Web],
            deep_reasoning: true,
        });
        let second = search.search(SearchQuery {
            query: "browser search protocol evidence".into(),
            provider: SearchProvider::Aggregated,
            max_results: 5,
            language: "en".into(),
            region: None,
            safe_search: true,
            time_range: None,
            source_classes: vec![SearchSourceClass::Docs, SearchSourceClass::Web],
            deep_reasoning: true,
        });

        assert_eq!(first.indexed_hits, 0);
        assert!(second.indexed_hits > 0);
        assert_eq!(second.acquisition_strategy, "fresh_plus_index");
        assert!(second.results.iter().any(|result| result
            .evidence_tags
            .iter()
            .any(|tag| tag == "indexed_recall")));
    }

    #[test]
    fn forced_refresh_bypasses_indexed_recall_strategy() {
        let mut search = WebSearch::new_ephemeral();
        let response = search.refresh_acquisition(SearchQuery {
            query: "latest browser research".into(),
            provider: SearchProvider::Aggregated,
            max_results: 5,
            language: "en".into(),
            region: None,
            safe_search: true,
            time_range: Some(TimeRange::Week),
            source_classes: vec![SearchSourceClass::News, SearchSourceClass::Docs],
            deep_reasoning: true,
        });

        assert_eq!(response.acquisition_strategy, "forced_refresh");
        assert_eq!(response.indexed_hits, 0);
        assert!(response.fresh_hits > 0);
    }

    #[test]
    fn refresh_plan_execution_runs_prioritized_tasks() {
        let mut search = WebSearch::new_ephemeral();
        let _ = search.refresh_acquisition(SearchQuery {
            query: "latest browser protocol market outlook".into(),
            provider: SearchProvider::Aggregated,
            max_results: 4,
            language: "en".into(),
            region: None,
            safe_search: true,
            time_range: Some(TimeRange::Week),
            source_classes: vec![SearchSourceClass::News],
            deep_reasoning: true,
        });

        let stale = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
            .saturating_sub(96 * 3600);
        search
            .acquisition_index
            .ingest_external_documents([IndexedSearchDocument {
                url: "https://news.aggregated.example/latest-browser-protocol-market-outlook"
                    .into(),
                canonical_url: String::new(),
                title: "Latest browser protocol market outlook".into(),
                domain: "news.aggregated.example".into(),
                snippet: "Stale market coverage".into(),
                primary_query: "latest browser protocol market outlook".into(),
                source_class: SearchSourceClass::News,
                relevance_score: 0.82,
                credibility_score: 0.8,
                freshness_score: 0.61,
                depth_signal: 0.58,
                evidence_tags: vec!["news".into()],
                queries_seen: 1,
                recall_count: 0,
                first_seen_at: stale,
                last_seen_at: stale,
                search_haystack: String::new(),
            }]);

        let sweep = search.execute_refresh_plan(4, 3);

        assert!(sweep.executed_tasks >= 1);
        assert!(sweep.search_results_ingested >= 3);
        assert!(sweep
            .refreshed_queries
            .iter()
            .any(|query| query == "latest browser protocol market outlook"));
        assert!(sweep.source_mix.contains_key("news"));
    }

    #[test]
    fn autonomous_tick_executes_seeded_profiles() {
        let mut search = WebSearch::new_ephemeral();

        let report = search.execute_autonomous_tick(true, Some(2));

        assert!(report.forced);
        assert_eq!(report.executed_profiles, 2);
        assert!(report.queued_queries >= 2);
        assert!(report
            .refreshed_queries
            .iter()
            .any(|query| query.contains("market update")));
        assert!(search.autonomous_frontier().total_runs >= 1);
    }
}

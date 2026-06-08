use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use url::Url;

use super::web_search::{SearchQuery, SearchResult, SearchSourceClass};

const PERSIST_BATCH_SIZE: u64 = 32;
const PERSIST_INTERVAL_SECS: u64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedSearchDocument {
    pub url: String,
    pub canonical_url: String,
    pub title: String,
    pub domain: String,
    pub snippet: String,
    pub primary_query: String,
    pub source_class: SearchSourceClass,
    pub relevance_score: f64,
    pub credibility_score: f64,
    pub freshness_score: f64,
    pub depth_signal: f64,
    pub evidence_tags: Vec<String>,
    pub queries_seen: u64,
    pub recall_count: u64,
    pub first_seen_at: u64,
    pub last_seen_at: u64,
    #[serde(skip)]
    pub search_haystack: String,
}

#[derive(Debug, Clone)]
pub struct IndexedSearchHit {
    pub document: IndexedSearchDocument,
    pub recall_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAcquisitionStats {
    pub total_documents: usize,
    pub total_ingestions: u64,
    pub total_recalls: u64,
    pub last_updated_at: u64,
    pub stale_documents: usize,
    pub refresh_queue_depth: usize,
    pub oldest_document_age_hours: u64,
    pub lane_pressure: Vec<SearchFreshnessLaneStats>,
    pub suggested_queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAcquisitionRefreshPlan {
    pub generated_at: u64,
    pub total_tasks: usize,
    pub total_stale_documents: usize,
    pub suggested_queries: Vec<String>,
    pub tasks: Vec<SearchAcquisitionRefreshTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAcquisitionRefreshTask {
    pub query: String,
    pub lane: String,
    pub source_class: SearchSourceClass,
    pub documents: usize,
    pub stale_documents: usize,
    pub refresh_queue_depth: usize,
    pub target_refresh_hours: u64,
    pub oldest_age_hours: u64,
    pub urgency_score: f64,
    pub sample_urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFreshnessLaneStats {
    pub lane: String,
    pub documents: usize,
    pub stale_documents: usize,
    pub refresh_queue_depth: usize,
    pub target_refresh_hours: u64,
    pub oldest_age_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchIndexSnapshot {
    documents: HashMap<String, IndexedSearchDocument>,
    total_ingestions: u64,
    total_recalls: u64,
    last_updated_at: u64,
}

#[derive(Serialize)]
struct SearchIndexSnapshotRef<'a> {
    documents: &'a HashMap<String, IndexedSearchDocument>,
    total_ingestions: u64,
    total_recalls: u64,
    last_updated_at: u64,
}

pub struct SearchAcquisitionIndex {
    documents: HashMap<String, IndexedSearchDocument>,
    persistence_path: Option<PathBuf>,
    total_ingestions: u64,
    total_recalls: u64,
    last_updated_at: u64,
    pending_persist_ops: u64,
    last_persisted_at: u64,
    persist_batch_size: u64,
    persist_interval_secs: u64,
}

impl SearchAcquisitionIndex {
    #[must_use]
    pub fn new() -> Self {
        let persistence_path = default_index_path();
        Self::load_from_path(Some(persistence_path))
    }

    #[must_use]
    pub fn new_ephemeral() -> Self {
        Self::load_from_path(None)
    }

    fn load_from_path(persistence_path: Option<PathBuf>) -> Self {
        if let Some(path) = persistence_path.as_ref() {
            if let Ok(raw) = fs::read_to_string(path) {
                if let Ok(snapshot) = serde_json::from_str::<SearchIndexSnapshot>(&raw) {
                    let mut documents = snapshot.documents;
                    hydrate_documents(&mut documents);
                    return Self {
                        documents,
                        persistence_path,
                        total_ingestions: snapshot.total_ingestions,
                        total_recalls: snapshot.total_recalls,
                        last_updated_at: snapshot.last_updated_at,
                        pending_persist_ops: 0,
                        last_persisted_at: snapshot.last_updated_at,
                        persist_batch_size: PERSIST_BATCH_SIZE,
                        persist_interval_secs: PERSIST_INTERVAL_SECS,
                    };
                }
            }
        }

        let now = now_epoch_secs();
        Self {
            documents: HashMap::new(),
            persistence_path,
            total_ingestions: 0,
            total_recalls: 0,
            last_updated_at: now,
            pending_persist_ops: 0,
            last_persisted_at: now,
            persist_batch_size: PERSIST_BATCH_SIZE,
            persist_interval_secs: PERSIST_INTERVAL_SECS,
        }
    }

    pub fn ingest_search_results(&mut self, query: &SearchQuery, results: &[SearchResult]) {
        let timestamp = now_epoch_secs();
        for result in results {
            let canonical_url = canonicalize_search_url(&result.url);
            let mut new_tags = result.evidence_tags.iter().cloned().collect::<HashSet<_>>();
            new_tags.insert("indexed".into());
            if query.deep_reasoning {
                new_tags.insert("deep_reasoning".into());
            }

            if let Some(existing) = self.documents.get_mut(&canonical_url) {
                new_tags.extend(existing.evidence_tags.iter().cloned());
                existing.url = canonical_url.clone();
                existing.canonical_url = canonical_url.clone();
                existing.title = result.title.clone();
                existing.domain = result.domain.clone();
                existing.snippet = result.snippet.clone();
                existing.primary_query = query.query.clone();
                existing.source_class = result.source_class.clone();
                existing.relevance_score = result.relevance_score;
                existing.credibility_score = result.credibility_score;
                existing.freshness_score = result.freshness_score;
                existing.depth_signal = result.depth_signal;
                existing.evidence_tags = sorted_tags(new_tags);
                existing.queries_seen += 1;
                existing.last_seen_at = timestamp;
                hydrate_document(existing);
            } else {
                let mut document = IndexedSearchDocument {
                    url: canonical_url.clone(),
                    canonical_url: canonical_url.clone(),
                    title: result.title.clone(),
                    domain: result.domain.clone(),
                    snippet: result.snippet.clone(),
                    primary_query: query.query.clone(),
                    source_class: result.source_class.clone(),
                    relevance_score: result.relevance_score,
                    credibility_score: result.credibility_score,
                    freshness_score: result.freshness_score,
                    depth_signal: result.depth_signal,
                    evidence_tags: sorted_tags(new_tags),
                    queries_seen: 1,
                    recall_count: 0,
                    first_seen_at: timestamp,
                    last_seen_at: timestamp,
                    search_haystack: String::new(),
                };
                hydrate_document(&mut document);
                self.documents.insert(canonical_url, document);
            }
        }

        self.total_ingestions += results.len() as u64;
        self.note_mutation(results.len() as u64, timestamp);
    }

    #[must_use]
    pub fn query(
        &mut self,
        query: &str,
        source_classes: &[SearchSourceClass],
        limit: usize,
    ) -> Vec<IndexedSearchHit> {
        let tokens = tokenize(query);
        if tokens.is_empty() {
            return Vec::new();
        }
        let now = now_epoch_secs();
        let diversity_penalty = index_diversity_penalty(self.documents.values());

        let mut hits = self
            .documents
            .values()
            .filter(|doc| {
                source_classes.is_empty()
                    || source_classes
                        .iter()
                        .any(|class| class == &doc.source_class)
            })
            .filter_map(|doc| {
                let haystack = &doc.search_haystack;
                let overlap = tokens
                    .iter()
                    .filter(|token| haystack.contains(token.as_str()))
                    .count();
                if overlap == 0 {
                    return None;
                }

                let overlap_ratio = overlap as f64 / tokens.len() as f64;
                let recall_bonus = (doc.recall_count as f64 * 0.015).clamp(0.0, 0.08);
                let score =
                    enterprise_recall_score(doc, overlap_ratio, recall_bonus, diversity_penalty, now);

                Some(IndexedSearchHit {
                    document: doc.clone(),
                    recall_score: score,
                })
            })
            .collect::<Vec<_>>();

        hits.sort_by(|left, right| {
            right
                .recall_score
                .partial_cmp(&left.recall_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let capped = limit.max(1).min(hits.len());
        let selected = hits.into_iter().take(capped).collect::<Vec<_>>();
        for hit in &selected {
            if let Some(doc) = self.documents.get_mut(&hit.document.canonical_url) {
                doc.recall_count += 1;
                doc.last_seen_at = now_epoch_secs();
            }
        }

        self.total_recalls += selected.len() as u64;
        if !selected.is_empty() {
            self.note_mutation(selected.len() as u64, now_epoch_secs());
        }
        selected
    }

    pub fn ingest_external_documents(
        &mut self,
        documents: impl IntoIterator<Item = IndexedSearchDocument>,
    ) {
        let timestamp = now_epoch_secs();
        let mut ingested = 0_u64;
        for doc in documents {
            ingested += 1;
            let canonical_url = canonicalize_search_url(&doc.url);
            let mut prepared = IndexedSearchDocument {
                url: canonical_url.clone(),
                canonical_url,
                first_seen_at: if doc.first_seen_at == 0 {
                    timestamp
                } else {
                    doc.first_seen_at
                },
                last_seen_at: if doc.last_seen_at == 0 {
                    timestamp
                } else {
                    doc.last_seen_at
                },
                search_haystack: String::new(),
                ..doc
            };
            hydrate_document(&mut prepared);
            self.documents
                .insert(prepared.canonical_url.clone(), prepared);
        }
        self.total_ingestions += ingested;
        self.note_mutation(ingested, timestamp);
    }

    #[must_use]
    pub fn stats(&self) -> SearchAcquisitionStats {
        let now = now_epoch_secs();
        let lane_pressure = build_lane_pressure(&self.documents, now);
        let stale_documents = lane_pressure
            .iter()
            .map(|lane| lane.stale_documents)
            .sum::<usize>();
        let refresh_queue_depth = lane_pressure
            .iter()
            .map(|lane| lane.refresh_queue_depth)
            .sum::<usize>();
        let oldest_document_age_hours = self
            .documents
            .values()
            .map(|doc| age_hours(now, doc.last_seen_at))
            .max()
            .unwrap_or(0);

        SearchAcquisitionStats {
            total_documents: self.documents.len(),
            total_ingestions: self.total_ingestions,
            total_recalls: self.total_recalls,
            last_updated_at: self.last_updated_at,
            stale_documents,
            refresh_queue_depth,
            oldest_document_age_hours,
            suggested_queries: build_suggested_queries(&self.documents, now),
            lane_pressure,
        }
    }

    #[must_use]
    pub fn refresh_plan(&self, limit: usize) -> SearchAcquisitionRefreshPlan {
        let now = now_epoch_secs();
        let tasks = build_refresh_tasks(&self.documents, now, limit.max(1));
        SearchAcquisitionRefreshPlan {
            generated_at: now,
            total_tasks: tasks.len(),
            total_stale_documents: tasks.iter().map(|task| task.stale_documents).sum(),
            suggested_queries: build_suggested_queries(&self.documents, now),
            tasks,
        }
    }

    fn note_mutation(&mut self, operations: u64, timestamp: u64) {
        self.pending_persist_ops = self.pending_persist_ops.saturating_add(operations.max(1));
        self.last_updated_at = timestamp;
        self.persist_best_effort(false, timestamp);
    }

    fn persist_best_effort(&mut self, force: bool, timestamp: u64) {
        let Some(path) = self.persistence_path.as_ref() else {
            return;
        };
        if self.pending_persist_ops == 0 {
            return;
        }
        if !force
            && self.pending_persist_ops < self.persist_batch_size
            && timestamp.saturating_sub(self.last_persisted_at) < self.persist_interval_secs
        {
            return;
        }

        let snapshot = SearchIndexSnapshotRef {
            documents: &self.documents,
            total_ingestions: self.total_ingestions,
            total_recalls: self.total_recalls,
            last_updated_at: self.last_updated_at,
        };
        let Ok(raw) = serde_json::to_vec(&snapshot) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        if fs::write(path, raw).is_ok() {
            self.pending_persist_ops = 0;
            self.last_persisted_at = timestamp;
        }
    }
}

impl Default for SearchAcquisitionIndex {
    fn default() -> Self {
        Self::new()
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(ToString::to_string)
        .collect()
}

fn hydrate_documents(documents: &mut HashMap<String, IndexedSearchDocument>) {
    for (canonical_url, document) in documents.iter_mut() {
        if document.canonical_url.is_empty() {
            document.canonical_url = canonical_url.clone();
        }
        if document.url.is_empty() {
            document.url = document.canonical_url.clone();
        }
        hydrate_document(document);
    }
}

fn hydrate_document(document: &mut IndexedSearchDocument) {
    document.search_haystack = build_search_haystack(document);
}

fn build_search_haystack(document: &IndexedSearchDocument) -> String {
    let mut haystack = String::with_capacity(
        document.title.len()
            + document.domain.len()
            + document.snippet.len()
            + document
                .evidence_tags
                .iter()
                .map(String::len)
                .sum::<usize>()
            + 8,
    );
    haystack.push_str(&document.title.to_lowercase());
    haystack.push(' ');
    haystack.push_str(&document.domain.to_lowercase());
    haystack.push(' ');
    haystack.push_str(&document.snippet.to_lowercase());
    if !document.evidence_tags.is_empty() {
        haystack.push(' ');
        haystack.push_str(&document.evidence_tags.join(" ").to_lowercase());
    }
    haystack
}

fn enterprise_recall_score(
    doc: &IndexedSearchDocument,
    overlap_ratio: f64,
    recall_bonus: f64,
    diversity_penalty: f64,
    now: u64,
) -> f64 {
    let target = refresh_target_hours(doc.source_class.clone()).max(1) as f64;
    let age_pressure = (age_hours(now, doc.last_seen_at) as f64 / target).clamp(0.0, 3.0);
    let freshness = (doc.freshness_score * 0.65 + (1.0 - age_pressure / 3.0) * 0.35)
        .clamp(0.0, 1.0);
    let independence = if doc.recall_count == 0 {
        0.95
    } else {
        (0.95 - (doc.recall_count as f64 * 0.02)).clamp(0.65, 0.95)
    };
    let evidence_weight = (1.15 * doc.credibility_score
        + 0.95 * doc.relevance_score
        + 0.55 * freshness
        + 0.35 * independence)
        .exp();
    let normalized_evidence = evidence_weight / (evidence_weight + 8.0);
    let raw = overlap_ratio * 0.34
        + doc.relevance_score * 0.22
        + doc.credibility_score * 0.16
        + freshness * 0.11
        + doc.depth_signal * 0.08
        + normalized_evidence * 0.06
        + recall_bonus;
    (raw * (-0.25 * diversity_penalty.max(0.0)).exp()).clamp(0.0, 1.0)
}

fn index_diversity_penalty<'a>(
    documents: impl IntoIterator<Item = &'a IndexedSearchDocument>,
) -> f64 {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut total = 0_usize;
    for doc in documents {
        total += 1;
        *counts.entry(doc.source_class.as_str().to_string()).or_default() += 1;
    }
    if total == 0 {
        return 1.0;
    }
    counts
        .values()
        .map(|count| (*count as f64 / total as f64 - 0.65).max(0.0))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

fn sorted_tags(tags: HashSet<String>) -> Vec<String> {
    let mut tags = tags.into_iter().collect::<Vec<_>>();
    tags.sort();
    tags
}

fn canonicalize_search_url(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw) else {
        return raw.trim().to_string();
    };

    let host = url.host_str().map(|value| value.to_lowercase());
    let _ = url.set_scheme(&url.scheme().to_lowercase());
    if let Some(host) = host {
        let _ = url.set_host(Some(&host));
    }

    url.set_fragment(None);

    let retained = url
        .query_pairs()
        .filter(|(key, _)| {
            let lower = key.to_ascii_lowercase();
            !matches!(
                lower.as_str(),
                "utm_source"
                    | "utm_medium"
                    | "utm_campaign"
                    | "utm_term"
                    | "utm_content"
                    | "gclid"
                    | "fbclid"
                    | "mc_cid"
                    | "mc_eid"
                    | "ref"
            )
        })
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();

    url.set_query(None);
    if !retained.is_empty() {
        for (key, value) in retained {
            url.query_pairs_mut().append_pair(&key, &value);
        }
    }

    if url.path().ends_with('/') && url.path() != "/" {
        let trimmed = url.path().trim_end_matches('/').to_string();
        url.set_path(&trimmed);
    }

    url.to_string()
}

fn build_lane_pressure(
    documents: &HashMap<String, IndexedSearchDocument>,
    now: u64,
) -> Vec<SearchFreshnessLaneStats> {
    let mut per_lane = BTreeMap::<String, Vec<&IndexedSearchDocument>>::new();
    for doc in documents.values() {
        per_lane
            .entry(doc.source_class.as_str().to_string())
            .or_default()
            .push(doc);
    }

    per_lane
        .into_iter()
        .map(|(lane, docs)| {
            let target_refresh_hours = refresh_target_hours(docs[0].source_class.clone());
            let mut stale_documents = 0_usize;
            let mut refresh_queue_depth = 0_usize;
            let mut oldest_age_hours = 0_u64;

            for doc in docs.iter().copied() {
                let age = age_hours(now, doc.last_seen_at);
                oldest_age_hours = oldest_age_hours.max(age);
                if age >= target_refresh_hours {
                    stale_documents += 1;
                }
                if age >= ((target_refresh_hours as f64) * 0.75).floor() as u64 {
                    refresh_queue_depth += 1;
                }
            }

            SearchFreshnessLaneStats {
                lane,
                documents: docs.len(),
                stale_documents,
                refresh_queue_depth,
                target_refresh_hours,
                oldest_age_hours,
            }
        })
        .collect()
}

fn build_suggested_queries(
    documents: &HashMap<String, IndexedSearchDocument>,
    now: u64,
) -> Vec<String> {
    let mut weights = BTreeMap::<String, (u64, u64)>::new();
    for doc in documents.values() {
        let age = age_hours(now, doc.last_seen_at);
        let target = refresh_target_hours(doc.source_class.clone());
        if age < ((target as f64) * 0.75).floor() as u64 || doc.primary_query.trim().is_empty() {
            continue;
        }

        let entry = weights.entry(doc.primary_query.clone()).or_insert((0, 0));
        entry.0 += age.max(1);
        entry.1 += 1;
    }

    let mut ranked = weights.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
             .0
            .cmp(&left.1 .0)
            .then_with(|| right.1 .1.cmp(&left.1 .1))
    });

    ranked.into_iter().take(4).map(|(query, _)| query).collect()
}

fn build_refresh_tasks(
    documents: &HashMap<String, IndexedSearchDocument>,
    now: u64,
    limit: usize,
) -> Vec<SearchAcquisitionRefreshTask> {
    let mut groups = BTreeMap::<(String, String), Vec<&IndexedSearchDocument>>::new();
    for doc in documents.values() {
        let query = doc.primary_query.trim();
        if query.is_empty() {
            continue;
        }

        groups
            .entry((query.to_string(), doc.source_class.as_str().to_string()))
            .or_default()
            .push(doc);
    }

    let mut tasks = groups
        .into_iter()
        .filter_map(|((query, lane), docs)| {
            let source_class = docs[0].source_class.clone();
            let target_refresh_hours = refresh_target_hours(source_class.clone());
            let mut stale_documents = 0_usize;
            let mut refresh_queue_depth = 0_usize;
            let mut oldest_age_hours = 0_u64;
            let mut sample_urls = Vec::new();

            for doc in docs.iter().copied() {
                let age = age_hours(now, doc.last_seen_at);
                oldest_age_hours = oldest_age_hours.max(age);
                if age >= target_refresh_hours {
                    stale_documents += 1;
                }
                if age >= ((target_refresh_hours as f64) * 0.75).floor() as u64 {
                    refresh_queue_depth += 1;
                    if sample_urls.len() < 3 {
                        sample_urls.push(doc.canonical_url.clone());
                    }
                }
            }

            if refresh_queue_depth == 0 {
                return None;
            }

            let documents = docs.len();
            let stale_ratio = stale_documents as f64 / documents as f64;
            let queue_ratio = refresh_queue_depth as f64 / documents as f64;
            let age_pressure = (oldest_age_hours as f64 / target_refresh_hours.max(1) as f64)
                .clamp(0.0, 3.0)
                / 3.0;
            let depth_bias = match source_class {
                SearchSourceClass::News | SearchSourceClass::Social => 0.06,
                SearchSourceClass::Docs | SearchSourceClass::Papers => 0.09,
                SearchSourceClass::Books => 0.04,
                SearchSourceClass::Web => 0.05,
            };
            let urgency_score =
                (stale_ratio * 0.45 + queue_ratio * 0.22 + age_pressure * 0.24 + depth_bias)
                    .clamp(0.0, 1.0);

            Some(SearchAcquisitionRefreshTask {
                query,
                lane,
                source_class,
                documents,
                stale_documents,
                refresh_queue_depth,
                target_refresh_hours,
                oldest_age_hours,
                urgency_score,
                sample_urls,
            })
        })
        .collect::<Vec<_>>();

    tasks.sort_by(|left, right| {
        right
            .urgency_score
            .partial_cmp(&left.urgency_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.stale_documents.cmp(&left.stale_documents))
            .then_with(|| right.oldest_age_hours.cmp(&left.oldest_age_hours))
            .then_with(|| right.refresh_queue_depth.cmp(&left.refresh_queue_depth))
    });
    tasks.truncate(limit);
    tasks
}

fn refresh_target_hours(class: SearchSourceClass) -> u64 {
    match class {
        SearchSourceClass::Social => 18,
        SearchSourceClass::News => 24,
        SearchSourceClass::Web => 72,
        SearchSourceClass::Docs => 120,
        SearchSourceClass::Papers => 168,
        SearchSourceClass::Books => 720,
    }
}

fn age_hours(now: u64, timestamp: u64) -> u64 {
    now.saturating_sub(timestamp) / 3600
}

fn default_index_path() -> PathBuf {
    if let Some(raw) = std::env::var_os("ASTRA_SEARCH_INDEX_PATH") {
        return PathBuf::from(raw);
    }

    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return Path::new(&local_app_data)
            .join("Astra")
            .join("search")
            .join("acquisition_index.json");
    }

    std::env::temp_dir()
        .join("astra")
        .join("search")
        .join("acquisition_index.json")
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::web_search::{SearchProvider, SearchResult};

    #[test]
    fn acquisition_index_recalls_related_documents() {
        let mut index = SearchAcquisitionIndex::new_ephemeral();
        index.ingest_search_results(
            &SearchQuery {
                query: "sovereign browser search".into(),
                provider: SearchProvider::Aggregated,
                max_results: 4,
                language: "en".into(),
                region: None,
                safe_search: true,
                time_range: None,
                source_classes: vec![SearchSourceClass::Docs],
                deep_reasoning: true,
            },
            &[SearchResult {
                title: "Sovereign browser search architecture".into(),
                url: "https://docs.example.com/sovereign-browser-search?utm_source=test".into(),
                domain: "docs.example.com".into(),
                snippet: "Indexed evidence retrieval for sovereign browsing.".into(),
                relevance_score: 0.91,
                credibility_score: 0.95,
                freshness_score: 0.66,
                depth_signal: 0.88,
                source: SearchProvider::Aggregated,
                source_class: SearchSourceClass::Docs,
                evidence_tags: vec!["docs".into(), "evidence".into()],
            }],
        );

        let hits = index.query(
            "browser evidence architecture",
            &[SearchSourceClass::Docs],
            3,
        );

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document.domain, "docs.example.com");
        assert_eq!(
            hits[0].document.url,
            "https://docs.example.com/sovereign-browser-search"
        );
        assert!(hits[0].recall_score > 0.6);
        assert_eq!(index.stats().total_documents, 1);
    }

    #[test]
    fn acquisition_stats_surface_stale_lane_pressure_and_suggested_queries() {
        let mut index = SearchAcquisitionIndex::new_ephemeral();
        let now = now_epoch_secs();
        index.ingest_external_documents([
            IndexedSearchDocument {
                url: "https://news.example.com/market-update?utm_source=feed".into(),
                canonical_url: String::new(),
                title: "Market update".into(),
                domain: "news.example.com".into(),
                snippet: "Fast-changing market coverage.".into(),
                primary_query: "latest market update".into(),
                source_class: SearchSourceClass::News,
                relevance_score: 0.84,
                credibility_score: 0.81,
                freshness_score: 0.95,
                depth_signal: 0.63,
                evidence_tags: vec!["news".into()],
                queries_seen: 1,
                recall_count: 0,
                first_seen_at: now.saturating_sub(96 * 3600),
                last_seen_at: now.saturating_sub(96 * 3600),
                search_haystack: String::new(),
            },
            IndexedSearchDocument {
                url: "https://docs.example.com/spec".into(),
                canonical_url: String::new(),
                title: "Spec".into(),
                domain: "docs.example.com".into(),
                snippet: "Stable protocol documentation.".into(),
                primary_query: "browser protocol spec".into(),
                source_class: SearchSourceClass::Docs,
                relevance_score: 0.92,
                credibility_score: 0.97,
                freshness_score: 0.7,
                depth_signal: 0.9,
                evidence_tags: vec!["docs".into()],
                queries_seen: 1,
                recall_count: 0,
                first_seen_at: now.saturating_sub(24 * 3600),
                last_seen_at: now.saturating_sub(24 * 3600),
                search_haystack: String::new(),
            },
        ]);

        let stats = index.stats();

        assert_eq!(stats.total_documents, 2);
        assert!(stats.stale_documents >= 1);
        assert!(stats.refresh_queue_depth >= 1);
        assert!(stats
            .lane_pressure
            .iter()
            .any(|lane| lane.lane == "news" && lane.stale_documents >= 1));
        assert!(stats
            .suggested_queries
            .contains(&"latest market update".to_string()));
    }

    #[test]
    fn refresh_plan_prioritizes_stale_queries_by_lane() {
        let mut index = SearchAcquisitionIndex::new_ephemeral();
        let now = now_epoch_secs();
        index.ingest_external_documents([
            IndexedSearchDocument {
                url: "https://news.example.com/market".into(),
                canonical_url: String::new(),
                title: "Market update".into(),
                domain: "news.example.com".into(),
                snippet: "Fast-changing market coverage.".into(),
                primary_query: "browser protocol market outlook".into(),
                source_class: SearchSourceClass::News,
                relevance_score: 0.84,
                credibility_score: 0.81,
                freshness_score: 0.95,
                depth_signal: 0.63,
                evidence_tags: vec!["news".into()],
                queries_seen: 1,
                recall_count: 0,
                first_seen_at: now.saturating_sub(96 * 3600),
                last_seen_at: now.saturating_sub(96 * 3600),
                search_haystack: String::new(),
            },
            IndexedSearchDocument {
                url: "https://docs.example.com/spec".into(),
                canonical_url: String::new(),
                title: "Spec".into(),
                domain: "docs.example.com".into(),
                snippet: "Stable protocol documentation.".into(),
                primary_query: "browser protocol spec".into(),
                source_class: SearchSourceClass::Docs,
                relevance_score: 0.92,
                credibility_score: 0.97,
                freshness_score: 0.7,
                depth_signal: 0.9,
                evidence_tags: vec!["docs".into()],
                queries_seen: 1,
                recall_count: 0,
                first_seen_at: now.saturating_sub(24 * 3600),
                last_seen_at: now.saturating_sub(24 * 3600),
                search_haystack: String::new(),
            },
        ]);

        let plan = index.refresh_plan(8);

        assert_eq!(plan.total_tasks, 1);
        assert_eq!(plan.tasks[0].query, "browser protocol market outlook");
        assert_eq!(plan.tasks[0].lane, "news");
        assert!(plan.tasks[0].urgency_score > 0.4);
        assert!(plan
            .suggested_queries
            .iter()
            .any(|query| query == "browser protocol market outlook"));
    }

    #[test]
    fn external_documents_are_queryable_without_rebuilding_text_per_lookup() {
        let mut index = SearchAcquisitionIndex::new_ephemeral();
        index.ingest_external_documents([IndexedSearchDocument {
            url: "https://docs.example.com/symbol-cortex".into(),
            canonical_url: String::new(),
            title: "Symbol cortex architecture".into(),
            domain: "docs.example.com".into(),
            snippet: "Typed symbolic expressions for deterministic recall.".into(),
            primary_query: "symbol cortex".into(),
            source_class: SearchSourceClass::Docs,
            relevance_score: 0.94,
            credibility_score: 0.98,
            freshness_score: 0.7,
            depth_signal: 0.92,
            evidence_tags: vec!["docs".into(), "symbolic".into()],
            queries_seen: 1,
            recall_count: 0,
            first_seen_at: now_epoch_secs(),
            last_seen_at: now_epoch_secs(),
            search_haystack: String::new(),
        }]);

        let hits = index.query("typed symbolic recall", &[SearchSourceClass::Docs], 2);

        assert_eq!(hits.len(), 1);
        assert!(hits[0].recall_score > 0.5);
        assert!(hits[0].document.search_haystack.contains("typed symbolic"));
    }
}

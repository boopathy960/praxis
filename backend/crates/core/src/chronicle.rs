// ─────────────────────────────────────────────────────────────
// Chronicle — tamper-evident personal continuity engine
// ─────────────────────────────────────────────────────────────
// Every system (and every person) suffers continuity loss: decisions are made
// and the rationale evaporates, lessons are learned and buried, commitments
// slip silently. In this backend, every subsystem produces receipts that sink
// into history — nothing remembers across subsystems, and the human carries
// the whole memory burden.
//
// Chronicle is the durable memory layer that closes that gap:
//
//   * **Episodes** — decisions (with rationale), learnings, events, and
//     commitments are recorded with provenance (which subsystem, which
//     record) and hash-chained so history is tamper-evident, like the
//     guardian and sandbox ledgers.
//   * **Lifecycle** — a consolidation tick decays importance over time,
//     archives what stopped mattering (never deleting: the chain stays
//     intact), and promotes recurring themes into durable *insights*.
//   * **Recall** — natural-language queries are scored against episodes and
//     insights by term match, importance, and recency, answering "what did I
//     learn / decide about X, and why?" with provenance.
//   * **Commitments** — episodes with due dates surface as overdue/upcoming
//     until explicitly resolved; unresolved commitments never decay away.
//   * **The Brief** — one synthesis of what matters now: overdue and upcoming
//     commitments, recent decisions, top insights, recent failures.
//
// Weave and the autonomy worker feed Chronicle automatically, so memory
// accrues with zero user effort. Storage is per-service SQLite under
// `ASTRA_DATA_DIR/chronicle`, so memory survives restarts.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, TenantScope, new_id, now_ms};

/// Total stored episodes cap to bound memory and the chain walk.
const MAX_EPISODES: usize = 4_096;
const MAX_CONTENT_LEN: usize = 4_096;
/// Importance multiplier applied by each consolidation tick.
const DECAY_FACTOR: f64 = 0.9;
/// Episodes decaying below this are archived (unresolved commitments never are).
const ARCHIVE_THRESHOLD: f64 = 0.05;
/// A theme shared by at least this many episodes is promoted to an insight.
const INSIGHT_MIN_EPISODES: usize = 3;

const STOPWORDS: &[&str] = &[
    "about", "after", "again", "across", "their", "there", "these", "thing", "those",
    "through", "under", "where", "which", "while", "with", "would", "into", "from",
    "that", "this", "have", "will", "your", "they", "them", "were", "been", "what",
    "when", "every", "should", "could", "using", "between",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeKind {
    /// A choice that was made, ideally with its rationale.
    Decision,
    /// Something learned that should inform future work.
    Learning,
    /// Something that happened (executions, woven intents, incidents).
    Event,
    /// Something promised, with an optional due date; tracked until resolved.
    Commitment,
}

impl EpisodeKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Learning => "learning",
            Self::Event => "event",
            Self::Commitment => "commitment",
        }
    }
}

/// One remembered moment. A plain episodic record (the hash chain was removed);
/// lifecycle fields (importance, resolved, archived) mutate freely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub episode_id: String,
    pub tenant_scope: TenantScope,
    pub kind: EpisodeKind,
    pub content: String,
    #[serde(default)]
    pub rationale: Option<String>,
    /// Which subsystem (or "user") recorded this.
    pub source: String,
    /// Identifier of the originating record (intent id, job id, ...).
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Salience in [0, 1]; decays each consolidation tick.
    pub importance: f64,
    #[serde(default)]
    pub due_at_ms: Option<i64>,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub archived: bool,
    /// Insertion order index (0-based).
    pub chain_index: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordEpisodeRequest {
    pub kind: EpisodeKind,
    pub content: String,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub importance: Option<f64>,
    #[serde(default)]
    pub due_at_ms: Option<i64>,
    #[serde(default = "default_scope")]
    pub tenant_scope: TenantScope,
}

fn default_scope() -> TenantScope {
    TenantScope::Global
}

/// A recurring theme distilled from multiple episodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub insight_id: String,
    pub theme: String,
    pub episode_count: usize,
    pub episode_ids: Vec<String>,
    pub summary: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecallRequest {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecallMatch {
    pub score: f64,
    pub episode: Episode,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecallResponse {
    pub query: String,
    pub matches: Vec<RecallMatch>,
    pub related_insights: Vec<Insight>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommitmentView {
    pub overdue: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_in_ms: Option<i64>,
    pub episode: Episode,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsolidationReport {
    pub decayed: usize,
    pub archived: usize,
    pub insights_promoted: usize,
    pub total_insights: usize,
}


#[derive(Debug, Clone, Serialize)]
pub struct ChronicleStats {
    pub total_episodes: usize,
    pub active_episodes: usize,
    pub archived_episodes: usize,
    pub open_commitments: usize,
    pub overdue_commitments: usize,
    pub insights: usize,
    pub by_kind: BTreeMap<String, usize>,
}

/// What matters now, synthesized from memory.
#[derive(Debug, Clone, Serialize)]
pub struct ChronicleBrief {
    pub generated_at_ms: i64,
    pub overdue_commitments: Vec<CommitmentView>,
    pub upcoming_commitments: Vec<CommitmentView>,
    pub recent_decisions: Vec<Episode>,
    pub top_insights: Vec<Insight>,
    pub recent_failures: Vec<Episode>,
}

#[derive(Default)]
struct ChronicleStore {
    episodes: BTreeMap<String, Episode>,
    /// Episode ids in chain order.
    order: Vec<String>,
    insights: BTreeMap<String, Insight>,
}

#[derive(Clone)]
pub struct ChronicleService {
    store: Shared<ChronicleStore>,
    db: Option<Arc<Mutex<Connection>>>,
}

impl ChronicleService {
    /// In-memory chronicle (tests and embedded use); nothing survives drop.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Shared::new(RwLock::new(ChronicleStore::default())),
            db: None,
        }
    }

    /// Durable chronicle backed by SQLite under `data_dir`. Existing episodes
    /// and insights are loaded so memory survives restarts.
    pub fn with_data_dir(data_dir: impl AsRef<Path>) -> Result<Self, AppError> {
        let store_path = data_dir.as_ref().join("chronicle.sqlite");
        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::Internal(format!("failed to create chronicle data directory: {error}"))
            })?;
        }
        let connection = Connection::open(&store_path).map_err(|error| {
            AppError::Internal(format!("failed to open chronicle store: {error}"))
        })?;
        connection
            .execute_batch(
                "
                PRAGMA journal_mode=WAL;
                PRAGMA synchronous=NORMAL;
                CREATE TABLE IF NOT EXISTS chronicle_episodes (
                    episode_id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS chronicle_insights (
                    insight_id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                ",
            )
            .map_err(|error| {
                AppError::Internal(format!("failed to initialize chronicle store: {error}"))
            })?;

        let mut store = ChronicleStore::default();
        {
            let mut statement = connection
                .prepare("SELECT payload FROM chronicle_episodes")
                .map_err(|error| AppError::Internal(format!("chronicle load failed: {error}")))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| AppError::Internal(format!("chronicle load failed: {error}")))?;
            let mut episodes: Vec<Episode> = Vec::new();
            for payload in rows.flatten() {
                if let Ok(episode) = serde_json::from_str::<Episode>(&payload) {
                    episodes.push(episode);
                }
            }
            episodes.sort_by_key(|episode| episode.chain_index);
            for episode in episodes {
                store.order.push(episode.episode_id.clone());
                store.episodes.insert(episode.episode_id.clone(), episode);
            }
        }
        {
            let mut statement = connection
                .prepare("SELECT payload FROM chronicle_insights")
                .map_err(|error| AppError::Internal(format!("chronicle load failed: {error}")))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|error| AppError::Internal(format!("chronicle load failed: {error}")))?;
            for payload in rows.flatten() {
                if let Ok(insight) = serde_json::from_str::<Insight>(&payload) {
                    // The in-memory map is keyed by theme so consolidation can
                    // refresh an existing insight instead of duplicating it.
                    store.insights.insert(insight.theme.clone(), insight);
                }
            }
        }

        Ok(Self {
            store: Shared::new(RwLock::new(store)),
            db: Some(Arc::new(Mutex::new(connection))),
        })
    }

    /// Records one episode.
    pub fn record(&self, request: RecordEpisodeRequest) -> Result<Episode, AppError> {
        let content = request.content.trim();
        if content.is_empty() {
            return Err(AppError::Validation(
                "chronicle episode content must not be empty".into(),
            ));
        }
        if content.len() > MAX_CONTENT_LEN {
            return Err(AppError::Validation("chronicle episode content is too long".into()));
        }
        let importance = request.importance.unwrap_or(0.5);
        if !importance.is_finite() || !(0.0..=1.0).contains(&importance) {
            return Err(AppError::Validation(
                "chronicle episode importance must be within [0, 1]".into(),
            ));
        }

        let mut store = self.store.write();
        if store.episodes.len() >= MAX_EPISODES {
            return Err(AppError::Validation(format!(
                "chronicle episode cap of {MAX_EPISODES} reached"
            )));
        }
        let chain_index = match store.order.last() {
            Some(last_id) => store.episodes[last_id].chain_index + 1,
            None => 0,
        };
        let now = now_ms();
        let episode_id = new_id("episode");
        let source = request.source.unwrap_or_else(|| "user".into());
        let tags: Vec<String> = request
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_ascii_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();
        let episode = Episode {
            episode_id: episode_id.clone(),
            tenant_scope: request.tenant_scope,
            kind: request.kind,
            content: content.to_string(),
            rationale: request.rationale,
            source,
            source_ref: request.source_ref,
            tags,
            importance,
            due_at_ms: request.due_at_ms,
            resolved: false,
            archived: false,
            chain_index,
            created_at_ms: now,
            updated_at_ms: now,
        };
        store.order.push(episode_id.clone());
        store.episodes.insert(episode_id, episode.clone());
        drop(store);
        self.persist_episode(&episode)?;
        Ok(episode)
    }

    #[must_use]
    pub fn list_episodes(&self) -> Vec<Episode> {
        let mut episodes: Vec<_> = self.store.read().episodes.values().cloned().collect();
        episodes.sort_by_key(|episode| std::cmp::Reverse(episode.created_at_ms));
        episodes
    }

    pub fn get_episode(&self, episode_id: &str) -> Result<Episode, AppError> {
        self.store
            .read()
            .episodes
            .get(episode_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("chronicle episode {episode_id}")))
    }

    /// Scores active episodes against a natural-language query by term match,
    /// importance, and recency; surfaces related insights alongside.
    pub fn recall(&self, request: RecallRequest) -> Result<RecallResponse, AppError> {
        let query = request.query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return Err(AppError::Validation("recall query must not be empty".into()));
        }
        let terms = significant_tokens(&query);
        let terms = if terms.is_empty() {
            // Very short queries still deserve an answer: match raw words.
            query.split_whitespace().map(str::to_string).collect()
        } else {
            terms
        };
        let limit = request.limit.unwrap_or(10).clamp(1, 50);
        let now = now_ms();

        let store = self.store.read();
        let mut matches: Vec<RecallMatch> = store
            .episodes
            .values()
            .filter(|episode| !episode.archived)
            .filter_map(|episode| {
                let haystack = episode_text(episode);
                let matched = terms.iter().filter(|term| haystack.contains(*term)).count();
                if matched == 0 {
                    return None;
                }
                let term_score = matched as f64 / terms.len() as f64;
                let age_days = ((now - episode.updated_at_ms).max(0)) as f64 / 86_400_000.0;
                let recency = 1.0 / (1.0 + age_days * 0.05);
                let score = term_score * (0.5 + episode.importance) * recency;
                Some(RecallMatch {
                    score: (score * 1_000.0).round() / 1_000.0,
                    episode: episode.clone(),
                })
            })
            .collect();
        matches.sort_by(|a, b| b.score.total_cmp(&a.score));
        matches.truncate(limit);

        let related_insights: Vec<Insight> = store
            .insights
            .values()
            .filter(|insight| terms.iter().any(|term| insight.theme.contains(term)))
            .cloned()
            .collect();

        Ok(RecallResponse {
            query,
            matches,
            related_insights,
        })
    }

    /// Unresolved commitments, soonest due first (open-ended ones last).
    #[must_use]
    pub fn commitments(&self) -> Vec<CommitmentView> {
        let now = now_ms();
        let mut views: Vec<CommitmentView> = self
            .store
            .read()
            .episodes
            .values()
            .filter(|episode| {
                episode.kind == EpisodeKind::Commitment && !episode.resolved && !episode.archived
            })
            .map(|episode| CommitmentView {
                overdue: episode.due_at_ms.is_some_and(|due| due < now),
                due_in_ms: episode.due_at_ms.map(|due| due - now),
                episode: episode.clone(),
            })
            .collect();
        views.sort_by_key(|view| view.episode.due_at_ms.unwrap_or(i64::MAX));
        views
    }

    pub fn resolve_commitment(&self, episode_id: &str) -> Result<Episode, AppError> {
        let episode = {
            let mut store = self.store.write();
            let episode = store
                .episodes
                .get_mut(episode_id)
                .ok_or_else(|| AppError::NotFound(format!("chronicle episode {episode_id}")))?;
            if episode.kind != EpisodeKind::Commitment {
                return Err(AppError::Validation(format!(
                    "episode {episode_id} is not a commitment"
                )));
            }
            episode.resolved = true;
            episode.updated_at_ms = now_ms();
            episode.clone()
        };
        self.persist_episode(&episode)?;
        Ok(episode)
    }

    /// One lifecycle tick: decay importance, archive what stopped mattering
    /// (unresolved commitments are exempt), and promote recurring themes into
    /// insights. Deterministic, so its effects are testable and auditable.
    pub fn consolidate(&self) -> Result<ConsolidationReport, AppError> {
        let now = now_ms();
        let mut changed: Vec<Episode> = Vec::new();
        let mut decayed = 0;
        let mut archived = 0;

        let mut store = self.store.write();
        for episode in store.episodes.values_mut() {
            if episode.archived {
                continue;
            }
            episode.importance = (episode.importance * DECAY_FACTOR).clamp(0.0, 1.0);
            episode.updated_at_ms = now;
            decayed += 1;
            let protected = episode.kind == EpisodeKind::Commitment && !episode.resolved;
            if episode.importance < ARCHIVE_THRESHOLD && !protected {
                episode.archived = true;
                archived += 1;
            }
            changed.push(episode.clone());
        }

        // Theme promotion: a significant token shared by enough active
        // episodes becomes (or refreshes) an insight.
        let mut themes: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for episode in store.episodes.values() {
            if episode.archived {
                continue;
            }
            let mut tokens = significant_tokens(&episode_text(episode));
            tokens.sort();
            tokens.dedup();
            for token in tokens {
                themes.entry(token).or_default().push(episode.episode_id.clone());
            }
        }
        let mut insights_promoted = 0;
        let mut changed_insights: Vec<Insight> = Vec::new();
        for (theme, episode_ids) in themes {
            if episode_ids.len() < INSIGHT_MIN_EPISODES {
                continue;
            }
            insights_promoted += 1;
            let summary = format!(
                "theme '{theme}' recurs across {} remembered episodes",
                episode_ids.len()
            );
            let entry = store
                .insights
                .entry(theme.clone())
                .and_modify(|insight| {
                    insight.episode_count = episode_ids.len();
                    insight.episode_ids = episode_ids.clone();
                    insight.summary = summary.clone();
                    insight.updated_at_ms = now;
                })
                .or_insert_with(|| Insight {
                    insight_id: new_id("insight"),
                    theme,
                    episode_count: episode_ids.len(),
                    episode_ids,
                    summary,
                    created_at_ms: now,
                    updated_at_ms: now,
                });
            changed_insights.push(entry.clone());
        }
        let total_insights = store.insights.len();
        drop(store);

        for episode in &changed {
            self.persist_episode(episode)?;
        }
        for insight in &changed_insights {
            self.persist_insight(insight)?;
        }

        Ok(ConsolidationReport {
            decayed,
            archived,
            insights_promoted,
            total_insights,
        })
    }

    #[must_use]
    pub fn stats(&self) -> ChronicleStats {
        let now = now_ms();
        let store = self.store.read();
        let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
        let mut archived = 0;
        let mut open_commitments = 0;
        let mut overdue_commitments = 0;
        for episode in store.episodes.values() {
            *by_kind.entry(episode.kind.as_str().to_string()).or_default() += 1;
            if episode.archived {
                archived += 1;
            }
            if episode.kind == EpisodeKind::Commitment && !episode.resolved && !episode.archived {
                open_commitments += 1;
                if episode.due_at_ms.is_some_and(|due| due < now) {
                    overdue_commitments += 1;
                }
            }
        }
        ChronicleStats {
            total_episodes: store.episodes.len(),
            active_episodes: store.episodes.len() - archived,
            archived_episodes: archived,
            open_commitments,
            overdue_commitments,
            insights: store.insights.len(),
            by_kind,
        }
    }

    /// Synthesizes what matters now from memory alone (the route layer adds
    /// live subsystem stats around it).
    #[must_use]
    pub fn brief(&self) -> ChronicleBrief {
        let commitments = self.commitments();
        let (overdue, upcoming): (Vec<_>, Vec<_>) =
            commitments.into_iter().partition(|view| view.overdue);
        let store = self.store.read();
        let mut decisions: Vec<Episode> = store
            .episodes
            .values()
            .filter(|episode| episode.kind == EpisodeKind::Decision && !episode.archived)
            .cloned()
            .collect();
        decisions.sort_by_key(|episode| std::cmp::Reverse(episode.created_at_ms));
        decisions.truncate(5);
        let mut failures: Vec<Episode> = store
            .episodes
            .values()
            .filter(|episode| !episode.archived && episode.tags.iter().any(|tag| tag == "failure"))
            .cloned()
            .collect();
        failures.sort_by_key(|episode| std::cmp::Reverse(episode.created_at_ms));
        failures.truncate(5);
        let mut insights: Vec<Insight> = store.insights.values().cloned().collect();
        insights.sort_by_key(|insight| std::cmp::Reverse(insight.episode_count));
        insights.truncate(5);
        ChronicleBrief {
            generated_at_ms: now_ms(),
            overdue_commitments: overdue,
            upcoming_commitments: upcoming.into_iter().take(5).collect(),
            recent_decisions: decisions,
            top_insights: insights,
            recent_failures: failures,
        }
    }

    fn persist_episode(&self, episode: &Episode) -> Result<(), AppError> {
        self.persist("chronicle_episodes", "episode_id", &episode.episode_id, episode)
    }

    fn persist_insight(&self, insight: &Insight) -> Result<(), AppError> {
        self.persist("chronicle_insights", "insight_id", &insight.insight_id, insight)
    }

    fn persist<T: Serialize>(
        &self,
        table: &str,
        id_column: &str,
        id: &str,
        value: &T,
    ) -> Result<(), AppError> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        let payload = serde_json::to_string(value).map_err(|error| {
            AppError::Internal(format!("chronicle serialization failed: {error}"))
        })?;
        let sql = format!(
            "INSERT OR REPLACE INTO {table} ({id_column}, payload, created_at_ms) VALUES (?1, ?2, ?3)"
        );
        db.lock()
            .execute(&sql, params![id, payload, now_ms()])
            .map_err(|error| AppError::Internal(format!("chronicle persistence failed: {error}")))?;
        Ok(())
    }
}

impl Default for ChronicleService {
    fn default() -> Self {
        Self::new()
    }
}

fn episode_text(episode: &Episode) -> String {
    let mut text = episode.content.to_ascii_lowercase();
    if let Some(rationale) = &episode.rationale {
        text.push(' ');
        text.push_str(&rationale.to_ascii_lowercase());
    }
    for tag in &episode.tags {
        text.push(' ');
        text.push_str(tag);
    }
    text
}

/// Lowercased alphanumeric tokens of length >= 4 that are not stopwords.
fn significant_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|token| token.len() >= 4 && !STOPWORDS.contains(&token.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(
        service: &ChronicleService,
        kind: EpisodeKind,
        content: &str,
        importance: f64,
    ) -> Episode {
        service
            .record(RecordEpisodeRequest {
                kind,
                content: content.into(),
                rationale: None,
                source: None,
                source_ref: None,
                tags: Vec::new(),
                importance: Some(importance),
                due_at_ms: None,
                tenant_scope: TenantScope::Global,
            })
            .expect("record")
    }

    #[test]
    fn record_validates_content_and_importance() {
        let service = ChronicleService::new();
        assert!(
            service
                .record(RecordEpisodeRequest {
                    kind: EpisodeKind::Event,
                    content: "  ".into(),
                    rationale: None,
                    source: None,
                    source_ref: None,
                    tags: Vec::new(),
                    importance: None,
                    due_at_ms: None,
                    tenant_scope: TenantScope::Global,
                })
                .is_err()
        );
        assert!(
            service
                .record(RecordEpisodeRequest {
                    kind: EpisodeKind::Event,
                    content: "valid".into(),
                    rationale: None,
                    source: None,
                    source_ref: None,
                    tags: Vec::new(),
                    importance: Some(7.0),
                    due_at_ms: None,
                    tenant_scope: TenantScope::Global,
                })
                .is_err()
        );
    }

    #[test]
    fn episodes_are_ordered_by_index() {
        let service = ChronicleService::new();
        let first = record(&service, EpisodeKind::Event, "first remembered event", 0.5);
        let second = record(&service, EpisodeKind::Learning, "second remembered lesson", 0.5);
        assert_eq!(first.chain_index, 0);
        assert_eq!(second.chain_index, 1);
        service.consolidate().expect("consolidate");
    }

    #[test]
    fn recall_scores_by_relevance_importance_and_provenance() {
        let service = ChronicleService::new();
        record(
            &service,
            EpisodeKind::Decision,
            "decided to adopt sqlite persistence for chronicle storage",
            0.9,
        );
        record(
            &service,
            EpisodeKind::Event,
            "watched a movie about volcanoes",
            0.2,
        );
        let response = service
            .recall(RecallRequest {
                query: "why did we choose sqlite persistence?".into(),
                limit: None,
            })
            .expect("recall");
        assert_eq!(response.matches.len(), 1);
        assert!(response.matches[0].episode.content.contains("sqlite"));
        assert!(response.matches[0].score > 0.0);
        assert!(service.recall(RecallRequest { query: " ".into(), limit: None }).is_err());
    }

    #[test]
    fn commitments_surface_overdue_and_resolve() {
        let service = ChronicleService::new();
        let overdue = service
            .record(RecordEpisodeRequest {
                kind: EpisodeKind::Commitment,
                content: "send the quarterly report".into(),
                rationale: None,
                source: None,
                source_ref: None,
                tags: Vec::new(),
                importance: Some(0.8),
                due_at_ms: Some(now_ms() - 1_000),
                tenant_scope: TenantScope::Global,
            })
            .expect("record");
        record(&service, EpisodeKind::Event, "unrelated background event", 0.5);

        let views = service.commitments();
        assert_eq!(views.len(), 1);
        assert!(views[0].overdue);

        let resolved = service.resolve_commitment(&overdue.episode_id).expect("resolve");
        assert!(resolved.resolved);
        assert!(service.commitments().is_empty());
        // Only commitments can be resolved.
        let event = record(&service, EpisodeKind::Event, "another plain event", 0.5);
        assert!(service.resolve_commitment(&event.episode_id).is_err());
    }

    #[test]
    fn consolidation_decays_archives_and_promotes_insights() {
        let service = ChronicleService::new();
        for index in 0..3 {
            record(
                &service,
                EpisodeKind::Learning,
                &format!("kubernetes deployment lesson number {index}"),
                0.6,
            );
        }
        // A faint memory that should archive after one decay tick.
        record(&service, EpisodeKind::Event, "barely notable happening", 0.05);
        // An unresolved commitment must never archive, no matter how faint.
        service
            .record(RecordEpisodeRequest {
                kind: EpisodeKind::Commitment,
                content: "renew the domain registration".into(),
                rationale: None,
                source: None,
                source_ref: None,
                tags: Vec::new(),
                importance: Some(0.05),
                due_at_ms: None,
                tenant_scope: TenantScope::Global,
            })
            .expect("record");

        let report = service.consolidate().expect("consolidate");
        assert!(report.insights_promoted >= 1, "kubernetes theme should promote");
        assert_eq!(report.archived, 1, "only the faint event archives");
        assert_eq!(service.commitments().len(), 1, "commitment survives decay");

        let recall = service
            .recall(RecallRequest { query: "kubernetes".into(), limit: None })
            .expect("recall");
        assert!(!recall.related_insights.is_empty());
    }

    #[test]
    fn brief_synthesizes_what_matters_now() {
        let service = ChronicleService::new();
        service
            .record(RecordEpisodeRequest {
                kind: EpisodeKind::Commitment,
                content: "reply to the security audit".into(),
                rationale: None,
                source: None,
                source_ref: None,
                tags: Vec::new(),
                importance: Some(0.9),
                due_at_ms: Some(now_ms() - 60_000),
                tenant_scope: TenantScope::Global,
            })
            .expect("record");
        service
            .record(RecordEpisodeRequest {
                kind: EpisodeKind::Event,
                content: "weave intent failed to materialize".into(),
                rationale: None,
                source: Some("weave".into()),
                source_ref: Some("weave_x".into()),
                tags: vec!["failure".into()],
                importance: Some(0.7),
                due_at_ms: None,
                tenant_scope: TenantScope::Global,
            })
            .expect("record");
        record(
            &service,
            EpisodeKind::Decision,
            "decided to keep the sandbox spend gate",
            0.8,
        );

        let brief = service.brief();
        assert_eq!(brief.overdue_commitments.len(), 1);
        assert_eq!(brief.recent_failures.len(), 1);
        assert_eq!(brief.recent_decisions.len(), 1);
    }

    #[test]
    fn durable_chronicle_survives_reload() {
        let dir = std::env::temp_dir().join(format!("astra_chronicle_{}", new_id("test")));
        {
            let service = ChronicleService::with_data_dir(&dir).expect("open");
            record(&service, EpisodeKind::Decision, "persisted decision", 0.7);
            record(&service, EpisodeKind::Learning, "persisted lesson", 0.6);
            service.consolidate().expect("consolidate");
        }
        {
            let reopened = ChronicleService::with_data_dir(&dir).expect("reopen");
            assert_eq!(reopened.list_episodes().len(), 2);
            // The index continues from the persisted head.
            let next = record(&reopened, EpisodeKind::Event, "post-restart event", 0.5);
            assert_eq!(next.chain_index, 2);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

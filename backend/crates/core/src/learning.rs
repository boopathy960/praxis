use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningSignal {
    Success,
    Failure,
    Surprise,
    Refutation,
    CapabilityGap,
    SafetyBlock,
    Insight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEpisode {
    pub episode_id: String,
    pub organ: String,
    pub signal: LearningSignal,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub score: Option<f64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordLearningEpisodeRequest {
    pub organ: String,
    pub signal: LearningSignal,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningStats {
    pub episodes: usize,
    pub successes: usize,
    pub failures: usize,
    pub capability_gaps: usize,
    pub average_score: f64,
}

#[derive(Clone, Default)]
pub struct LearningService {
    episodes: Shared<Vec<LearningEpisode>>,
}

impl LearningService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            episodes: Shared::new(RwLock::new(Vec::new())),
        }
    }

    pub fn record(
        &self,
        request: RecordLearningEpisodeRequest,
    ) -> Result<LearningEpisode, AppError> {
        let organ = request.organ.trim();
        let summary = request.summary.trim();
        if organ.is_empty() || summary.is_empty() {
            return Err(AppError::Validation(
                "learning episode organ and summary must not be empty".into(),
            ));
        }
        let episode = LearningEpisode {
            episode_id: new_id("learn"),
            organ: organ.to_string(),
            signal: request.signal,
            summary: summary.to_string(),
            evidence: request.evidence,
            score: request.score.map(|score| score.clamp(0.0, 1.0)),
            created_at_ms: now_ms(),
        };
        self.episodes.write().push(episode.clone());
        Ok(episode)
    }

    pub fn list(&self, limit: usize) -> Vec<LearningEpisode> {
        self.episodes
            .read()
            .iter()
            .rev()
            .take(limit.clamp(1, 1000))
            .cloned()
            .collect()
    }

    pub fn stats(&self) -> LearningStats {
        let episodes = self.episodes.read();
        let average_score = {
            let scores = episodes
                .iter()
                .filter_map(|episode| episode.score)
                .collect::<Vec<_>>();
            if scores.is_empty() {
                0.0
            } else {
                scores.iter().sum::<f64>() / scores.len() as f64
            }
        };
        LearningStats {
            episodes: episodes.len(),
            successes: episodes
                .iter()
                .filter(|episode| matches!(episode.signal, LearningSignal::Success))
                .count(),
            failures: episodes
                .iter()
                .filter(|episode| {
                    matches!(
                        episode.signal,
                        LearningSignal::Failure | LearningSignal::Refutation
                    )
                })
                .count(),
            capability_gaps: episodes
                .iter()
                .filter(|episode| matches!(episode.signal, LearningSignal::CapabilityGap))
                .count(),
            average_score,
        }
    }
}

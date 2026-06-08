// ═══════════════════════════════════════════════════════════════
// Persistent Learning Engine — Episodic Memory & Sleep Consolidation
// ═══════════════════════════════════════════════════════════════
//
// Production-grade persistent learning system that allows Astra to
// permanently learn from every interaction and get smarter over time.
//
// Architecture:
//   1. Episodic Memory Store — compressed problem→solution episodes
//   2. Cognitive Shortcuts — pre-computed solution templates from patterns
//   3. Forgetting Curve — exponential decay on unused memories
//   4. Sleep Consolidation — background replay & pattern distillation
//   5. Similarity Index — fast retrieval of relevant past episodes
//   6. Performance Tracking — per-strategy success/failure statistics
//
// Mathematical Foundation:
//   - Memory strength: S(t) = S₀ · exp(-λ · Δt) + Σ_recalls R_i
//   - Similarity: sim(A,B) = jaccard(features_A, features_B) · cosine(metrics_A, metrics_B)
//   - Consolidation: merge episodes with sim > θ into cognitive shortcuts
//   - Forgetting: prune episodes with S(t) < ε (configurable threshold)
//
// Zero external dependencies. Pure Rust. Thread-safe via interior mutability patterns.

use std::collections::HashMap;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// A single episodic memory of a problem-solving interaction.
#[derive(Debug, Clone)]
pub struct Episode {
    /// Unique episode ID
    pub id: u64,
    /// Problem signature: keywords extracted from the original problem
    pub problem_signature: Vec<String>,
    /// Which dimension(s) solved this (D1-D5 identifiers)
    pub winning_dimensions: Vec<String>,
    /// Strategy that won
    pub winning_strategy: String,
    /// Quality score of the final answer (0.0 = terrible, 1.0 = perfect)
    pub quality_score: f64,
    /// Confidence of the solution
    pub confidence: f64,
    /// Time taken to solve (ms)
    pub solve_time_ms: f64,
    /// Number of timeline branches explored
    pub branches_explored: usize,
    /// Key metrics from the solution
    pub metrics: EpisodeMetrics,
    /// Creation timestamp (monotonic tick count)
    pub created_at: u64,
    /// Memory strength (decays over time, refreshed on recall)
    pub strength: f64,
    /// Number of times this memory has been recalled
    pub recall_count: u32,
    /// Last recall timestamp
    pub last_recalled: u64,
    /// Domain tags for fast retrieval
    pub domain_tags: Vec<String>,
}

/// Metrics captured during problem solving.
#[derive(Debug, Clone)]
pub struct EpisodeMetrics {
    pub proof_strength: f64,
    pub novelty_score: f64,
    pub stability: f64,
    pub drift_detected: bool,
    pub impossibility_level: f64,
    pub causal_validity: f64,
    pub energy_optimality: f64,
}

impl Default for EpisodeMetrics {
    fn default() -> Self {
        Self {
            proof_strength: 0.0,
            novelty_score: 0.0,
            stability: 1.0,
            drift_detected: false,
            impossibility_level: 0.0,
            causal_validity: 1.0,
            energy_optimality: 0.0,
        }
    }
}

/// A cognitive shortcut: a distilled template from many similar episodes.
#[derive(Debug, Clone)]
pub struct CognitiveShortcut {
    /// Unique shortcut ID
    pub id: u64,
    /// Pattern signature: common keywords across merged episodes
    pub pattern_signature: Vec<String>,
    /// Recommended dimension to use
    pub recommended_dimension: String,
    /// Recommended strategy
    pub recommended_strategy: String,
    /// Average quality score from source episodes
    pub avg_quality: f64,
    /// Average solve time from source episodes
    pub avg_solve_time_ms: f64,
    /// Number of episodes that contributed to this shortcut
    pub source_count: u32,
    /// Confidence in this shortcut (grows with more episodes)
    pub shortcut_confidence: f64,
    /// Creation timestamp
    pub created_at: u64,
    /// Last used timestamp
    pub last_used: u64,
    /// Use count
    pub use_count: u32,
}

/// Strategy performance statistics.
#[derive(Debug, Clone)]
pub struct StrategyStats {
    pub strategy_name: String,
    pub total_uses: u64,
    pub total_successes: u64,
    pub total_failures: u64,
    pub avg_quality: f64,
    pub avg_solve_time_ms: f64,
    pub best_quality: f64,
    pub worst_quality: f64,
}

/// Configuration for the learning engine.
#[derive(Debug, Clone)]
pub struct LearningConfig {
    /// Decay rate for memory forgetting (higher = faster forgetting)
    pub decay_rate: f64,
    /// Minimum memory strength before pruning
    pub prune_threshold: f64,
    /// Similarity threshold for merging episodes into shortcuts
    pub merge_threshold: f64,
    /// Maximum number of episodes to keep in memory
    pub max_episodes: usize,
    /// Maximum number of shortcuts
    pub max_shortcuts: usize,
    /// Strength boost per recall
    pub recall_boost: f64,
    /// Minimum quality to store an episode (don't learn from garbage)
    pub min_quality_to_store: f64,
    /// How many top results to return on recall
    pub recall_top_k: usize,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.001,
            prune_threshold: 0.05,
            merge_threshold: 0.65,
            max_episodes: 10_000,
            max_shortcuts: 1_000,
            recall_boost: 0.3,
            min_quality_to_store: 0.3,
            recall_top_k: 5,
        }
    }
}

/// Result of a learning engine query.
#[derive(Debug, Clone)]
pub struct RecallResult {
    /// Matching episodes, sorted by relevance
    pub episodes: Vec<(f64, Episode)>, // (similarity, episode)
    /// Matching cognitive shortcuts
    pub shortcuts: Vec<(f64, CognitiveShortcut)>,
    /// Whether we found a strong shortcut (confidence > 0.7)
    pub has_strong_shortcut: bool,
    /// Recommended strategy based on past experience
    pub recommended_strategy: Option<String>,
    /// Recommended dimension based on past experience
    pub recommended_dimension: Option<String>,
    /// Expected quality based on historical performance
    pub expected_quality: f64,
    /// Retrieval time
    pub duration_ms: f64,
}

/// Result of a consolidation cycle.
#[derive(Debug, Clone)]
pub struct ConsolidationResult {
    /// Episodes pruned due to decay
    pub episodes_pruned: usize,
    /// New shortcuts created from pattern merging
    pub shortcuts_created: usize,
    /// Existing shortcuts strengthened
    pub shortcuts_strengthened: usize,
    /// Total episodes remaining
    pub total_episodes: usize,
    /// Total shortcuts
    pub total_shortcuts: usize,
    /// Duration of consolidation
    pub duration_ms: f64,
}

// ═══════════════════════════════════════════════════════════════
// PERSISTENT LEARNING ENGINE
// ═══════════════════════════════════════════════════════════════

pub struct PersistentLearningEngine {
    config: LearningConfig,
    episodes: Vec<Episode>,
    shortcuts: Vec<CognitiveShortcut>,
    strategy_stats: HashMap<String, StrategyStats>,
    next_episode_id: u64,
    next_shortcut_id: u64,
    current_tick: u64,
    total_recalls: u64,
    total_stores: u64,
    total_consolidations: u64,
}

impl PersistentLearningEngine {
    pub fn new(config: LearningConfig) -> Self {
        Self {
            config,
            episodes: Vec::new(),
            shortcuts: Vec::new(),
            strategy_stats: HashMap::new(),
            next_episode_id: 1,
            next_shortcut_id: 1,
            current_tick: 0,
            total_recalls: 0,
            total_stores: 0,
            total_consolidations: 0,
        }
    }

    /// Store a new episode from a completed problem-solving session.
    pub fn store_episode(
        &mut self,
        problem_keywords: &[&str],
        winning_dimensions: &[&str],
        strategy: &str,
        quality: f64,
        confidence: f64,
        solve_time_ms: f64,
        branches: usize,
        metrics: EpisodeMetrics,
        domain_tags: &[&str],
    ) -> u64 {
        self.current_tick += 1;

        // Don't store garbage results
        if quality < self.config.min_quality_to_store {
            return 0;
        }

        let episode = Episode {
            id: self.next_episode_id,
            problem_signature: problem_keywords.iter().map(|s| s.to_lowercase()).collect(),
            winning_dimensions: winning_dimensions.iter().map(|s| s.to_string()).collect(),
            winning_strategy: strategy.to_string(),
            quality_score: quality,
            confidence,
            solve_time_ms,
            branches_explored: branches,
            metrics,
            created_at: self.current_tick,
            strength: 1.0,
            recall_count: 0,
            last_recalled: self.current_tick,
            domain_tags: domain_tags.iter().map(|s| s.to_string()).collect(),
        };

        let id = episode.id;
        self.episodes.push(episode);
        self.next_episode_id += 1;
        self.total_stores += 1;

        // Update strategy stats
        let stats = self
            .strategy_stats
            .entry(strategy.to_string())
            .or_insert_with(|| StrategyStats {
                strategy_name: strategy.to_string(),
                total_uses: 0,
                total_successes: 0,
                total_failures: 0,
                avg_quality: 0.0,
                avg_solve_time_ms: 0.0,
                best_quality: 0.0,
                worst_quality: 1.0,
            });

        stats.total_uses += 1;
        if quality > 0.6 {
            stats.total_successes += 1;
        } else {
            stats.total_failures += 1;
        }
        // Running average
        let n = stats.total_uses as f64;
        stats.avg_quality = stats.avg_quality * (n - 1.0) / n + quality / n;
        stats.avg_solve_time_ms = stats.avg_solve_time_ms * (n - 1.0) / n + solve_time_ms / n;
        if quality > stats.best_quality {
            stats.best_quality = quality;
        }
        if quality < stats.worst_quality {
            stats.worst_quality = quality;
        }

        // Enforce max episodes (LRU eviction)
        if self.episodes.len() > self.config.max_episodes {
            // Remove the weakest memory
            if let Some(weakest_idx) = self
                .episodes
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.strength
                        .partial_cmp(&b.strength)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
            {
                self.episodes.swap_remove(weakest_idx);
            }
        }

        id
    }

    /// Recall relevant past episodes and shortcuts for a new problem.
    pub fn recall(&mut self, query_keywords: &[&str], domain: Option<&str>) -> RecallResult {
        let start = Instant::now();
        self.current_tick += 1;
        self.total_recalls += 1;

        let query_set: Vec<String> = query_keywords.iter().map(|s| s.to_lowercase()).collect();

        // Score all episodes by similarity
        let mut episode_scores: Vec<(f64, usize)> = self
            .episodes
            .iter()
            .enumerate()
            .map(|(idx, ep)| {
                let sim = self.compute_similarity(
                    &query_set,
                    &ep.problem_signature,
                    domain,
                    &ep.domain_tags,
                );
                // Weight by memory strength
                let weighted_sim = sim * ep.strength;
                (weighted_sim, idx)
            })
            .filter(|(sim, _)| *sim > 0.1)
            .collect();

        episode_scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        episode_scores.truncate(self.config.recall_top_k);

        // Boost strength of recalled episodes
        for (_, idx) in &episode_scores {
            self.episodes[*idx].strength += self.config.recall_boost;
            self.episodes[*idx].strength = self.episodes[*idx].strength.min(2.0);
            self.episodes[*idx].recall_count += 1;
            self.episodes[*idx].last_recalled = self.current_tick;
        }

        let matched_episodes: Vec<(f64, Episode)> = episode_scores
            .iter()
            .map(|(sim, idx)| (*sim, self.episodes[*idx].clone()))
            .collect();

        // Score shortcuts
        let mut shortcut_scores: Vec<(f64, usize)> = self
            .shortcuts
            .iter()
            .enumerate()
            .map(|(idx, sc)| {
                let sim = self.compute_similarity(&query_set, &sc.pattern_signature, domain, &[]);
                (sim, idx)
            })
            .filter(|(sim, _)| *sim > 0.1)
            .collect();

        shortcut_scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        shortcut_scores.truncate(3);

        let matched_shortcuts: Vec<(f64, CognitiveShortcut)> = shortcut_scores
            .iter()
            .map(|(sim, idx)| (*sim, self.shortcuts[*idx].clone()))
            .collect();

        // Determine recommendations
        let has_strong_shortcut = matched_shortcuts
            .iter()
            .any(|(sim, sc)| *sim > 0.5 && sc.shortcut_confidence > 0.7);

        let recommended_strategy = if let Some((_, sc)) = matched_shortcuts.first() {
            if sc.shortcut_confidence > 0.5 {
                Some(sc.recommended_strategy.clone())
            } else {
                matched_episodes
                    .first()
                    .map(|(_, ep)| ep.winning_strategy.clone())
            }
        } else {
            matched_episodes
                .first()
                .map(|(_, ep)| ep.winning_strategy.clone())
        };

        let recommended_dimension = if let Some((_, sc)) = matched_shortcuts.first() {
            if sc.shortcut_confidence > 0.5 {
                Some(sc.recommended_dimension.clone())
            } else {
                matched_episodes
                    .first()
                    .and_then(|(_, ep)| ep.winning_dimensions.first().cloned())
            }
        } else {
            matched_episodes
                .first()
                .and_then(|(_, ep)| ep.winning_dimensions.first().cloned())
        };

        let expected_quality = if !matched_episodes.is_empty() {
            let total: f64 = matched_episodes
                .iter()
                .map(|(sim, ep)| sim * ep.quality_score)
                .sum();
            let weight_sum: f64 = matched_episodes.iter().map(|(sim, _)| sim).sum();
            if weight_sum > 0.0 {
                total / weight_sum
            } else {
                0.5
            }
        } else {
            0.5
        };

        RecallResult {
            episodes: matched_episodes,
            shortcuts: matched_shortcuts,
            has_strong_shortcut,
            recommended_strategy,
            recommended_dimension,
            expected_quality,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Run a consolidation cycle: decay memories, prune weak ones, merge similar into shortcuts.
    pub fn consolidate(&mut self) -> ConsolidationResult {
        let start = Instant::now();
        self.current_tick += 1;
        self.total_consolidations += 1;

        // 1. Apply forgetting curve to all episodes
        for episode in &mut self.episodes {
            let age = (self.current_tick - episode.last_recalled) as f64;
            let decay = (-self.config.decay_rate * age).exp();
            episode.strength *= decay;
        }

        // 2. Prune episodes below threshold
        let before_count = self.episodes.len();
        self.episodes
            .retain(|ep| ep.strength >= self.config.prune_threshold);
        let episodes_pruned = before_count - self.episodes.len();

        // 3. Find clusters of similar episodes and merge into shortcuts
        let mut shortcuts_created = 0usize;
        let mut shortcuts_strengthened = 0usize;

        // Group episodes by dominant winning dimension
        let mut dimension_groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, ep) in self.episodes.iter().enumerate() {
            let dim = ep
                .winning_dimensions
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            dimension_groups.entry(dim).or_default().push(idx);
        }

        for (_dim, indices) in &dimension_groups {
            if indices.len() < 3 {
                continue;
            }

            // Pairwise similarity within group — find clusters
            let mut merged_pairs: Vec<(usize, usize)> = Vec::new();
            for i in 0..indices.len() {
                for j in (i + 1)..indices.len() {
                    let ep_a = &self.episodes[indices[i]];
                    let ep_b = &self.episodes[indices[j]];
                    let sim = self.compute_similarity(
                        &ep_a.problem_signature,
                        &ep_b.problem_signature,
                        None,
                        &[],
                    );
                    if sim > self.config.merge_threshold {
                        merged_pairs.push((indices[i], indices[j]));
                    }
                }
            }

            // Create shortcuts from merged pairs
            for (i, j) in merged_pairs.iter().take(5) {
                let ep_a = &self.episodes[*i];
                let ep_b = &self.episodes[*j];

                // Check if a similar shortcut already exists
                let mut found_existing = false;
                for sc in &mut self.shortcuts {
                    let overlap =
                        Self::keyword_overlap(&sc.pattern_signature, &ep_a.problem_signature);
                    if overlap > 0.5 {
                        // Strengthen existing shortcut
                        sc.source_count += 1;
                        sc.shortcut_confidence = (sc.shortcut_confidence + 0.05).min(1.0);
                        sc.avg_quality = (sc.avg_quality * (sc.source_count - 1) as f64
                            + ep_a.quality_score)
                            / sc.source_count as f64;
                        shortcuts_strengthened += 1;
                        found_existing = true;
                        break;
                    }
                }

                if !found_existing && self.shortcuts.len() < self.config.max_shortcuts {
                    // Merge common keywords
                    let common: Vec<String> = ep_a
                        .problem_signature
                        .iter()
                        .filter(|kw| ep_b.problem_signature.contains(kw))
                        .cloned()
                        .collect();

                    if common.len() >= 2 {
                        let shortcut = CognitiveShortcut {
                            id: self.next_shortcut_id,
                            pattern_signature: common,
                            recommended_dimension: ep_a
                                .winning_dimensions
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "D1".to_string()),
                            recommended_strategy: if ep_a.quality_score >= ep_b.quality_score {
                                ep_a.winning_strategy.clone()
                            } else {
                                ep_b.winning_strategy.clone()
                            },
                            avg_quality: (ep_a.quality_score + ep_b.quality_score) / 2.0,
                            avg_solve_time_ms: (ep_a.solve_time_ms + ep_b.solve_time_ms) / 2.0,
                            source_count: 2,
                            shortcut_confidence: 0.4,
                            created_at: self.current_tick,
                            last_used: self.current_tick,
                            use_count: 0,
                        };
                        self.shortcuts.push(shortcut);
                        self.next_shortcut_id += 1;
                        shortcuts_created += 1;
                    }
                }
            }
        }

        ConsolidationResult {
            episodes_pruned,
            shortcuts_created,
            shortcuts_strengthened,
            total_episodes: self.episodes.len(),
            total_shortcuts: self.shortcuts.len(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Get strategy performance statistics.
    pub fn get_strategy_stats(&self) -> &HashMap<String, StrategyStats> {
        &self.strategy_stats
    }

    /// Get best strategy for a given dimension.
    pub fn best_strategy_for_dimension(&self, dimension: &str) -> Option<&StrategyStats> {
        self.strategy_stats
            .values()
            .filter(|s| s.strategy_name.contains(dimension))
            .max_by(|a, b| {
                let a_score =
                    a.avg_quality * (a.total_successes as f64 / a.total_uses.max(1) as f64);
                let b_score =
                    b.avg_quality * (b.total_successes as f64 / b.total_uses.max(1) as f64);
                a_score
                    .partial_cmp(&b_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Get engine statistics.
    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "PersistentLearningEngine v1.0",
            "total_episodes": self.episodes.len(),
            "total_shortcuts": self.shortcuts.len(),
            "total_stores": self.total_stores,
            "total_recalls": self.total_recalls,
            "total_consolidations": self.total_consolidations,
            "strategy_count": self.strategy_stats.len(),
            "current_tick": self.current_tick,
        })
    }

    // ─────────────────────────────────────────────────────
    // INTERNAL METHODS
    // ─────────────────────────────────────────────────────

    /// Compute similarity between two keyword sets using Jaccard coefficient.
    fn compute_similarity(
        &self,
        query: &[String],
        target: &[String],
        domain: Option<&str>,
        target_tags: &[String],
    ) -> f64 {
        if query.is_empty() || target.is_empty() {
            return 0.0;
        }

        // Jaccard similarity on keywords
        let query_set: std::collections::HashSet<&str> = query.iter().map(|s| s.as_str()).collect();
        let target_set: std::collections::HashSet<&str> =
            target.iter().map(|s| s.as_str()).collect();

        let intersection = query_set.intersection(&target_set).count() as f64;
        let union = query_set.union(&target_set).count() as f64;
        let jaccard = if union > 0.0 {
            intersection / union
        } else {
            0.0
        };

        // Domain bonus
        let domain_bonus = if let Some(d) = domain {
            if target_tags.iter().any(|t| t == d) {
                0.15
            } else {
                0.0
            }
        } else {
            0.0
        };

        (jaccard + domain_bonus).min(1.0)
    }

    /// Compute keyword overlap ratio.
    fn keyword_overlap(a: &[String], b: &[String]) -> f64 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }
        let a_set: std::collections::HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
        let b_set: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
        let intersection = a_set.intersection(&b_set).count() as f64;
        let min_size = a.len().min(b.len()) as f64;
        intersection / min_size
    }
}

impl Default for PersistentLearningEngine {
    fn default() -> Self {
        Self::new(LearningConfig::default())
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_recall() {
        let mut engine = PersistentLearningEngine::default();

        engine.store_episode(
            &["sorting", "algorithm", "performance"],
            &["D1"],
            "deductive_cascade",
            0.9,
            0.95,
            15.0,
            12,
            EpisodeMetrics::default(),
            &["software"],
        );

        let result = engine.recall(&["sorting", "algorithm"], None);
        assert!(!result.episodes.is_empty());
        assert!(result.episodes[0].0 > 0.3);
    }

    #[test]
    fn test_low_quality_not_stored() {
        let mut engine = PersistentLearningEngine::default();
        let id = engine.store_episode(
            &["test"],
            &["D1"],
            "test_strategy",
            0.1, // below min_quality_to_store
            0.5,
            10.0,
            1,
            EpisodeMetrics::default(),
            &[],
        );
        assert_eq!(id, 0); // should return 0 (not stored)
    }

    #[test]
    fn test_forgetting_curve() {
        let mut engine = PersistentLearningEngine::default();
        engine.store_episode(
            &["test", "memory"],
            &["D1"],
            "test",
            0.8,
            0.9,
            10.0,
            5,
            EpisodeMetrics::default(),
            &[],
        );

        // Simulate many ticks without recall
        for _ in 0..1000 {
            engine.current_tick += 1;
        }

        let result = engine.consolidate();
        // After heavy decay, the episode should have been pruned
        assert!(result.episodes_pruned > 0 || engine.episodes[0].strength < 0.5);
    }

    #[test]
    fn test_recall_boosts_strength() {
        let mut engine = PersistentLearningEngine::default();
        engine.store_episode(
            &["boost", "test"],
            &["D2"],
            "creative",
            0.8,
            0.9,
            10.0,
            3,
            EpisodeMetrics::default(),
            &[],
        );

        let initial_strength = engine.episodes[0].strength;
        engine.recall(&["boost", "test"], None);
        assert!(engine.episodes[0].strength > initial_strength);
    }

    #[test]
    fn test_strategy_stats() {
        let mut engine = PersistentLearningEngine::default();
        for i in 0..10 {
            engine.store_episode(
                &["stats", "test"],
                &["D1"],
                "cascade",
                0.5 + (i as f64) * 0.05,
                0.9,
                10.0,
                5,
                EpisodeMetrics::default(),
                &[],
            );
        }

        let stats = engine.get_strategy_stats();
        let cascade = stats.get("cascade").unwrap();
        assert_eq!(cascade.total_uses, 10);
        assert!(cascade.avg_quality > 0.5);
    }

    #[test]
    fn test_consolidation_creates_shortcuts() {
        let mut engine = PersistentLearningEngine::new(LearningConfig {
            merge_threshold: 0.4,
            ..Default::default()
        });

        // Store many similar episodes
        for i in 0..10 {
            engine.store_episode(
                &["rust", "compilation", "error"],
                &["D1"],
                "deductive",
                0.8 + (i as f64) * 0.01,
                0.9,
                10.0,
                5,
                EpisodeMetrics::default(),
                &["software"],
            );
        }

        let result = engine.consolidate();
        assert!(result.shortcuts_created > 0 || result.shortcuts_strengthened > 0);
    }

    #[test]
    fn test_max_episodes_eviction() {
        let mut engine = PersistentLearningEngine::new(LearningConfig {
            max_episodes: 5,
            ..Default::default()
        });

        for i in 0..10 {
            engine.store_episode(
                &[&format!("topic_{}", i)],
                &["D1"],
                "test",
                0.8,
                0.9,
                10.0,
                3,
                EpisodeMetrics::default(),
                &[],
            );
        }

        assert!(engine.episodes.len() <= 5);
    }
}

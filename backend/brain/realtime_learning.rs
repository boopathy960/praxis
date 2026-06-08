// ─────────────────────────────────────────────────────────────
// Realtime Learning Engine — Online Pattern Extraction
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/realtime_learning_engine.py

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Pattern {
    pub id: String,
    pub pattern_type: String,
    pub description: String,
    pub frequency: u64,
    pub confidence: f64,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SkillProfile {
    pub domain: String,
    pub success_rate: f64,
    pub total_attempts: u64,
    pub avg_confidence: f64,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
}

/// Realtime Learning Engine — online pattern extraction + skill profiling.
pub struct RealtimeLearningEngine {
    patterns: HashMap<String, Pattern>,
    domain_stats: HashMap<String, DomainStats>,
    sequence_buffer: Vec<String>,
    next_pattern_id: u64,
}

#[derive(Debug, Clone, Default)]
struct DomainStats {
    successes: u64,
    failures: u64,
    total_confidence: f64,
    strategies_used: HashMap<String, u64>,
}

impl RealtimeLearningEngine {
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
            domain_stats: HashMap::new(),
            sequence_buffer: Vec::new(),
            next_pattern_id: 0,
        }
    }

    /// Learn from a completed task.
    pub fn learn_from_task(
        &mut self,
        domain: &str,
        strategy: &str,
        success: bool,
        confidence: f64,
    ) {
        let stats = self.domain_stats.entry(domain.to_string()).or_default();
        if success {
            stats.successes += 1;
        } else {
            stats.failures += 1;
        }
        stats.total_confidence += confidence;
        *stats
            .strategies_used
            .entry(strategy.to_string())
            .or_insert(0) += 1;

        // Track sequence
        self.sequence_buffer.push(format!(
            "{}:{}:{}",
            domain,
            strategy,
            if success { "ok" } else { "fail" }
        ));
        if self.sequence_buffer.len() > 500 {
            self.sequence_buffer.drain(0..250);
        }

        // Extract patterns periodically
        if self.sequence_buffer.len() % 20 == 0 {
            self.extract_patterns();
        }
    }

    /// Extract recurring patterns from the sequence buffer.
    fn extract_patterns(&mut self) {
        let mut bigrams: HashMap<String, u64> = HashMap::new();
        for window in self.sequence_buffer.windows(2) {
            let key = format!("{} -> {}", window[0], window[1]);
            *bigrams.entry(key).or_insert(0) += 1;
        }

        for (pattern_str, count) in &bigrams {
            if *count >= 3 {
                let existing = self
                    .patterns
                    .values()
                    .find(|p| p.description == *pattern_str);
                if existing.is_none() {
                    let id = format!("pat_{}", self.next_pattern_id);
                    self.next_pattern_id += 1;
                    self.patterns.insert(
                        id.clone(),
                        Pattern {
                            id,
                            pattern_type: "sequence".into(),
                            description: pattern_str.clone(),
                            frequency: *count,
                            confidence: (*count as f64 / self.sequence_buffer.len() as f64)
                                .min(1.0),
                            examples: Vec::new(),
                        },
                    );
                }
            }
        }
    }

    /// Get skill profile for a domain.
    pub fn skill_profile(&self, domain: &str) -> SkillProfile {
        let stats = self.domain_stats.get(domain);
        match stats {
            Some(s) => {
                let total = s.successes + s.failures;
                let success_rate = if total > 0 {
                    s.successes as f64 / total as f64
                } else {
                    0.0
                };
                let avg_conf = if total > 0 {
                    s.total_confidence / total as f64
                } else {
                    0.0
                };

                let mut strategies: Vec<(&String, &u64)> = s.strategies_used.iter().collect();
                strategies.sort_by(|a, b| b.1.cmp(a.1));

                let strengths = strategies
                    .iter()
                    .take(3)
                    .filter(|(_, &count)| count > 2)
                    .map(|(s, _)| (*s).clone())
                    .collect();

                SkillProfile {
                    domain: domain.to_string(),
                    success_rate,
                    total_attempts: total,
                    avg_confidence: avg_conf,
                    strengths,
                    weaknesses: Vec::new(),
                }
            }
            None => SkillProfile {
                domain: domain.to_string(),
                success_rate: 0.0,
                total_attempts: 0,
                avg_confidence: 0.0,
                strengths: Vec::new(),
                weaknesses: Vec::new(),
            },
        }
    }

    /// Get all discovered patterns.
    pub fn discovered_patterns(&self) -> Vec<&Pattern> {
        let mut patterns: Vec<&Pattern> = self.patterns.values().collect();
        patterns.sort_by(|a, b| b.frequency.cmp(&a.frequency));
        patterns
    }

    pub fn total_patterns(&self) -> usize {
        self.patterns.len()
    }
    pub fn domains_tracked(&self) -> usize {
        self.domain_stats.len()
    }
}

impl Default for RealtimeLearningEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────
// Meta-Cognition Engine — Third-Order Metacognition
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/meta_cognition.py

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CognitiveState {
    pub confidence: f64,
    pub uncertainty: f64,
    pub strategy_effectiveness: HashMap<String, f64>,
    pub knowledge_gaps: Vec<String>,
    pub reasoning_depth: usize,
    pub cognitive_load: f64,
}

#[derive(Debug, Clone)]
pub struct MetaCognitiveInsight {
    pub observation: String,
    pub recommendation: String,
    pub confidence_adjustment: f64,
    pub strategy_suggestion: Option<String>,
}

/// Meta-Cognition Engine — thinks about its own thinking.
/// Three levels:
///   1. Monitor: observe cognitive performance
///   2. Evaluate: assess strategy effectiveness
///   3. Regulate: adjust strategies and confidence
pub struct MetaCognitionEngine {
    cognitive_history: Vec<CognitiveState>,
    strategy_performance: HashMap<String, Vec<f64>>,
    invented_strategies: Vec<String>,
    reflection_count: u64,
}

impl MetaCognitionEngine {
    pub fn new() -> Self {
        Self {
            cognitive_history: Vec::new(),
            strategy_performance: HashMap::new(),
            invented_strategies: Vec::new(),
            reflection_count: 0,
        }
    }

    /// Record a cognitive state for analysis.
    pub fn record_state(&mut self, state: CognitiveState) {
        self.cognitive_history.push(state);
        if self.cognitive_history.len() > 1000 {
            self.cognitive_history.drain(0..500);
        }
    }

    /// Record strategy performance.
    pub fn record_strategy_outcome(&mut self, strategy: &str, score: f64) {
        self.strategy_performance
            .entry(strategy.to_string())
            .or_default()
            .push(score);
    }

    /// Perform metacognitive reflection on recent performance.
    pub fn reflect(&mut self) -> Vec<MetaCognitiveInsight> {
        self.reflection_count += 1;
        let mut insights = Vec::new();

        // Level 1: Monitor — check recent confidence trends
        if self.cognitive_history.len() >= 5 {
            let recent: Vec<f64> = self
                .cognitive_history
                .iter()
                .rev()
                .take(5)
                .map(|s| s.confidence)
                .collect();
            let avg = recent.iter().sum::<f64>() / recent.len() as f64;
            let trend = recent.first().unwrap_or(&0.5) - recent.last().unwrap_or(&0.5);

            if trend < -0.1 {
                insights.push(MetaCognitiveInsight {
                    observation: "Confidence is declining in recent iterations".into(),
                    recommendation: "Consider switching to a different reasoning strategy".into(),
                    confidence_adjustment: 0.0,
                    strategy_suggestion: Some("decomposition".into()),
                });
            }

            if avg < 0.4 {
                insights.push(MetaCognitiveInsight {
                    observation: format!("Average confidence very low: {:.3}", avg),
                    recommendation:
                        "Problem may be outside current capabilities. Try simpler decomposition."
                            .into(),
                    confidence_adjustment: -0.1,
                    strategy_suggestion: Some("divide_and_conquer".into()),
                });
            }
        }

        // Level 2: Evaluate — assess strategy effectiveness
        for (strategy, scores) in &self.strategy_performance {
            if scores.len() >= 3 {
                let avg = scores.iter().sum::<f64>() / scores.len() as f64;
                let recent_avg = scores.iter().rev().take(3).sum::<f64>() / 3.0;

                if recent_avg < avg * 0.7 {
                    insights.push(MetaCognitiveInsight {
                        observation: format!(
                            "Strategy '{}' declining: avg={:.3} → recent={:.3}",
                            strategy, avg, recent_avg
                        ),
                        recommendation: format!("Reduce reliance on '{}'", strategy),
                        confidence_adjustment: 0.0,
                        strategy_suggestion: None,
                    });
                }
            }
        }

        // Level 3: Regulate — invent new strategies
        if self.reflection_count % 10 == 0 && !self.strategy_performance.is_empty() {
            let best_strategies: Vec<(&String, f64)> = self
                .strategy_performance
                .iter()
                .map(|(s, scores)| (s, scores.iter().sum::<f64>() / scores.len() as f64))
                .collect();

            if let Some((best, _)) = best_strategies
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                let new_strategy = format!("evolved_{}_v{}", best, self.reflection_count);
                self.invented_strategies.push(new_strategy.clone());
                insights.push(MetaCognitiveInsight {
                    observation: format!("Invented new strategy: {}", new_strategy),
                    recommendation: "Try hybrid of best-performing strategies".into(),
                    confidence_adjustment: 0.05,
                    strategy_suggestion: Some(new_strategy),
                });
            }
        }

        insights
    }

    /// Get the best-performing strategy.
    pub fn best_strategy(&self) -> Option<(String, f64)> {
        self.strategy_performance
            .iter()
            .filter(|(_, scores)| !scores.is_empty())
            .map(|(s, scores)| (s.clone(), scores.iter().sum::<f64>() / scores.len() as f64))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Get all invented strategies.
    pub fn invented_strategies(&self) -> &[String] {
        &self.invented_strategies
    }

    pub fn reflection_count(&self) -> u64 {
        self.reflection_count
    }
}

impl Default for MetaCognitionEngine {
    fn default() -> Self {
        Self::new()
    }
}

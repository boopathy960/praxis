// ═══════════════════════════════════════════════════════════════
// SELF-REFLECTION v3.0 — Deep Introspection & Cognitive Genome
// ═══════════════════════════════════════════════════════════════
//
// Production-grade self-improvement engine:
//   1. Outcome Analysis — what happened vs. expected
//   2. Causal Attribution — WHY did it succeed/fail
//   3. Strategy Evaluation — which strategies work when
//   4. Failure Pattern Mining — recurring failure signatures
//   5. Confidence Calibration — is the system over/under confident
//   6. Cognitive Genome Evolution — evolves reasoning parameters
//   7. Capability Estimation — knows what it can/can't do
//   8. Transfer Learning — apply lessons across domains

use crate::crypto::hash::sha3_256_hex;
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════
// TASK OUTCOME
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct TaskOutcome {
    pub task_id: String,
    pub task_description: String,
    pub success: bool,
    pub confidence: f64,
    pub duration_ms: f64,
    pub strategy_used: String,
    pub iterations: u32,
    pub error: Option<String>,
    pub domain: Option<String>,
    pub complexity: Option<f64>,
}

// ═══════════════════════════════════════════════════════════════
// REFLECTION INSIGHTS
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct ReflectionInsight {
    pub id: String,
    pub insight_type: InsightType,
    pub description: String,
    pub confidence: f64,
    pub actionable: bool,
    pub applied: bool,
    pub priority: f64,
    pub created_at: i64,
    pub related_tasks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum InsightType {
    StrategyEffective,
    StrategyIneffective,
    FailurePattern,
    PerformanceBottleneck,
    ConfidenceCalibration,
    NewHeuristic,
    CapabilityBoundary,
    TransferOpportunity,
    DomainExpertise,
}

// ═══════════════════════════════════════════════════════════════
// COGNITIVE GENOME — Self-evolving reasoning parameters
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct CognitiveGenome {
    pub exploration_weight: f64,
    pub risk_tolerance: f64,
    pub confidence_threshold: f64,
    pub preferred_depth: f64,
    pub strategy_weights: HashMap<String, f64>,
    pub learning_rate: f64,
    pub generation: u64,
    pub fitness_history: Vec<f64>,
    // v3 additions
    pub domain_expertise: HashMap<String, f64>,
    pub capability_map: HashMap<String, f64>,
    pub calibration_offset: f64,
}

impl CognitiveGenome {
    pub fn new() -> Self {
        let mut strategy_weights = HashMap::new();
        for s in &[
            "ChainOfThought",
            "TreeOfThought",
            "Decomposition",
            "BackwardChaining",
            "HypothesisTest",
            "AnalogicalReasoning",
            "SelfCritique",
            "FormalReasoning",
            "CausalReasoning",
            "MetaCognition",
            "BayesianInference",
            "CounterfactualReasoning",
            "AbductiveReasoning",
            "ConstraintSatisfaction",
            "AdversarialReasoning",
        ] {
            strategy_weights.insert(s.to_string(), 1.0);
        }

        Self {
            exploration_weight: 0.3,
            risk_tolerance: 0.5,
            confidence_threshold: 0.55,
            preferred_depth: 10.0,
            strategy_weights,
            learning_rate: 0.05,
            generation: 0,
            fitness_history: Vec::new(),
            domain_expertise: HashMap::new(),
            capability_map: HashMap::new(),
            calibration_offset: 0.0,
        }
    }

    /// Evolve genome based on fitness signal.
    pub fn evolve(&mut self, fitness: f64) {
        self.generation += 1;
        self.fitness_history.push(fitness);
        let lr = self.learning_rate * if fitness > 0.7 { 0.5 } else { 1.5 };

        // Exploration: more when failing
        if fitness < 0.5 {
            self.exploration_weight = (self.exploration_weight + lr * 0.1).min(0.8);
        } else {
            self.exploration_weight = (self.exploration_weight - lr * 0.05).max(0.1);
        }

        // Risk tolerance
        if fitness > 0.8 {
            self.risk_tolerance = (self.risk_tolerance + lr * 0.05).min(0.9);
        } else if fitness < 0.3 {
            self.risk_tolerance = (self.risk_tolerance - lr * 0.1).max(0.1);
        }

        // Confidence threshold
        let avg = if self.fitness_history.len() > 5 {
            self.fitness_history.iter().rev().take(5).sum::<f64>() / 5.0
        } else {
            fitness
        };
        if avg > 0.7 {
            self.confidence_threshold = (self.confidence_threshold - lr * 0.02).max(0.3);
        } else {
            self.confidence_threshold = (self.confidence_threshold + lr * 0.03).min(0.9);
        }

        if self.fitness_history.len() > 100 {
            self.fitness_history.drain(0..50);
        }
    }

    pub fn update_strategy(&mut self, strategy: &str, success: bool) {
        if let Some(weight) = self.strategy_weights.get_mut(strategy) {
            if success {
                *weight = (*weight + self.learning_rate).min(3.0);
            } else {
                *weight = (*weight - self.learning_rate * 0.5).max(0.1);
            }
        }
    }

    pub fn update_domain_expertise(&mut self, domain: &str, score: f64) {
        let entry = self
            .domain_expertise
            .entry(domain.to_string())
            .or_insert(0.5);
        *entry = *entry * 0.9 + score * 0.1;
    }

    pub fn update_capability(&mut self, capability: &str, demonstrated: bool) {
        let entry = self
            .capability_map
            .entry(capability.to_string())
            .or_insert(0.5);
        if demonstrated {
            *entry = (*entry + 0.1).min(1.0);
        } else {
            *entry = (*entry - 0.05).max(0.0);
        }
    }

    pub fn current_fitness(&self) -> f64 {
        if self.fitness_history.is_empty() {
            return 0.5;
        }
        self.fitness_history.iter().rev().take(10).sum::<f64>()
            / self.fitness_history.len().min(10) as f64
    }

    /// Get calibrated confidence (adjust for systematic over/under-confidence).
    pub fn calibrated_confidence(&self, raw_confidence: f64) -> f64 {
        (raw_confidence + self.calibration_offset).clamp(0.0, 1.0)
    }
}

// ═══════════════════════════════════════════════════════════════
// CAUSAL ATTRIBUTOR — Explains why tasks succeed/fail
// ═══════════════════════════════════════════════════════════════

struct CausalAttributor {
    factor_scores: HashMap<String, (f64, u32)>, // factor -> (total_correlation, count)
}

impl CausalAttributor {
    fn new() -> Self {
        Self {
            factor_scores: HashMap::new(),
        }
    }

    fn attribute(&mut self, outcome: &TaskOutcome) -> Vec<(String, f64)> {
        let mut attributions = Vec::new();
        let success_val = if outcome.success { 1.0 } else { 0.0 };

        // Strategy attribution
        let strat_key = format!("strategy:{}", outcome.strategy_used);
        self.update_factor(&strat_key, success_val);
        attributions.push((strat_key, success_val));

        // Duration attribution (fast = usually confident)
        if outcome.duration_ms < 500.0 && outcome.success {
            self.update_factor("fast_success", 1.0);
            attributions.push(("fast_execution".into(), 0.8));
        } else if outcome.duration_ms > 5000.0 && !outcome.success {
            self.update_factor("slow_failure", 1.0);
            attributions.push(("timeout_likely".into(), 0.7));
        }

        // Confidence calibration
        if outcome.success && outcome.confidence < 0.4 {
            self.update_factor("under_confident", 1.0);
            attributions.push(("under_confident".into(), 0.6));
        } else if !outcome.success && outcome.confidence > 0.8 {
            self.update_factor("over_confident", 1.0);
            attributions.push(("over_confident".into(), 0.8));
        }

        // Iteration count as factor
        if outcome.iterations > 3 && !outcome.success {
            self.update_factor("max_iters_exhausted", 1.0);
            attributions.push(("exhausted_iterations".into(), 0.65));
        }

        // Domain attribution
        if let Some(domain) = &outcome.domain {
            let domain_key = format!("domain:{}", domain);
            self.update_factor(&domain_key, success_val);
            attributions.push((domain_key, success_val));
        }

        attributions
    }

    fn update_factor(&mut self, factor: &str, value: f64) {
        let entry = self
            .factor_scores
            .entry(factor.to_string())
            .or_insert((0.0, 0));
        entry.0 += value;
        entry.1 += 1;
    }

    fn get_top_factors(&self, n: usize) -> Vec<(String, f64)> {
        let mut sorted: Vec<_> = self
            .factor_scores
            .iter()
            .map(|(k, (total, count))| (k.clone(), total / *count as f64))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(n);
        sorted
    }
}

// ═══════════════════════════════════════════════════════════════
// CAPABILITY ESTIMATOR — Knows what the system can/can't do
// ═══════════════════════════════════════════════════════════════

struct CapabilityEstimator {
    task_type_scores: HashMap<String, (u32, u32)>, // (successes, total)
    _complexity_threshold: f64,
}

impl CapabilityEstimator {
    fn new() -> Self {
        Self {
            task_type_scores: HashMap::new(),
            _complexity_threshold: 0.8,
        }
    }

    fn record(&mut self, task_type: &str, success: bool) {
        let entry = self
            .task_type_scores
            .entry(task_type.to_string())
            .or_insert((0, 0));
        entry.1 += 1;
        if success {
            entry.0 += 1;
        }
    }

    fn can_handle(&self, task_type: &str) -> f64 {
        self.task_type_scores
            .get(task_type)
            .map(|(s, t)| *s as f64 / (*t).max(1) as f64)
            .unwrap_or(0.5) // Unknown = uncertain
    }

    fn get_strengths(&self) -> Vec<(String, f64)> {
        let mut sorted: Vec<_> = self
            .task_type_scores
            .iter()
            .filter(|(_, (_, t))| *t >= 3)
            .map(|(k, (s, t))| (k.clone(), *s as f64 / *t as f64))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted
    }

    fn get_weaknesses(&self) -> Vec<(String, f64)> {
        let mut sorted: Vec<_> = self
            .task_type_scores
            .iter()
            .filter(|(_, (_, t))| *t >= 3)
            .map(|(k, (s, t))| (k.clone(), *s as f64 / *t as f64))
            .filter(|(_, rate)| *rate < 0.5)
            .collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted
    }
}

// ═══════════════════════════════════════════════════════════════
// SELF-REFLECTION ENGINE v3.0
// ═══════════════════════════════════════════════════════════════

pub struct SelfReflection {
    genome: CognitiveGenome,
    insights: Vec<ReflectionInsight>,
    outcome_history: Vec<TaskOutcome>,
    strategy_outcomes: HashMap<String, (u32, u32)>,
    failure_patterns: HashMap<String, u32>,
    causal: CausalAttributor,
    capability: CapabilityEstimator,
    // Confidence calibration tracking
    predicted_vs_actual: Vec<(f64, bool)>, // (predicted_confidence, actual_success)
    max_insights: usize,
    max_history: usize,
}

impl SelfReflection {
    pub fn new() -> Self {
        Self {
            genome: CognitiveGenome::new(),
            insights: Vec::new(),
            outcome_history: Vec::new(),
            strategy_outcomes: HashMap::new(),
            failure_patterns: HashMap::new(),
            causal: CausalAttributor::new(),
            capability: CapabilityEstimator::new(),
            predicted_vs_actual: Vec::new(),
            max_insights: 2000,
            max_history: 10000,
        }
    }

    /// Full reflection pipeline on a completed task.
    pub fn reflect(&mut self, outcome: TaskOutcome) -> Vec<ReflectionInsight> {
        let mut new_insights = Vec::new();

        // 1. Causal Attribution
        let _attributions = self.causal.attribute(&outcome);

        // 2. Strategy evaluation
        let (success_rate, total_count) = {
            let (successes, total) = self
                .strategy_outcomes
                .entry(outcome.strategy_used.clone())
                .or_insert((0, 0));
            *total += 1;
            if outcome.success {
                *successes += 1;
            }
            (*successes as f64 / (*total).max(1) as f64, *total)
        };

        if total_count >= 5 {
            if success_rate > 0.8 {
                new_insights.push(self.create_insight(
                    InsightType::StrategyEffective,
                    &format!(
                        "'{}' is highly effective ({:.0}% over {} uses)",
                        outcome.strategy_used,
                        success_rate * 100.0,
                        total_count
                    ),
                    success_rate,
                    vec![outcome.task_id.clone()],
                ));
            } else if success_rate < 0.3 {
                new_insights.push(self.create_insight(
                    InsightType::StrategyIneffective,
                    &format!(
                        "'{}' underperforming ({:.0}% over {} uses)",
                        outcome.strategy_used,
                        success_rate * 100.0,
                        total_count
                    ),
                    1.0 - success_rate,
                    vec![outcome.task_id.clone()],
                ));
            }
        }

        // 3. Failure pattern mining
        if !outcome.success {
            if let Some(ref error) = outcome.error {
                let pattern = categorize_error(error);
                let count_val = {
                    let count = self.failure_patterns.entry(pattern.clone()).or_insert(0);
                    *count += 1;
                    *count
                };
                if count_val >= 3 {
                    new_insights.push(self.create_insight(
                        InsightType::FailurePattern,
                        &format!("Recurring '{}' — {} occurrences", pattern, count_val),
                        0.9,
                        vec![outcome.task_id.clone()],
                    ));
                }
            }
        }

        // 4. Performance bottleneck
        if outcome.duration_ms > 5000.0 {
            new_insights.push(self.create_insight(
                InsightType::PerformanceBottleneck,
                &format!(
                    "Took {:.0}ms — bottleneck in '{}'",
                    outcome.duration_ms, outcome.strategy_used
                ),
                0.6,
                vec![outcome.task_id.clone()],
            ));
        }

        // 5. Confidence calibration
        self.predicted_vs_actual
            .push((outcome.confidence, outcome.success));
        if self.predicted_vs_actual.len() >= 20 {
            let calibration = self.compute_calibration();
            if calibration.abs() > 0.1 {
                let cal_type = if calibration > 0.0 {
                    "overconfident"
                } else {
                    "underconfident"
                };
                new_insights.push(self.create_insight(
                    InsightType::ConfidenceCalibration,
                    &format!(
                        "System is {} by {:.1}%",
                        cal_type,
                        calibration.abs() * 100.0
                    ),
                    0.8,
                    vec![],
                ));
                self.genome.calibration_offset = -calibration * 0.5;
            }
        }

        // 6. Capability boundary detection
        let task_type = categorize_task(&outcome.task_description);
        self.capability.record(&task_type, outcome.success);
        let cap = self.capability.can_handle(&task_type);
        if cap < 0.3
            && self
                .capability
                .task_type_scores
                .get(&task_type)
                .map(|(_, t)| *t >= 5)
                .unwrap_or(false)
        {
            new_insights.push(self.create_insight(
                InsightType::CapabilityBoundary,
                &format!(
                    "Weak at '{}' tasks ({:.0}% success)",
                    task_type,
                    cap * 100.0
                ),
                0.85,
                vec![outcome.task_id.clone()],
            ));
        }

        // 7. Domain expertise update
        if let Some(domain) = &outcome.domain {
            self.genome
                .update_domain_expertise(domain, if outcome.success { 0.8 } else { 0.2 });
        }

        // 8. Transfer learning opportunities
        if outcome.success {
            let similar_failures: Vec<_> = self
                .outcome_history
                .iter()
                .filter(|o| {
                    !o.success && word_overlap(&o.task_description, &outcome.task_description) > 0.3
                })
                .take(3)
                .collect();
            if !similar_failures.is_empty() {
                new_insights.push(self.create_insight(
                    InsightType::TransferOpportunity,
                    &format!(
                        "Success on '{}' could help {} similar failed tasks",
                        &outcome.task_description[..outcome.task_description.len().min(50)],
                        similar_failures.len()
                    ),
                    0.7,
                    vec![outcome.task_id.clone()],
                ));
            }
        }

        // Genome evolution
        let fitness = if outcome.success {
            0.5 + outcome.confidence * 0.5
        } else {
            0.5 * (1.0 - outcome.confidence)
        };
        self.genome.evolve(fitness);
        self.genome
            .update_strategy(&outcome.strategy_used, outcome.success);
        self.genome.update_capability(&task_type, outcome.success);

        // Store
        self.outcome_history.push(outcome);
        if self.outcome_history.len() > self.max_history {
            self.outcome_history.drain(0..self.max_history / 2);
        }
        for insight in &new_insights {
            self.insights.push(insight.clone());
        }
        if self.insights.len() > self.max_insights {
            self.insights.drain(0..self.max_insights / 2);
        }

        new_insights
    }

    fn compute_calibration(&self) -> f64 {
        if self.predicted_vs_actual.len() < 10 {
            return 0.0;
        }
        let recent = &self.predicted_vs_actual[self.predicted_vs_actual.len() - 20..];
        let avg_predicted: f64 = recent.iter().map(|(p, _)| p).sum::<f64>() / recent.len() as f64;
        let avg_actual: f64 = recent
            .iter()
            .map(|(_, a)| if *a { 1.0 } else { 0.0 })
            .sum::<f64>()
            / recent.len() as f64;
        avg_predicted - avg_actual // positive = overconfident
    }

    fn create_insight(
        &self,
        insight_type: InsightType,
        description: &str,
        confidence: f64,
        related: Vec<String>,
    ) -> ReflectionInsight {
        let id = sha3_256_hex(
            format!(
                "insight:{}:{}",
                description,
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            )
            .as_bytes(),
        )[..12]
            .to_string();
        let priority = confidence
            * match insight_type {
                InsightType::FailurePattern => 1.0,
                InsightType::CapabilityBoundary => 0.9,
                InsightType::ConfidenceCalibration => 0.85,
                InsightType::PerformanceBottleneck => 0.8,
                InsightType::StrategyIneffective => 0.75,
                InsightType::TransferOpportunity => 0.7,
                _ => 0.5,
            };
        ReflectionInsight {
            id,
            insight_type,
            description: description.to_string(),
            confidence,
            actionable: matches!(
                insight_type,
                InsightType::FailurePattern
                    | InsightType::StrategyIneffective
                    | InsightType::PerformanceBottleneck
                    | InsightType::CapabilityBoundary
            ),
            applied: false,
            priority,
            created_at: Utc::now().timestamp(),
            related_tasks: related,
        }
    }

    pub fn genome(&self) -> &CognitiveGenome {
        &self.genome
    }

    pub fn actionable_insights(&self) -> Vec<&ReflectionInsight> {
        let mut results: Vec<_> = self
            .insights
            .iter()
            .filter(|i| i.actionable && !i.applied)
            .collect();
        results.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    pub fn mark_applied(&mut self, insight_id: &str) -> bool {
        if let Some(insight) = self.insights.iter_mut().find(|i| i.id == insight_id) {
            insight.applied = true;
            true
        } else {
            false
        }
    }

    pub fn get_strengths(&self) -> Vec<(String, f64)> {
        self.capability.get_strengths()
    }
    pub fn get_weaknesses(&self) -> Vec<(String, f64)> {
        self.capability.get_weaknesses()
    }
    pub fn get_causal_factors(&self) -> Vec<(String, f64)> {
        self.causal.get_top_factors(10)
    }

    pub fn get_stats(&self) -> serde_json::Value {
        let strategy_stats: HashMap<String, serde_json::Value> = self.strategy_outcomes.iter()
            .map(|(s, (succ, total))| (s.clone(), serde_json::json!({
                "successes": succ, "total": total, "rate": *succ as f64 / (*total).max(1) as f64,
            }))).collect();

        serde_json::json!({
            "engine": "SelfReflection v3.0 — Deep Introspection",
            "genome": {
                "generation": self.genome.generation,
                "exploration_weight": self.genome.exploration_weight,
                "risk_tolerance": self.genome.risk_tolerance,
                "confidence_threshold": self.genome.confidence_threshold,
                "current_fitness": self.genome.current_fitness(),
                "calibration_offset": self.genome.calibration_offset,
                "domain_expertise": self.genome.domain_expertise,
                "strategy_weights": self.genome.strategy_weights,
            },
            "total_outcomes": self.outcome_history.len(),
            "total_insights": self.insights.len(),
            "actionable_insights": self.actionable_insights().len(),
            "strategy_performance": strategy_stats,
            "failure_patterns": self.failure_patterns,
            "strengths": self.get_strengths(),
            "weaknesses": self.get_weaknesses(),
            "top_causal_factors": self.get_causal_factors(),
        })
    }
}

fn categorize_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("timeout") {
        "timeout".into()
    } else if lower.contains("memory") || lower.contains("oom") {
        "memory_exhaustion".into()
    } else if lower.contains("overflow") {
        "overflow".into()
    } else if lower.contains("index") || lower.contains("bounds") {
        "index_error".into()
    } else if lower.contains("parse") || lower.contains("syntax") {
        "parse_error".into()
    } else if lower.contains("permission") || lower.contains("denied") {
        "permission_error".into()
    } else if lower.contains("network") || lower.contains("connection") {
        "network_error".into()
    } else if lower.contains("hallucin") {
        "hallucination".into()
    } else if lower.contains("verification") {
        "verification_failure".into()
    } else {
        "unknown_error".into()
    }
}

fn categorize_task(description: &str) -> String {
    let lower = description.to_lowercase();
    if lower.contains("search") || lower.contains("find") {
        "search".into()
    } else if lower.contains("navigate") || lower.contains("browse") {
        "navigation".into()
    } else if lower.contains("analyze") || lower.contains("examine") {
        "analysis".into()
    } else if lower.contains("compare") {
        "comparison".into()
    } else if lower.contains("create") || lower.contains("generate") {
        "creation".into()
    } else if lower.contains("debug") || lower.contains("fix") {
        "debugging".into()
    } else if lower.contains("explain") || lower.contains("how") {
        "explanation".into()
    } else if lower.contains("prove") || lower.contains("verify") {
        "verification".into()
    } else {
        "general".into()
    }
}

fn word_overlap(a: &str, b: &str) -> f64 {
    let set_a: std::collections::HashSet<&str> = a
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 3)
        .collect();
    let set_b: std::collections::HashSet<&str> = b
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 3)
        .collect();
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflection_v3() {
        let mut sr = SelfReflection::new();
        let initial_gen = sr.genome().generation;
        sr.reflect(TaskOutcome {
            task_id: "t1".into(),
            task_description: "Search for quantum computing".into(),
            success: true,
            confidence: 0.85,
            duration_ms: 100.0,
            strategy_used: "ChainOfThought".into(),
            iterations: 2,
            error: None,
            domain: Some("science".into()),
            complexity: Some(0.5),
        });
        assert_eq!(sr.genome().generation, initial_gen + 1);
    }

    #[test]
    fn test_capability_tracking() {
        let mut sr = SelfReflection::new();
        for i in 0..10 {
            sr.reflect(TaskOutcome {
                task_id: format!("t{}", i),
                task_description: "Search for information".into(),
                success: i < 8,
                confidence: 0.7,
                duration_ms: 200.0,
                strategy_used: "TreeOfThought".into(),
                iterations: 1,
                error: if i >= 8 { Some("timeout".into()) } else { None },
                domain: None,
                complexity: None,
            });
        }
        let strengths = sr.get_strengths();
        assert!(!strengths.is_empty());
    }

    #[test]
    fn test_calibration() {
        let mut sr = SelfReflection::new();
        // Consistently overconfident
        for i in 0..25 {
            sr.reflect(TaskOutcome {
                task_id: format!("t{}", i),
                task_description: "test task".into(),
                success: false,
                confidence: 0.9,
                duration_ms: 100.0,
                strategy_used: "ChainOfThought".into(),
                iterations: 1,
                error: Some("failed".into()),
                domain: None,
                complexity: None,
            });
        }
        // Should detect overconfidence
        assert!(sr.genome().calibration_offset < 0.0);
    }
}

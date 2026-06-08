// ═══════════════════════════════════════════════════════════════
// DIMENSION 4: METACOGNITIVE SOVEREIGNTY v1.0
// ═══════════════════════════════════════════════════════════════
//
// A fully sovereign self-awareness system that monitors, diagnoses,
// and REWRITES the brain's own cognitive architecture in real-time.
//
// Core subsystems:
//   1. Cognitive Dashboard — real-time telemetry of all dimensions
//   2. Bias Detector — catch over-reliance on specific strategies
//   3. Self-Rewrite Protocol — diagnose failures and patch in-memory
//   4. Recursive Self-Observation — the sovereign observes ITSELF
//      and adjusts its own correction aggressiveness
//
// Extends existing meta_cognition.rs (3-level monitor/evaluate/regulate).
// Pure Rust.  Dependencies: std, chrono, log.

use log::{debug, info};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use super::advanced_reasoning_formulas;
use super::cognitive_math;

// ═══════════════════════════════════════════════════════════════
// COGNITIVE DASHBOARD — Real-Time Brain Telemetry
// ═══════════════════════════════════════════════════════════════

/// Telemetry snapshot for a single cognitive cycle.
#[derive(Debug, Clone)]
pub struct CognitiveTelemetry {
    pub timestamp_ms: i64,
    pub dimension_states: HashMap<String, DimensionState>,
    pub total_thoughts_per_sec: f64,
    pub active_strategies: Vec<String>,
    pub entropy: f64,
    pub health: CognitiveHealth,
}

/// State of one dimension at a point in time.
#[derive(Debug, Clone)]
pub struct DimensionState {
    pub name: String,
    pub is_active: bool,
    pub current_confidence: f64,
    pub throughput: f64, // operations/sec
    pub error_rate: f64, // ratio of failed ops
    pub avg_latency_ms: f64,
    pub strategy_distribution: HashMap<String, f64>,
}

/// Overall brain health assessment.
#[derive(Debug, Clone, PartialEq)]
pub enum CognitiveHealth {
    Optimal,
    Healthy,
    Degraded { reason: String },
    Critical { reason: String },
}

impl CognitiveHealth {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Optimal => "optimal",
            Self::Healthy => "healthy",
            Self::Degraded { .. } => "degraded",
            Self::Critical { .. } => "critical",
        }
    }
}

/// Real-time cognitive dashboard.
pub struct CognitiveDashboard {
    history: VecDeque<CognitiveTelemetry>,
    max_history: usize,
    current_cycle: u64,
}

impl CognitiveDashboard {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::new(),
            max_history,
            current_cycle: 0,
        }
    }

    /// Record a telemetry snapshot.
    pub fn record(&mut self, telemetry: CognitiveTelemetry) {
        self.current_cycle += 1;
        self.history.push_back(telemetry);
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }
    }

    /// Compute trend: is performance improving or degrading?
    pub fn confidence_trend(&self, window: usize) -> f64 {
        let history: Vec<f64> = self
            .history
            .iter()
            .rev()
            .take(window)
            .flat_map(|t| t.dimension_states.values().map(|d| d.current_confidence))
            .collect();

        if history.len() < 4 {
            return 0.0;
        }

        // Linear regression slope
        let n = history.len() as f64;
        let x_mean = (n - 1.0) / 2.0;
        let y_mean: f64 = history.iter().sum::<f64>() / n;

        let mut num = 0.0;
        let mut den = 0.0;
        for (i, &y) in history.iter().enumerate() {
            let x = i as f64;
            num += (x - x_mean) * (y - y_mean);
            den += (x - x_mean) * (x - x_mean);
        }

        if den.abs() < 1e-12 {
            0.0
        } else {
            num / den
        }
    }

    /// Average throughput across all dimensions.
    pub fn avg_throughput(&self, window: usize) -> f64 {
        let vals: Vec<f64> = self
            .history
            .iter()
            .rev()
            .take(window)
            .map(|t| t.total_thoughts_per_sec)
            .collect();
        if vals.is_empty() {
            0.0
        } else {
            vals.iter().sum::<f64>() / vals.len() as f64
        }
    }

    /// Current health assessment.
    pub fn assess_health(&self) -> CognitiveHealth {
        if self.history.is_empty() {
            return CognitiveHealth::Healthy; // No data yet
        }

        let recent = &self.history[self.history.len() - 1];
        let trend = self.confidence_trend(10);

        // Check for critical conditions
        for (name, state) in &recent.dimension_states {
            if state.error_rate > 0.5 {
                return CognitiveHealth::Critical {
                    reason: format!(
                        "Dimension {} has {:.0}% error rate",
                        name,
                        state.error_rate * 100.0
                    ),
                };
            }
        }

        // Check for degradation
        if trend < -0.05 {
            return CognitiveHealth::Degraded {
                reason: format!("Confidence declining: slope={:.4}", trend),
            };
        }

        // Check entropy (too high = chaotic, too low = stuck)
        if recent.entropy > 0.95 {
            return CognitiveHealth::Degraded {
                reason: "Cognitive entropy too high — reasoning is chaotic".to_string(),
            };
        }
        if recent.entropy < 0.05 {
            return CognitiveHealth::Degraded {
                reason: "Cognitive entropy too low — stuck in single strategy".to_string(),
            };
        }

        if trend > 0.02 {
            CognitiveHealth::Optimal
        } else {
            CognitiveHealth::Healthy
        }
    }

    pub fn latest(&self) -> Option<&CognitiveTelemetry> {
        self.history.back()
    }
}

// ═══════════════════════════════════════════════════════════════
// BIAS DETECTOR — Strategy Over-Reliance Detection
// ═══════════════════════════════════════════════════════════════

/// A detected cognitive bias.
#[derive(Debug, Clone)]
pub struct CognitiveBias {
    pub bias_type: BiasType,
    pub severity: f64,
    pub affected_strategy: String,
    pub evidence: String,
    pub correction: BiasCorrection,
}

/// Types of cognitive bias.
#[derive(Debug, Clone, PartialEq)]
pub enum BiasType {
    /// Over-reliance on a single strategy
    StrategyFixation,
    /// Ignoring contradictory evidence
    ConfirmationBias,
    /// Overconfidence in uncertain results
    OverconfidenceBias,
    /// Anchoring on first solution found
    AnchoringBias,
    /// Avoiding complexity (premature simplification)
    SimplificationBias,
    /// Repeating failed approaches
    RecencyBias,
}

impl BiasType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StrategyFixation => "strategy_fixation",
            Self::ConfirmationBias => "confirmation_bias",
            Self::OverconfidenceBias => "overconfidence_bias",
            Self::AnchoringBias => "anchoring_bias",
            Self::SimplificationBias => "simplification_bias",
            Self::RecencyBias => "recency_bias",
        }
    }
}

/// Correction to apply for a detected bias.
#[derive(Debug, Clone)]
pub struct BiasCorrection {
    pub action: CorrectionAction,
    pub parameters: HashMap<String, f64>,
    pub description: String,
}

/// Types of corrections.
#[derive(Debug, Clone)]
pub enum CorrectionAction {
    InjectNoise { amount: f64 },
    ForceStrategyRotation,
    ReduceConfidence { factor: f64 },
    ExpandSearch { breadth_multiplier: f64 },
    ResetStrategyWeights,
    IncreaseExploration { epsilon: f64 },
}

/// Detects cognitive biases from strategy usage history.
pub struct BiasDetector {
    strategy_history: VecDeque<String>,
    pub confidence_history: VecDeque<f64>,
    outcome_history: VecDeque<bool>,
    max_history: usize,
    fixation_threshold: f64,
    overconfidence_threshold: f64,
}

impl BiasDetector {
    pub fn new() -> Self {
        Self {
            strategy_history: VecDeque::new(),
            confidence_history: VecDeque::new(),
            outcome_history: VecDeque::new(),
            max_history: 100,
            fixation_threshold: 0.5,
            overconfidence_threshold: 0.9,
        }
    }

    /// Record a cognitive event for bias analysis.
    pub fn record_event(&mut self, strategy: &str, confidence: f64, success: bool) {
        self.strategy_history.push_back(strategy.to_string());
        self.confidence_history.push_back(confidence);
        self.outcome_history.push_back(success);

        while self.strategy_history.len() > self.max_history {
            self.strategy_history.pop_front();
            self.confidence_history.pop_front();
            self.outcome_history.pop_front();
        }
    }

    /// Run bias detection on accumulated history.
    pub fn detect(&self) -> Vec<CognitiveBias> {
        let mut biases = Vec::new();

        if self.strategy_history.len() < 5 {
            return biases; // Not enough data
        }

        // 1. Strategy Fixation: one strategy dominates
        biases.extend(self.detect_fixation());

        // 2. Confirmation Bias: ignoring failures
        biases.extend(self.detect_confirmation_bias());

        // 3. Overconfidence: high confidence + low success rate
        biases.extend(self.detect_overconfidence());

        // 4. Anchoring: first strategy always wins
        biases.extend(self.detect_anchoring());

        // 5. Recency: repeating recently failed strategies
        biases.extend(self.detect_recency_bias());

        biases
    }

    fn detect_fixation(&self) -> Vec<CognitiveBias> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let window = self.strategy_history.len().min(20);
        for strategy in self.strategy_history.iter().rev().take(window) {
            *counts.entry(strategy.as_str()).or_insert(0) += 1;
        }

        let total = window as f64;
        let mut biases = Vec::new();

        for (strategy, count) in &counts {
            let ratio = *count as f64 / total;
            if ratio > self.fixation_threshold {
                biases.push(CognitiveBias {
                    bias_type: BiasType::StrategyFixation,
                    severity: ratio,
                    affected_strategy: strategy.to_string(),
                    evidence: format!(
                        "'{}' used {}/{} times ({:.0}%) in recent window — exceeds {:.0}% threshold",
                        strategy, count, window, ratio * 100.0, self.fixation_threshold * 100.0
                    ),
                    correction: BiasCorrection {
                        action: CorrectionAction::ForceStrategyRotation,
                        parameters: {
                            let mut p = HashMap::new();
                            p.insert("fixated_strategy".to_string(), ratio);
                            p
                        },
                        description: "Force rotation to underused strategies to break fixation".to_string(),
                    },
                });
            }
        }

        biases
    }

    fn detect_confirmation_bias(&self) -> Vec<CognitiveBias> {
        let mut biases = Vec::new();
        let window = self.outcome_history.len().min(15);
        let recent_outcomes: Vec<bool> = self
            .outcome_history
            .iter()
            .rev()
            .take(window)
            .cloned()
            .collect();

        let successes = recent_outcomes.iter().filter(|&&o| o).count();
        let failures = recent_outcomes.iter().filter(|&&o| !o).count();

        // If we have mostly failures but haven't changed strategy
        if failures > successes * 2 && self.strategy_history.len() >= 10 {
            let unique_recent: HashSet<&String> =
                self.strategy_history.iter().rev().take(5).collect();
            if unique_recent.len() <= 2 {
                biases.push(CognitiveBias {
                    bias_type: BiasType::ConfirmationBias,
                    severity: failures as f64 / (successes + failures).max(1) as f64,
                    affected_strategy: unique_recent
                        .iter()
                        .next()
                        .map(|s: &&String| s.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    evidence: format!(
                        "{} failures vs {} successes, but only {} unique strategies tried",
                        failures,
                        successes,
                        unique_recent.len()
                    ),
                    correction: BiasCorrection {
                        action: CorrectionAction::ExpandSearch {
                            breadth_multiplier: 3.0,
                        },
                        parameters: HashMap::new(),
                        description: "Dramatically expand search to include untested strategies"
                            .to_string(),
                    },
                });
            }
        }

        biases
    }

    fn detect_overconfidence(&self) -> Vec<CognitiveBias> {
        let mut biases = Vec::new();
        let window = self.confidence_history.len().min(10);

        let conf_values: Vec<f64> = self
            .confidence_history
            .iter()
            .rev()
            .take(window)
            .cloned()
            .collect();
        let outcomes: Vec<bool> = self
            .outcome_history
            .iter()
            .rev()
            .take(window)
            .cloned()
            .collect();

        let high_conf_failures: usize = conf_values
            .iter()
            .zip(outcomes.iter())
            .filter(|(&c, &o)| c > self.overconfidence_threshold && !o)
            .count();

        if high_conf_failures >= 3 {
            biases.push(CognitiveBias {
                bias_type: BiasType::OverconfidenceBias,
                severity: high_conf_failures as f64 / window as f64,
                affected_strategy: "all".to_string(),
                evidence: format!(
                    "{} failures with confidence > {:.0}% in last {} cycles — \
                     calibration is poor",
                    high_conf_failures, self.overconfidence_threshold * 100.0, window
                ),
                correction: BiasCorrection {
                    action: CorrectionAction::ReduceConfidence { factor: 0.7 },
                    parameters: {
                        let mut p = HashMap::new();
                        p.insert("reduction_factor".to_string(), 0.7);
                        p
                    },
                    description: "Apply confidence deflation (multiply all by 0.7) until calibration improves".to_string(),
                },
            });
        }

        biases
    }

    fn detect_anchoring(&self) -> Vec<CognitiveBias> {
        let mut biases = Vec::new();

        // Check if the first strategy tried is always the one selected
        if self.strategy_history.len() < 10 {
            return biases;
        }

        let first_strategy = &self.strategy_history[0];
        let first_count = self
            .strategy_history
            .iter()
            .filter(|s| s == &first_strategy)
            .count();
        let ratio = first_count as f64 / self.strategy_history.len() as f64;

        if ratio > 0.6 {
            biases.push(CognitiveBias {
                bias_type: BiasType::AnchoringBias,
                severity: ratio,
                affected_strategy: first_strategy.clone(),
                evidence: format!(
                    "First strategy '{}' used {:.0}% of the time — anchored to initial approach",
                    first_strategy,
                    ratio * 100.0
                ),
                correction: BiasCorrection {
                    action: CorrectionAction::InjectNoise { amount: 0.3 },
                    parameters: {
                        let mut p = HashMap::new();
                        p.insert("noise_amount".to_string(), 0.3);
                        p
                    },
                    description: "Inject 30% noise into strategy selection to break anchoring"
                        .to_string(),
                },
            });
        }

        biases
    }

    fn detect_recency_bias(&self) -> Vec<CognitiveBias> {
        let mut biases = Vec::new();

        if self.strategy_history.len() < 6 {
            return biases;
        }

        // Check if recently failed strategies are being retried immediately
        let recent_5: Vec<(&String, &bool)> = self
            .strategy_history
            .iter()
            .zip(self.outcome_history.iter())
            .rev()
            .take(5)
            .collect();

        let mut failed_then_retried = 0;
        for i in 0..recent_5.len().saturating_sub(1) {
            let (strategy_later, _) = recent_5[i];
            let (strategy_earlier, &succeeded_earlier) = recent_5[i + 1];
            if !succeeded_earlier && strategy_later == strategy_earlier {
                failed_then_retried += 1;
            }
        }

        if failed_then_retried >= 2 {
            biases.push(CognitiveBias {
                bias_type: BiasType::RecencyBias,
                severity: failed_then_retried as f64 / 4.0,
                affected_strategy: recent_5
                    .first()
                    .map(|(s, _)| s.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                evidence: format!(
                    "{} instances of immediately retrying a failed strategy",
                    failed_then_retried
                ),
                correction: BiasCorrection {
                    action: CorrectionAction::IncreaseExploration { epsilon: 0.5 },
                    parameters: {
                        let mut p = HashMap::new();
                        p.insert("epsilon".to_string(), 0.5);
                        p
                    },
                    description:
                        "Set exploration epsilon to 0.5 — try random strategies 50% of the time"
                            .to_string(),
                },
            });
        }

        biases
    }
}

impl Default for BiasDetector {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// SELF-REWRITE PROTOCOL
// ═══════════════════════════════════════════════════════════════

/// A self-modification record for audit trail.
#[derive(Debug, Clone)]
pub struct SelfModification {
    pub id: u64,
    pub timestamp_ms: i64,
    pub diagnosis: String,
    pub patch_applied: String,
    pub target_subsystem: String,
    pub effect: ModificationEffect,
    pub confidence_before: f64,
    pub confidence_after: f64,
}

/// Effect of a self-modification.
#[derive(Debug, Clone)]
pub enum ModificationEffect {
    Improved { delta: f64 },
    Neutral,
    Degraded { delta: f64 },
    Unknown,
}

/// The self-rewrite protocol that can diagnose systematic failures
/// and apply live patches to knowledge structures.
pub struct SelfRewriter {
    modifications: Vec<SelfModification>,
    next_id: u64,
    max_modifications_per_cycle: usize,
    /// knowledge patches: subsystem → list of (key, value) patches
    active_patches: HashMap<String, Vec<(String, String)>>,
    /// Strategy weight overrides
    strategy_overrides: HashMap<String, f64>,
}

impl SelfRewriter {
    pub fn new() -> Self {
        Self {
            modifications: Vec::new(),
            next_id: 0,
            max_modifications_per_cycle: 3,
            active_patches: HashMap::new(),
            strategy_overrides: HashMap::new(),
        }
    }

    /// Apply corrections from bias detector results.
    pub fn apply_corrections(
        &mut self,
        biases: &[CognitiveBias],
        current_confidence: f64,
    ) -> Vec<SelfModification> {
        let mut mods = Vec::new();
        let now = chrono::Utc::now().timestamp_millis();

        for bias in biases.iter().take(self.max_modifications_per_cycle) {
            let patch = match &bias.correction.action {
                CorrectionAction::ForceStrategyRotation => {
                    // Reduce weight of fixated strategy, boost others
                    let fixated = &bias.affected_strategy;
                    self.strategy_overrides
                        .entry(fixated.clone())
                        .and_modify(|w| *w *= 0.5)
                        .or_insert(0.5);
                    format!(
                        "Reduced weight of '{}' by 50%, boosting alternatives",
                        fixated
                    )
                }
                CorrectionAction::InjectNoise { amount } => {
                    // Add noise to strategy selection weights
                    for (_, weight) in self.strategy_overrides.iter_mut() {
                        *weight += (rand::random::<f64>() - 0.5) * amount;
                        *weight = weight.clamp(0.1, 2.0);
                    }
                    format!(
                        "Injected {:.0}% noise into strategy weights",
                        amount * 100.0
                    )
                }
                CorrectionAction::ReduceConfidence { factor } => {
                    self.active_patches
                        .entry("confidence".to_string())
                        .or_default()
                        .push(("deflation_factor".to_string(), factor.to_string()));
                    format!("Applied confidence deflation factor of {:.2}", factor)
                }
                CorrectionAction::ExpandSearch { breadth_multiplier } => {
                    self.active_patches
                        .entry("search".to_string())
                        .or_default()
                        .push((
                            "breadth_multiplier".to_string(),
                            breadth_multiplier.to_string(),
                        ));
                    format!("Expanded search breadth by {}×", breadth_multiplier)
                }
                CorrectionAction::ResetStrategyWeights => {
                    self.strategy_overrides.clear();
                    "Reset all strategy weights to uniform".to_string()
                }
                CorrectionAction::IncreaseExploration { epsilon } => {
                    self.active_patches
                        .entry("exploration".to_string())
                        .or_default()
                        .push(("epsilon".to_string(), epsilon.to_string()));
                    format!("Set exploration epsilon to {:.2}", epsilon)
                }
            };

            let modification = SelfModification {
                id: self.next_id,
                timestamp_ms: now,
                diagnosis: format!(
                    "Bias: {} (severity={:.2}) on '{}'",
                    bias.bias_type.as_str(),
                    bias.severity,
                    bias.affected_strategy
                ),
                patch_applied: patch,
                target_subsystem: bias.affected_strategy.clone(),
                effect: ModificationEffect::Unknown, // Will be evaluated next cycle
                confidence_before: current_confidence,
                confidence_after: current_confidence, // Updated after evaluation
            };

            self.next_id += 1;
            mods.push(modification.clone());
            self.modifications.push(modification);
        }

        mods
    }

    /// Get strategy weight override (returns 1.0 if no override).
    pub fn get_strategy_weight(&self, strategy: &str) -> f64 {
        *self.strategy_overrides.get(strategy).unwrap_or(&1.0)
    }

    /// Get active patch for a subsystem.
    pub fn get_patches(&self, subsystem: &str) -> Option<&Vec<(String, String)>> {
        self.active_patches.get(subsystem)
    }

    /// Evaluate whether recent modifications helped.
    pub fn evaluate_modifications(&mut self, current_confidence: f64) {
        // Check last few modifications
        for modification in self.modifications.iter_mut().rev().take(5) {
            if matches!(modification.effect, ModificationEffect::Unknown) {
                let delta = current_confidence - modification.confidence_before;
                modification.confidence_after = current_confidence;
                modification.effect = if delta > 0.05 {
                    ModificationEffect::Improved { delta }
                } else if delta < -0.05 {
                    ModificationEffect::Degraded { delta }
                } else {
                    ModificationEffect::Neutral
                };
            }
        }
    }

    /// Get the modification log.
    pub fn audit_log(&self) -> &[SelfModification] {
        &self.modifications
    }
}

impl Default for SelfRewriter {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// RECURSIVE SELF-OBSERVATION
// ═══════════════════════════════════════════════════════════════

/// The sovereign observes ITSELF: meta-meta-cognition.
struct RecursiveObserver {
    /// History of correction aggressiveness over time
    aggressiveness_history: VecDeque<f64>,
    /// History of correction outcomes
    correction_outcomes: VecDeque<bool>,
    max_history: usize,
}

impl RecursiveObserver {
    fn new() -> Self {
        Self {
            aggressiveness_history: VecDeque::new(),
            correction_outcomes: VecDeque::new(),
            max_history: 50,
        }
    }

    /// Record a correction outcome.
    fn record(&mut self, aggressiveness: f64, outcome_positive: bool) {
        self.aggressiveness_history.push_back(aggressiveness);
        self.correction_outcomes.push_back(outcome_positive);
        while self.aggressiveness_history.len() > self.max_history {
            self.aggressiveness_history.pop_front();
            self.correction_outcomes.pop_front();
        }
    }

    /// Compute optimal aggressiveness from history.
    fn optimal_aggressiveness(&self) -> f64 {
        if self.aggressiveness_history.len() < 5 {
            return 0.5; // Default: moderate
        }

        // Find aggressiveness level that produced best outcomes
        let mut bins: HashMap<u32, (u32, u32)> = HashMap::new(); // bin → (success, total)

        for (agg, &outcome) in self
            .aggressiveness_history
            .iter()
            .zip(self.correction_outcomes.iter())
        {
            let bin = (*agg * 10.0) as u32; // Quantize to 0.1 increments
            let entry = bins.entry(bin).or_insert((0, 0));
            if outcome {
                entry.0 += 1;
            }
            entry.1 += 1;
        }

        let best_bin = bins
            .iter()
            .filter(|(_, (_, total))| *total >= 2) // Need at least 2 samples
            .max_by(|(_, (s1, t1)), (_, (s2, t2))| {
                let r1 = *s1 as f64 / *t1 as f64;
                let r2 = *s2 as f64 / *t2 as f64;
                r1.partial_cmp(&r2).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(bin, _)| *bin);

        best_bin.map(|b| b as f64 / 10.0).unwrap_or(0.5)
    }
}

// ═══════════════════════════════════════════════════════════════
// METACOGNITIVE SOVEREIGN — THE MAIN ENGINE
// ═══════════════════════════════════════════════════════════════

/// Result of a metacognitive observation cycle.
#[derive(Debug, Clone)]
pub struct MetacognitionResult {
    pub health: CognitiveHealth,
    pub biases_detected: Vec<CognitiveBias>,
    pub corrections_applied: Vec<SelfModification>,
    pub confidence_trend: f64,
    pub throughput: f64,
    pub aggressiveness: f64,
    pub recommendation: String,
    pub duration_ms: f64,
}

/// Configuration for the sovereign.
#[derive(Debug, Clone)]
pub struct SovereignConfig {
    pub observation_window: usize,
    pub initial_aggressiveness: f64,
    pub dashboard_history_size: usize,
}

impl Default for SovereignConfig {
    fn default() -> Self {
        Self {
            observation_window: 20,
            initial_aggressiveness: 0.5,
            dashboard_history_size: 500,
        }
    }
}

/// Dimension 4: Metacognitive Sovereignty.
///
/// Provides full self-awareness: monitors all dimensions,
/// detects cognitive biases, applies live corrections, and
/// recursively observes its own correction behavior.
pub struct MetacognitiveSovereign {
    config: SovereignConfig,
    dashboard: CognitiveDashboard,
    bias_detector: BiasDetector,
    self_rewriter: SelfRewriter,
    observer: RecursiveObserver,
    correction_aggressiveness: f64,
    /// Cognitive momentum tracker (novel formula: velocity/accel/jerk of cognition).
    momentum: advanced_reasoning_formulas::CognitiveMomentum,
    /// Running confidence samples for Lyapunov stability analysis.
    stability_samples: Vec<f64>,
    /// Previous strategy performance distribution for KS drift detection.
    prev_strategy_perf: Vec<f64>,
    total_observations: u64,
    total_corrections: u64,
    total_self_adjustments: u64,
}

impl MetacognitiveSovereign {
    pub fn new(config: SovereignConfig) -> Self {
        let aggressiveness = config.initial_aggressiveness;
        let history_size = config.dashboard_history_size;
        Self {
            config,
            dashboard: CognitiveDashboard::new(history_size),
            bias_detector: BiasDetector::new(),
            self_rewriter: SelfRewriter::new(),
            observer: RecursiveObserver::new(),
            correction_aggressiveness: aggressiveness,
            momentum: advanced_reasoning_formulas::CognitiveMomentum::new(0.85),
            stability_samples: Vec::new(),
            prev_strategy_perf: Vec::new(),
            total_observations: 0,
            total_corrections: 0,
            total_self_adjustments: 0,
        }
    }

    /// Run a full metacognitive observation cycle.
    ///
    /// This is called AFTER each problem is solved to monitor
    /// brain health and apply corrections for the next cycle.
    pub fn observe(
        &mut self,
        dimension_states: HashMap<String, DimensionState>,
        strategy_used: &str,
        confidence: f64,
        success: bool,
    ) -> MetacognitionResult {
        let start = Instant::now();
        self.total_observations += 1;

        // Step 1: Record telemetry
        let throughput = dimension_states.values().map(|d| d.throughput).sum::<f64>();
        let entropy = self.compute_strategy_entropy(&dimension_states);

        let telemetry = CognitiveTelemetry {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            dimension_states,
            total_thoughts_per_sec: throughput,
            active_strategies: vec![strategy_used.to_string()],
            entropy,
            health: CognitiveHealth::Healthy, // Will be assessed below
        };
        self.dashboard.record(telemetry);

        // Step 2: Record event for bias detection
        self.bias_detector
            .record_event(strategy_used, confidence, success);

        // Step 3: Detect biases
        let biases = self.bias_detector.detect();

        // Step 4: Apply corrections (gated by aggressiveness)
        let corrections =
            if !biases.is_empty() && rand::random::<f64>() < self.correction_aggressiveness {
                let mods = self.self_rewriter.apply_corrections(&biases, confidence);
                self.total_corrections += mods.len() as u64;
                mods
            } else {
                Vec::new()
            };

        // Step 5: Evaluate previous corrections
        self.self_rewriter.evaluate_modifications(confidence);

        // Step 6: Recursive self-observation — adjust aggressiveness
        let correction_improved = corrections
            .iter()
            .any(|m| matches!(m.effect, ModificationEffect::Improved { .. }));
        self.observer.record(
            self.correction_aggressiveness,
            correction_improved || corrections.is_empty(),
        );

        let optimal_agg = self.observer.optimal_aggressiveness();
        let old_agg = self.correction_aggressiveness;
        // EMA towards optimal
        self.correction_aggressiveness = self.correction_aggressiveness * 0.8 + optimal_agg * 0.2;

        if (self.correction_aggressiveness - old_agg).abs() > 0.05 {
            self.total_self_adjustments += 1;
            debug!(
                "[D4-Sovereign] Self-adjusted aggressiveness: {:.3} → {:.3}",
                old_agg, self.correction_aggressiveness
            );
        }

        // Step 7: Cognitive Momentum analysis (novel formula)
        self.momentum.update(confidence);
        let momentum_action = self.momentum.recommendation();
        let kinetic_energy = self.momentum.kinetic_energy();
        debug!(
            "[D4-Sovereign] Momentum: vel={:.4}, accel={:.4}, jerk={:.4}, KE={:.4}, action={:?}",
            self.momentum.velocity(),
            self.momentum.acceleration(),
            self.momentum.jerk(),
            kinetic_energy,
            momentum_action
        );

        // Step 8: Lyapunov Stability analysis (novel formula)
        self.stability_samples.push(confidence);
        if self.stability_samples.len() > 200 {
            self.stability_samples.drain(..100);
        }
        let stability_info = if self.stability_samples.len() >= 15 {
            let stability = cognitive_math::lyapunov_stability(&self.stability_samples, 3);
            debug!(
                "[D4-Sovereign] Lyapunov CSI={:.4}, class={:?}, action={:?}",
                stability.csi, stability.classification, stability.action
            );
            Some(stability)
        } else {
            None
        };

        // Step 9: KS Drift Detection (novel formula)
        let current_strategy_perf: Vec<f64> = self
            .bias_detector
            .confidence_history
            .iter()
            .rev()
            .take(20)
            .cloned()
            .collect();
        if !self.prev_strategy_perf.is_empty() && current_strategy_perf.len() >= 5 {
            let drift =
                cognitive_math::ks_two_sample(&self.prev_strategy_perf, &current_strategy_perf);
            if drift.drift_detected {
                info!(
                    "[D4-Sovereign] ⚠ KS Drift detected: D={:.4}, p={:.4} — strategy performance shifted!",
                    drift.ks_statistic, drift.p_value
                );
            }
        }
        self.prev_strategy_perf = current_strategy_perf;

        // Step 10: Assess health (enhanced with Lyapunov)
        let mut health = self.dashboard.assess_health();
        // Override health if Lyapunov says unstable
        if let Some(ref stab) = stability_info {
            if matches!(
                stab.classification,
                cognitive_math::StabilityClass::Unstable
            ) {
                health = CognitiveHealth::Degraded {
                    reason: format!("Lyapunov instability: CSI={:.4}", stab.csi),
                };
            }
        }

        let trend = self
            .dashboard
            .confidence_trend(self.config.observation_window);
        let avg_throughput = self
            .dashboard
            .avg_throughput(self.config.observation_window);

        // Step 11: Generate recommendation
        let recommendation = self.generate_recommendation(&health, &biases, trend);

        let duration = start.elapsed().as_secs_f64() * 1000.0;

        info!(
            "[D4-Sovereign] Health={}, biases={}, corrections={}, agg={:.2}, trend={:.4}, \
             momentum={:?}, KE={:.4}, {:.1}ms",
            health.as_str(),
            biases.len(),
            corrections.len(),
            self.correction_aggressiveness,
            trend,
            momentum_action,
            kinetic_energy,
            duration
        );

        MetacognitionResult {
            health,
            biases_detected: biases,
            corrections_applied: corrections,
            confidence_trend: trend,
            throughput: avg_throughput,
            aggressiveness: self.correction_aggressiveness,
            recommendation,
            duration_ms: duration,
        }
    }

    fn compute_strategy_entropy(&self, states: &HashMap<String, DimensionState>) -> f64 {
        let distributions: Vec<f64> = states
            .values()
            .flat_map(|d| d.strategy_distribution.values())
            .cloned()
            .collect();

        if distributions.is_empty() {
            return 0.5;
        }

        let total: f64 = distributions.iter().sum();
        if total < 1e-12 {
            return 0.0;
        }

        let mut entropy = 0.0;
        for &p in &distributions {
            let prob = p / total;
            if prob > 1e-12 {
                entropy -= prob * prob.ln();
            }
        }

        // Normalize by max possible entropy
        let max_entropy = (distributions.len() as f64).ln().max(1.0);
        (entropy / max_entropy).clamp(0.0, 1.0)
    }

    fn generate_recommendation(
        &self,
        health: &CognitiveHealth,
        biases: &[CognitiveBias],
        trend: f64,
    ) -> String {
        match health {
            CognitiveHealth::Critical { reason } => {
                format!(
                    "CRITICAL: {}. Recommend: emergency strategy reset + expanded exploration.",
                    reason
                )
            }
            CognitiveHealth::Degraded { reason } => {
                if !biases.is_empty() {
                    format!(
                        "Degraded ({}). {} biases detected. Primary bias: {} (severity={:.2}). \
                         Recommendation: apply corrections and increase exploration.",
                        reason,
                        biases.len(),
                        biases[0].bias_type.as_str(),
                        biases[0].severity
                    )
                } else {
                    format!(
                        "Degraded ({}). No biases detected — possible external factor.",
                        reason
                    )
                }
            }
            CognitiveHealth::Healthy => {
                if trend > 0.0 {
                    "Healthy and improving. Maintain current strategy mix.".to_string()
                } else {
                    "Healthy but flat. Consider small exploratory perturbation.".to_string()
                }
            }
            CognitiveHealth::Optimal => {
                "Optimal performance. Lock in current configuration.".to_string()
            }
        }
    }

    /// Get strategy weight override from self-rewriter.
    pub fn get_strategy_weight(&self, strategy: &str) -> f64 {
        self.self_rewriter.get_strategy_weight(strategy)
    }

    /// Get current correction aggressiveness.
    pub fn aggressiveness(&self) -> f64 {
        self.correction_aggressiveness
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "Dimension4_MetacognitiveSovereign_v1.0",
            "total_observations": self.total_observations,
            "total_corrections": self.total_corrections,
            "total_self_adjustments": self.total_self_adjustments,
            "correction_aggressiveness": self.correction_aggressiveness,
            "health": self.dashboard.assess_health().as_str(),
            "confidence_trend": self.dashboard.confidence_trend(20),
            "avg_throughput": self.dashboard.avg_throughput(20),
            "audit_log_size": self.self_rewriter.audit_log().len(),
        })
    }
}

impl Default for MetacognitiveSovereign {
    fn default() -> Self {
        Self::new(SovereignConfig::default())
    }
}

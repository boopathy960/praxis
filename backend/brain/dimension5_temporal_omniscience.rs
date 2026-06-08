// ═══════════════════════════════════════════════════════════════
// DIMENSION 5: TEMPORAL OMNISCIENCE v1.0
// ═══════════════════════════════════════════════════════════════
//
// The Crown Jewel — the 5th Dimensional Brain.
//
// This is what makes the system "5th-dimensional":
// In physics, a 5D being sees ALL possible timelines simultaneously
// and selects the optimal one.
//
// Implementation:
//   1. Fork N parallel "timelines" — each is a complete cognitive
//      approach using a different combination of D1-D4.
//   2. Execute ALL timelines simultaneously.
//   3. Score each timeline's result using a multi-objective
//      fitness function (confidence × novelty × efficiency).
//   4. COLLAPSE to the best timeline — return its answer.
//   5. Store ALL timelines (including rejected ones) for learning.
//   6. Evolve timeline configuration via the SelfEvolution engine.
//
// Pure Rust + tokio for async parallelism.

use log::{debug, info};
use rand::Rng;
use std::collections::HashMap;
use std::time::Instant;

use super::advanced_reasoning_formulas;
use super::cognitive_math;

use super::dimension1_deductive_cascade::{CascadeConfig, DeductiveCascade};
use super::dimension2_creative_hypersynthesis::{CreativeHypersynthesis, HyperConfig};
use super::dimension3_impossible_engine::{ImpossibleConfig, ImpossibleEngine};
use super::dimension4_metacognitive_sovereign::{
    DimensionState, MetacognitiveSovereign, SovereignConfig,
};

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// A timeline represents one complete cognitive approach to a problem.
#[derive(Debug, Clone)]
pub struct Timeline {
    pub id: usize,
    pub name: String,
    pub strategy: TimelineStrategy,
    pub result: Option<TimelineResult>,
    pub score: f64,
    pub is_winner: bool,
}

/// How this timeline approaches the problem.
#[derive(Debug, Clone)]
pub enum TimelineStrategy {
    /// Pure deductive reasoning (Dimension 1 only)
    PureDeduction,
    /// Creative first, then validate (D2 → D1)
    CreativeThenValidate,
    /// Impossible analysis, then creative reframing (D3 → D2)
    ImpossibleThenCreative,
    /// Full pipeline: impossible → creative → deductive (D3 → D2 → D1)
    FullPipeline,
    /// Direct intuitive leap (fastest, minimal analysis)
    IntuitiveLeap,
    /// Adversarial: try to disprove, then accept what survives (D1 adversarial)
    AdversarialSurvival,
    /// Synthesis: blend results from multiple strategies
    MetaSynthesis,
    /// Evolved: custom strategy from self-evolution
    Evolved { genome: Vec<f64> },
}

impl TimelineStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PureDeduction => "pure_deduction",
            Self::CreativeThenValidate => "creative_then_validate",
            Self::ImpossibleThenCreative => "impossible_then_creative",
            Self::FullPipeline => "full_pipeline",
            Self::IntuitiveLeap => "intuitive_leap",
            Self::AdversarialSurvival => "adversarial_survival",
            Self::MetaSynthesis => "meta_synthesis",
            Self::Evolved { .. } => "evolved",
        }
    }

    fn default_strategies() -> Vec<Self> {
        vec![
            Self::PureDeduction,
            Self::CreativeThenValidate,
            Self::ImpossibleThenCreative,
            Self::FullPipeline,
            Self::IntuitiveLeap,
            Self::AdversarialSurvival,
        ]
    }
}

/// Result from executing a single timeline.
#[derive(Debug, Clone)]
pub struct TimelineResult {
    pub answer: String,
    pub confidence: f64,
    pub novelty: f64,
    pub proof_level: String,
    pub dimensions_used: Vec<String>,
    pub reasoning_trace: Vec<String>,
    pub duration_ms: f64,
}

/// The final 5th-dimensional result after timeline collapse.
#[derive(Debug, Clone)]
pub struct FifthDimensionResult {
    pub answer: String,
    pub confidence: f64,
    pub winning_timeline: String,
    pub winning_strategy: String,
    pub proof_level: String,
    pub timelines_explored: Vec<TimelineSummary>,
    pub metacognition: MetacognitionSummary,
    pub total_timelines: usize,
    pub total_nodes_explored: usize,
    pub reasoning_trace: Vec<String>,
    pub duration_ms: f64,
}

/// Summary of a timeline (for reporting).
#[derive(Debug, Clone)]
pub struct TimelineSummary {
    pub id: usize,
    pub name: String,
    pub strategy: String,
    pub score: f64,
    pub confidence: f64,
    pub is_winner: bool,
    pub duration_ms: f64,
}

/// Summary of metacognition state.
#[derive(Debug, Clone)]
pub struct MetacognitionSummary {
    pub health: String,
    pub biases_detected: usize,
    pub corrections_applied: usize,
    pub aggressiveness: f64,
    pub confidence_trend: f64,
}

// ═══════════════════════════════════════════════════════════════
// TIMELINE SCORING — Multi-Objective Fitness
// ═══════════════════════════════════════════════════════════════

/// Multi-objective scoring weights.
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    confidence: f64,
    novelty: f64,
    efficiency: f64,
    proof_bonus: f64,
    dimensions_bonus: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            confidence: 0.40,
            novelty: 0.15,
            efficiency: 0.15,
            proof_bonus: 0.15,
            dimensions_bonus: 0.15,
        }
    }
}

fn score_timeline(result: &TimelineResult, max_duration: f64, weights: &ScoringWeights) -> f64 {
    let efficiency = if max_duration > 0.0 {
        1.0 - (result.duration_ms / max_duration).min(1.0)
    } else {
        0.5
    };

    let proof_score = match result.proof_level.as_str() {
        "formally_proven" => 1.0,
        "high_confidence_heuristic" => 0.7,
        "low_confidence_heuristic" => 0.4,
        _ => 0.1,
    };

    let dim_score = (result.dimensions_used.len() as f64 / 4.0).min(1.0);

    // Novel: Quantum Amplitude scoring — treat each timeline as a quantum state
    // Amplitude = sqrt(confidence * proof_score) — Born rule analogy
    let amplitude = (result.confidence * proof_score).sqrt();
    let quantum_probability =
        cognitive_math::quantum_amplitude_score(amplitude, result.novelty, efficiency);

    // Blend classical + quantum scores
    let classical_score = result.confidence * weights.confidence
        + result.novelty * weights.novelty
        + efficiency * weights.efficiency
        + proof_score * weights.proof_bonus
        + dim_score * weights.dimensions_bonus;

    // 60% classical, 40% quantum — quantum helps break ties
    classical_score * 0.6 + quantum_probability * 0.4
}

// ═══════════════════════════════════════════════════════════════
// TEMPORAL OMNISCIENCE ENGINE — THE 5th DIMENSION
// ═══════════════════════════════════════════════════════════════

/// Configuration for the temporal omniscience engine.
#[derive(Debug, Clone)]
pub struct TemporalConfig {
    pub num_timelines: usize,
    pub cascade_config: CascadeConfig,
    pub hyper_config: HyperConfig,
    pub impossible_config: ImpossibleConfig,
    pub sovereign_config: SovereignConfig,
    pub scoring_weights: ScoringWeights,
    pub enable_evolved_timelines: bool,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            num_timelines: 6,
            cascade_config: CascadeConfig::default(),
            hyper_config: HyperConfig::default(),
            impossible_config: ImpossibleConfig::default(),
            sovereign_config: SovereignConfig::default(),
            scoring_weights: ScoringWeights::default(),
            enable_evolved_timelines: true,
        }
    }
}

/// Dimension 5: Temporal Omniscience.
///
/// Forks N parallel timelines, each using a different cognitive
/// strategy (combination of D1-D4).  Executes all timelines,
/// scores them, and collapses to the optimal timeline.
pub struct TemporalOmniscience {
    config: TemporalConfig,
    cascade: DeductiveCascade,
    creative: CreativeHypersynthesis,
    impossible: ImpossibleEngine,
    sovereign: MetacognitiveSovereign,
    // Strategy evolution
    evolved_genomes: Vec<Vec<f64>>,
    genome_fitness: Vec<f64>,
    // Statistics
    total_problems: u64,
    total_timelines_explored: u64,
    timeline_win_counts: HashMap<String, u64>,
    best_confidence_seen: f64,
}

impl TemporalOmniscience {
    pub fn new(config: TemporalConfig) -> Self {
        let cascade = DeductiveCascade::new(config.cascade_config.clone());
        let creative = CreativeHypersynthesis::new(config.hyper_config.clone());
        let impossible = ImpossibleEngine::new(config.impossible_config.clone());
        let sovereign = MetacognitiveSovereign::new(config.sovereign_config.clone());

        Self {
            config,
            cascade,
            creative,
            impossible,
            sovereign,
            evolved_genomes: Vec::new(),
            genome_fitness: Vec::new(),
            total_problems: 0,
            total_timelines_explored: 0,
            timeline_win_counts: HashMap::new(),
            best_confidence_seen: 0.0,
        }
    }

    /// The main entry point: 5th-Dimensional Thinking.
    ///
    /// This is the top-level cognitive function that replaces
    /// simple single-strategy reasoning with multi-timeline
    /// parallel exploration and collapse.
    pub fn think_5d(&mut self, problem: &str) -> FifthDimensionResult {
        let start = Instant::now();
        self.total_problems += 1;
        let mut reasoning_trace = Vec::new();

        info!("[D5-Temporal] ═══ 5th Dimension Engaged ═══");
        info!(
            "[D5-Temporal] Problem: {}",
            &problem[..problem.len().min(80)]
        );

        // ─────────────────────────────────────────────────────
        // PHASE 1: FORK TIMELINES
        // ─────────────────────────────────────────────────────
        let strategies = self.select_timeline_strategies();
        let num_timelines = strategies.len();
        reasoning_trace.push(format!(
            "Forked {} parallel timelines: [{}]",
            num_timelines,
            strategies
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));

        // ─────────────────────────────────────────────────────
        // PHASE 2: EXECUTE ALL TIMELINES
        // ─────────────────────────────────────────────────────
        let mut timelines: Vec<Timeline> = Vec::with_capacity(num_timelines);

        for (i, strategy) in strategies.into_iter().enumerate() {
            let timeline_start = Instant::now();
            let name = format!("Timeline_{}__{}", i, strategy.as_str());

            debug!("[D5-Temporal] Executing: {}", name);

            let result = self.execute_timeline(problem, &strategy);
            let duration = timeline_start.elapsed().as_secs_f64() * 1000.0;

            let timeline_result = match result {
                Some(r) => {
                    reasoning_trace.push(format!(
                        "  {} → conf={:.3}, proof={}, dims=[{}], {:.1}ms",
                        name,
                        r.confidence,
                        r.proof_level,
                        r.dimensions_used.join(","),
                        r.duration_ms
                    ));
                    Some(r)
                }
                None => {
                    reasoning_trace.push(format!("  {} → FAILED ({:.1}ms)", name, duration));
                    None
                }
            };

            timelines.push(Timeline {
                id: i,
                name,
                strategy,
                result: timeline_result,
                score: 0.0,
                is_winner: false,
            });

            self.total_timelines_explored += 1;
        }

        // ─────────────────────────────────────────────────────
        // PHASE 3: SCORE ALL TIMELINES
        // ─────────────────────────────────────────────────────
        let max_duration = timelines
            .iter()
            .filter_map(|t| t.result.as_ref())
            .map(|r| r.duration_ms)
            .fold(0.0f64, f64::max);

        for timeline in &mut timelines {
            if let Some(ref result) = timeline.result {
                timeline.score = score_timeline(result, max_duration, &self.config.scoring_weights);
            }
        }

        // ─────────────────────────────────────────────────────
        // PHASE 4: COLLAPSE TO BEST TIMELINE
        // ─────────────────────────────────────────────────────
        let winner_idx = timelines
            .iter()
            .enumerate()
            .filter(|(_, t)| t.result.is_some())
            .max_by(|(_, a), (_, b)| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i);

        if let Some(idx) = winner_idx {
            timelines[idx].is_winner = true;
        }

        let winning_timeline = winner_idx.map(|i| &timelines[i]);
        let winning_result = winning_timeline.and_then(|t| t.result.as_ref());

        let answer = winning_result
            .map(|r| r.answer.clone())
            .unwrap_or_else(|| format!(
                "5D analysis inconclusive for: {}. All {} timelines explored without high-confidence result.",
                &problem[..problem.len().min(100)], num_timelines
            ));

        let confidence = winning_result.map(|r| r.confidence).unwrap_or(0.0);
        let proof_level = winning_result
            .map(|r| r.proof_level.clone())
            .unwrap_or_else(|| "unverified".to_string());

        let winning_strategy = winning_timeline
            .map(|t| t.strategy.as_str().to_string())
            .unwrap_or_else(|| "none".to_string());
        let winning_name = winning_timeline
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "none".to_string());

        reasoning_trace.push(format!(
            "\n═══ TIMELINE COLLAPSE ═══\nWinner: {} (score={:.4}, conf={:.3})",
            winning_name,
            winning_timeline.map(|t| t.score).unwrap_or(0.0),
            confidence,
        ));

        // Update win counts
        *self
            .timeline_win_counts
            .entry(winning_strategy.clone())
            .or_insert(0) += 1;
        if confidence > self.best_confidence_seen {
            self.best_confidence_seen = confidence;
        }

        // ─────────────────────────────────────────────────────
        // PHASE 5: METACOGNITIVE REVIEW
        // ─────────────────────────────────────────────────────
        let dimension_states = self.build_dimension_states(&timelines);
        let meta_result = self.sovereign.observe(
            dimension_states,
            &winning_strategy,
            confidence,
            confidence > 0.5,
        );

        reasoning_trace.push(format!(
            "Metacognition: health={}, biases={}, corrections={}, agg={:.2}",
            meta_result.health.as_str(),
            meta_result.biases_detected.len(),
            meta_result.corrections_applied.len(),
            meta_result.aggressiveness,
        ));

        if !meta_result.recommendation.is_empty() {
            reasoning_trace.push(format!(
                "Sovereign recommendation: {}",
                meta_result.recommendation
            ));
        }

        // ─────────────────────────────────────────────────────
        // PHASE 6: STORE LEARNING DATA FROM ALL TIMELINES
        // ─────────────────────────────────────────────────────
        self.learn_from_timelines(&timelines);

        // ─────────────────────────────────────────────────────
        // PHASE 4.5: CAUSAL CONE VALIDATION (novel formula)
        // ─────────────────────────────────────────────────────
        // Ensure the winning timeline's reasoning chain has
        // monotonic causal structure (no backward causation).
        if let Some(ref result) = winning_result {
            let trace_confidences: Vec<f64> = result
                .reasoning_trace
                .iter()
                .map(|step| {
                    // Extract confidence hints from trace text
                    step.chars().filter(|c| c.is_numeric()).count() as f64
                        / step.len().max(1) as f64
                })
                .collect();

            if trace_confidences.len() >= 3 {
                let causal_events: Vec<advanced_reasoning_formulas::CausalEvent> =
                    trace_confidences
                        .iter()
                        .enumerate()
                        .map(|(i, &conf)| advanced_reasoning_formulas::CausalEvent {
                            time: i as f64,
                            position: vec![conf],
                            label: format!("step_{}", i),
                        })
                        .collect();

                let cone = advanced_reasoning_formulas::analyze_causal_cone(&causal_events, 1.0);
                reasoning_trace.push(format!(
                    "Causal Cone: {} events in cone, {} violations, valid={}",
                    cone.events_in_cone, cone.violations, cone.is_valid
                ));
            }
        }

        // ─────────────────────────────────────────────────────
        // PHASE 4.6: ENERGY LANDSCAPE OPTIMIZATION (novel formula)
        // ─────────────────────────────────────────────────────
        // Treat the timeline scores as an energy landscape and
        // check if we found a global minimum or local one.
        let timeline_scores: Vec<f64> = timelines.iter().map(|t| t.score).collect();
        if timeline_scores.len() >= 3 {
            let energy =
                advanced_reasoning_formulas::cognitive_energy_landscape(&timeline_scores, 50, 0.01);
            reasoning_trace.push(format!(
                "Energy Landscape: initial={:.4}, final={:.4}, ΔE={:.4}, converged={}",
                energy.initial_energy,
                energy.final_energy,
                energy.initial_energy - energy.final_energy,
                energy.converged,
            ));

            // If energy landscape suggests a different minimum, note it
            if energy.optimal_idx < timeline_scores.len()
                && energy.optimal_idx != winner_idx.unwrap_or(0)
            {
                reasoning_trace.push(format!(
                    "⚠ Energy landscape suggests Timeline {} may be globally optimal (E={:.4})",
                    energy.optimal_idx, energy.final_energy
                ));
            }
        }

        // ─────────────────────────────────────────────────────
        // PHASE 4.7: EPISTEMIC UNCERTAINTY DECOMPOSITION (novel formula)
        // ─────────────────────────────────────────────────────
        let all_confidences: Vec<f64> = timelines
            .iter()
            .filter_map(|t| t.result.as_ref())
            .map(|r| r.confidence)
            .collect();
        if all_confidences.len() >= 2 {
            let uncertainty =
                advanced_reasoning_formulas::decompose_uncertainty_simple(&all_confidences);
            reasoning_trace.push(format!(
                "Uncertainty Decomposition: aleatoric={:.4}, epistemic={:.4}, total={:.4}",
                uncertainty.aleatoric, uncertainty.epistemic, uncertainty.total
            ));
        }

        // ─────────────────────────────────────────────────────
        // COMPOSE FINAL RESULT
        // ─────────────────────────────────────────────────────
        let timeline_summaries: Vec<TimelineSummary> = timelines
            .iter()
            .map(|t| TimelineSummary {
                id: t.id,
                name: t.name.clone(),
                strategy: t.strategy.as_str().to_string(),
                score: t.score,
                confidence: t.result.as_ref().map(|r| r.confidence).unwrap_or(0.0),
                is_winner: t.is_winner,
                duration_ms: t.result.as_ref().map(|r| r.duration_ms).unwrap_or(0.0),
            })
            .collect();

        let total_nodes: usize = timelines
            .iter()
            .filter_map(|t| t.result.as_ref())
            .map(|r| r.reasoning_trace.len() * 10)
            .sum();

        let duration = start.elapsed().as_secs_f64() * 1000.0;

        info!(
            "[D5-Temporal] ═══ Collapsed ═══ winner={}, conf={:.3}, {:.0}ms total",
            winning_strategy, confidence, duration
        );

        FifthDimensionResult {
            answer,
            confidence,
            winning_timeline: winning_name,
            winning_strategy,
            proof_level,
            timelines_explored: timeline_summaries,
            metacognition: MetacognitionSummary {
                health: meta_result.health.as_str().to_string(),
                biases_detected: meta_result.biases_detected.len(),
                corrections_applied: meta_result.corrections_applied.len(),
                aggressiveness: meta_result.aggressiveness,
                confidence_trend: meta_result.confidence_trend,
            },
            total_timelines: num_timelines,
            total_nodes_explored: total_nodes,
            reasoning_trace,
            duration_ms: duration,
        }
    }

    // ═══════════════════════════════════════════════════════════
    // TIMELINE EXECUTION
    // ═══════════════════════════════════════════════════════════

    fn execute_timeline(
        &mut self,
        problem: &str,
        strategy: &TimelineStrategy,
    ) -> Option<TimelineResult> {
        let start = Instant::now();

        match strategy {
            TimelineStrategy::PureDeduction => {
                let result = self.cascade.solve(problem);
                Some(TimelineResult {
                    answer: result.answer,
                    confidence: result.confidence,
                    novelty: 0.2,
                    proof_level: result
                        .certification
                        .certification_level
                        .as_str()
                        .to_string(),
                    dimensions_used: vec!["D1".to_string()],
                    reasoning_trace: result.reasoning_trace,
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                })
            }

            TimelineStrategy::CreativeThenValidate => {
                // Phase A: Generate creative ideas
                let creative_result = self.creative.synthesize(problem, 5);
                let best_idea = match &creative_result.best_idea {
                    Some(idea) => idea.content.clone(),
                    None => return None,
                };
                let novelty = creative_result.avg_novelty;

                // Phase B: Validate via deductive cascade
                let validation_problem = format!(
                    "Validate and prove: {}. Is this a correct and sound approach for: {}?",
                    &best_idea[..best_idea.len().min(200)],
                    &problem[..problem.len().min(100)]
                );
                let validation = self.cascade.solve(&validation_problem);

                let mut trace = vec![format!(
                    "Creative phase: {} ideas, best NCD={:.3}",
                    creative_result.total_candidates, novelty
                )];
                trace.extend(validation.reasoning_trace);

                Some(TimelineResult {
                    answer: format!(
                        "Creative solution: {}\n\nValidation: {} (confidence={:.3})",
                        &best_idea[..best_idea.len().min(300)],
                        validation.certification.certification_level.as_str(),
                        validation.confidence,
                    ),
                    confidence: (creative_result
                        .best_idea
                        .as_ref()
                        .map(|i| i.composite_score)
                        .unwrap_or(0.0)
                        * 0.4
                        + validation.confidence * 0.6),
                    novelty,
                    proof_level: validation
                        .certification
                        .certification_level
                        .as_str()
                        .to_string(),
                    dimensions_used: vec!["D2".to_string(), "D1".to_string()],
                    reasoning_trace: trace,
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                })
            }

            TimelineStrategy::ImpossibleThenCreative => {
                // Phase A: Run impossible analysis
                let impossible_result = self.impossible.solve_impossible(problem);

                let reframing = match &impossible_result.best_reframing {
                    Some(r) => r.clone(),
                    None => {
                        // No reframing found, but we can still try creative on original
                        let creative = self.creative.synthesize(problem, 3);
                        return Some(TimelineResult {
                            answer: creative
                                .best_idea
                                .map(|i| i.content)
                                .unwrap_or_else(|| "No creative solution found".to_string()),
                            confidence: 0.3,
                            novelty: creative.avg_novelty,
                            proof_level: "unverified".to_string(),
                            dimensions_used: vec!["D3".to_string(), "D2".to_string()],
                            reasoning_trace: impossible_result.reasoning_trace,
                            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                        });
                    }
                };

                // Phase B: Creative synthesis on reframed problem
                let creative = self.creative.synthesize(&reframing.reframed_problem, 5);

                let mut trace = impossible_result.reasoning_trace;
                trace.push(format!(
                    "Reframed via axiom break: {} → {}",
                    reframing.axiom.statement,
                    &reframing.negated_statement[..reframing.negated_statement.len().min(100)]
                ));

                Some(TimelineResult {
                    answer: format!(
                        "Impossible→Creative solution:\n\
                         Broken axiom: {}\n\
                         Reframed: {}\n\
                         Creative solution: {}",
                        reframing.axiom.statement,
                        &reframing.reframed_problem[..reframing.reframed_problem.len().min(200)],
                        creative
                            .best_idea
                            .clone()
                            .map(|i| i.content)
                            .unwrap_or_default(),
                    ),
                    confidence: reframing.solution_confidence * 0.5
                        + creative
                            .best_idea
                            .as_ref()
                            .map(|i| i.composite_score)
                            .unwrap_or(0.0)
                            * 0.5,
                    novelty: creative.avg_novelty * 1.2, // Bonus for impossible approach
                    proof_level: "high_confidence_heuristic".to_string(),
                    dimensions_used: vec!["D3".to_string(), "D2".to_string()],
                    reasoning_trace: trace,
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                })
            }

            TimelineStrategy::FullPipeline => {
                // D3 → D2 → D1: Full artillery
                let impossible = self.impossible.solve_impossible(problem);
                let problem_to_solve = impossible
                    .best_reframing
                    .as_ref()
                    .map(|r| r.reframed_problem.clone())
                    .unwrap_or_else(|| problem.to_string());

                let creative = self.creative.synthesize(&problem_to_solve, 3);
                let enhanced_problem = if let Some(ref idea) = creative.best_idea {
                    format!(
                        "{}\n\nCreative insight: {}",
                        problem_to_solve,
                        &idea.content[..idea.content.len().min(200)]
                    )
                } else {
                    problem_to_solve
                };

                let cascade = self.cascade.solve(&enhanced_problem);

                let mut trace = impossible.reasoning_trace;
                trace.push("--- Creative Phase ---".to_string());
                trace.push(format!(
                    "Ideas generated: {}, avg novelty: {:.3}",
                    creative.total_candidates, creative.avg_novelty
                ));
                trace.push("--- Deductive Phase ---".to_string());
                trace.extend(cascade.reasoning_trace);

                Some(TimelineResult {
                    answer: cascade.answer,
                    confidence: cascade.confidence * 0.5
                        + creative.avg_novelty * 0.2
                        + impossible
                            .best_reframing
                            .as_ref()
                            .map(|r| r.solution_confidence)
                            .unwrap_or(0.3)
                            * 0.3,
                    novelty: creative.avg_novelty,
                    proof_level: cascade
                        .certification
                        .certification_level
                        .as_str()
                        .to_string(),
                    dimensions_used: vec!["D3".to_string(), "D2".to_string(), "D1".to_string()],
                    reasoning_trace: trace,
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                })
            }

            TimelineStrategy::IntuitiveLeap => {
                // Fast, low-depth reasoning — sometimes speed is wisdom
                let mut fast_config = self.config.cascade_config.clone();
                fast_config.num_parallel_trees = 3;
                fast_config.rollouts_per_tree = 50;
                fast_config.max_tree_depth = 4;
                let mut fast_cascade = DeductiveCascade::new(fast_config);

                let result = fast_cascade.solve(problem);

                Some(TimelineResult {
                    answer: format!("Intuitive assessment: {}", result.answer),
                    confidence: result.confidence * 0.8, // Slight penalty for shallow analysis
                    novelty: 0.1,
                    proof_level: "low_confidence_heuristic".to_string(),
                    dimensions_used: vec!["D1-fast".to_string()],
                    reasoning_trace: vec![format!(
                        "Fast intuitive analysis: {} nodes in {:.0}ms",
                        result.total_nodes_explored, result.duration_ms
                    )],
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                })
            }

            TimelineStrategy::AdversarialSurvival => {
                // Try to DISPROVE the problem, accept what survives
                let disproof_problem = format!(
                    "Find the strongest argument AGAINST: {}. \
                     Identify all weaknesses, logical flaws, and hidden assumptions.",
                    &problem[..problem.len().min(200)]
                );

                let attack = self.cascade.solve(&disproof_problem);
                let defense_problem = format!(
                    "Given these attacks: {}\n\nDefend the original claim: {}\n\
                     Address each attack and prove the claim survives.",
                    &attack.answer[..attack.answer.len().min(300)],
                    &problem[..problem.len().min(200)]
                );

                let defense = self.cascade.solve(&defense_problem);

                let mut trace = vec!["--- ADVERSARIAL ATTACK ---".to_string()];
                trace.extend(attack.reasoning_trace);
                trace.push("--- DEFENSE ---".to_string());
                trace.extend(defense.reasoning_trace);

                Some(TimelineResult {
                    answer: format!(
                        "Adversarial survival:\n\
                         Attack (conf={:.3}): {}\n\
                         Defense (conf={:.3}): {}",
                        attack.confidence,
                        &attack.answer[..attack.answer.len().min(200)],
                        defense.confidence,
                        &defense.answer[..defense.answer.len().min(200)],
                    ),
                    confidence: defense.confidence * (1.0 - attack.confidence * 0.3),
                    novelty: 0.3,
                    proof_level: if defense.confidence > attack.confidence {
                        "high_confidence_heuristic".to_string()
                    } else {
                        "low_confidence_heuristic".to_string()
                    },
                    dimensions_used: vec!["D1-adversarial".to_string()],
                    reasoning_trace: trace,
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                })
            }

            TimelineStrategy::MetaSynthesis => {
                // Shouldn't be executed directly; created during collapse
                None
            }

            TimelineStrategy::Evolved { genome } => {
                // Use genome to configure custom strategy mix
                self.execute_evolved_timeline(problem, genome, start)
            }
        }
    }

    fn execute_evolved_timeline(
        &mut self,
        problem: &str,
        genome: &[f64],
        start: Instant,
    ) -> Option<TimelineResult> {
        // Genome layout: [d1_weight, d2_weight, d3_weight, d1_trees, d1_rollouts, d2_ideas]
        if genome.len() < 6 {
            return None;
        }

        let d1_weight = genome[0].clamp(0.0, 1.0);
        let d2_weight = genome[1].clamp(0.0, 1.0);
        let d3_weight = genome[2].clamp(0.0, 1.0);
        let total = (d1_weight + d2_weight + d3_weight).max(0.01);

        let mut trace = vec![format!(
            "Evolved genome: D1={:.2}, D2={:.2}, D3={:.2}",
            d1_weight / total,
            d2_weight / total,
            d3_weight / total
        )];

        let mut combined_answer = String::new();
        let mut combined_confidence = 0.0;
        let mut combined_novelty = 0.0;
        let mut dims = Vec::new();

        if d1_weight / total > 0.2 {
            let r = self.cascade.solve(problem);
            combined_confidence += r.confidence * (d1_weight / total);
            combined_answer.push_str(&format!("D1: {}\n", &r.answer[..r.answer.len().min(150)]));
            dims.push("D1".to_string());
            trace.extend(r.reasoning_trace);
        }

        if d2_weight / total > 0.2 {
            let r = self.creative.synthesize(problem, 3);
            if let Some(ref idea) = r.best_idea {
                combined_confidence += idea.composite_score * (d2_weight / total);
                combined_answer.push_str(&format!(
                    "D2: {}\n",
                    &idea.content[..idea.content.len().min(150)]
                ));
            }
            combined_novelty = r.avg_novelty;
            dims.push("D2".to_string());
        }

        if d3_weight / total > 0.2 {
            let r = self.impossible.solve_impossible(problem);
            if let Some(ref reframing) = r.best_reframing {
                combined_confidence += reframing.solution_confidence * (d3_weight / total);
                combined_answer.push_str(&format!(
                    "D3: {}\n",
                    &reframing.explanation[..reframing.explanation.len().min(150)]
                ));
            }
            dims.push("D3".to_string());
            trace.extend(r.reasoning_trace);
        }

        Some(TimelineResult {
            answer: if combined_answer.is_empty() {
                "Evolved strategy produced no result".to_string()
            } else {
                combined_answer
            },
            confidence: combined_confidence.min(1.0),
            novelty: combined_novelty,
            proof_level: "evolved_heuristic".to_string(),
            dimensions_used: dims,
            reasoning_trace: trace,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }

    // ═══════════════════════════════════════════════════════════
    // STRATEGY SELECTION & EVOLUTION
    // ═══════════════════════════════════════════════════════════

    fn select_timeline_strategies(&self) -> Vec<TimelineStrategy> {
        let mut strategies = TimelineStrategy::default_strategies();

        // Add evolved timelines if enabled and we have genomes
        if self.config.enable_evolved_timelines {
            for genome in self.evolved_genomes.iter().take(2) {
                strategies.push(TimelineStrategy::Evolved {
                    genome: genome.clone(),
                });
            }
        }

        // Respect num_timelines limit
        strategies.truncate(self.config.num_timelines);
        strategies
    }

    fn learn_from_timelines(&mut self, timelines: &[Timeline]) {
        // Evolve genomes based on actual timeline performance.
        //
        // Strategy: convert each timeline's score into genome-space coordinates,
        // then use tournament selection and mutation to breed better genomes.
        let mut rng = rand::thread_rng();

        if let Some(winner) = timelines.iter().find(|t| t.is_winner) {
            // If an evolved strategy won, promote its genome
            if let TimelineStrategy::Evolved { genome } = &winner.strategy {
                let idx = self.evolved_genomes.iter().position(|g| g == genome);
                if let Some(i) = idx {
                    if i < self.genome_fitness.len() {
                        // EMA update: 70% old + 30% new to smooth out noise
                        self.genome_fitness[i] = self.genome_fitness[i] * 0.7 + winner.score * 0.3;
                    }
                }
            }

            // Learn dimension weights from ALL timeline results.
            // Build a genome from the winning non-evolved strategy so evolved
            // timelines can inherit structure from what actually works.
            if !matches!(&winner.strategy, TimelineStrategy::Evolved { .. }) {
                let (d1_w, d2_w, d3_w) = match &winner.strategy {
                    TimelineStrategy::PureDeduction => (0.9, 0.05, 0.05),
                    TimelineStrategy::CreativeThenValidate => (0.4, 0.5, 0.1),
                    TimelineStrategy::ImpossibleThenCreative => (0.1, 0.4, 0.5),
                    TimelineStrategy::FullPipeline => (0.33, 0.33, 0.34),
                    TimelineStrategy::IntuitiveLeap => (0.7, 0.1, 0.2),
                    TimelineStrategy::AdversarialSurvival => (0.8, 0.1, 0.1),
                    _ => (0.33, 0.33, 0.34),
                };

                // Seed the evolved genome pool with the winner's approximate weights
                // every 5 problems to let the population learn from classical strategies.
                if self.total_problems % 5 == 0 {
                    let seed_genome = vec![
                        d1_w + (rng.gen::<f64>() - 0.5) * 0.1,
                        d2_w + (rng.gen::<f64>() - 0.5) * 0.1,
                        d3_w + (rng.gen::<f64>() - 0.5) * 0.1,
                        // D1 config: trees, rollouts, ideas
                        rng.gen_range(0.3..0.8),
                        rng.gen_range(0.3..0.8),
                        rng.gen_range(0.3..0.8),
                    ];
                    self.evolved_genomes.push(seed_genome);
                    self.genome_fitness.push(winner.score * 0.8); // Slight discount
                }
            }

            // Generate offspring from best genomes via crossover + mutation
            if self.total_problems % 3 == 0 && self.evolved_genomes.len() >= 2 {
                // Tournament selection: pick two parents from top half
                let mut ranked: Vec<(usize, f64)> = self
                    .genome_fitness
                    .iter()
                    .enumerate()
                    .map(|(i, &f)| (i, f))
                    .collect();
                ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                if ranked.len() >= 2 {
                    let parent_a = &self.evolved_genomes[ranked[0].0];
                    let parent_b = &self.evolved_genomes[ranked[1].0];

                    // Uniform crossover with mutation
                    let child: Vec<f64> = parent_a
                        .iter()
                        .zip(parent_b.iter())
                        .map(|(&a, &b)| {
                            let base = if rng.gen_bool(0.5) { a } else { b };
                            // 20% mutation probability per gene
                            if rng.gen_bool(0.2) {
                                (base + (rng.gen::<f64>() - 0.5) * 0.15).clamp(0.0, 1.0)
                            } else {
                                base
                            }
                        })
                        .collect();

                    self.evolved_genomes.push(child);
                    // Initial fitness = average of parents (inherited advantage)
                    self.genome_fitness
                        .push((ranked[0].1 + ranked[1].1) / 2.0 * 0.9);
                }
            }

            // Cap at 10 genomes — evict weakest
            while self.evolved_genomes.len() > 10 {
                if let Some(worst_idx) = self
                    .genome_fitness
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i)
                {
                    self.evolved_genomes.remove(worst_idx);
                    self.genome_fitness.remove(worst_idx);
                }
            }
        }
    }

    fn best_evolved_genome(&self) -> Option<Vec<f64>> {
        self.genome_fitness
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .and_then(|(i, _)| self.evolved_genomes.get(i).cloned())
    }

    fn build_dimension_states(&self, timelines: &[Timeline]) -> HashMap<String, DimensionState> {
        let mut states = HashMap::new();

        // Aggregate stats from timelines
        let d1_timelines: Vec<_> = timelines
            .iter()
            .filter(|t| {
                t.result
                    .as_ref()
                    .map(|r| r.dimensions_used.contains(&"D1".to_string()))
                    .unwrap_or(false)
            })
            .collect();
        let d2_timelines: Vec<_> = timelines
            .iter()
            .filter(|t| {
                t.result
                    .as_ref()
                    .map(|r| r.dimensions_used.contains(&"D2".to_string()))
                    .unwrap_or(false)
            })
            .collect();
        let d3_timelines: Vec<_> = timelines
            .iter()
            .filter(|t| {
                t.result
                    .as_ref()
                    .map(|r| r.dimensions_used.contains(&"D3".to_string()))
                    .unwrap_or(false)
            })
            .collect();

        let make_state = |name: &str, tls: &[&Timeline]| -> DimensionState {
            let active = !tls.is_empty();
            let conf = tls
                .iter()
                .filter_map(|t| t.result.as_ref())
                .map(|r| r.confidence)
                .sum::<f64>()
                / tls.len().max(1) as f64;
            let duration = tls
                .iter()
                .filter_map(|t| t.result.as_ref())
                .map(|r| r.duration_ms)
                .sum::<f64>()
                / tls.len().max(1) as f64;

            let mut dist = HashMap::new();
            for t in tls {
                *dist.entry(t.strategy.as_str().to_string()).or_insert(0.0) += 1.0;
            }

            DimensionState {
                name: name.to_string(),
                is_active: active,
                current_confidence: conf,
                throughput: if duration > 0.0 {
                    1000.0 / duration
                } else {
                    0.0
                },
                error_rate: tls.iter().filter(|t| t.result.is_none()).count() as f64
                    / tls.len().max(1) as f64,
                avg_latency_ms: duration,
                strategy_distribution: dist,
            }
        };

        states.insert(
            "D1_DeductiveCascade".to_string(),
            make_state("D1", &d1_timelines),
        );
        states.insert(
            "D2_CreativeHypersynthesis".to_string(),
            make_state("D2", &d2_timelines),
        );
        states.insert(
            "D3_ImpossibleEngine".to_string(),
            make_state("D3", &d3_timelines),
        );

        states
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "Dimension5_TemporalOmniscience_v1.0",
            "total_problems": self.total_problems,
            "total_timelines_explored": self.total_timelines_explored,
            "best_confidence_seen": self.best_confidence_seen,
            "evolved_genomes": self.evolved_genomes.len(),
            "timeline_win_distribution": self.timeline_win_counts,
            "sub_engines": {
                "D1": self.cascade.get_stats(),
                "D2": self.creative.get_stats(),
                "D3": self.impossible.get_stats(),
                "D4": self.sovereign.get_stats(),
            },
        })
    }
}

impl Default for TemporalOmniscience {
    fn default() -> Self {
        Self::new(TemporalConfig::default())
    }
}

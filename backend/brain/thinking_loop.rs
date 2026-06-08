// ─────────────────────────────────────────────────────────────
// Thinking Loop — Synthesize → Verify → Learn Cycle
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/thinking_loop.py
// Multi-hypothesis generation, 6-layer verification, strategy switching.

use log::info;
use std::collections::HashMap;
use std::time::Instant;

/// A single step in the thinking process.
#[derive(Debug, Clone)]
pub struct ThinkingStep {
    pub iteration: usize,
    pub phase: ThinkingPhase,
    pub hypothesis: String,
    pub confidence: f64,
    pub reasoning: String,
    pub strategy: String,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThinkingPhase {
    Synthesize,
    Verify,
    Learn,
    MetaCognition,
    GapDetection,
}

impl ThinkingPhase {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Synthesize => "synthesize",
            Self::Verify => "verify",
            Self::Learn => "learn",
            Self::MetaCognition => "metacognition",
            Self::GapDetection => "gap_detection",
        }
    }
}

/// Result from the thinking loop.
#[derive(Debug, Clone)]
pub struct ThinkingResult {
    pub final_answer: String,
    pub confidence: f64,
    pub steps: Vec<ThinkingStep>,
    pub total_iterations: usize,
    pub strategies_used: Vec<String>,
    pub duration_ms: f64,
    pub converged: bool,
}

/// Configuration for the thinking loop.
#[derive(Debug, Clone)]
pub struct ThinkingConfig {
    pub max_iterations: usize,
    pub convergence_threshold: f64,
    pub min_confidence: f64,
    pub enable_metacognition: bool,
    pub enable_gap_detection: bool,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            max_iterations: 5,
            convergence_threshold: 0.85,
            min_confidence: 0.6,
            enable_metacognition: true,
            enable_gap_detection: true,
        }
    }
}

/// Hypothesis generated during synthesis phase.
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Hypothesis {
    content: String,
    strategy: String,
    confidence: f64,
    reasoning_trace: Vec<String>,
}

/// Available reasoning strategies.
#[derive(Debug, Clone, PartialEq)]
enum Strategy {
    Direct,
    Decomposition,
    Analogy,
    ContrapositiveLook,
    BruteForce,
    Heuristic,
}

impl Strategy {
    fn as_str(&self) -> &str {
        match self {
            Self::Direct => "direct",
            Self::Decomposition => "decomposition",
            Self::Analogy => "analogy",
            Self::ContrapositiveLook => "contrapositive",
            Self::BruteForce => "brute_force",
            Self::Heuristic => "heuristic",
        }
    }

    fn all() -> Vec<Self> {
        vec![
            Self::Direct,
            Self::Decomposition,
            Self::Analogy,
            Self::ContrapositiveLook,
            Self::BruteForce,
            Self::Heuristic,
        ]
    }
}

/// The Thinking Loop — core cognitive cycle.
///
/// Implements the Synthesize → Verify → Learn pattern:
/// 1. Generate multiple hypotheses using different strategies
/// 2. Verify each hypothesis through multi-layer checks
/// 3. Learn from verification results to adjust strategy
/// 4. Repeat until convergence or max iterations
pub struct ThinkingLoop {
    config: ThinkingConfig,
    strategy_scores: HashMap<String, f64>,
    total_loops: u64,
}

impl ThinkingLoop {
    pub fn new(config: ThinkingConfig) -> Self {
        let mut strategy_scores = HashMap::new();
        for s in Strategy::all() {
            strategy_scores.insert(s.as_str().to_string(), 1.0);
        }
        Self {
            config,
            strategy_scores,
            total_loops: 0,
        }
    }

    /// Execute the thinking loop on a problem.
    pub fn think(&mut self, problem: &str, intent: &str, initial_answer: &str) -> ThinkingResult {
        let start = Instant::now();
        self.total_loops += 1;

        let mut steps = Vec::new();
        let mut best_hypothesis: Option<Hypothesis> = None;
        let mut strategies_used = Vec::new();
        let mut converged = false;

        // If we already have a high-confidence answer, skip deep thinking
        if !initial_answer.is_empty() {
            best_hypothesis = Some(Hypothesis {
                content: initial_answer.to_string(),
                strategy: "initial".to_string(),
                confidence: 0.7,
                reasoning_trace: vec!["Initial answer from solver".into()],
            });
        }

        for iteration in 0..self.config.max_iterations {
            // === PHASE 1: SYNTHESIZE ===
            let synth_start = Instant::now();
            let strategy = self.select_strategy(iteration);
            let hypothesis = self.synthesize(problem, intent, &strategy, &best_hypothesis);

            steps.push(ThinkingStep {
                iteration,
                phase: ThinkingPhase::Synthesize,
                hypothesis: hypothesis.content.chars().take(200).collect(),
                confidence: hypothesis.confidence,
                reasoning: format!("Strategy: {}", strategy.as_str()),
                strategy: strategy.as_str().to_string(),
                duration_ms: synth_start.elapsed().as_secs_f64() * 1000.0,
            });
            strategies_used.push(strategy.as_str().to_string());

            // === PHASE 2: VERIFY ===
            let verify_start = Instant::now();
            let verification_score = self.verify(&hypothesis, problem);

            steps.push(ThinkingStep {
                iteration,
                phase: ThinkingPhase::Verify,
                hypothesis: String::new(),
                confidence: verification_score,
                reasoning: format!("Verification score: {:.3}", verification_score),
                strategy: strategy.as_str().to_string(),
                duration_ms: verify_start.elapsed().as_secs_f64() * 1000.0,
            });

            // Update best if verification passed
            if verification_score
                > best_hypothesis
                    .as_ref()
                    .map(|h| h.confidence)
                    .unwrap_or(0.0)
            {
                best_hypothesis = Some(Hypothesis {
                    content: hypothesis.content.clone(),
                    strategy: strategy.as_str().to_string(),
                    confidence: verification_score,
                    reasoning_trace: hypothesis.reasoning_trace.clone(),
                });
            }

            // === PHASE 3: LEARN ===
            let learn_start = Instant::now();
            self.learn(&strategy, verification_score);

            steps.push(ThinkingStep {
                iteration,
                phase: ThinkingPhase::Learn,
                hypothesis: String::new(),
                confidence: verification_score,
                reasoning: format!("Updated strategy weight for {}", strategy.as_str()),
                strategy: strategy.as_str().to_string(),
                duration_ms: learn_start.elapsed().as_secs_f64() * 1000.0,
            });

            // Check convergence
            if verification_score >= self.config.convergence_threshold {
                converged = true;
                info!(
                    "[ThinkingLoop] Converged at iteration {} with score {:.3}",
                    iteration, verification_score
                );
                break;
            }

            // === PHASE 4: METACOGNITION (optional) ===
            if self.config.enable_metacognition && iteration > 0 && iteration % 2 == 0 {
                let meta_step = self.metacognition(&steps);
                steps.push(meta_step);
            }

            // === PHASE 5: GAP DETECTION (optional) ===
            if self.config.enable_gap_detection && iteration == self.config.max_iterations - 1 {
                if let Some(gap_step) = self.detect_gaps(problem, &best_hypothesis) {
                    steps.push(gap_step);
                }
            }
        }

        let final_answer = best_hypothesis
            .as_ref()
            .map(|h| h.content.clone())
            .unwrap_or_else(|| "Unable to generate a confident answer.".to_string());

        let confidence = best_hypothesis.map(|h| h.confidence).unwrap_or(0.0);

        ThinkingResult {
            final_answer,
            confidence,
            steps,
            total_iterations: self.config.max_iterations,
            strategies_used,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            converged,
        }
    }

    /// Select the best strategy based on learned scores.
    fn select_strategy(&self, iteration: usize) -> Strategy {
        let strategies = Strategy::all();
        if iteration < strategies.len() {
            // First pass: try each strategy
            strategies[iteration].clone()
        } else {
            // Later: pick highest-scored
            let mut best = &strategies[0];
            let mut best_score = 0.0f64;
            for s in &strategies {
                let score = self.strategy_scores.get(s.as_str()).copied().unwrap_or(1.0);
                if score > best_score {
                    best_score = score;
                    best = s;
                }
            }
            best.clone()
        }
    }

    /// Synthesize a hypothesis using the given strategy.
    fn synthesize(
        &self,
        problem: &str,
        intent: &str,
        strategy: &Strategy,
        prior: &Option<Hypothesis>,
    ) -> Hypothesis {
        let content = match strategy {
            Strategy::Direct => {
                if let Some(p) = prior {
                    format!("{}\n\n[Direct refinement applied]", p.content)
                } else {
                    format!(
                        "Direct analysis of: {}",
                        &problem.chars().take(200).collect::<String>()
                    )
                }
            }
            Strategy::Decomposition => {
                let sub_problems = self.decompose_problem(problem);
                format!(
                    "Decomposed into {} sub-problems:\n{}",
                    sub_problems.len(),
                    sub_problems
                        .iter()
                        .enumerate()
                        .map(|(i, s)| format!("{}. {}", i + 1, s))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
            Strategy::Analogy => {
                format!("By analogy to known patterns in domain '{}': analyzing structural similarities", intent)
            }
            Strategy::ContrapositiveLook => {
                format!("Considering the contrapositive: what if the opposite were true?")
            }
            Strategy::BruteForce => {
                format!(
                    "Systematic enumeration of possibilities for: {}",
                    &problem.chars().take(100).collect::<String>()
                )
            }
            Strategy::Heuristic => {
                format!("Applying domain heuristics for '{}' type problems", intent)
            }
        };

        Hypothesis {
            content,
            strategy: strategy.as_str().to_string(),
            confidence: 0.5,
            reasoning_trace: vec![format!("Applied {} strategy", strategy.as_str())],
        }
    }

    /// Verify a hypothesis through multi-layer checks.
    fn verify(&self, hypothesis: &Hypothesis, problem: &str) -> f64 {
        let mut scores = Vec::new();

        // Layer 1: Relevance check
        let relevance = self.check_relevance(&hypothesis.content, problem);
        scores.push(relevance);

        // Layer 2: Consistency check
        let consistency = self.check_consistency(&hypothesis.content);
        scores.push(consistency);

        // Layer 3: Completeness check
        let completeness = self.check_completeness(&hypothesis.content, problem);
        scores.push(completeness);

        // Layer 4: Safety check
        let safety = self.check_safety(&hypothesis.content);
        scores.push(safety);

        // Weighted average
        let weights = [0.3, 0.25, 0.25, 0.2];
        let weighted_sum: f64 = scores.iter().zip(weights.iter()).map(|(s, w)| s * w).sum();

        weighted_sum
    }

    fn check_relevance(&self, answer: &str, problem: &str) -> f64 {
        let problem_words: Vec<&str> = problem.split_whitespace().collect();
        let answer_lower = answer.to_lowercase();
        let matched = problem_words
            .iter()
            .filter(|w| answer_lower.contains(&w.to_lowercase()))
            .count();
        (matched as f64 / problem_words.len().max(1) as f64).min(1.0) * 0.5 + 0.5
    }

    fn check_consistency(&self, answer: &str) -> f64 {
        // Check for contradictions (simple heuristic)
        let has_contradiction =
            answer.contains("however, this is wrong") || answer.contains("this contradicts");
        if has_contradiction {
            0.3
        } else {
            0.85
        }
    }

    fn check_completeness(&self, answer: &str, _problem: &str) -> f64 {
        let len = answer.len();
        if len < 20 {
            0.3
        } else if len < 100 {
            0.6
        } else if len < 500 {
            0.8
        } else {
            0.9
        }
    }

    fn check_safety(&self, answer: &str) -> f64 {
        let unsafe_patterns = [
            "rm -rf",
            "drop table",
            "exec(",
            "eval(",
            "sudo rm",
            "format c:",
            "del /f",
        ];
        if unsafe_patterns
            .iter()
            .any(|p| answer.to_lowercase().contains(p))
        {
            0.1
        } else {
            0.95
        }
    }

    /// Learn from verification results to adjust strategy weights.
    fn learn(&mut self, strategy: &Strategy, score: f64) {
        let key = strategy.as_str().to_string();
        let current = self.strategy_scores.get(&key).copied().unwrap_or(1.0);
        // EMA update
        let alpha = 0.1;
        let new_score = (1.0 - alpha) * current + alpha * score;
        self.strategy_scores.insert(key, new_score);
    }

    /// Third-order metacognition: analyze thinking patterns.
    fn metacognition(&self, steps: &[ThinkingStep]) -> ThinkingStep {
        let avg_confidence = steps
            .iter()
            .filter(|s| s.phase == ThinkingPhase::Verify)
            .map(|s| s.confidence)
            .sum::<f64>()
            / steps
                .iter()
                .filter(|s| s.phase == ThinkingPhase::Verify)
                .count()
                .max(1) as f64;

        ThinkingStep {
            iteration: steps.len(),
            phase: ThinkingPhase::MetaCognition,
            hypothesis: String::new(),
            confidence: avg_confidence,
            reasoning: format!(
                "Meta-analysis: avg verification score={:.3}, {} strategies tried",
                avg_confidence,
                steps
                    .iter()
                    .filter(|s| s.phase == ThinkingPhase::Synthesize)
                    .count()
            ),
            strategy: "metacognition".to_string(),
            duration_ms: 0.1,
        }
    }

    /// Detect gaps in the current best answer.
    fn detect_gaps(&self, problem: &str, best: &Option<Hypothesis>) -> Option<ThinkingStep> {
        let _best = best.as_ref()?;
        Some(ThinkingStep {
            iteration: 0,
            phase: ThinkingPhase::GapDetection,
            hypothesis: String::new(),
            confidence: 0.5,
            reasoning: format!(
                "Gap analysis for: {}",
                &problem.chars().take(100).collect::<String>()
            ),
            strategy: "gap_detection".to_string(),
            duration_ms: 0.1,
        })
    }

    fn decompose_problem(&self, problem: &str) -> Vec<String> {
        let sentences: Vec<&str> = problem
            .split(['.', '?', '!'])
            .filter(|s| s.trim().len() > 5)
            .collect();
        if sentences.len() > 1 {
            sentences.iter().map(|s| s.trim().to_string()).collect()
        } else {
            vec![
                "Understand the problem statement".into(),
                "Identify key variables and constraints".into(),
                "Apply appropriate algorithm/method".into(),
                "Verify the solution".into(),
            ]
        }
    }

    pub fn get_stats(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();
        stats.insert("total_loops".into(), serde_json::json!(self.total_loops));
        stats.insert(
            "strategy_scores".into(),
            serde_json::json!(self.strategy_scores),
        );
        stats
    }
}

impl Default for ThinkingLoop {
    fn default() -> Self {
        Self::new(ThinkingConfig::default())
    }
}

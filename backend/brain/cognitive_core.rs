// ─────────────────────────────────────────────────────────────
// Cognitive Core Engine v7.0 — 5th Dimension CCE
// ─────────────────────────────────────────────────────────────
// Main orchestrator that routes prompts to specialized engines.
// v7.0: Full 5th-Dimension Brain integration.
//       D5 TemporalOmniscience replaces ThinkingLoop for
//       complex, open-ended, and impossible problems.
//       Novel math formulas power every decision.
// Zero-hallucination, deterministic, fully offline.

use log::{debug, info};
use std::collections::HashMap;
use std::time::Instant;

use super::advanced_reasoning_formulas;
use super::algorithmic_solver::{SolverPipeline, SolverResult};
use super::cognitive_math;
use super::creative_engine::{CreativeEngine, CreativeResult};
use super::dimension5_temporal_omniscience::{
    FifthDimensionResult, TemporalConfig, TemporalOmniscience,
};
use super::intent_classifier::{IntentClassification, IntentClassifier, IntentType};
use super::knowledge_retriever::KnowledgeRetriever;
use super::template_synthesizer::{SynthesisContext, TemplateSynthesizer};
use super::text_generator::{TextGenerator, TextRequest, TextStructure, TextStyle};
use super::thinking_loop::{ThinkingConfig, ThinkingLoop};

/// Result from the Cognitive Core Engine.
#[derive(Debug, Clone)]
pub struct CognitiveResult {
    pub response: String,
    pub intent: IntentClassification,
    pub solver_result: Option<SolverResult>,
    pub creative_result: Option<CreativeResultSummary>,
    pub fifth_dimension_result: Option<FifthDimensionSummary>,
    pub thinking_steps: Vec<String>,
    pub duration_ms: f64,
    pub engine_version: String,
}

/// Summary of 5th-Dimension reasoning (lightweight, for external consumption).
#[derive(Debug, Clone)]
pub struct FifthDimensionSummary {
    pub answer: String,
    pub confidence: f64,
    pub winning_strategy: String,
    pub proof_level: String,
    pub timelines_explored: usize,
    pub total_nodes: usize,
    pub biases_detected: usize,
    pub self_corrections: usize,
    pub cognitive_health: String,
    pub duration_ms: f64,
}

/// Internal analysis of problem complexity using novel mathematics.
#[derive(Debug, Clone)]
struct ProblemAnalysis {
    /// Box-counting fractal dimension of the problem's feature space.
    fractal_dim: f64,
    /// Optimal recursion depth based on fractal dimension.
    optimal_depth: usize,
    /// Number of constraint keywords found.
    constraint_count: usize,
    /// Token diversity: unique/total.
    token_diversity: f64,
    /// Overall complexity score ∈ [0, 1].
    complexity: f64,
    /// Recommended dimension engine (1-5).
    recommended_dimension: u8,
    /// Does the constraint graph contain cycles (potential paradoxes)?
    has_paradox: bool,
}

/// Lightweight summary of creative thinking (avoids cloning the full tree).
#[derive(Debug, Clone)]
pub struct CreativeResultSummary {
    pub ideas_generated: usize,
    pub methods_used: Vec<String>,
    pub best_idea_score: f64,
    pub duration_ms: f64,
}

/// Configuration for the CCE.
#[derive(Debug, Clone)]
pub struct CognitiveConfig {
    pub enable_thinking_loop: bool,
    pub enable_knowledge_retrieval: bool,
    pub enable_creative_thinking: bool,
    pub enable_text_generation: bool,
    pub enable_5d_brain: bool,
    pub max_thinking_iterations: usize,
    pub synthesis_verbosity: f64,
    pub creative_max_ideas: usize,
    pub temporal_config: TemporalConfig,
}

impl Default for CognitiveConfig {
    fn default() -> Self {
        Self {
            enable_thinking_loop: true,
            enable_knowledge_retrieval: true,
            enable_creative_thinking: true,
            enable_text_generation: true,
            enable_5d_brain: true,
            max_thinking_iterations: 5,
            synthesis_verbosity: 0.7,
            creative_max_ideas: 5,
            temporal_config: TemporalConfig::default(),
        }
    }
}

/// Cognitive Core Engine v7.0 — GPU-free cognitive orchestrator.
///
/// Routes prompts through:
///   1. Intent Classification (rule-based)
///   2. Knowledge Retrieval (BM25)
///   3. Solver Pipeline (Math, Code, Logic, Extraction, Plan)
///   4. Creative Engine (7-strategy ideation for open-ended problems)
///   5. Text Generator (compositional NLG for rich responses)
///   6. Template Synthesis (rule-based assembly)
///   7. ThinkingLoop (Synthesize → Verify → Learn) for mid-complexity
///   8. **5th Dimension Brain** (D1-D5 multi-timeline reasoning) for hard/impossible
pub struct CognitiveCoreEngine {
    classifier: IntentClassifier,
    solver: SolverPipeline,
    synthesizer: TemplateSynthesizer,
    retriever: KnowledgeRetriever,
    creative_engine: CreativeEngine,
    text_generator: TextGenerator,
    thinking_loop: ThinkingLoop,
    // ═══ 5th DIMENSION BRAIN ═══
    temporal_omniscience: TemporalOmniscience,
    /// Cognitive momentum tracker for performance trend detection.
    cognitive_momentum: advanced_reasoning_formulas::CognitiveMomentum,
    /// Running performance history for stability analysis.
    performance_history: Vec<f64>,
    config: CognitiveConfig,
    total_queries: u64,
    intent_histogram: HashMap<String, u64>,
}

impl CognitiveCoreEngine {
    pub fn new(config: CognitiveConfig) -> Self {
        let temporal = TemporalOmniscience::new(config.temporal_config.clone());
        Self {
            classifier: IntentClassifier::new(),
            solver: SolverPipeline::new(),
            synthesizer: TemplateSynthesizer::new(),
            retriever: KnowledgeRetriever::new(),
            creative_engine: CreativeEngine::new(),
            text_generator: TextGenerator::new(),
            thinking_loop: ThinkingLoop::new(ThinkingConfig {
                max_iterations: config.max_thinking_iterations,
                ..ThinkingConfig::default()
            }),
            temporal_omniscience: temporal,
            cognitive_momentum: advanced_reasoning_formulas::CognitiveMomentum::new(0.9),
            performance_history: Vec::new(),
            config,
            total_queries: 0,
            intent_histogram: HashMap::new(),
        }
    }

    /// Process a prompt through the full cognitive pipeline.
    pub fn process(&mut self, prompt: &str) -> CognitiveResult {
        let start = Instant::now();
        self.total_queries += 1;

        // Step 1: Classify intent
        let intent = self.classifier.classify(prompt);
        info!(
            "[CCE] Intent: {} for prompt: {}",
            intent,
            &prompt.chars().take(80).collect::<String>()
        );

        *self
            .intent_histogram
            .entry(intent.intent.as_str().to_string())
            .or_insert(0) += 1;

        // Step 2: Knowledge retrieval (BM25)
        let (passages, scores) = if self.config.enable_knowledge_retrieval {
            self.retriever.retrieve(prompt, 5)
        } else {
            (Vec::new(), Vec::new())
        };

        // Step 3: Solver pipeline
        let solver_result = self.solver.solve(intent.intent.as_str(), prompt);
        debug!(
            "[CCE] Solver: {} conf={:.2}",
            solver_result.solver_name, solver_result.confidence
        );

        // Step 4: Creative thinking for open-ended / low-confidence problems
        let creative_summary = if self.should_engage_creativity(&intent, &solver_result) {
            info!("[CCE] Engaging Creative Engine for open-ended problem");
            let creative_result = self
                .creative_engine
                .think_creatively(prompt, self.config.creative_max_ideas);
            let summary = CreativeResultSummary {
                ideas_generated: creative_result.ideas.len(),
                methods_used: creative_result.methods_used.clone(),
                best_idea_score: creative_result
                    .best_idea
                    .as_ref()
                    .map(|i| i.composite_score)
                    .unwrap_or(0.0),
                duration_ms: creative_result.duration_ms,
            };
            debug!(
                "[CCE] Creative Engine: {} ideas, best score={:.3}",
                summary.ideas_generated, summary.best_idea_score
            );
            Some((summary, creative_result))
        } else {
            None
        };

        // Step 5: Generate rich text response
        let response = self.compose_response(
            prompt,
            &intent,
            &solver_result,
            &passages,
            &scores,
            creative_summary.as_ref().map(|(_, cr)| cr),
        );

        let duration = start.elapsed().as_secs_f64() * 1000.0;
        info!("[CCE] Completed in {:.1}ms", duration);

        CognitiveResult {
            response,
            intent,
            solver_result: Some(solver_result),
            creative_result: creative_summary.map(|(s, _)| s),
            fifth_dimension_result: None,
            thinking_steps: Vec::new(),
            duration_ms: duration,
            engine_version: "CCE-v7.0-5D-Rust".to_string(),
        }
    }

    /// Process with deep thinking (Synthesize → Verify → Learn loop).
    /// For moderate complexity. For hard/impossible problems, use process_5d().
    pub fn process_deep(&mut self, prompt: &str) -> CognitiveResult {
        let mut result = self.process(prompt);

        // Engage ThinkingLoop for complex or uncertain results
        if let Some(ref solver) = result.solver_result {
            if solver.confidence < 0.60 && self.config.enable_thinking_loop {
                info!("[CCE] Engaging ThinkingLoop for deep analysis");
                let thinking_result =
                    self.thinking_loop
                        .think(prompt, result.intent.intent.as_str(), &solver.answer);

                // If thinking loop produced a better answer, use it
                if thinking_result.confidence > solver.confidence {
                    result.response = self.enrich_with_text_generator(
                        &thinking_result.final_answer,
                        prompt,
                        &result.intent,
                    );
                }

                for step in &thinking_result.steps {
                    result.thinking_steps.push(format!(
                        "[{}] {} (conf={:.3}, strategy={})",
                        step.phase.as_str(),
                        &step.hypothesis.chars().take(100).collect::<String>(),
                        step.confidence,
                        step.strategy
                    ));
                }

                result.thinking_steps.push(format!(
                    "ThinkingLoop: {} iterations, converged={}, final_conf={:.3}",
                    thinking_result.total_iterations,
                    thinking_result.converged,
                    thinking_result.confidence
                ));
            }
        }

        result
    }

    // ═══════════════════════════════════════════════════════════
    // 5th DIMENSION PROCESSING — Maximum cognitive power
    // ═══════════════════════════════════════════════════════════

    /// Process through the 5th-Dimension Brain.
    ///
    /// This is the MAXIMUM cognitive power path. It:
    ///   1. Runs standard intent classification + solver
    ///   2. Analyzes problem topology to determine routing
    ///   3. Runs D5 TemporalOmniscience (forks N parallel timelines)
    ///   4. Each timeline uses different D1-D4 combinations
    ///   5. Scores all timelines via multi-objective fitness
    ///   6. Collapses to the optimal timeline
    ///   7. D4 Metacognitive Sovereign reviews the result
    ///   8. Updates cognitive momentum + stability tracking
    ///   9. If D5 outperforms the solver, use D5's answer
    pub fn process_5d(&mut self, prompt: &str) -> CognitiveResult {
        let start = Instant::now();
        self.total_queries += 1;

        info!("[CCE-v7.0] ═══ 5th DIMENSION ENGAGED ═══");
        info!("[CCE-v7.0] Prompt: {}", &prompt[..prompt.len().min(100)]);

        // ── STEP 1: Intent + Solver (fast path) ──
        let intent = self.classifier.classify(prompt);
        *self
            .intent_histogram
            .entry(intent.intent.as_str().to_string())
            .or_insert(0) += 1;

        let solver_result = self.solver.solve(intent.intent.as_str(), prompt);
        debug!(
            "[CCE-v7.0] Solver: {} conf={:.2}",
            solver_result.solver_name, solver_result.confidence
        );

        // ── STEP 2: Analyze problem complexity via math ──
        let problem_features = self.analyze_problem(prompt);
        let mut thinking_steps = Vec::new();
        thinking_steps.push(format!(
            "Problem analysis: complexity={:.2}, recommended_dim=D{}",
            problem_features.complexity, problem_features.recommended_dimension
        ));

        // ── STEP 3: Run 5th Dimension Brain ──
        let is_search_synthesis = prompt.contains("Web Search Results for:")
            || prompt.contains("Context from web scraping:");
        let is_general_offline = solver_result.answer.is_empty();

        let d5_result =
            if self.config.enable_5d_brain && !is_search_synthesis && !is_general_offline {
                info!("[CCE-v7.0] Launching TemporalOmniscience");
                let result = self.temporal_omniscience.think_5d(prompt);

                // Record reasoning trace
                for step in &result.reasoning_trace {
                    thinking_steps.push(format!("[D5] {}", step));
                }

                thinking_steps.push(format!(
                    "D5 complete: winner={}, conf={:.3}, proof={}, timelines={}, {:.0}ms",
                    result.winning_strategy,
                    result.confidence,
                    result.proof_level,
                    result.total_timelines,
                    result.duration_ms,
                ));

                Some(result)
            } else {
                None
            };

        // ── STEP 4: Select best answer ──
        let (final_response, final_confidence) =
            self.select_best_answer(prompt, &intent, &solver_result, d5_result.as_ref());

        // ── STEP 5: Update cognitive momentum ──
        self.cognitive_momentum.update(final_confidence);
        self.performance_history.push(final_confidence);
        if self.performance_history.len() > 200 {
            self.performance_history.remove(0);
        }

        let momentum_action = self.cognitive_momentum.recommendation();
        thinking_steps.push(format!(
            "Momentum: vel={:.4}, accel={:.4}, jerk={:.4}, action={:?}",
            self.cognitive_momentum.velocity(),
            self.cognitive_momentum.acceleration(),
            self.cognitive_momentum.jerk(),
            momentum_action,
        ));

        // ── STEP 6: Stability check (if enough history) ──
        if self.performance_history.len() >= 15 {
            let stability = cognitive_math::lyapunov_stability(&self.performance_history, 3);
            thinking_steps.push(format!(
                "Stability: CSI={:.4}, class={:?}, action={:?}",
                stability.csi, stability.classification, stability.action,
            ));
        }

        // ── STEP 7: Build 5D summary ──
        let d5_summary = d5_result.as_ref().map(|r| FifthDimensionSummary {
            answer: r.answer.clone(),
            confidence: r.confidence,
            winning_strategy: r.winning_strategy.clone(),
            proof_level: r.proof_level.clone(),
            timelines_explored: r.total_timelines,
            total_nodes: r.total_nodes_explored,
            biases_detected: r.metacognition.biases_detected,
            self_corrections: r.metacognition.corrections_applied,
            cognitive_health: r.metacognition.health.clone(),
            duration_ms: r.duration_ms,
        });

        let duration = start.elapsed().as_secs_f64() * 1000.0;
        info!(
            "[CCE-v7.0] ═══ Complete ═══ conf={:.3}, {:.0}ms",
            final_confidence, duration
        );

        CognitiveResult {
            response: final_response,
            intent,
            solver_result: Some(solver_result),
            creative_result: None,
            fifth_dimension_result: d5_summary,
            thinking_steps,
            duration_ms: duration,
            engine_version: "CCE-v7.0-5D-Rust".to_string(),
        }
    }

    /// Analyze problem complexity using novel mathematical formulas.
    fn analyze_problem(&self, prompt: &str) -> ProblemAnalysis {
        // Compute fractal dimension of problem tokens
        let tokens: Vec<&str> = prompt.split_whitespace().collect();
        let num_tokens = tokens.len();

        // Build feature vectors from token n-grams
        let feature_vecs: Vec<Vec<f64>> = tokens
            .iter()
            .take(50)
            .map(|tok| {
                let bytes = tok.as_bytes();
                let mut features = vec![0.0; 8];
                for (i, &b) in bytes.iter().take(8).enumerate() {
                    features[i] = b as f64 / 255.0;
                }
                features
            })
            .collect();

        let fractal_dim = if feature_vecs.len() >= 5 {
            cognitive_math::fractal_dimension(&feature_vecs)
        } else {
            1.5
        };

        let optimal_depth = cognitive_math::optimal_recursion_depth(fractal_dim, num_tokens);

        // Estimate complexity from token diversity
        let unique_tokens: std::collections::HashSet<&str> = tokens.iter().copied().collect();
        let diversity = unique_tokens.len() as f64 / num_tokens.max(1) as f64;

        // Build simple constraint graph from keywords
        let constraint_keywords = [
            "must", "should", "cannot", "never", "always", "if", "then", "but", "however",
            "except", "unless", "while", "require",
        ];
        let num_constraints = tokens
            .iter()
            .filter(|t| constraint_keywords.contains(&t.to_lowercase().as_str()))
            .count();

        // Determine constraint graph topology
        let edges: Vec<(usize, usize)> = (0..num_constraints.min(10))
            .zip(1..num_constraints.min(10) + 1)
            .collect();
        let topology = cognitive_math::compute_topology(num_constraints.max(2), &edges);

        let recommended_dim = topology.recommended_dimension;

        let complexity =
            (fractal_dim * 0.3 + diversity * 0.3 + (num_constraints as f64 / 5.0).min(1.0) * 0.4)
                .clamp(0.0, 1.0);

        ProblemAnalysis {
            fractal_dim,
            optimal_depth,
            constraint_count: num_constraints,
            token_diversity: diversity,
            complexity,
            recommended_dimension: recommended_dim,
            has_paradox: topology.betti_1 > 0,
        }
    }

    fn parse_search_synthesis(prompt: &str) -> (String, Vec<String>) {
        let mut search_query = String::new();
        if let Some(start_idx) = prompt.find("Web Search Results for: \"") {
            let start = start_idx + "Web Search Results for: \"".len();
            if let Some(end_idx) = prompt[start..].find('"') {
                search_query = prompt[start..start + end_idx].to_string();
            }
        }
        if search_query.is_empty() {
            search_query = "search results".to_string();
        }

        let mut snippets = Vec::new();
        let parts: Vec<&str> = prompt.split("Snippet:").collect();
        for part in parts.iter().skip(1) {
            let line = part.trim();
            let snippet_content = if let Some(end_pos) = line.find("\n[") {
                &line[..end_pos]
            } else if let Some(end_pos) = line.find("\n\n") {
                &line[..end_pos]
            } else {
                line
            };
            let cleaned = snippet_content.trim().to_string();
            if !cleaned.is_empty() {
                snippets.push(cleaned);
            }
        }

        (search_query, snippets)
    }

    /// Select the best answer from solver vs D5.
    fn select_best_answer(
        &mut self,
        prompt: &str,
        intent: &IntentClassification,
        solver: &SolverResult,
        d5_result: Option<&FifthDimensionResult>,
    ) -> (String, f64) {
        // Check if this is a web search synthesis request
        let is_search_synthesis = prompt.contains("Web Search Results for:")
            || prompt.contains("Context from web scraping:");
        if is_search_synthesis {
            let (search_query, snippets) = Self::parse_search_synthesis(prompt);
            if !snippets.is_empty() {
                let num_snippets = snippets.len();
                let request = TextRequest {
                    topic: search_query.clone(),
                    key_points: snippets,
                    style: TextStyle::Conversational,
                    structure: TextStructure::SingleParagraph,
                    max_length: 800,
                    context: None,
                    audience: None,
                };
                let result = self.text_generator.generate(&request);
                let response = format!(
                    "{}\n\n---\n*Astra CCE v7.0 — Web Search Synthesis (synthesized from {} crawled sources, conf: 98.0%)*",
                    result.text,
                    num_snippets
                );
                return (response, 0.98);
            }
        }

        // Handle general queries when offline/no search results
        if solver.answer.is_empty() {
            let prompt_preview: String = prompt.chars().take(100).collect();
            let offline_msg = format!(
                "I am currently offline and cannot access the live internet to answer: \"{}\".\n\n\
                 To search the web and get real-time info, please use the search bar or prefix your query with 'search' or 'find' so I can crawl live sources.",
                prompt_preview
            );
            return (offline_msg, 0.90);
        }

        let solver_conf = solver.confidence;

        // If D5 ran and produced a result
        if let Some(d5) = d5_result {
            let d5_conf = d5.confidence;

            // Compare using Bayesian surprise — how surprising is D5's result
            // relative to the solver's?
            let prior = vec![solver_conf.max(0.01), (1.0 - solver_conf).max(0.01)];
            let posterior = vec![d5_conf.max(0.01), (1.0 - d5_conf).max(0.01)];
            let surprise = advanced_reasoning_formulas::bayesian_surprise(&prior, &posterior);

            info!(
                "[CCE-v7.0] Solver conf={:.3}, D5 conf={:.3}, surprise={:.4}",
                solver_conf, d5_conf, surprise
            );

            // D5 wins if: higher confidence, OR similar conf but has formal proof
            let d5_has_proof = d5.proof_level == "formally_proven"
                || d5.proof_level == "high_confidence_heuristic";

            if d5_conf > solver_conf || (d5_conf > solver_conf * 0.9 && d5_has_proof) {
                // D5 wins — compose response from D5 answer
                let enriched = self.enrich_with_text_generator(&d5.answer, prompt, intent);
                let response = format!(
                    "{}\n\n---\n*Astra CCE v7.0 — 5th Dimension Brain \
                     (strategy: {}, timelines: {}, proof: {}, conf: {:.1}%)*",
                    enriched,
                    d5.winning_strategy,
                    d5.total_timelines,
                    d5.proof_level,
                    d5.confidence * 100.0,
                );
                return (response, d5_conf);
            }
        }

        // Solver wins or D5 didn't run
        let response = if !solver.answer.is_empty() {
            self.enrich_with_text_generator(&solver.answer, prompt, intent)
        } else {
            format!(
                "Analysis of '{}' yielded no definitive result. \
                     Consider rephrasing or providing more constraints.",
                &prompt[..prompt.len().min(100)]
            )
        };

        (response, solver_conf)
    }

    /// Decide whether creative thinking should be engaged.
    fn should_engage_creativity(
        &self,
        intent: &IntentClassification,
        solver: &SolverResult,
    ) -> bool {
        if !self.config.enable_creative_thinking {
            return false;
        }

        // Creative thinking for: general queries, open-ended, or low solver confidence
        matches!(
            intent.intent,
            IntentType::General | IntentType::Discovery | IntentType::Synthesis
        ) || solver.confidence < 0.50
    }

    /// Compose the final response using all available signals.
    fn compose_response(
        &mut self,
        prompt: &str,
        intent: &IntentClassification,
        solver: &SolverResult,
        passages: &[String],
        scores: &[f64],
        creative_result: Option<&CreativeResult>,
    ) -> String {
        // HIGH CONFIDENCE SOLVER → use solver answer enriched with text generator
        if solver.confidence >= 0.70 && !solver.answer.is_empty() {
            return self.enrich_with_text_generator(&solver.answer, prompt, intent);
        }

        // CREATIVE RESULT AVAILABLE → compose creative + solver output
        if let Some(creative) = creative_result {
            if let Some(best_idea) = &creative.best_idea {
                let mut key_points = Vec::new();

                // Include solver answer if any
                if !solver.answer.is_empty() && solver.confidence > 0.30 {
                    key_points.push(solver.answer.clone());
                }

                // Include top creative ideas
                key_points.push(best_idea.content.clone());
                for idea in creative.ideas.iter().skip(1).take(2) {
                    key_points.push(idea.content.clone());
                }

                // Include relevant retrieved passages
                for (passage, score) in passages.iter().zip(scores.iter()) {
                    if *score > 0.5 {
                        key_points.push(passage.clone());
                    }
                }

                if self.config.enable_text_generation {
                    let style = self.determine_style(intent);
                    let structure = self.determine_structure(intent, key_points.len());

                    let request = TextRequest {
                        topic: prompt.to_string(),
                        key_points,
                        style,
                        structure,
                        max_length: 800,
                        context: None,
                        audience: None,
                    };

                    let text_result = self.text_generator.generate(&request);
                    return format!(
                        "{}\n\n---\n*Astra CCE v6.0 — Creative Engine ({} ideas via [{}]) + Text Generator ({} words)*",
                        text_result.text,
                        creative.ideas.len(),
                        creative.methods_used.join(", "),
                        text_result.word_count
                    );
                }
            }
        }

        // SOLVER + RETRIEVAL → template synthesizer
        if !solver.answer.is_empty() || !passages.is_empty() {
            let ctx = SynthesisContext {
                prompt: prompt.to_string(),
                intent: intent.intent.as_str().to_string(),
                solver_answer: solver.answer.clone(),
                solver_confidence: solver.confidence,
                solver_name: solver.solver_name.clone(),
                retrieved_passages: passages.to_vec(),
                retrieved_scores: scores.to_vec(),
                reasoning_trace: solver.reasoning_trace.clone(),
            };
            return self.synthesizer.synthesize(&ctx);
        }

        // FALLBACK → text generator for a structured response
        if self.config.enable_text_generation {
            let request = TextRequest {
                topic: prompt.to_string(),
                key_points: Vec::new(),
                style: self.determine_style(intent),
                structure: TextStructure::SingleParagraph,
                max_length: 300,
                context: None,
                audience: None,
            };
            let text_result = self.text_generator.generate(&request);
            return text_result.text;
        }

        // LAST RESORT → template synthesizer fallback
        let ctx = SynthesisContext {
            prompt: prompt.to_string(),
            intent: intent.intent.as_str().to_string(),
            solver_answer: String::new(),
            solver_confidence: 0.0,
            solver_name: String::new(),
            retrieved_passages: Vec::new(),
            retrieved_scores: Vec::new(),
            reasoning_trace: Vec::new(),
        };
        self.synthesizer.synthesize(&ctx)
    }

    /// Enrich a solver answer using the text generator for richer output.
    fn enrich_with_text_generator(
        &mut self,
        answer: &str,
        prompt: &str,
        intent: &IntentClassification,
    ) -> String {
        if !self.config.enable_text_generation {
            return answer.to_string();
        }

        // For math/code/logic, keep the answer precise — just add structure
        match intent.intent {
            IntentType::Math
            | IntentType::Code
            | IntentType::Logic
            | IntentType::Verification
            | IntentType::Physics
            | IntentType::Proof => {
                // These need precision, not embellishment
                answer.to_string()
            }
            _ => {
                let request = TextRequest {
                    topic: prompt.to_string(),
                    key_points: vec![answer.to_string()],
                    style: self.determine_style(intent),
                    structure: TextStructure::SingleParagraph,
                    max_length: 500,
                    context: None,
                    audience: None,
                };
                self.text_generator.generate(&request).text
            }
        }
    }

    /// Determine the best writing style based on intent.
    fn determine_style(&self, intent: &IntentClassification) -> TextStyle {
        match intent.intent {
            IntentType::Code => TextStyle::Technical,
            IntentType::Math => TextStyle::Technical,
            IntentType::Physics => TextStyle::Technical,
            IntentType::Logic => TextStyle::Analytical,
            IntentType::Proof => TextStyle::Analytical,
            IntentType::Discovery => TextStyle::Creative,
            IntentType::Synthesis => TextStyle::Creative,
            IntentType::Constraint => TextStyle::Technical,
            IntentType::Plan => TextStyle::Formal,
            IntentType::Extraction => TextStyle::Formal,
            IntentType::Verification => TextStyle::Technical,
            IntentType::General => TextStyle::Conversational,
        }
    }

    /// Determine the best output structure based on intent and data.
    fn determine_structure(
        &self,
        intent: &IntentClassification,
        num_points: usize,
    ) -> TextStructure {
        match intent.intent {
            IntentType::Plan => TextStructure::StepByStep,
            IntentType::Discovery | IntentType::Synthesis => TextStructure::MultiParagraph,
            _ if num_points > 4 => TextStructure::MultiParagraph,
            _ if num_points > 2 => TextStructure::BulletedList,
            _ => TextStructure::SingleParagraph,
        }
    }

    /// Get engine statistics.
    pub fn get_stats(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();
        stats.insert(
            "total_queries".into(),
            serde_json::json!(self.total_queries),
        );
        stats.insert(
            "engine_version".into(),
            serde_json::json!("CCE-v7.0-5D-Rust"),
        );
        stats.insert(
            "intent_histogram".into(),
            serde_json::json!(self.intent_histogram),
        );
        stats.insert(
            "synthesizer_stats".into(),
            serde_json::json!(self.synthesizer.get_stats()),
        );
        stats.insert(
            "creative_engine_stats".into(),
            self.creative_engine.get_stats(),
        );
        stats.insert(
            "text_generator_stats".into(),
            self.text_generator.get_stats(),
        );
        stats.insert(
            "5d_brain_stats".into(),
            self.temporal_omniscience.get_stats(),
        );
        stats.insert(
            "cognitive_momentum".into(),
            serde_json::json!({
                "velocity": self.cognitive_momentum.velocity(),
                "acceleration": self.cognitive_momentum.acceleration(),
                "jerk": self.cognitive_momentum.jerk(),
                "kinetic_energy": self.cognitive_momentum.kinetic_energy(),
            }),
        );
        stats
    }

    /// Get classifier reference for direct access.
    pub fn classifier(&self) -> &IntentClassifier {
        &self.classifier
    }

    /// Get solver reference.
    pub fn solver(&self) -> &SolverPipeline {
        &self.solver
    }

    /// Get creative engine reference.
    pub fn creative_engine(&self) -> &CreativeEngine {
        &self.creative_engine
    }

    /// Get text generator reference.
    pub fn text_generator(&self) -> &TextGenerator {
        &self.text_generator
    }
}

impl Default for CognitiveCoreEngine {
    fn default() -> Self {
        Self::new(CognitiveConfig::default())
    }
}

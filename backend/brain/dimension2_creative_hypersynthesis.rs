// ═══════════════════════════════════════════════════════════════
// DIMENSION 2: CREATIVE HYPERSYNTHESIS v1.0
// ═══════════════════════════════════════════════════════════════
//
// Extends the existing CreativeEngine (7-strategy combinatorial)
// with three production-grade capabilities:
//
//   1. Dream State Daemon — continuous background ideation that
//      pulls random concept pairs from memory and attempts forced
//      synthesis, building a reservoir of novel ideas.
//
//   2. Normalized Compression Distance (NCD) Novelty Scoring —
//      measures true novelty by computing how incompressible an
//      idea is relative to the full idea history.
//
//   3. Conceptual Interpolation — walks between concept embeddings
//      using TF-IDF vector arithmetic to discover "in-between"
//      concepts no single strategy would find.
//
// All pure Rust.  No Python service required for core operation.
// Dependencies: rand 0.8, std, flate2 (for NCD compression).

use log::info;
use rand::Rng;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use super::advanced_reasoning_formulas;
use super::cognitive_math;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// A novel concept discovered through hypersynthesis.
#[derive(Debug, Clone)]
pub struct HyperIdea {
    pub id: u64,
    pub content: String,
    pub source_a: String,
    pub source_b: String,
    pub method: HyperMethod,
    pub novelty_ncd: f64,
    pub relevance: f64,
    pub coherence: f64,
    pub composite_score: f64,
    pub reasoning_chain: Vec<String>,
}

/// How a hyper-idea was generated.
#[derive(Debug, Clone, PartialEq)]
pub enum HyperMethod {
    ConceptInterpolation,
    ForcedSynthesis,
    DreamAssociation,
    EntropyCycled,
    AxiomInversion,
    CrossDomainBlend,
    RecursiveAbstraction,
    StochasticBeam,
}

impl HyperMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConceptInterpolation => "concept_interpolation",
            Self::ForcedSynthesis => "forced_synthesis",
            Self::DreamAssociation => "dream_association",
            Self::EntropyCycled => "entropy_cycled",
            Self::AxiomInversion => "axiom_inversion",
            Self::CrossDomainBlend => "cross_domain_blend",
            Self::RecursiveAbstraction => "recursive_abstraction",
            Self::StochasticBeam => "stochastic_beam",
        }
    }
}

/// Result from a hypersynthesis session.
#[derive(Debug, Clone)]
pub struct HyperResult {
    pub ideas: Vec<HyperIdea>,
    pub best_idea: Option<HyperIdea>,
    pub total_candidates: usize,
    pub methods_used: Vec<String>,
    pub dream_ideas_consumed: usize,
    pub avg_novelty: f64,
    pub duration_ms: f64,
}

// ═══════════════════════════════════════════════════════════════
// CONCEPT STORE — Rich semantic concept graph for creative work
// ═══════════════════════════════════════════════════════════════

/// A concept with TF-IDF vector for semantic arithmetic.
#[derive(Debug, Clone)]
struct HyperConcept {
    label: String,
    domain: String,
    properties: Vec<String>,
    tf_idf: HashMap<String, f64>,
}

impl HyperConcept {
    fn new(label: &str, domain: &str, properties: &[&str]) -> Self {
        let mut tf = HashMap::new();
        // Build TF vector from label + properties
        for word in label
            .split_whitespace()
            .chain(properties.iter().copied())
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|w| w.len() > 2)
        {
            *tf.entry(word).or_insert(0.0) += 1.0;
        }
        Self {
            label: label.to_string(),
            domain: domain.to_string(),
            properties: properties.iter().map(|s| s.to_string()).collect(),
            tf_idf: tf,
        }
    }

    /// Cosine similarity between two concepts.
    fn similarity(&self, other: &Self) -> f64 {
        let dot: f64 = self
            .tf_idf
            .iter()
            .map(|(k, v)| v * other.tf_idf.get(k).unwrap_or(&0.0))
            .sum();
        let norm_a: f64 = self
            .tf_idf
            .values()
            .map(|v| v * v)
            .sum::<f64>()
            .sqrt()
            .max(1e-10);
        let norm_b: f64 = other
            .tf_idf
            .values()
            .map(|v| v * v)
            .sum::<f64>()
            .sqrt()
            .max(1e-10);
        dot / (norm_a * norm_b)
    }
}

// ═══════════════════════════════════════════════════════════════
// NORMALIZED COMPRESSION DISTANCE (NCD)
// ═══════════════════════════════════════════════════════════════
//
// NCD(x,y) = (C(xy) - min(C(x),C(y))) / max(C(x),C(y))
// where C(s) = compressed length of string s.
//
// High NCD = very different (high novelty).
// We use a simple byte-level LZ77 approximation without
// external deps, which is sufficient for novelty scoring.

/// Estimate compressed size using a simple LZ77-style sliding window.
/// Returns the number of bytes needed to represent the data after
/// greedy longest-match compression.
fn lz77_compressed_size(data: &[u8]) -> usize {
    if data.is_empty() {
        return 0;
    }
    let window_size: usize = 256;
    let mut output_bits: usize = 0;
    let mut pos = 0;

    while pos < data.len() {
        let window_start = pos.saturating_sub(window_size);
        let mut best_length = 0usize;

        // Search for longest match in the window
        for start in window_start..pos {
            let mut length = 0;
            while pos + length < data.len()
                && start + length < pos
                && data[start + length] == data[pos + length]
                && length < 255
            {
                length += 1;
            }
            if length > best_length {
                best_length = length;
            }
        }

        if best_length >= 3 {
            // Encode as (offset, length) pair: ~2 bytes
            output_bits += 16;
            pos += best_length;
        } else {
            // Literal byte: 9 bits (1 flag + 8 data)
            output_bits += 9;
            pos += 1;
        }
    }

    (output_bits + 7) / 8 // Round up to bytes
}

/// Compute Normalized Compression Distance between two strings.
fn ncd(a: &str, b: &str) -> f64 {
    let ca = lz77_compressed_size(a.as_bytes()) as f64;
    let cb = lz77_compressed_size(b.as_bytes()) as f64;
    let combined = format!("{}{}", a, b);
    let cab = lz77_compressed_size(combined.as_bytes()) as f64;

    let min_c = ca.min(cb).max(1.0);
    let max_c = ca.max(cb).max(1.0);

    ((cab - min_c) / max_c).clamp(0.0, 1.0)
}

// ═══════════════════════════════════════════════════════════════
// DREAM STATE RESERVOIR
// ═══════════════════════════════════════════════════════════════

/// Stores ideas generated during "dream state" background ideation.
#[derive(Debug)]
struct DreamReservoir {
    ideas: VecDeque<HyperIdea>,
    max_capacity: usize,
    total_dreamed: u64,
}

impl DreamReservoir {
    fn new(capacity: usize) -> Self {
        Self {
            ideas: VecDeque::new(),
            max_capacity: capacity,
            total_dreamed: 0,
        }
    }

    fn push(&mut self, idea: HyperIdea) {
        self.total_dreamed += 1;
        self.ideas.push_back(idea);
        while self.ideas.len() > self.max_capacity {
            self.ideas.pop_front();
        }
    }

    /// Drain up to N ideas from the reservoir for problem-solving.
    fn drain(&mut self, n: usize) -> Vec<HyperIdea> {
        let mut taken = Vec::new();
        for _ in 0..n {
            if let Some(idea) = self.ideas.pop_front() {
                taken.push(idea);
            } else {
                break;
            }
        }
        taken
    }

    fn is_empty(&self) -> bool {
        self.ideas.is_empty()
    }
}

// ═══════════════════════════════════════════════════════════════
// CREATIVE HYPERSYNTHESIS ENGINE
// ═══════════════════════════════════════════════════════════════

/// Configuration for the hypersynthesis engine.
#[derive(Debug, Clone)]
pub struct HyperConfig {
    pub max_ideas: usize,
    pub dream_reservoir_size: usize,
    pub ncd_novelty_threshold: f64,
    pub interpolation_steps: usize,
    pub entropy_high: f64,
    pub entropy_low: f64,
}

impl Default for HyperConfig {
    fn default() -> Self {
        Self {
            max_ideas: 10,
            dream_reservoir_size: 200,
            ncd_novelty_threshold: 0.3,
            interpolation_steps: 5,
            entropy_high: 0.9,
            entropy_low: 0.1,
        }
    }
}

/// Dimension 2: Creative Hypersynthesis Engine.
///
/// Combines GPU-free combinatorial creativity with:
/// - Concept vector interpolation for discovery
/// - NCD-based true novelty scoring
/// - Dream reservoir for background ideation
/// - Entropy cycling (divergent → convergent)
pub struct CreativeHypersynthesis {
    config: HyperConfig,
    concepts: Vec<HyperConcept>,
    dream_reservoir: DreamReservoir,
    idea_history: Vec<String>,
    next_id: u64,
    total_sessions: u64,
    total_ideas: u64,
    total_novel_ideas: u64,
}

impl CreativeHypersynthesis {
    pub fn new(config: HyperConfig) -> Self {
        let mut engine = Self {
            dream_reservoir: DreamReservoir::new(config.dream_reservoir_size),
            config,
            concepts: Vec::new(),
            idea_history: Vec::new(),
            next_id: 0,
            total_sessions: 0,
            total_ideas: 0,
            total_novel_ideas: 0,
        };
        engine.seed_knowledge();
        engine
    }

    /// Seed the concept store with cross-domain knowledge.
    fn seed_knowledge(&mut self) {
        // Physics / Mathematics
        let physics = vec![
            (
                "quantum_superposition",
                "physics",
                vec!["simultaneous", "probability", "collapse", "wave"],
            ),
            (
                "entropy",
                "physics",
                vec!["disorder", "information", "thermal", "irreversible"],
            ),
            (
                "relativity",
                "physics",
                vec!["spacetime", "curvature", "mass", "light"],
            ),
            (
                "emergence",
                "physics",
                vec!["complex", "simple_rules", "self_organizing", "nonlinear"],
            ),
            (
                "fractal",
                "mathematics",
                vec!["self_similar", "recursive", "scale_invariant", "infinite"],
            ),
            (
                "topology",
                "mathematics",
                vec!["continuous", "deformation", "invariant", "connected"],
            ),
        ];

        // Biology / Neuroscience
        let biology = vec![
            (
                "neuroplasticity",
                "neuroscience",
                vec!["adaptive", "rewiring", "learning", "synaptic"],
            ),
            (
                "evolution",
                "biology",
                vec!["mutation", "selection", "fitness", "adaptation"],
            ),
            (
                "symbiosis",
                "biology",
                vec!["mutualism", "cooperative", "coevolution", "interdependent"],
            ),
            (
                "homeostasis",
                "biology",
                vec!["equilibrium", "feedback", "regulation", "stability"],
            ),
            (
                "epigenetics",
                "biology",
                vec!["expression", "environment", "heritable", "modification"],
            ),
        ];

        // Computer Science
        let cs = vec![
            (
                "distributed_consensus",
                "cs",
                vec!["byzantine", "agreement", "fault_tolerant", "replicated"],
            ),
            (
                "recursion",
                "cs",
                vec!["self_referential", "base_case", "stack", "divide"],
            ),
            (
                "neural_network",
                "cs",
                vec!["layers", "weights", "backpropagation", "gradient"],
            ),
            (
                "blockchain",
                "cs",
                vec!["immutable", "decentralized", "hash_chain", "trustless"],
            ),
            (
                "genetic_algorithm",
                "cs",
                vec!["crossover", "mutation", "population", "fitness"],
            ),
        ];

        // Philosophy / Cognitive Science
        let philosophy = vec![
            (
                "qualia",
                "philosophy",
                vec!["subjective", "experience", "consciousness", "phenomenal"],
            ),
            (
                "dialectic",
                "philosophy",
                vec!["thesis", "antithesis", "synthesis", "contradiction"],
            ),
            (
                "epistemology",
                "philosophy",
                vec!["knowledge", "justification", "belief", "truth"],
            ),
            (
                "metacognition",
                "cogsci",
                vec![
                    "thinking_about_thinking",
                    "self_awareness",
                    "monitoring",
                    "regulation",
                ],
            ),
        ];

        // Art / Design
        let art = vec![
            (
                "negative_space",
                "art",
                vec!["absence", "inverse", "framing", "implied"],
            ),
            (
                "golden_ratio",
                "art",
                vec!["proportion", "harmony", "fibonacci", "aesthetic"],
            ),
            (
                "improvisation",
                "art",
                vec!["spontaneous", "responsive", "creative", "unplanned"],
            ),
        ];

        for (label, domain, props) in physics
            .into_iter()
            .chain(biology)
            .chain(cs)
            .chain(philosophy)
            .chain(art)
        {
            let prop_refs: Vec<&str> = props.iter().map(|s| s.as_ref()).collect();
            self.concepts
                .push(HyperConcept::new(label, domain, &prop_refs));
        }
    }

    /// Main entry: generate creative ideas for a problem.
    pub fn synthesize(&mut self, problem: &str, max_ideas: usize) -> HyperResult {
        let start = Instant::now();
        self.total_sessions += 1;

        let mut all_ideas = Vec::new();
        let mut methods_used = Vec::new();
        let mut rng = rand::thread_rng();

        // Extract problem concepts
        let problem_keywords = Self::extract_keywords(problem);

        // Phase 1: Drain dream reservoir for relevant ideas
        let dream_ideas = self.drain_relevant_dreams(problem, 5);
        let dream_count = dream_ideas.len();
        if !dream_ideas.is_empty() {
            methods_used.push("dream_association".to_string());
        }
        all_ideas.extend(dream_ideas);

        // Phase 2: Concept interpolation
        let interpolated = self.concept_interpolation(problem, &problem_keywords, &mut rng);
        if !interpolated.is_empty() {
            methods_used.push("concept_interpolation".to_string());
        }
        all_ideas.extend(interpolated);

        // Phase 3: Forced cross-domain synthesis
        let forced = self.forced_synthesis(problem, &problem_keywords, &mut rng);
        if !forced.is_empty() {
            methods_used.push("forced_synthesis".to_string());
        }
        all_ideas.extend(forced);

        // Phase 4: Axiom inversion
        let inverted = self.axiom_inversion(problem);
        if !inverted.is_empty() {
            methods_used.push("axiom_inversion".to_string());
        }
        all_ideas.extend(inverted);

        // Phase 5: Recursive abstraction (abstract → re-concretize)
        let abstracted = self.recursive_abstraction(problem, &problem_keywords);
        if !abstracted.is_empty() {
            methods_used.push("recursive_abstraction".to_string());
        }
        all_ideas.extend(abstracted);

        // Phase 6: Entropy cycling — high entropy (wild) → low entropy (prune)
        let cycled = self.entropy_cycling(&mut all_ideas, problem, &mut rng);
        if !cycled.is_empty() {
            methods_used.push("entropy_cycled".to_string());
        }
        all_ideas.extend(cycled);

        let total_candidates = all_ideas.len();

        // Score all ideas using NCD novelty
        for idea in &mut all_ideas {
            idea.novelty_ncd = self.compute_novelty(&idea.content);
            idea.relevance = Self::compute_relevance(&idea.content, &problem_keywords);
            idea.composite_score = self.composite_score(idea);
        }

        // Sort, dedup, truncate
        all_ideas.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_ideas.dedup_by(|a, b| a.content == b.content);
        all_ideas.truncate(max_ideas.max(self.config.max_ideas));

        let avg_novelty = if all_ideas.is_empty() {
            0.0
        } else {
            all_ideas.iter().map(|i| i.novelty_ncd).sum::<f64>() / all_ideas.len() as f64
        };

        let best_idea = all_ideas.first().cloned();

        // Update history for future NCD scoring
        for idea in &all_ideas {
            self.idea_history.push(idea.content.clone());
            if idea.novelty_ncd >= self.config.ncd_novelty_threshold {
                self.total_novel_ideas += 1;
            }
        }
        self.total_ideas += all_ideas.len() as u64;

        // Trim history to prevent unbounded growth on low-end hardware.
        // 500 recent ideas is sufficient for NCD novelty scoring context.
        if self.idea_history.len() > 500 {
            self.idea_history.drain(..250);
        }

        // Refill dream reservoir with unused candidates
        self.background_dream(problem, &mut rng);

        let duration = start.elapsed().as_secs_f64() * 1000.0;
        info!(
            "[D2-HyperCreative] {} ideas ({} novel), avg NCD={:.3}, {:.1}ms",
            all_ideas.len(),
            self.total_novel_ideas,
            avg_novelty,
            duration
        );

        HyperResult {
            ideas: all_ideas,
            best_idea,
            total_candidates,
            methods_used,
            dream_ideas_consumed: dream_count,
            avg_novelty,
            duration_ms: duration,
        }
    }

    // ───────────────────────────────────────────────────────────
    // STRATEGY 1: CONCEPT INTERPOLATION
    // ───────────────────────────────────────────────────────────
    // Walk between two concept TF-IDF vectors to discover
    // "in-between" concepts.

    fn concept_interpolation(
        &self,
        _problem: &str,
        keywords: &HashSet<String>,
        _rng: &mut impl Rng,
    ) -> Vec<HyperIdea> {
        let mut ideas = Vec::new();

        // Find concepts related to the problem
        let relevant: Vec<&HyperConcept> = self
            .concepts
            .iter()
            .filter(|c| {
                keywords
                    .iter()
                    .any(|kw| c.label.contains(kw) || c.properties.iter().any(|p| p.contains(kw)))
            })
            .collect();

        // Find concepts from different domains for cross-pollination
        let foreign: Vec<&HyperConcept> = self
            .concepts
            .iter()
            .filter(|c| !relevant.iter().any(|r| r.domain == c.domain))
            .collect();

        for concept_a in relevant.iter().take(3) {
            for concept_b in foreign.iter().take(3) {
                // Interpolate TF-IDF vectors
                for step in 1..self.config.interpolation_steps {
                    let t = step as f64 / self.config.interpolation_steps as f64;
                    let interpolated =
                        self.interpolate_vectors(&concept_a.tf_idf, &concept_b.tf_idf, t);

                    // Find the closest existing concept to the interpolated point
                    let nearest = self.nearest_concept(&interpolated);
                    let nearest_label =
                        nearest.map(|c| c.label.as_str()).unwrap_or("novel_concept");

                    let content = format!(
                        "Interpolation at t={:.2} between '{}' ({}) and '{}' ({}) → nearest existing: '{}'. \
                         Properties blend: [{}] + [{}]. \
                         Novel insight: a hybrid {}+{} approach that is {:.0}% {} and {:.0}% {}.",
                        t,
                        concept_a.label, concept_a.domain,
                        concept_b.label, concept_b.domain,
                        nearest_label,
                        concept_a.properties.iter().take(2).cloned().collect::<Vec<_>>().join(", "),
                        concept_b.properties.iter().take(2).cloned().collect::<Vec<_>>().join(", "),
                        concept_a.label, concept_b.label,
                        (1.0 - t) * 100.0, concept_a.domain,
                        t * 100.0, concept_b.domain,
                    );

                    ideas.push(self.make_idea(
                        content,
                        &concept_a.label,
                        &concept_b.label,
                        HyperMethod::ConceptInterpolation,
                        vec![format!(
                            "Interpolated {} ↔ {} at t={:.2}",
                            concept_a.label, concept_b.label, t
                        )],
                    ));
                }
            }
        }

        ideas
    }

    fn interpolate_vectors(
        &self,
        a: &HashMap<String, f64>,
        b: &HashMap<String, f64>,
        t: f64,
    ) -> HashMap<String, f64> {
        let mut result = HashMap::new();
        let all_keys: HashSet<&String> = a.keys().chain(b.keys()).collect();
        for key in all_keys {
            let va = a.get(key).unwrap_or(&0.0);
            let vb = b.get(key).unwrap_or(&0.0);
            let interpolated = va * (1.0 - t) + vb * t;
            if interpolated > 0.01 {
                result.insert(key.clone(), interpolated);
            }
        }
        result
    }

    fn nearest_concept(&self, vector: &HashMap<String, f64>) -> Option<&HyperConcept> {
        let temp = HyperConcept {
            label: String::new(),
            domain: String::new(),
            properties: Vec::new(),
            tf_idf: vector.clone(),
        };
        self.concepts.iter().max_by(|a, b| {
            a.similarity(&temp)
                .partial_cmp(&b.similarity(&temp))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    // ───────────────────────────────────────────────────────────
    // STRATEGY 2: FORCED SYNTHESIS
    // ───────────────────────────────────────────────────────────
    // Force-combine concepts from radically different domains.

    fn forced_synthesis(
        &self,
        problem: &str,
        _keywords: &HashSet<String>,
        rng: &mut impl Rng,
    ) -> Vec<HyperIdea> {
        let mut ideas = Vec::new();
        if self.concepts.len() < 2 {
            return ideas;
        }

        for _ in 0..5 {
            let idx_a = rng.gen_range(0..self.concepts.len());
            let mut idx_b = rng.gen_range(0..self.concepts.len());
            // Ensure different domains
            let mut attempts = 0;
            while self.concepts[idx_a].domain == self.concepts[idx_b].domain && attempts < 10 {
                idx_b = rng.gen_range(0..self.concepts.len());
                attempts += 1;
            }

            let ca = &self.concepts[idx_a];
            let cb = &self.concepts[idx_b];

            let shared: Vec<&String> = ca
                .properties
                .iter()
                .filter(|p| {
                    cb.properties
                        .iter()
                        .any(|q| p.contains(q.as_str()) || q.contains(p.as_str()))
                })
                .collect();

            let content = format!(
                "Forced synthesis: What if '{}' ({}) merged with '{}' ({})? \
                 Shared traits: [{}]. \
                 New concept: A system that is '{}' (from {}) applied with '{}' principles (from {}). \
                 Application to problem: this hybrid can address '{}' by combining {} approaches.",
                ca.label, ca.domain,
                cb.label, cb.domain,
                shared.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                ca.properties.first().map(|s| s.as_str()).unwrap_or("novel"),
                ca.domain,
                cb.properties.first().map(|s| s.as_str()).unwrap_or("innovative"),
                cb.domain,
                problem.chars().take(80).collect::<String>(),
                ca.domain,
            );

            ideas.push(self.make_idea(
                content,
                &ca.label,
                &cb.label,
                HyperMethod::ForcedSynthesis,
                vec![format!("Forced merge: {} × {}", ca.label, cb.label)],
            ));
        }

        ideas
    }

    // ───────────────────────────────────────────────────────────
    // STRATEGY 3: AXIOM INVERSION
    // ───────────────────────────────────────────────────────────
    // Take the core assumptions in the problem and systematically
    // negate each one to explore the "impossible" space.

    fn axiom_inversion(&self, problem: &str) -> Vec<HyperIdea> {
        let mut ideas = Vec::new();
        let problem_lower = problem.to_lowercase();

        let axiom_patterns: Vec<(&str, &str, &str)> = vec![
            (
                "must",
                "What if it didn't have to?",
                "Removing necessity constraints reveals optional paths",
            ),
            (
                "cannot",
                "What if it could?",
                "Self-imposed impossibility limits often aren't physical laws",
            ),
            (
                "always",
                "What if only sometimes?",
                "Intermittency can be a feature via probabilistic execution",
            ),
            (
                "never",
                "What if occasionally?",
                "Rare exceptions can become primary strategies",
            ),
            (
                "requires",
                "What if it needed nothing?",
                "Zero-dependency design eliminates fragile chains",
            ),
            (
                "limited",
                "What if unlimited?",
                "Remove resource limits to see the ideal, then approximate",
            ),
            (
                "slow",
                "What if instant?",
                "Assuming zero latency reveals the true bottleneck",
            ),
            (
                "expensive",
                "What if free?",
                "Remove cost to find the ideal architecture, then optimize",
            ),
            (
                "sequential",
                "What if parallel?",
                "Sequential assumptions hide massive parallelism",
            ),
            (
                "centralized",
                "What if distributed?",
                "Centralization creates single points of failure",
            ),
            (
                "deterministic",
                "What if probabilistic?",
                "Embracing uncertainty enables Monte Carlo approaches",
            ),
            (
                "linear",
                "What if exponential?",
                "Non-linearity enables compound growth and emergence",
            ),
        ];

        for (pattern, inversion, insight) in &axiom_patterns {
            if problem_lower.contains(pattern)
                || self.is_conceptually_adjacent(&problem_lower, pattern)
            {
                let content = format!(
                    "Axiom inversion on '{}': {}. {}. \
                     Reframed problem: {} but with the '{}' constraint removed. \
                     This opens solution paths in: {}-free design space.",
                    pattern,
                    inversion,
                    insight,
                    problem.chars().take(100).collect::<String>(),
                    pattern,
                    pattern,
                );

                ideas.push(self.make_idea(
                    content,
                    pattern,
                    &format!("not_{}", pattern),
                    HyperMethod::AxiomInversion,
                    vec![
                        format!("Inverted axiom: '{}'", pattern),
                        insight.to_string(),
                    ],
                ));
            }
        }

        // Always include top 3 universal inversions
        if ideas.is_empty() {
            for (pattern, inversion, insight) in axiom_patterns.iter().take(3) {
                let content = format!(
                    "Universal axiom challenge: '{}' → {}. {}. Applied to: {}",
                    pattern,
                    inversion,
                    insight,
                    problem.chars().take(100).collect::<String>()
                );
                ideas.push(self.make_idea(
                    content,
                    pattern,
                    &format!("not_{}", pattern),
                    HyperMethod::AxiomInversion,
                    vec![format!("Universal inversion: '{}'", pattern)],
                ));
            }
        }

        ideas
    }

    fn is_conceptually_adjacent(&self, text: &str, concept: &str) -> bool {
        // Check if any concept in our knowledge base bridges the text and the pattern
        self.concepts.iter().any(|c| {
            c.properties.iter().any(|p| text.contains(p.as_str()))
                && (c.label.contains(concept) || c.properties.iter().any(|p| p.contains(concept)))
        })
    }

    // ───────────────────────────────────────────────────────────
    // STRATEGY 4: RECURSIVE ABSTRACTION
    // ───────────────────────────────────────────────────────────
    // Abstract the problem to its most general form, find
    // solutions at the abstract level, then re-concretize.

    fn recursive_abstraction(&self, problem: &str, keywords: &HashSet<String>) -> Vec<HyperIdea> {
        let mut ideas = Vec::new();

        // Level 1: Pattern extraction — what TYPE of problem is this?
        let problem_type = if keywords.iter().any(|k| {
            matches!(
                k.as_str(),
                "optimize" | "fast" | "efficient" | "performance"
            )
        }) {
            "optimization"
        } else if keywords
            .iter()
            .any(|k| matches!(k.as_str(), "create" | "design" | "build" | "new"))
        {
            "construction"
        } else if keywords
            .iter()
            .any(|k| matches!(k.as_str(), "fix" | "debug" | "solve" | "error"))
        {
            "diagnostic"
        } else if keywords
            .iter()
            .any(|k| matches!(k.as_str(), "predict" | "forecast" | "estimate" | "model"))
        {
            "prediction"
        } else {
            "transformation"
        };

        // Level 2: Find abstract solutions in different domains
        let abstract_solutions: Vec<(&str, &str)> = match problem_type {
            "optimization" => vec![
                (
                    "gradient_descent",
                    "Iteratively move toward the optimum by following the derivative",
                ),
                (
                    "simulated_annealing",
                    "Accept worse solutions early to escape local minima",
                ),
                (
                    "genetic_algorithm",
                    "Evolve a population of solutions through crossover and mutation",
                ),
            ],
            "construction" => vec![
                (
                    "composability",
                    "Build from small, proven, composable primitives",
                ),
                (
                    "biomimicry",
                    "Copy construction patterns from nature (shells, bones, webs)",
                ),
                (
                    "constraint_first",
                    "Define constraints first, then fill the remaining solution space",
                ),
            ],
            "diagnostic" => vec![
                ("binary_search", "Halve the search space with each test"),
                (
                    "root_cause_analysis",
                    "Follow the causal chain to the deepest cause",
                ),
                (
                    "differential_diagnosis",
                    "Compare against known failure patterns",
                ),
            ],
            "prediction" => vec![
                (
                    "ensemble_methods",
                    "Combine multiple weak predictors into one strong predictor",
                ),
                (
                    "bayesian_updating",
                    "Start with a prior, update with evidence",
                ),
                ("causal_inference", "Model causes, not just correlations"),
            ],
            _ => vec![
                (
                    "isomorphism",
                    "Find a structurally identical problem that's already solved",
                ),
                (
                    "duality",
                    "Every problem has a dual — solve the dual instead",
                ),
                (
                    "meta_strategy",
                    "Don't solve the problem — solve the class of problems it belongs to",
                ),
            ],
        };

        for (method, description) in abstract_solutions {
            let content = format!(
                "Abstract→Concrete: Problem type is '{}'. Abstract solution: {} — {}. \
                 Re-concretized: apply {} directly to '{}' by treating the problem components as {}.",
                problem_type, method, description,
                method, problem.chars().take(100).collect::<String>(),
                problem_type,
            );

            ideas.push(self.make_idea(
                content,
                problem_type,
                method,
                HyperMethod::RecursiveAbstraction,
                vec![
                    format!("Classified problem as: {}", problem_type),
                    format!("Abstract solution: {}", method),
                    "Re-concretized to original domain".to_string(),
                ],
            ));
        }

        ideas
    }

    // ───────────────────────────────────────────────────────────
    // STRATEGY 5: ENTROPY CYCLING
    // ───────────────────────────────────────────────────────────
    // Alternate between wild divergent generation and strict
    // convergent pruning to produce ideas that are both novel
    // AND coherent.

    fn entropy_cycling(
        &self,
        existing: &mut Vec<HyperIdea>,
        problem: &str,
        rng: &mut impl Rng,
    ) -> Vec<HyperIdea> {
        if existing.len() < 2 {
            return Vec::new();
        }

        let mut cycled = Vec::new();

        // HIGH ENTROPY PASS: randomly mutate existing ideas
        for idea in existing.iter().take(5) {
            if rng.gen::<f64>() > self.config.entropy_high {
                continue;
            }

            let mutations = [
                "What if we REVERSED the order of operations in this idea?",
                "What if we applied this idea to ITSELF recursively?",
                "What if we combined this with its OPPOSITE?",
                "What if this only worked 50% of the time — and that was a FEATURE?",
                "What if we scaled this by 1000x — what breaks? What emerges?",
            ];
            let mutation = mutations[rng.gen_range(0..mutations.len())];

            let content = format!(
                "Entropy-mutated: {} Applied to: '{}'. \
                 Original idea seed: {}",
                mutation,
                problem.chars().take(80).collect::<String>(),
                idea.content.chars().take(120).collect::<String>(),
            );

            cycled.push(self.make_idea(
                content,
                &idea.source_a,
                "entropy_mutation",
                HyperMethod::EntropyCycled,
                vec![format!("Mutation: {}", mutation)],
            ));
        }

        // LOW ENTROPY PASS: strict coherence filter applied to all
        cycled.retain(|idea| {
            let keywords = Self::extract_keywords(problem);
            let relevance = Self::compute_relevance(&idea.content, &keywords);
            relevance > self.config.entropy_low
        });

        cycled
    }

    // ───────────────────────────────────────────────────────────
    // DREAM STATE — Background ideation
    // ───────────────────────────────────────────────────────────

    fn background_dream(&mut self, seed_problem: &str, rng: &mut impl Rng) {
        if self.concepts.len() < 2 {
            return;
        }

        // Generate 3 dream ideas from random concept pairs
        for _ in 0..3 {
            let idx_a = rng.gen_range(0..self.concepts.len());
            let idx_b = rng.gen_range(0..self.concepts.len());
            if idx_a == idx_b {
                continue;
            }

            let ca = &self.concepts[idx_a];
            let cb = &self.concepts[idx_b];

            let content =
                format!(
                "Dream association: '{}' ({}) and '{}' ({}) share a deep structural similarity — \
                 both involve [{}]. What if this connection applied to: {}",
                ca.label, ca.domain,
                cb.label, cb.domain,
                ca.properties.iter().take(2).cloned().collect::<Vec<_>>().join(", "),
                seed_problem.chars().take(80).collect::<String>(),
            );

            let idea = self.make_idea(
                content,
                &ca.label,
                &cb.label,
                HyperMethod::DreamAssociation,
                vec!["Background dream ideation".to_string()],
            );
            self.dream_reservoir.push(idea);
        }
    }

    fn drain_relevant_dreams(&mut self, problem: &str, max: usize) -> Vec<HyperIdea> {
        if self.dream_reservoir.is_empty() {
            return Vec::new();
        }

        let keywords = Self::extract_keywords(problem);
        let mut drained = self.dream_reservoir.drain(max);

        // Filter for relevance
        drained.retain(|idea| Self::compute_relevance(&idea.content, &keywords) > 0.1);

        drained
    }

    // ───────────────────────────────────────────────────────────
    // SCORING & UTILITIES
    // ───────────────────────────────────────────────────────────

    fn compute_novelty(&self, content: &str) -> f64 {
        if self.idea_history.is_empty() {
            return 1.0; // First idea is maximally novel
        }

        // Measure 1: NCD novelty (compression-based)
        let sample_size = self.idea_history.len().min(20);
        let step = self.idea_history.len().max(1) / sample_size.max(1);
        let mut total_ncd = 0.0;
        let mut count = 0;

        for i in (0..self.idea_history.len())
            .step_by(step.max(1))
            .take(sample_size)
        {
            total_ncd += ncd(content, &self.idea_history[i]);
            count += 1;
        }
        let ncd_score = if count == 0 {
            1.0
        } else {
            total_ncd / count as f64
        };

        // Measure 2: Kolmogorov Creative Distance (algorithmic novelty)
        // Sample a few recent ideas for KCD comparison
        let kcd_score = if self.idea_history.len() >= 3 {
            let recent: Vec<&str> = self
                .idea_history
                .iter()
                .rev()
                .take(5)
                .map(|s| s.as_str())
                .collect();
            let total_kcd: f64 = recent
                .iter()
                .map(|prev| cognitive_math::kolmogorov_creative_distance(content, prev))
                .sum::<f64>();
            total_kcd / recent.len() as f64
        } else {
            1.0
        };

        // Blend both novelty measures (KCD is deeper, NCD is faster)
        ncd_score * 0.5 + kcd_score * 0.5
    }

    fn compute_relevance(content: &str, keywords: &HashSet<String>) -> f64 {
        if keywords.is_empty() {
            return 0.5;
        }
        let content_lower = content.to_lowercase();
        let hits = keywords
            .iter()
            .filter(|kw| content_lower.contains(kw.as_str()))
            .count();
        (hits as f64 / keywords.len() as f64).min(1.0)
    }

    fn composite_score(&self, idea: &HyperIdea) -> f64 {
        // Semantic tensor interaction score:
        // Convert source concept labels to feature vectors and compute tensor product
        let source_a_vec: Vec<f64> = idea
            .source_a
            .bytes()
            .take(8)
            .map(|b| b as f64 / 255.0)
            .collect();
        let source_b_vec: Vec<f64> = idea
            .source_b
            .bytes()
            .take(8)
            .map(|b| b as f64 / 255.0)
            .collect();

        let tensor_features =
            advanced_reasoning_formulas::semantic_tensor_product(&source_a_vec, &source_b_vec, 3);
        let tensor_richness = if tensor_features.is_empty() {
            0.5
        } else {
            // Ratio of top singular values — higher = stronger interaction
            let sum: f64 = tensor_features.iter().sum::<f64>().max(1e-10);
            (tensor_features[0] / sum).clamp(0.0, 1.0)
        };

        idea.relevance * 0.25
        + idea.novelty_ncd * 0.25
        + idea.coherence * 0.20
        + tensor_richness * 0.15  // Novel: semantic interaction strength
        + match idea.method {
            HyperMethod::ConceptInterpolation => 0.12,
            HyperMethod::ForcedSynthesis => 0.10,
            HyperMethod::RecursiveAbstraction => 0.11,
            HyperMethod::AxiomInversion => 0.13,
            HyperMethod::EntropyCycled => 0.08,
            HyperMethod::DreamAssociation => 0.09,
            _ => 0.07,
        }
    }

    fn extract_keywords(text: &str) -> HashSet<String> {
        let stop: HashSet<&str> = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "have", "has", "had", "do",
            "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall",
            "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through",
            "and", "but", "or", "not", "no", "this", "that", "these", "those", "it", "its", "how",
            "what", "when", "where", "why", "who", "which", "if", "then", "else", "than", "very",
        ]
        .iter()
        .copied()
        .collect();

        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() > 2 && !stop.contains(w))
            .map(|w| w.to_string())
            .collect()
    }

    fn make_idea(
        &self,
        content: String,
        source_a: &str,
        source_b: &str,
        method: HyperMethod,
        chain: Vec<String>,
    ) -> HyperIdea {
        HyperIdea {
            id: self.next_id,
            content,
            source_a: source_a.to_string(),
            source_b: source_b.to_string(),
            method,
            novelty_ncd: 0.0, // Computed later
            relevance: 0.0,   // Computed later
            coherence: 0.7,   // Base coherence
            composite_score: 0.0,
            reasoning_chain: chain,
        }
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "Dimension2_CreativeHypersynthesis_v1.0",
            "total_sessions": self.total_sessions,
            "total_ideas": self.total_ideas,
            "total_novel_ideas": self.total_novel_ideas,
            "novelty_rate": if self.total_ideas == 0 { 0.0 } else { self.total_novel_ideas as f64 / self.total_ideas as f64 },
            "concept_store_size": self.concepts.len(),
            "dream_reservoir_size": self.dream_reservoir.ideas.len(),
            "dream_total_dreamed": self.dream_reservoir.total_dreamed,
            "idea_history_size": self.idea_history.len(),
        })
    }
}

impl Default for CreativeHypersynthesis {
    fn default() -> Self {
        Self::new(HyperConfig::default())
    }
}

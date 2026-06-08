// ═══════════════════════════════════════════════════════════════
// DIMENSION 3: IMPOSSIBLE PROBLEM ENGINE v1.0
// ═══════════════════════════════════════════════════════════════
//
// When Dimensions 1 and 2 fail — when the problem is "impossible"
// under current axioms — this engine systematically BREAKS the
// axioms to reframe the problem in a new space where it becomes
// solvable.
//
// Core techniques:
//   1. Axiom Extraction — identify all implicit assumptions
//   2. Systematic Axiom Destruction — negate each axiom individually
//   3. Gödel Escape Hatch — detect self-referential paradoxes
//      and escape by reasoning from a meta-level
//   4. Proof by Contradiction — assume impossibility, derive
//      a contradiction, therefore the solution exists
//   5. Constraint Dissolution — find the constraint that makes
//      the problem impossible and dissolve it
//
// Pure Rust.  No external dependencies beyond std + rand + log.

use log::info;
use std::collections::HashSet;
use std::time::Instant;

use super::advanced_reasoning_formulas;
use super::cognitive_math;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// An axiom extracted from a problem statement.
#[derive(Debug, Clone)]
pub struct Axiom {
    pub id: usize,
    pub statement: String,
    pub category: AxiomCategory,
    pub confidence: f64,
    pub is_breakable: bool,
    pub source_fragment: String,
}

/// Categories of axioms.
#[derive(Debug, Clone, PartialEq)]
pub enum AxiomCategory {
    ResourceConstraint,  // "limited memory", "bounded time"
    PhysicalLaw,         // "conservation of energy"
    LogicalNecessity,    // "A or not-A" (tautologies)
    SocialConvention,    // "users expect X"
    TechnicalAssumption, // "must use framework X"
    TemporalConstraint,  // "must be done before X"
    IdentityAxiom,       // "X is X" (self-referential)
    ExistenceAxiom,      // "X exists", "X is possible"
}

impl AxiomCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ResourceConstraint => "resource_constraint",
            Self::PhysicalLaw => "physical_law",
            Self::LogicalNecessity => "logical_necessity",
            Self::SocialConvention => "social_convention",
            Self::TechnicalAssumption => "technical_assumption",
            Self::TemporalConstraint => "temporal_constraint",
            Self::IdentityAxiom => "identity_axiom",
            Self::ExistenceAxiom => "existence_axiom",
        }
    }

    /// How safe is it to break this type of axiom?
    fn breakability_score(&self) -> f64 {
        match self {
            Self::SocialConvention => 0.9, // Very breakable
            Self::TechnicalAssumption => 0.85,
            Self::TemporalConstraint => 0.7,
            Self::ResourceConstraint => 0.6,
            Self::ExistenceAxiom => 0.4,
            Self::IdentityAxiom => 0.3,
            Self::PhysicalLaw => 0.1,       // Very hard to break
            Self::LogicalNecessity => 0.05, // Almost unbreakable
        }
    }
}

/// Result of breaking a single axiom.
#[derive(Debug, Clone)]
pub struct AxiomBreakResult {
    pub axiom: Axiom,
    pub negated_statement: String,
    pub reframed_problem: String,
    pub solution_in_new_space: Option<String>,
    pub solution_confidence: f64,
    pub explanation: String,
    pub reasoning_chain: Vec<String>,
}

/// A self-reference detected in the problem (for Gödel escape).
#[derive(Debug, Clone)]
pub struct SelfReference {
    pub statement: String,
    pub reference_target: String,
    pub loop_depth: usize,
    pub escape_strategy: String,
    pub resolution: String,
}

/// Result from a contradiction search.
#[derive(Debug, Clone)]
pub struct ContradictionResult {
    pub assumption: String,
    pub derived_contradiction: String,
    pub proof_steps: Vec<String>,
    pub conclusion: String,
    pub confidence: f64,
}

/// The complete result from the Impossible Problem Engine.
#[derive(Debug, Clone)]
pub struct ImpossibleResult {
    pub original_problem: String,
    pub axioms_extracted: Vec<Axiom>,
    pub axiom_breaks: Vec<AxiomBreakResult>,
    pub godel_escapes: Vec<SelfReference>,
    pub contradictions: Vec<ContradictionResult>,
    pub best_reframing: Option<AxiomBreakResult>,
    pub impossibility_assessment: ImpossibilityLevel,
    pub reasoning_trace: Vec<String>,
    pub methods_used: Vec<String>,
    pub duration_ms: f64,
}

/// How impossible is the problem?
#[derive(Debug, Clone, PartialEq)]
pub enum ImpossibilityLevel {
    /// Problem is solvable — axiom break found a path
    Solvable { reframing: String },
    /// Problem is solvable if we accept a trade-off
    ConditionalSolvable { condition: String },
    /// Problem contains a paradox that was escaped
    ParadoxResolved { resolution: String },
    /// Problem appears fundamentally impossible
    FundamentallyImpossible { reason: String },
    /// Cannot determine — need more information
    Undetermined,
}

impl ImpossibilityLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Solvable { .. } => "solvable",
            Self::ConditionalSolvable { .. } => "conditional_solvable",
            Self::ParadoxResolved { .. } => "paradox_resolved",
            Self::FundamentallyImpossible { .. } => "fundamentally_impossible",
            Self::Undetermined => "undetermined",
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// AXIOM EXTRACTOR
// ═══════════════════════════════════════════════════════════════

/// Extracts implicit axioms from problem statements.
struct AxiomExtractor {
    /// Pattern → (category, template for axiom statement)
    patterns: Vec<(Vec<&'static str>, AxiomCategory, &'static str)>,
}

impl AxiomExtractor {
    fn new() -> Self {
        Self {
            patterns: vec![
                // Resource constraints
                (
                    vec![
                        "limited",
                        "bounded",
                        "finite",
                        "constrained",
                        "maximum",
                        "at most",
                        "only",
                    ],
                    AxiomCategory::ResourceConstraint,
                    "There is a finite/limited resource or capacity",
                ),
                (
                    vec!["memory", "storage", "disk", "ram"],
                    AxiomCategory::ResourceConstraint,
                    "Memory/storage is a constraining factor",
                ),
                (
                    vec![
                        "time",
                        "latency",
                        "deadline",
                        "timeout",
                        "fast",
                        "real-time",
                    ],
                    AxiomCategory::TemporalConstraint,
                    "There are temporal constraints or latency requirements",
                ),
                // Technical assumptions
                (
                    vec![
                        "must use",
                        "requires",
                        "framework",
                        "library",
                        "api",
                        "protocol",
                    ],
                    AxiomCategory::TechnicalAssumption,
                    "A specific technology or approach is assumed",
                ),
                (
                    vec!["single", "one", "only one", "centralized", "server"],
                    AxiomCategory::TechnicalAssumption,
                    "Assumes a centralized/singular architecture",
                ),
                (
                    vec![
                        "sequential",
                        "ordered",
                        "step by step",
                        "synchronous",
                        "blocking",
                    ],
                    AxiomCategory::TechnicalAssumption,
                    "Operations assumed to be sequential",
                ),
                // Social conventions
                (
                    vec!["user", "interface", "experience", "friendly", "intuitive"],
                    AxiomCategory::SocialConvention,
                    "Involves user-facing conventions or expectations",
                ),
                (
                    vec![
                        "standard",
                        "convention",
                        "best practice",
                        "typical",
                        "normal",
                    ],
                    AxiomCategory::SocialConvention,
                    "Follows established conventions or standards",
                ),
                // Physical laws
                (
                    vec![
                        "speed of light",
                        "thermodynamics",
                        "conservation",
                        "entropy",
                        "gravity",
                    ],
                    AxiomCategory::PhysicalLaw,
                    "Constrained by physical laws",
                ),
                // Identity / self-reference
                (
                    vec![
                        "itself",
                        "self",
                        "recursive",
                        "paradox",
                        "circular",
                        "this statement",
                    ],
                    AxiomCategory::IdentityAxiom,
                    "Contains self-referential or recursive structure",
                ),
                // Existence
                (
                    vec![
                        "impossible",
                        "cannot",
                        "never",
                        "no way",
                        "unfeasible",
                        "unsolvable",
                    ],
                    AxiomCategory::ExistenceAxiom,
                    "Assumes non-existence of a solution",
                ),
                (
                    vec!["always", "guaranteed", "certain", "inevitable", "must"],
                    AxiomCategory::ExistenceAxiom,
                    "Assumes universal truth or guarantee",
                ),
            ],
        }
    }

    /// Extract axioms from a problem statement.
    fn extract(&self, problem: &str) -> Vec<Axiom> {
        let problem_lower = problem.to_lowercase();
        let mut axioms = Vec::new();
        let mut id = 0;

        for (keywords, category, template) in &self.patterns {
            for keyword in keywords {
                if problem_lower.contains(keyword) {
                    // Find the sentence containing this keyword
                    let fragment = self.find_fragment(&problem_lower, keyword);
                    let breakable = category.breakability_score();

                    axioms.push(Axiom {
                        id,
                        statement: format!("{}: detected via '{}'", template, keyword),
                        category: category.clone(),
                        confidence: 0.7,
                        is_breakable: breakable > 0.3,
                        source_fragment: fragment,
                    });
                    id += 1;
                    break; // One axiom per pattern group
                }
            }
        }

        // Always add implicit axioms
        axioms.push(Axiom {
            id,
            statement: "The problem must be solved within the stated domain".to_string(),
            category: AxiomCategory::TechnicalAssumption,
            confidence: 0.9,
            is_breakable: true,
            source_fragment: problem[..problem.len().min(100)].to_string(),
        });
        id += 1;

        axioms.push(Axiom {
            id,
            statement: "The problem has exactly one correct answer".to_string(),
            category: AxiomCategory::SocialConvention,
            confidence: 0.6,
            is_breakable: true,
            source_fragment: String::new(),
        });

        axioms
    }

    fn find_fragment(&self, text: &str, keyword: &str) -> String {
        if let Some(pos) = text.find(keyword) {
            let start = pos.saturating_sub(40);
            let end = (pos + keyword.len() + 40).min(text.len());
            text[start..end].to_string()
        } else {
            String::new()
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// GÖDEL ESCAPE HATCH
// ═══════════════════════════════════════════════════════════════
//
// Detect self-referential paradoxes
// (e.g., "This statement is false")
// and escape by moving to a meta-level.

struct GodelDetector {
    self_ref_patterns: Vec<(&'static str, &'static str)>,
    max_meta_levels: usize,
}

impl GodelDetector {
    fn new(max_levels: usize) -> Self {
        Self {
            self_ref_patterns: vec![
                (
                    "this statement",
                    "Direct self-reference: the statement references itself",
                ),
                ("itself", "Reflexive self-reference"),
                ("its own", "Possessive self-reference"),
                ("paradox", "Explicitly contains a paradox"),
                ("circular", "Circular reasoning detected"),
                ("infinite regress", "Infinite regress pattern"),
                ("bootstrap", "Bootstrap/chicken-and-egg reference"),
                ("liar", "Liar-type paradox reference"),
                ("halting", "Undecidability reference (halting problem)"),
                ("self-referential", "Explicit self-reference marker"),
            ],
            max_meta_levels: max_levels,
        }
    }

    fn detect_and_escape(&self, problem: &str) -> Vec<SelfReference> {
        let problem_lower = problem.to_lowercase();
        let mut escapes = Vec::new();

        for (pattern, description) in &self.self_ref_patterns {
            if problem_lower.contains(pattern) {
                let resolution = self.escape_at_meta_level(problem, pattern, 1);
                escapes.push(SelfReference {
                    statement: format!("{}: found '{}'", description, pattern),
                    reference_target: pattern.to_string(),
                    loop_depth: 1,
                    escape_strategy: format!("Gödel escape at meta-level 1"),
                    resolution,
                });
            }
        }

        // Check for structural self-reference (sentences that reference
        // their own components)
        let sentences: Vec<&str> = problem
            .split(&['.', '!', '?'][..])
            .filter(|s| s.trim().len() > 10)
            .collect();

        for (i, sentence) in sentences.iter().enumerate() {
            let lower = sentence.to_lowercase();
            // A sentence references another sentence's content
            for (j, other) in sentences.iter().enumerate() {
                if i == j {
                    continue;
                }
                let other_words: Vec<&str> =
                    other.split_whitespace().filter(|w| w.len() > 4).collect();
                let shared = other_words
                    .iter()
                    .filter(|w| lower.contains(&w.to_lowercase()))
                    .count();
                if shared >= 3 && other_words.len() > 3 {
                    let reference_ratio = shared as f64 / other_words.len() as f64;
                    if reference_ratio > 0.5 {
                        let resolution = self.escape_at_meta_level(problem, "structural_loop", 2);
                        escapes.push(SelfReference {
                            statement: format!(
                                "Structural self-reference between sentences {} and {}",
                                i, j
                            ),
                            reference_target: format!("sentence_{}_to_{}", i, j),
                            loop_depth: 2,
                            escape_strategy:
                                "Gödel escape: reason about the structure from outside the system"
                                    .to_string(),
                            resolution,
                        });
                        break;
                    }
                }
            }
        }

        escapes
    }

    fn escape_at_meta_level(&self, problem: &str, pattern: &str, level: usize) -> String {
        if level > self.max_meta_levels {
            return format!(
                "Meta-level {} reached. The self-reference in '{}' is fundamentally \
                 undecidable within any finite meta-hierarchy (Gödel's Second Incompleteness). \
                 Resolution: acknowledge undecidability and proceed with bounded confidence.",
                level,
                &problem[..problem.len().min(80)]
            );
        }

        match pattern {
            "this statement" | "liar" => format!(
                "Gödel Escape Level {}: The statement 'This statement is false' is neither \
                 true nor false — it is MEANINGLESS within propositional logic. \
                 Resolution: move to a typed logic where statements cannot reference \
                 their own truth value (Tarski's hierarchy). The problem is dissolved, not solved.",
                level
            ),
            "halting" => format!(
                "Gödel Escape Level {}: This is equivalent to the Halting Problem. \
                 No general algorithm exists, but SPECIFIC instances can be decided. \
                 Resolution: classify this specific instance and apply decision procedure \
                 if the instance belongs to a decidable subset.",
                level
            ),
            "circular" | "bootstrap" => format!(
                "Gödel Escape Level {}: Break the circularity by introducing a \
                 fixed-point — an initial value that is 'assumed true until proven false'. \
                 This converts circular reasoning into iterative convergence (like Banach's \
                 Fixed-Point Theorem). Resolution: seed with a default and iterate.",
                level
            ),
            "infinite regress" => format!(
                "Gödel Escape Level {}: Halt the regress by imposing a finite depth \
                 bound and treating the base case as an axiom. This is how real mathematics \
                 works — even ZFC has axioms that aren't proven. \
                 Resolution: choose pragmatic axioms and build upward.",
                level
            ),
            _ => format!(
                "Gödel Escape Level {}: The self-reference detected is structural. \
                 Breaking out by reformulating the problem from an external viewpoint \
                 that does not participate in the self-referential loop. \
                 Meta-observation: '{}' viewed from outside its own system.",
                level,
                &problem[..problem.len().min(100)]
            ),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// PROOF BY CONTRADICTION ENGINE
// ═══════════════════════════════════════════════════════════════

struct ContradictionMiner {
    logical_rules: Vec<(&'static str, &'static str, &'static str)>,
}

impl ContradictionMiner {
    fn new() -> Self {
        Self {
            // (assumption, derived consequence, contradiction with)
            logical_rules: vec![
                ("X is impossible", "No instance of X can exist", "But instances of X-like solutions exist in adjacent domains"),
                ("X has no solution", "The solution space is empty", "But relaxing one constraint produces a non-empty solution space"),
                ("X and Y are contradictory", "X and Y cannot coexist", "But nature routinely combines X-like and Y-like properties (wave-particle duality)"),
                ("X must be perfect", "No approximation is acceptable", "But all real systems operate with bounded accuracy (Heisenberg)"),
                ("X requires infinite resources", "No finite system can achieve X", "But approximation algorithms achieve (1-ε)X with finite resources"),
                ("X is undecidable", "No algorithm can decide X generally", "But specific instances of X are decidable (Rice's theorem admits exceptions)"),
            ],
        }
    }

    fn search_contradictions(&self, problem: &str, axioms: &[Axiom]) -> Vec<ContradictionResult> {
        let problem_lower = problem.to_lowercase();
        let mut results = Vec::new();

        // Direct contradiction search via rules
        for (assumption, consequence, contradiction) in &self.logical_rules {
            let assumption_lower = assumption.to_lowercase();
            // Check if the assumption pattern loosely matches any axiom
            let matches_axiom = axioms.iter().any(|ax| {
                let ax_lower = ax.statement.to_lowercase();
                assumption_lower
                    .split_whitespace()
                    .filter(|w| w.len() > 2)
                    .any(|w| ax_lower.contains(w))
            });

            let matches_problem = assumption_lower
                .split_whitespace()
                .filter(|w| w.len() > 3)
                .any(|w| problem_lower.contains(w));

            if matches_axiom || matches_problem {
                results.push(ContradictionResult {
                    assumption: assumption.to_string(),
                    derived_contradiction: format!(
                        "Assume: {}. Then: {}. But: {}. Contradiction!",
                        assumption, consequence, contradiction
                    ),
                    proof_steps: vec![
                        format!("1. Assume for contradiction: {}", assumption),
                        format!("2. By logical necessity: {}", consequence),
                        format!("3. However, we observe: {}", contradiction),
                        "4. Steps 2 and 3 contradict each other".to_string(),
                        format!("5. Therefore, the assumption '{}' is FALSE", assumption),
                        "6. The negation holds: the solution may exist under relaxed conditions"
                            .to_string(),
                    ],
                    conclusion: format!(
                        "By reductio ad absurdum: '{}' is false. \
                         The solution exists, but requires relaxing the assumption.",
                        assumption
                    ),
                    confidence: 0.65,
                });
            }
        }

        // Structural contradiction: look for internal inconsistency in axioms
        for i in 0..axioms.len() {
            for j in (i + 1)..axioms.len() {
                let a = &axioms[i];
                let b = &axioms[j];

                // Check if two axioms potentially conflict
                let a_words: HashSet<&str> = a
                    .statement
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .collect();
                let b_words: HashSet<&str> = b
                    .statement
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .collect();
                let overlap = a_words.intersection(&b_words).count();

                if overlap >= 2 && a.category != b.category {
                    results.push(ContradictionResult {
                        assumption: format!(
                            "Both '{}' AND '{}' hold simultaneously",
                            a.statement, b.statement
                        ),
                        derived_contradiction: format!(
                            "Axiom {} ({}) potentially conflicts with Axiom {} ({}) — \
                             they share {} semantic tokens but operate under different constraint categories",
                            a.id, a.category.as_str(), b.id, b.category.as_str(), overlap
                        ),
                        proof_steps: vec![
                            format!("1. Axiom {}: {}", a.id, a.statement),
                            format!("2. Axiom {}: {}", b.id, b.statement),
                            format!("3. Conflict detected: {} shared tokens across different categories", overlap),
                            "4. At least one axiom must be relaxed for consistency".to_string(),
                        ],
                        conclusion: format!(
                            "Internal inconsistency: axioms {} and {} cannot simultaneously hold. \
                             Relaxing the more breakable one ({}) opens a solution path.",
                            a.id, b.id,
                            if a.category.breakability_score() > b.category.breakability_score() {
                                a.id
                            } else {
                                b.id
                            }
                        ),
                        confidence: 0.5,
                    });
                }
            }
        }

        results
    }
}

// ═══════════════════════════════════════════════════════════════
// IMPOSSIBLE PROBLEM ENGINE — MAIN
// ═══════════════════════════════════════════════════════════════

/// Configuration for the impossible engine.
#[derive(Debug, Clone)]
pub struct ImpossibleConfig {
    pub max_axiom_breaks: usize,
    pub godel_max_meta_levels: usize,
    pub min_break_confidence: f64,
}

impl Default for ImpossibleConfig {
    fn default() -> Self {
        Self {
            max_axiom_breaks: 10,
            godel_max_meta_levels: 3,
            min_break_confidence: 0.3,
        }
    }
}

/// Dimension 3: Impossible Problem Engine.
///
/// Called when Dimensions 1 and 2 fail (confidence < threshold).
/// Systematically breaks axioms, escapes paradoxes, and uses
/// proof-by-contradiction to find unexpected solutions.
pub struct ImpossibleEngine {
    config: ImpossibleConfig,
    axiom_extractor: AxiomExtractor,
    godel_detector: GodelDetector,
    contradiction_miner: ContradictionMiner,
    total_problems: u64,
    total_reframed: u64,
    total_paradoxes_escaped: u64,
    total_contradictions_found: u64,
}

impl ImpossibleEngine {
    pub fn new(config: ImpossibleConfig) -> Self {
        let godel_levels = config.godel_max_meta_levels;
        Self {
            config,
            axiom_extractor: AxiomExtractor::new(),
            godel_detector: GodelDetector::new(godel_levels),
            contradiction_miner: ContradictionMiner::new(),
            total_problems: 0,
            total_reframed: 0,
            total_paradoxes_escaped: 0,
            total_contradictions_found: 0,
        }
    }

    /// Attempt to solve an "impossible" problem.
    pub fn solve_impossible(&mut self, problem: &str) -> ImpossibleResult {
        let start = Instant::now();
        self.total_problems += 1;
        let mut reasoning_trace = Vec::new();
        let mut methods_used = Vec::new();

        info!(
            "[D3-Impossible] Analyzing: {}",
            &problem[..problem.len().min(80)]
        );

        // Phase 1: Extract axioms
        let axioms = self.axiom_extractor.extract(problem);
        reasoning_trace.push(format!(
            "Extracted {} axioms: [{}]",
            axioms.len(),
            axioms
                .iter()
                .map(|a| a.category.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        methods_used.push("axiom_extraction".to_string());

        // Phase 2: Gödel escape (check for self-reference first)
        let godel_escapes = self.godel_detector.detect_and_escape(problem);
        if !godel_escapes.is_empty() {
            self.total_paradoxes_escaped += godel_escapes.len() as u64;
            reasoning_trace.push(format!(
                "Detected {} self-referential structures, escaped via Gödel meta-levels",
                godel_escapes.len()
            ));
            methods_used.push("godel_escape".to_string());
        }

        // Phase 3: Proof by contradiction
        let contradictions = self
            .contradiction_miner
            .search_contradictions(problem, &axioms);
        if !contradictions.is_empty() {
            self.total_contradictions_found += contradictions.len() as u64;
            reasoning_trace.push(format!(
                "Found {} contradictions proving solution existence",
                contradictions.len()
            ));
            methods_used.push("proof_by_contradiction".to_string());
        }

        // Phase 4: Systematic axiom destruction
        let mut axiom_breaks = Vec::new();
        let breakable: Vec<&Axiom> = axioms.iter().filter(|a| a.is_breakable).collect();

        for axiom in breakable.iter().take(self.config.max_axiom_breaks) {
            let break_result = self.break_axiom(problem, axiom);
            if break_result.solution_confidence >= self.config.min_break_confidence {
                axiom_breaks.push(break_result);
            }
        }

        if !axiom_breaks.is_empty() {
            self.total_reframed += 1;
            reasoning_trace.push(format!(
                "Successfully reframed via {} axiom breaks",
                axiom_breaks.len()
            ));
            methods_used.push("axiom_destruction".to_string());
        }

        // Phase 5: Impossibility Spectrum Analyzer (novel formula)
        let constraint_vectors: Vec<advanced_reasoning_formulas::ConstraintVector> = axioms
            .iter()
            .map(|ax| {
                // Build a feature vector from axiom properties
                let mut components = vec![0.0; 8];
                components[0] = ax.confidence;
                components[1] = ax.category.breakability_score();
                components[2] = if ax.is_breakable { 1.0 } else { 0.0 };
                components[3] = ax.statement.len() as f64 / 100.0;
                // Category encoding
                let cat_idx = match ax.category {
                    AxiomCategory::ResourceConstraint => 4,
                    AxiomCategory::PhysicalLaw => 5,
                    AxiomCategory::LogicalNecessity => 6,
                    _ => 7,
                };
                components[cat_idx] = 1.0;
                advanced_reasoning_formulas::ConstraintVector {
                    label: format!("axiom_{}: {}", ax.id, ax.category.as_str()),
                    components,
                }
            })
            .collect();

        // Build impossibility vector from problem features
        let mut impossibility_vec = vec![0.0; 8];
        impossibility_vec[0] = if axiom_breaks.is_empty() { 1.0 } else { 0.3 };
        impossibility_vec[1] = godel_escapes.len() as f64 * 0.4;
        impossibility_vec[2] = contradictions.len() as f64 * 0.3;
        impossibility_vec[3] = axioms.len() as f64 / 10.0;

        let spectrum = advanced_reasoning_formulas::analyze_impossibility_spectrum(
            &constraint_vectors,
            &impossibility_vec,
        );
        reasoning_trace.push(format!(
            "Impossibility Spectrum: total={:.3}, dominant='{}' (contribution={:.3})",
            spectrum.total_impossibility,
            spectrum.dominant_constraint,
            spectrum.dominant_contribution,
        ));
        for (label, proj) in &spectrum.components {
            if proj.abs() > 0.05 {
                reasoning_trace.push(format!("  Constraint '{}': projection={:.3}", label, proj));
            }
        }
        methods_used.push("impossibility_spectrum".to_string());

        // Phase 6: Dimensional Lifting (project into solvable space)
        if axiom_breaks.is_empty() {
            // Problem is still impossible — try lifting to higher dimensions
            let problem_vec: Vec<f64> =
                problem.bytes().take(16).map(|b| b as f64 / 255.0).collect();
            // Use problem_vec itself as a centroid for RBF expansion
            let lifted = cognitive_math::dimensional_lift(
                &problem_vec,
                &[problem_vec.clone()],
                cognitive_math::LiftKernel::RBF { gamma: 1.0 },
            );
            reasoning_trace.push(format!(
                "Dimensional lift: {}D → {}D (added {} features via RBF kernel)",
                problem_vec.len(),
                lifted.len(),
                lifted.len() - problem_vec.len(),
            ));
            methods_used.push("dimensional_lift".to_string());
        }

        // Phase 7: Topological analysis of constraint graph
        let num_axioms = axioms.len();
        let mut topology_edges = Vec::new();
        for i in 0..num_axioms {
            for j in (i + 1)..num_axioms {
                // Connect axioms that share keywords
                let a_words: HashSet<&str> = axioms[i]
                    .statement
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .collect();
                let b_words: HashSet<&str> = axioms[j]
                    .statement
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .collect();
                if a_words.intersection(&b_words).count() >= 2 {
                    topology_edges.push((i, j));
                }
            }
        }
        let topology = cognitive_math::compute_topology(num_axioms.max(2), &topology_edges);
        reasoning_trace.push(format!(
            "Topology: β₀={}, β₁={}, χ={}, paradox_cycles={}",
            topology.betti_0, topology.betti_1, topology.euler_characteristic, topology.betti_1,
        ));
        if topology.betti_1 > 0 {
            reasoning_trace.push(
                "⚠ Constraint graph has cycles → potential circular dependencies / paradoxes"
                    .to_string(),
            );
        }

        // Phase 8: Assess impossibility level
        let best_reframing = axiom_breaks
            .iter()
            .max_by(|a, b| {
                a.solution_confidence
                    .partial_cmp(&b.solution_confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned();

        let impossibility_assessment =
            self.assess_impossibility(&best_reframing, &godel_escapes, &contradictions);

        reasoning_trace.push(format!(
            "Impossibility assessment: {}",
            impossibility_assessment.as_str()
        ));

        let duration = start.elapsed().as_secs_f64() * 1000.0;
        info!(
            "[D3-Impossible] Result: {}, {} axiom breaks, {} paradoxes, spectrum_dom='{}', {:.1}ms",
            impossibility_assessment.as_str(),
            axiom_breaks.len(),
            godel_escapes.len(),
            spectrum.dominant_constraint,
            duration
        );

        ImpossibleResult {
            original_problem: problem.to_string(),
            axioms_extracted: axioms,
            axiom_breaks,
            godel_escapes,
            contradictions,
            best_reframing,
            impossibility_assessment,
            reasoning_trace,
            methods_used,
            duration_ms: duration,
        }
    }

    /// Break a single axiom and attempt to solve in the new space.
    fn break_axiom(&self, problem: &str, axiom: &Axiom) -> AxiomBreakResult {
        let negated = self.negate_axiom(&axiom.statement);
        let reframed = self.reframe_problem(problem, axiom, &negated);
        let (solution, confidence) = self.solve_in_new_space(&reframed, axiom);

        let mut chain = Vec::new();
        chain.push(format!("Original axiom: {}", axiom.statement));
        chain.push(format!("Negated axiom: {}", negated));
        chain.push(format!(
            "Reframed problem: {}",
            &reframed[..reframed.len().min(150)]
        ));
        if let Some(ref sol) = solution {
            chain.push(format!("Solution found: {}", &sol[..sol.len().min(150)]));
        }

        AxiomBreakResult {
            axiom: axiom.clone(),
            negated_statement: negated,
            reframed_problem: reframed,
            solution_in_new_space: solution,
            solution_confidence: confidence,
            explanation: format!(
                "By removing the '{}' constraint (category: {}), new solution paths open.",
                axiom.statement,
                axiom.category.as_str()
            ),
            reasoning_chain: chain,
        }
    }

    fn negate_axiom(&self, statement: &str) -> String {
        let lower = statement.to_lowercase();
        if lower.contains("must") {
            statement
                .replace("must", "need not")
                .replace("Must", "Need not")
        } else if lower.contains("cannot") || lower.contains("can't") {
            statement.replace("cannot", "CAN").replace("can't", "CAN")
        } else if lower.contains("always") {
            statement
                .replace("always", "sometimes")
                .replace("Always", "Sometimes")
        } else if lower.contains("never") {
            statement
                .replace("never", "sometimes")
                .replace("Never", "Sometimes")
        } else if lower.contains("limited") || lower.contains("finite") {
            format!(
                "Negation: {} → REMOVED. Resources are treated as unbounded.",
                statement
            )
        } else if lower.contains("impossible") {
            statement
                .replace("impossible", "POSSIBLE")
                .replace("Impossible", "POSSIBLE")
        } else if lower.contains("sequential") {
            statement
                .replace("sequential", "parallel")
                .replace("Sequential", "Parallel")
        } else {
            format!("NOT({})", statement)
        }
    }

    fn reframe_problem(&self, problem: &str, axiom: &Axiom, negated: &str) -> String {
        format!(
            "REFRAMED PROBLEM: '{}' but with the constraint '{}' replaced by '{}'. \
             Category of removed constraint: {}. \
             In this new axiom space, the problem becomes: solve '{}' without assuming '{}'.",
            &problem[..problem.len().min(150)],
            axiom.statement,
            negated,
            axiom.category.as_str(),
            &problem[..problem.len().min(100)],
            axiom.statement,
        )
    }

    fn solve_in_new_space(&self, _reframed: &str, axiom: &Axiom) -> (Option<String>, f64) {
        // Heuristic solution generation based on axiom category
        let (solution, confidence) = match axiom.category {
            AxiomCategory::ResourceConstraint => (
                format!(
                    "In the resource-unconstrained space: use streaming/paging to handle \
                     arbitrary scale, or use compression to reduce requirements by 10-100×. \
                     Specific technique: bounded-memory algorithms (e.g., Count-Min Sketch \
                     for frequency, HyperLogLog for cardinality, reservoir sampling for \
                     uniform random access)."
                ),
                0.7,
            ),
            AxiomCategory::TechnicalAssumption => (
                format!(
                    "Remove the technology lock-in: evaluate alternative architectures. \
                     If the assumption was 'centralized': use distributed consensus (Raft/Paxos). \
                     If 'sequential': use dataflow parallelism (MapReduce/actors). \
                     If 'specific framework': use a polyglot approach with FFI bridges."
                ),
                0.75,
            ),
            AxiomCategory::SocialConvention => (
                format!(
                    "Challenge the convention: what if users DON'T need this? \
                     Validate by checking if the convention solves a real problem or is \
                     just cargo-culted. Replace with data-driven UX: A/B test the \
                     convention-free version."
                ),
                0.8,
            ),
            AxiomCategory::TemporalConstraint => (
                format!(
                    "Decouple time from correctness: use eventual consistency, \
                     speculative execution, or optimistic concurrency. \
                     Pre-compute likely results and serve them instantly, then \
                     reconcile asynchronously."
                ),
                0.7,
            ),
            AxiomCategory::ExistenceAxiom => (
                format!(
                    "The 'impossibility' was assumed, not proven. By proof-by-contradiction, \
                     if assuming impossibility leads to a contradiction with known facts, \
                     then a solution MUST exist. It may be approximate or probabilistic."
                ),
                0.55,
            ),
            AxiomCategory::IdentityAxiom => (
                format!(
                    "Self-referential structure detected. Apply the Y-combinator approach: \
                     express the self-reference as a fixed-point and compute iteratively \
                     until convergence."
                ),
                0.5,
            ),
            AxiomCategory::PhysicalLaw => (
                format!(
                    "Physical law cannot be broken, but can be worked around. \
                     Use information-theoretic equivalents: if you can't move atoms faster, \
                     move bits instead. Cache, pre-compute, approximate."
                ),
                0.4,
            ),
            AxiomCategory::LogicalNecessity => (
                format!(
                    "Logical necessities (tautologies) cannot be broken. However, \
                     the problem's mapping to this logical form may be incorrect. \
                     Re-examine whether the problem truly reduces to a tautological constraint."
                ),
                0.3,
            ),
        };

        (
            Some(solution),
            confidence * axiom.category.breakability_score(),
        )
    }

    fn assess_impossibility(
        &self,
        best_reframing: &Option<AxiomBreakResult>,
        godel_escapes: &[SelfReference],
        contradictions: &[ContradictionResult],
    ) -> ImpossibilityLevel {
        // Priority 1: If we found a high-confidence reframing
        if let Some(ref reframing) = best_reframing {
            if reframing.solution_confidence > 0.6 {
                return ImpossibilityLevel::Solvable {
                    reframing: reframing.reframed_problem
                        [..reframing.reframed_problem.len().min(200)]
                        .to_string(),
                };
            }
            if reframing.solution_confidence > 0.3 {
                return ImpossibilityLevel::ConditionalSolvable {
                    condition: format!("Requires relaxing: {}", reframing.axiom.statement),
                };
            }
        }

        // Priority 2: If we escaped a paradox
        if !godel_escapes.is_empty() {
            return ImpossibilityLevel::ParadoxResolved {
                resolution: godel_escapes[0].resolution.clone(),
            };
        }

        // Priority 3: If contradictions were found
        if !contradictions.is_empty() {
            let best = &contradictions[0];
            if best.confidence > 0.5 {
                return ImpossibilityLevel::ConditionalSolvable {
                    condition: best.conclusion.clone(),
                };
            }
        }

        // Default: Undetermined
        ImpossibilityLevel::Undetermined
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "Dimension3_ImpossibleEngine_v1.0",
            "total_problems": self.total_problems,
            "total_reframed": self.total_reframed,
            "total_paradoxes_escaped": self.total_paradoxes_escaped,
            "total_contradictions_found": self.total_contradictions_found,
            "reframe_rate": if self.total_problems == 0 { 0.0 }
                else { self.total_reframed as f64 / self.total_problems as f64 },
        })
    }
}

impl Default for ImpossibleEngine {
    fn default() -> Self {
        Self::new(ImpossibleConfig::default())
    }
}

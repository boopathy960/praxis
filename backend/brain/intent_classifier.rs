// ─────────────────────────────────────────────────────────────
// Intent Classifier — Rule-Based Intent Classification
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/cognitive_core.py::IntentClassifier
// No ML — uses keyword scoring and regex pattern matching.
// Classifies into 11 intent types with weighted scoring.

use regex::Regex;
use std::collections::HashMap;

/// Supported intent types for prompt classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IntentType {
    Math,
    Physics,
    Code,
    Logic,
    Proof,
    Synthesis,
    Constraint,
    Discovery,
    Extraction,
    Verification,
    Plan,
    General,
}

impl IntentType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Math => "math",
            Self::Physics => "physics",
            Self::Code => "code",
            Self::Logic => "logic",
            Self::Proof => "proof",
            Self::Synthesis => "synthesis",
            Self::Constraint => "constraint",
            Self::Discovery => "discovery",
            Self::Extraction => "extraction",
            Self::Verification => "verification",
            Self::Plan => "plan",
            Self::General => "general",
        }
    }
}

/// Result of prompt intent classification.
#[derive(Debug, Clone)]
pub struct IntentClassification {
    pub intent: IntentType,
    pub confidence: f64,
    pub signals: Vec<String>,
}

impl Default for IntentClassification {
    fn default() -> Self {
        Self {
            intent: IntentType::General,
            confidence: 0.3,
            signals: Vec::new(),
        }
    }
}

impl std::fmt::Display for IntentClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (conf={:.2})", self.intent.as_str(), self.confidence)
    }
}

/// Intent pattern configuration for a single intent type.
struct IntentPattern {
    keywords: Vec<&'static str>,
    regex_patterns: Vec<&'static str>,
    weight: f64,
}

/// Rule-based intent classifier using keyword scoring and pattern matching.
pub struct IntentClassifier {
    patterns: HashMap<IntentType, IntentPattern>,
}

impl IntentClassifier {
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        patterns.insert(
            IntentType::Math,
            IntentPattern {
                keywords: vec![
                    "calculate",
                    "compute",
                    "solve",
                    "equation",
                    "formula",
                    "sum",
                    "product",
                    "difference",
                    "quotient",
                    "remainder",
                    "average",
                    "mean",
                    "median",
                    "factorial",
                    "fibonacci",
                    "prime",
                    "square root",
                    "percentage",
                    "percent",
                    "integral",
                    "derivative",
                    "matrix",
                    "vector",
                    "plus",
                    "minus",
                    "times",
                    "divided",
                    "multiply",
                    "statistics",
                    "variance",
                    "std",
                    "probability",
                ],
                regex_patterns: vec![
                    r"\d+\s*[+\-*/^%]\s*\d+",
                    r"what is \d+",
                    r"\d+x\s*[+\-]\s*\d+\s*=",
                    r"\d+\s*!",
                ],
                weight: 1.2,
            },
        );

        patterns.insert(
            IntentType::Physics,
            IntentPattern {
                keywords: vec![
                    "velocity",
                    "acceleration",
                    "force",
                    "momentum",
                    "impulse",
                    "newton",
                    "gravity",
                    "weight",
                    "friction",
                    "torque",
                    "energy",
                    "kinetic",
                    "potential",
                    "joule",
                    "watt",
                    "power",
                    "work done",
                    "conservation",
                    "elastic",
                    "temperature",
                    "heat",
                    "thermodynamic",
                    "entropy",
                    "pressure",
                    "ideal gas",
                    "carnot",
                    "boltzmann",
                    "charge",
                    "electric",
                    "magnetic",
                    "coulomb",
                    "ohm",
                    "resistance",
                    "voltage",
                    "current",
                    "capacitor",
                    "wave",
                    "frequency",
                    "wavelength",
                    "photon",
                    "projectile",
                    "free fall",
                    "pendulum",
                    "spring",
                ],
                regex_patterns: vec![
                    r"(?:m/s|kg|N|Pa|J|W|Hz|V|A|C|F)",
                    r"(?:F|E|P|W)\s*=\s*",
                    r"free fall|projectile",
                ],
                weight: 1.3,
            },
        );

        patterns.insert(
            IntentType::Code,
            IntentPattern {
                keywords: vec![
                    "function",
                    "class",
                    "method",
                    "implement",
                    "code",
                    "algorithm",
                    "python",
                    "javascript",
                    "java",
                    "write",
                    "program",
                    "script",
                    "import",
                    "library",
                    "framework",
                    "api",
                    "endpoint",
                    "database",
                    "query",
                    "sql",
                    "sort",
                    "search",
                    "tree",
                    "graph",
                    "hash",
                    "array",
                    "stack",
                    "queue",
                    "linked list",
                    "binary",
                    "recursion",
                    "dynamic programming",
                    "bfs",
                    "dfs",
                    "decorator",
                    "fibonacci",
                    "merge sort",
                    "quick sort",
                    "bubble sort",
                ],
                regex_patterns: vec![
                    r"(?:write|create|implement|build)\s+(?:a\s+)?(?:function|class|method)",
                    r"```",
                    r"def\s+\w+",
                    r"how to (?:code|write|implement)",
                ],
                weight: 1.1,
            },
        );

        patterns.insert(
            IntentType::Logic,
            IntentPattern {
                keywords: vec![
                    "therefore",
                    "premise",
                    "conclude",
                    "syllogism",
                    "truth table",
                    "boolean",
                    "logical",
                    "implies",
                    "if and only if",
                    "contrapositive",
                    "converse",
                    "valid",
                    "invalid",
                    "fallacy",
                    "deduction",
                    "induction",
                    "tautology",
                    "contradiction",
                ],
                regex_patterns: vec![
                    r"(?:all|no|some)\s+\w+\s+are\s+\w+",
                    r"true\s+(?:and|or)\s+(?:true|false)",
                ],
                weight: 1.0,
            },
        );

        patterns.insert(
            IntentType::Proof,
            IntentPattern {
                keywords: vec![
                    "prove",
                    "theorem",
                    "lemma",
                    "corollary",
                    "axiom",
                    "modus ponens",
                    "modus tollens",
                    "syllogism",
                    "resolution",
                    "refutation",
                    "qed",
                    "proof",
                    "demonstrate",
                    "show that",
                    "hence",
                    "thus",
                    "by induction",
                    "base case",
                    "inductive step",
                    "all humans are",
                    "all men are",
                    "contradiction",
                ],
                regex_patterns: vec![
                    r"prove\s+(?:that|the)",
                    r"(?:all|no|some)\s+\w+\s+are\s+\w+",
                    r"by\s+(?:induction|contradiction)",
                    r"show\s+that",
                ],
                weight: 1.4,
            },
        );

        patterns.insert(
            IntentType::Synthesis,
            IntentPattern {
                keywords: vec![
                    "synthesize",
                    "generate program",
                    "from examples",
                    "input output",
                    "i/o examples",
                    "given examples",
                    "write a function that",
                    "create function from",
                    "learn from examples",
                    "infer function",
                ],
                regex_patterns: vec![
                    r"input.*output",
                    r"examples?:\s*\(",
                    r"x\s*=.*y\s*=",
                    r"given.*pairs",
                ],
                weight: 1.3,
            },
        );

        patterns.insert(
            IntentType::Constraint,
            IntentPattern {
                keywords: vec![
                    "n-queens",
                    "queens",
                    "sudoku",
                    "constraint",
                    "coloring",
                    "graph color",
                    "scheduling",
                    "assignment",
                    "satisfy",
                    "csp",
                    "backtracking",
                    "arc consistency",
                    "puzzle",
                ],
                regex_patterns: vec![
                    r"\d+[\s-]*queens?",
                    r"sudoku",
                    r"graph\s+color",
                    r"schedule.*tasks?",
                ],
                weight: 1.3,
            },
        );

        patterns.insert(
            IntentType::Discovery,
            IntentPattern {
                keywords: vec![
                    "discover",
                    "find formula",
                    "find equation",
                    "symbolic regression",
                    "fit data",
                    "evolve",
                    "genetic programming",
                    "find relationship",
                    "formula from data",
                    "pattern in data",
                    "regression",
                    "curve fitting",
                ],
                regex_patterns: vec![
                    r"find.*formula",
                    r"discover.*(?:equation|relationship)",
                    r"x\s*=\s*\[.*\].*y\s*=\s*\[",
                    r"fit.*data",
                ],
                weight: 1.4,
            },
        );

        patterns.insert(
            IntentType::Extraction,
            IntentPattern {
                keywords: vec![
                    "extract",
                    "parse",
                    "json",
                    "xml",
                    "csv",
                    "structured",
                    "format",
                    "convert",
                    "transform",
                    "taskspec",
                    "action_type",
                    "task_type",
                ],
                regex_patterns: vec![r"extract.*(?:from|into)", r"convert.*to\s+(?:json|xml|csv)"],
                weight: 1.3,
            },
        );

        patterns.insert(
            IntentType::Verification,
            IntentPattern {
                keywords: vec![
                    "verify",
                    "check",
                    "validate",
                    "correct",
                    "true or false",
                    "is it true",
                    "confirm",
                    "confidence",
                    "score",
                    "evaluate",
                    "assessment",
                ],
                regex_patterns: vec![r"(?:is|does|can|will)\s+\w+.*\?", r"verify|validate|check"],
                weight: 0.9,
            },
        );

        patterns.insert(
            IntentType::Plan,
            IntentPattern {
                keywords: vec![
                    "plan",
                    "design",
                    "architect",
                    "strategy",
                    "roadmap",
                    "step by step",
                    "how to",
                    "guide",
                    "tutorial",
                    "build",
                    "create",
                    "project",
                    "phase",
                    "milestone",
                ],
                regex_patterns: vec![
                    r"(?:how|help)\s+(?:do|to|me)\s+(?:i|build|create)",
                    r"step by step",
                    r"plan.*(?:for|to)",
                ],
                weight: 0.8,
            },
        );

        Self { patterns }
    }

    /// Classify the intent of a prompt using keyword scoring + regex matching.
    pub fn classify(&self, prompt: &str) -> IntentClassification {
        let prompt_lower = prompt.to_lowercase();
        let mut scores: HashMap<&IntentType, f64> = HashMap::new();
        let mut signals: HashMap<&IntentType, Vec<String>> = HashMap::new();

        for (intent, pattern) in &self.patterns {
            let mut score = 0.0f64;
            let mut intent_signals = Vec::new();

            // Keyword scoring
            for keyword in &pattern.keywords {
                if prompt_lower.contains(keyword) {
                    score += 1.0;
                    intent_signals.push(format!("keyword:{}", keyword));
                }
            }

            // Pattern scoring (higher weight)
            for regex_pat in &pattern.regex_patterns {
                if let Ok(re) = Regex::new(regex_pat) {
                    if re.is_match(&prompt_lower) || re.is_match(prompt) {
                        score += 2.0;
                        let pat_preview: String = regex_pat.chars().take(30).collect();
                        intent_signals.push(format!("pattern:{}", pat_preview));
                    }
                }
            }

            // Apply weight
            score *= pattern.weight;
            scores.insert(intent, score);
            signals.insert(intent, intent_signals);
        }

        // Pick highest-scoring intent
        let max_score = scores.values().cloned().fold(0.0f64, f64::max);
        if max_score == 0.0 {
            return IntentClassification::default();
        }

        let best_intent = scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(k, _)| (*k).clone())
            .unwrap_or(IntentType::General);

        // Normalize confidence (0-1)
        let total_score: f64 = scores.values().sum();
        let confidence = (max_score / total_score.max(1.0) + 0.3).min(1.0);

        IntentClassification {
            intent: best_intent.clone(),
            confidence,
            signals: signals.get(&best_intent).cloned().unwrap_or_default(),
        }
    }
}

impl Default for IntentClassifier {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────
// Hallucination Destroyer — 5-Layer Verification + Fact Anchoring
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/hallucination_destroyer.py

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum VerificationStatus {
    Verified,
    PartiallyVerified,
    Unverified,
    Contradicted,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    pub confidence: f64,
    pub layer_scores: HashMap<String, f64>,
    pub issues: Vec<String>,
    pub anchored_facts: Vec<String>,
}

/// Hallucination Destroyer — 5-layer verification to eliminate false claims.
/// Layers:
///   1. Self-Consistency: Does the answer contradict itself?
///   2. Fact Anchoring: Can claims be grounded in known facts?
///   3. Logic Validation: Is the reasoning logically sound?
///   4. Confidence Calibration: Are confidence levels appropriate?
///   5. Adversarial Challenge: Can the answer survive attack?
pub struct HallucinationDestroyer {
    known_facts: HashMap<String, Vec<String>>,
    contradiction_patterns: Vec<(&'static str, &'static str)>,
    total_checks: u64,
    total_destroyed: u64,
}

impl HallucinationDestroyer {
    pub fn new() -> Self {
        let mut known_facts: HashMap<String, Vec<String>> = HashMap::new();

        // Seed with foundational facts so Layer 2 (Fact Anchoring) has
        // ground truth to validate against from the start.
        known_facts.insert(
            "mathematics".into(),
            vec![
                "the sum of angles in a triangle is 180 degrees".into(),
                "pi is approximately 3.14159".into(),
                "prime numbers have exactly two distinct factors".into(),
                "zero is neither positive nor negative".into(),
                "division by zero is undefined".into(),
                "the square root of 2 is irrational".into(),
                "e is approximately 2.71828".into(),
                "there are infinitely many prime numbers".into(),
                "the empty set is a subset of every set".into(),
                "factorial of 0 is 1".into(),
            ],
        );
        known_facts.insert(
            "logic".into(),
            vec![
                "a proposition and its negation cannot both be true".into(),
                "modus ponens: if P then Q, P is true, therefore Q is true".into(),
                "modus tollens: if P then Q, Q is false, therefore P is false".into(),
                "a tautology is always true".into(),
                "a contradiction is always false".into(),
                "DeMorgan: not(A and B) equals (not A) or (not B)".into(),
                "double negation: not(not A) equals A".into(),
                "logical implication is not symmetric".into(),
                "correlation does not imply causation".into(),
                "the converse of a true statement is not necessarily true".into(),
            ],
        );
        known_facts.insert("physics".into(), vec![
            "the speed of light in vacuum is approximately 299792458 meters per second".into(),
            "energy cannot be created or destroyed only transformed".into(),
            "entropy in an isolated system never decreases".into(),
            "force equals mass times acceleration".into(),
            "every action has an equal and opposite reaction".into(),
            "nothing with mass can travel at the speed of light".into(),
            "absolute zero is approximately minus 273.15 celsius".into(),
            "electrons have negative charge".into(),
            "gravity is always attractive between masses".into(),
            "the uncertainty principle limits simultaneous measurement of position and momentum".into(),
        ]);
        known_facts.insert("computer_science".into(), vec![
            "the halting problem is undecidable".into(),
            "P versus NP is an unsolved problem".into(),
            "sorting comparison-based algorithms have a lower bound of O(n log n)".into(),
            "a binary search requires a sorted collection".into(),
            "a hash table has average O(1) lookup time".into(),
            "recursion requires a base case to terminate".into(),
            "deadlock requires mutual exclusion, hold and wait, no preemption, and circular wait".into(),
            "a Turing machine can simulate any algorithmic process".into(),
            "NP-complete problems are at least as hard as any problem in NP".into(),
            "a stack follows last-in first-out ordering".into(),
        ]);
        known_facts.insert(
            "general".into(),
            vec![
                "water boils at 100 degrees celsius at sea level".into(),
                "the earth orbits the sun".into(),
                "DNA contains genetic instructions".into(),
                "oxygen is required for combustion".into(),
                "sound cannot travel through a vacuum".into(),
            ],
        );
        known_facts.insert(
            "reasoning".into(),
            vec![
                "an argument can be valid but unsound".into(),
                "absence of evidence is not evidence of absence".into(),
                "the burden of proof lies with the claimant".into(),
                "anecdotal evidence does not constitute proof".into(),
                "a single counterexample disproves a universal claim".into(),
            ],
        );

        Self {
            known_facts,
            contradiction_patterns: vec![
                ("always", "never"),
                ("all", "none"),
                ("true", "false"),
                ("correct", "incorrect"),
                ("increase", "decrease"),
                ("positive", "negative"),
                ("possible", "impossible"),
            ],
            total_checks: 0,
            total_destroyed: 0,
        }
    }

    /// Add known facts for fact anchoring.
    pub fn add_known_facts(&mut self, domain: &str, facts: Vec<String>) {
        self.known_facts
            .entry(domain.to_string())
            .or_default()
            .extend(facts);
    }

    /// Run 5-layer verification on a response.
    pub fn verify(&mut self, response: &str, domain: &str) -> VerificationResult {
        self.total_checks += 1;
        let mut layer_scores = HashMap::new();
        let mut issues = Vec::new();
        // Layer 1: Self-Consistency
        let consistency_score = self.check_self_consistency(response, &mut issues);
        layer_scores.insert("self_consistency".into(), consistency_score);

        // Layer 2: Fact Anchoring
        let (anchoring_score, anchored_facts) = self.check_fact_anchoring(response, domain);
        layer_scores.insert("fact_anchoring".into(), anchoring_score);

        // Layer 3: Logic Validation
        let logic_score = self.check_logic_validity(response, &mut issues);
        layer_scores.insert("logic_validation".into(), logic_score);

        // Layer 4: Confidence Calibration
        let calibration_score = self.check_confidence_calibration(response, &mut issues);
        layer_scores.insert("confidence_calibration".into(), calibration_score);

        // Layer 5: Adversarial Challenge
        let adversarial_score = self.check_adversarial_resilience(response, &mut issues);
        layer_scores.insert("adversarial_challenge".into(), adversarial_score);

        // Composite score
        let weights = [0.2, 0.25, 0.2, 0.15, 0.2];
        let scores = [
            consistency_score,
            anchoring_score,
            logic_score,
            calibration_score,
            adversarial_score,
        ];
        let composite: f64 = scores.iter().zip(weights.iter()).map(|(s, w)| s * w).sum();

        let status = if composite >= 0.8 {
            VerificationStatus::Verified
        } else if composite >= 0.5 {
            VerificationStatus::PartiallyVerified
        } else if composite >= 0.2 {
            VerificationStatus::Unverified
        } else {
            self.total_destroyed += 1;
            VerificationStatus::Contradicted
        };

        VerificationResult {
            status,
            confidence: composite,
            layer_scores,
            issues,
            anchored_facts,
        }
    }

    /// Layer 1: Check for internal contradictions.
    fn check_self_consistency(&self, response: &str, issues: &mut Vec<String>) -> f64 {
        let response_lower = response.to_lowercase();
        let mut contradictions = 0;

        for (term_a, term_b) in &self.contradiction_patterns {
            if response_lower.contains(term_a) && response_lower.contains(term_b) {
                // Check if they're in close proximity (potential contradiction)
                if let (Some(pos_a), Some(pos_b)) =
                    (response_lower.find(term_a), response_lower.find(term_b))
                {
                    let distance = if pos_a > pos_b {
                        pos_a - pos_b
                    } else {
                        pos_b - pos_a
                    };
                    if distance < 200 {
                        contradictions += 1;
                        issues.push(format!(
                            "Potential contradiction: '{}' and '{}' found near each other",
                            term_a, term_b
                        ));
                    }
                }
            }
        }

        if contradictions == 0 {
            1.0
        } else if contradictions == 1 {
            0.6
        } else {
            0.3
        }
    }

    /// Layer 2: Check if claims can be grounded in known facts.
    fn check_fact_anchoring(&self, response: &str, domain: &str) -> (f64, Vec<String>) {
        let response_lower = response.to_lowercase();
        let mut anchored = Vec::new();

        let facts = self
            .known_facts
            .get(domain)
            .or_else(|| self.known_facts.get("general"))
            .cloned()
            .unwrap_or_default();

        if facts.is_empty() {
            return (0.7, anchored); // No facts to check against — assume mostly OK
        }

        for fact in &facts {
            let fact_lower = fact.to_lowercase();
            let fact_words: Vec<&str> = fact_lower.split_whitespace().collect();
            let matched = fact_words
                .iter()
                .filter(|w| response_lower.contains(*w))
                .count();
            if matched as f64 / fact_words.len().max(1) as f64 > 0.5 {
                anchored.push(fact.clone());
            }
        }

        let score = if anchored.is_empty() {
            0.4
        } else {
            (anchored.len() as f64 / facts.len().max(1) as f64).min(1.0) * 0.5 + 0.5
        };

        (score, anchored)
    }

    /// Layer 3: Check logical validity of reasoning.
    fn check_logic_validity(&self, response: &str, issues: &mut Vec<String>) -> f64 {
        let has_reasoning = response.contains("because")
            || response.contains("therefore")
            || response.contains("since")
            || response.contains("thus");
        let has_conclusion = response.contains("conclusion")
            || response.contains("result")
            || response.contains("answer");

        let mut score: f64 = 0.7;
        if has_reasoning {
            score += 0.15;
        }
        if has_conclusion {
            score += 0.15;
        }

        // Check for logical fallacies
        let fallacy_indicators = [
            "obviously",
            "everyone knows",
            "it goes without saying",
            "clearly obvious",
            "unquestionable",
        ];
        for fallacy in &fallacy_indicators {
            if response.to_lowercase().contains(fallacy) {
                score -= 0.1;
                issues.push(format!(
                    "Possible appeal to authority/obviousness: '{}'",
                    fallacy
                ));
            }
        }

        score.clamp(0.0, 1.0)
    }

    /// Layer 4: Check if confidence levels are appropriately calibrated.
    fn check_confidence_calibration(&self, response: &str, issues: &mut Vec<String>) -> f64 {
        let response_lower = response.to_lowercase();
        let overconfident_markers = [
            "100% certain",
            "absolutely guaranteed",
            "impossible to be wrong",
            "certainly",
            "undoubtedly",
            "without question",
        ];
        let hedging_markers = [
            "might",
            "possibly",
            "perhaps",
            "could be",
            "uncertain",
            "approximately",
        ];

        let overconfident_count = overconfident_markers
            .iter()
            .filter(|m| response_lower.contains(*m))
            .count();
        let hedging_count = hedging_markers
            .iter()
            .filter(|m| response_lower.contains(*m))
            .count();

        if overconfident_count > 2 {
            issues.push("Response appears overconfident with absolute claims".into());
            return 0.4;
        }

        if hedging_count > 3 && response.len() < 200 {
            issues.push("Too much hedging for a short response — may lack substance".into());
            return 0.5;
        }

        0.85
    }

    /// Layer 5: Adversarial challenge — can the answer survive scrutiny?
    fn check_adversarial_resilience(&self, response: &str, issues: &mut Vec<String>) -> f64 {
        let len = response.len();

        // Very short responses are suspicious for complex questions
        if len < 20 {
            issues.push("Response too short — may not adequately address the question".into());
            return 0.3;
        }

        // Check for vagueness
        let vague_markers = ["something like", "sort of", "kind of", "stuff", "things"];
        let vague_count = vague_markers
            .iter()
            .filter(|m| response.to_lowercase().contains(*m))
            .count();

        if vague_count > 2 {
            issues.push("Response contains vague language".into());
            return 0.5;
        }

        0.85
    }

    pub fn destruction_rate(&self) -> f64 {
        if self.total_checks == 0 {
            0.0
        } else {
            self.total_destroyed as f64 / self.total_checks as f64
        }
    }

    pub fn total_checks(&self) -> u64 {
        self.total_checks
    }
}

impl Default for HallucinationDestroyer {
    fn default() -> Self {
        Self::new()
    }
}

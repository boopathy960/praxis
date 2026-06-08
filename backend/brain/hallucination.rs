// ═══════════════════════════════════════════════════════════════
// Hallucination Destroyer — 5-Layer Verification Pipeline
// ═══════════════════════════════════════════════════════════════
// Ensures every output is grounded in verifiable logic chains.
// Pure rule-based verification with no generative-model dependency.
//
// Verification Stack:
//   Layer 1: Syntactic     — well-formed, no truncation, valid structure
//   Layer 2: Semantic      — internal consistency, no self-contradiction
//   Layer 3: Logical       — no logical fallacies, arithmetic checks
//   Layer 4: Cross-ref     — claims anchored to known facts
//   Layer 5: Calibration   — confidence matches evidence strength
//
// Output: hallucination_score ∈ [0.0, 1.0]
//   0.0 = fully grounded   |   1.0 = pure hallucination

use serde::Serialize;
use std::collections::{HashMap, HashSet};

// ── Data Structures ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VerificationLayer {
    Syntactic = 1,
    Semantic = 2,
    Logical = 3,
    CrossReference = 4,
    Calibration = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HallucinationType {
    Contradiction,
    UnsupportedClaim,
    LogicalFallacy,
    FabricatedFact,
    Overconfidence,
    Structural,
    NumericalError,
}

#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub layer: VerificationLayer,
    pub violation_type: HallucinationType,
    pub description: String,
    pub severity: f64,
    pub location: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerificationResult {
    pub hallucination_score: f64,
    pub grounding_score: f64,
    pub violations: Vec<Violation>,
    pub layer_scores: HashMap<String, f64>,
    pub anchored_claims: usize,
    pub unanchored_claims: usize,
    pub total_claims: usize,
    pub confidence_calibration: f64,
    pub is_clean: bool,
    pub duration_ms: f64,
}

#[allow(dead_code)]
struct AnchorPoint {
    statement: String,
    tokens: HashSet<String>,
}

// ── Layer Checkers ──

fn check_syntactic(response: &str) -> (f64, Vec<Violation>) {
    let mut violations = Vec::new();
    let mut score: f64 = 1.0;
    let stripped = response.trim();

    if stripped.len() < 5 {
        violations.push(Violation {
            layer: VerificationLayer::Syntactic,
            violation_type: HallucinationType::Structural,
            description: "Response is empty or trivially short".into(),
            severity: 0.9,
            location: String::new(),
            suggestion: String::new(),
        });
        return (0.0, violations);
    }

    // Truncation detection
    if stripped.len() > 50 {
        let last = stripped.chars().last().unwrap_or(' ');
        if !".!?\"':)]}…".contains(last) {
            violations.push(Violation {
                layer: VerificationLayer::Syntactic,
                violation_type: HallucinationType::Structural,
                description: "Response appears truncated (no terminal punctuation)".into(),
                severity: 0.3,
                location: String::new(),
                suggestion: "Complete the final sentence".into(),
            });
            score -= 0.15;
        }
    }

    // Unmatched brackets
    for (open, close) in &[('(', ')'), ('[', ']'), ('{', '}')] {
        let open_count = stripped.matches(*open).count();
        let close_count = stripped.matches(*close).count();
        if open_count != close_count {
            violations.push(Violation {
                layer: VerificationLayer::Syntactic,
                violation_type: HallucinationType::Structural,
                description: format!("Unmatched bracket pair: '{}' / '{}'", open, close),
                severity: 0.2,
                location: String::new(),
                suggestion: String::new(),
            });
            score -= 0.1;
        }
    }

    // Unmatched code blocks
    if stripped.matches("```").count() % 2 != 0 {
        violations.push(Violation {
            layer: VerificationLayer::Syntactic,
            violation_type: HallucinationType::Structural,
            description: "Unmatched code block (odd number of ```)".into(),
            severity: 0.3,
            location: String::new(),
            suggestion: String::new(),
        });
        score -= 0.15;
    }

    // Excessive repetition
    let sentences: Vec<&str> = response
        .split(|c: char| c == '.' || c == '!' || c == '?')
        .map(|s| s.trim())
        .filter(|s| s.len() > 10)
        .collect();
    if !sentences.is_empty() {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for s in &sentences {
            *seen.entry(s.to_lowercase()).or_insert(0) += 1;
        }
        let repeats = seen.values().filter(|&&c| c > 2).count();
        if repeats > 0 {
            violations.push(Violation {
                layer: VerificationLayer::Syntactic,
                violation_type: HallucinationType::Structural,
                description: format!(
                    "Excessive repetition: {} sentence(s) repeated 3+ times",
                    repeats
                ),
                severity: 0.5,
                location: String::new(),
                suggestion: String::new(),
            });
            score -= 0.25;
        }
    }

    (score.max(0.0), violations)
}

fn check_semantic(response: &str) -> (f64, Vec<Violation>) {
    let mut violations = Vec::new();
    let sentences: Vec<&str> = response
        .split(|c: char| c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|s| s.trim())
        .filter(|s| s.len() > 8)
        .collect();

    if sentences.len() < 2 {
        return (1.0, violations);
    }

    let mut score: f64 = 1.0;
    let antonyms: &[(&str, &str)] = &[
        ("always", "never"),
        ("true", "false"),
        ("possible", "impossible"),
        ("increase", "decrease"),
        ("faster", "slower"),
        ("more", "less"),
        ("best", "worst"),
        ("safe", "unsafe"),
        ("correct", "incorrect"),
        ("valid", "invalid"),
        ("efficient", "inefficient"),
        ("positive", "negative"),
        ("higher", "lower"),
        ("greater", "smaller"),
        ("optimal", "suboptimal"),
        ("convergent", "divergent"),
        ("stable", "unstable"),
        ("yes", "no"),
        ("can", "cannot"),
    ];

    for i in 0..sentences.len() {
        let end = (i + 8).min(sentences.len());
        for j in (i + 1)..end {
            let s_i = sentences[i].to_lowercase();
            let s_j = sentences[j].to_lowercase();

            for &(word, antonym) in antonyms {
                if s_i.contains(word) && s_j.contains(antonym) {
                    let w_i: HashSet<&str> =
                        s_i.split_whitespace().filter(|w| *w != word).collect();
                    let w_j: HashSet<&str> =
                        s_j.split_whitespace().filter(|w| *w != antonym).collect();
                    if w_i.intersection(&w_j).count() >= 2 {
                        violations.push(Violation {
                            layer: VerificationLayer::Semantic,
                            violation_type: HallucinationType::Contradiction,
                            description: format!(
                                "Contradiction: '{}' vs '{}' about same subject",
                                word, antonym
                            ),
                            severity: 0.7,
                            location: format!("Sentences {} and {}", i + 1, j + 1),
                            suggestion: String::new(),
                        });
                        score -= 0.2;
                    }
                }
            }
        }
    }

    (score.max(0.0), violations)
}

fn check_logical(response: &str) -> (f64, Vec<Violation>) {
    let mut violations = Vec::new();
    let mut score: f64 = 1.0;
    let lower = response.to_lowercase();

    // Fallacy pattern checks (simplified regex-free)
    let fallacy_checks: &[(&str, &str, f64)] = &[
        (
            "everyone knows",
            "Appeal to popularity / common knowledge",
            0.3,
        ),
        (
            "everyone says",
            "Appeal to popularity / common knowledge",
            0.3,
        ),
        (
            "everyone believes",
            "Appeal to popularity / common knowledge",
            0.3,
        ),
        ("obviously ", "Assertion without evidence", 0.2),
        ("clearly ", "Assertion without evidence", 0.2),
        ("undoubtedly ", "Assertion without evidence", 0.2),
        ("without doubt", "Assertion without evidence", 0.2),
        ("no true ", "No true Scotsman fallacy", 0.4),
        ("slippery slope", "Slippery slope reasoning", 0.4),
        ("inevitably lead", "Slippery slope reasoning", 0.4),
    ];

    for &(pattern, name, severity) in fallacy_checks {
        if lower.contains(pattern) {
            violations.push(Violation {
                layer: VerificationLayer::Logical,
                violation_type: HallucinationType::LogicalFallacy,
                description: name.to_string(),
                severity,
                location: String::new(),
                suggestion: String::new(),
            });
            score -= severity * 0.3;
        }
    }

    // Arithmetic verification: find "X op Y = Z" patterns
    let chars: Vec<char> = response.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        // Simple arithmetic pattern detector
        if chars[i].is_ascii_digit() {
            if let Some((a, op, b, claimed, end)) = parse_arithmetic(&chars, i) {
                let actual = match op {
                    '+' => Some(a + b),
                    '-' => Some(a - b),
                    '*' | '×' => Some(a * b),
                    '/' | '÷' => {
                        if b != 0.0 {
                            Some(a / b)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(actual) = actual {
                    if (actual - claimed).abs() > 0.01 {
                        violations.push(Violation {
                            layer: VerificationLayer::Logical,
                            violation_type: HallucinationType::NumericalError,
                            description: format!(
                                "Arithmetic error: {} {} {} = {}, not {}",
                                a, op, b, actual, claimed
                            ),
                            severity: 0.9,
                            location: String::new(),
                            suggestion: String::new(),
                        });
                        score -= 0.3;
                    }
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }

    (score.max(0.0), violations)
}

/// Try to parse "number op number = number" starting at position i
fn parse_arithmetic(chars: &[char], start: usize) -> Option<(f64, char, f64, f64, usize)> {
    let len = chars.len();
    // Parse first number
    let (a, mut pos) = parse_number(chars, start)?;
    // Skip whitespace
    while pos < len && chars[pos] == ' ' {
        pos += 1;
    }
    if pos >= len {
        return None;
    }
    // Parse operator
    let op = chars[pos];
    if !"+−-*/×÷".contains(op) {
        return None;
    }
    let op = if op == '−' { '-' } else { op };
    pos += 1;
    // Skip whitespace
    while pos < len && chars[pos] == ' ' {
        pos += 1;
    }
    // Parse second number
    let (b, mut pos) = parse_number(chars, pos)?;
    // Skip whitespace
    while pos < len && chars[pos] == ' ' {
        pos += 1;
    }
    // Parse '='
    if pos >= len || chars[pos] != '=' {
        return None;
    }
    pos += 1;
    while pos < len && chars[pos] == ' ' {
        pos += 1;
    }
    // Parse result
    let (claimed, end) = parse_number(chars, pos)?;
    Some((a, op, b, claimed, end))
}

fn parse_number(chars: &[char], start: usize) -> Option<(f64, usize)> {
    let len = chars.len();
    if start >= len {
        return None;
    }
    let mut pos = start;
    let mut s = String::new();
    if pos < len && (chars[pos] == '-' || chars[pos] == '+') {
        s.push(chars[pos]);
        pos += 1;
    }
    let has_digit = pos < len && chars[pos].is_ascii_digit();
    if !has_digit {
        return None;
    }
    while pos < len && (chars[pos].is_ascii_digit() || chars[pos] == '.') {
        s.push(chars[pos]);
        pos += 1;
    }
    s.parse::<f64>().ok().map(|v| (v, pos))
}

fn check_cross_reference(response: &str) -> (f64, Vec<Violation>, usize, usize) {
    let anchors = build_anchors();
    let mut violations = Vec::new();

    let sentences: Vec<&str> = response
        .split(|c: char| c == '.' || c == '!' || c == '?' || c == '\n')
        .map(|s| s.trim())
        .filter(|s| s.len() > 15)
        .collect();

    if sentences.is_empty() {
        return (1.0, violations, 0, 0);
    }

    let claim_indicators = [
        "is", "are", "equals", "has", "have", "will", "was", "were", "requires", "takes", "runs",
        "uses", "produces", "computes",
    ];

    let claims: Vec<&&str> = sentences
        .iter()
        .filter(|s| {
            let lower = s.to_lowercase();
            let words: Vec<&str> = lower.split_whitespace().collect();
            claim_indicators.iter().any(|ind| words.contains(ind))
        })
        .collect();

    if claims.is_empty() {
        return (1.0, violations, 0, 0);
    }

    let mut anchored = 0usize;
    let mut unanchored = 0usize;

    for claim in &claims {
        let claim_tokens: HashSet<String> = claim
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let mut best_sim: f64 = 0.0;
        for anchor in &anchors {
            if anchor.tokens.is_empty() {
                continue;
            }
            let intersection = claim_tokens.intersection(&anchor.tokens).count();
            let union = claim_tokens.union(&anchor.tokens).count();
            let sim = if union > 0 {
                intersection as f64 / union as f64
            } else {
                0.0
            };
            best_sim = best_sim.max(sim);
        }

        if best_sim > 0.25 {
            anchored += 1;
        } else {
            unanchored += 1;
        }
    }

    let total = anchored + unanchored;
    if total == 0 {
        return (1.0, violations, 0, 0);
    }

    let score = 0.4 + 0.6 * (anchored as f64 / total as f64);

    if unanchored > anchored * 2 {
        violations.push(Violation {
            layer: VerificationLayer::CrossReference,
            violation_type: HallucinationType::UnsupportedClaim,
            description: format!(
                "{} claims not anchored to known facts (vs {} anchored)",
                unanchored, anchored
            ),
            severity: 0.4,
            location: String::new(),
            suggestion: String::new(),
        });
    }

    (score.max(0.0), violations, anchored, unanchored)
}

fn check_calibration(response: &str, stated_confidence: f64) -> (f64, Vec<Violation>) {
    let mut violations = Vec::new();
    let lower = response.to_lowercase();

    let high_confidence = [
        "definitely",
        "certainly",
        "absolutely",
        "guaranteed",
        "proven",
        "without doubt",
        "100%",
        "always",
        "never fails",
        "impossible to",
    ];
    let low_evidence = [
        "might",
        "perhaps",
        "arguably",
        "some say",
        "could be",
        "not sure",
        "uncertain",
        "debatable",
        "allegedly",
    ];

    let high_count = high_confidence
        .iter()
        .filter(|p| lower.contains(*p))
        .count();
    let low_count = low_evidence.iter().filter(|p| lower.contains(*p)).count();

    if high_count > 0 && low_count > 0 {
        violations.push(Violation {
            layer: VerificationLayer::Calibration,
            violation_type: HallucinationType::Overconfidence,
            description:
                "Mix of high-confidence and uncertainty language suggests poor calibration".into(),
            severity: 0.5,
            location: String::new(),
            suggestion: String::new(),
        });
    }

    if stated_confidence > 0.9 && low_count > 1 {
        violations.push(Violation {
            layer: VerificationLayer::Calibration,
            violation_type: HallucinationType::Overconfidence,
            description: format!(
                "Stated confidence {:.0}% but uses {} hedging phrases",
                stated_confidence * 100.0,
                low_count,
            ),
            severity: 0.4,
            location: String::new(),
            suggestion: String::new(),
        });
    }

    let calibration = if high_count + low_count == 0 {
        1.0
    } else {
        let mixed = high_count.min(low_count) as f64 / (high_count + low_count).max(1) as f64;
        1.0 - mixed * 0.5
    };

    (calibration, violations)
}

fn build_anchors() -> Vec<AnchorPoint> {
    let statements = [
        "pi is approximately 3.14159",
        "e is approximately 2.71828",
        "speed of light is approximately 3e8 m/s",
        "gravitational acceleration is approximately 9.8 m/s²",
        "absolute zero is -273.15 degrees Celsius",
        "water freezes at 0 degrees Celsius",
        "water boils at 100 degrees Celsius at standard pressure",
        "1 + 1 = 2",
        "sum of angles in a triangle is 180 degrees",
        "Pythagorean theorem: a² + b² = c²",
        "there are infinitely many prime numbers",
        "quicksort average case is O(n log n)",
        "merge sort worst case is O(n log n)",
        "binary search requires O(log n) time",
        "hash table average lookup is O(1)",
    ];

    statements
        .iter()
        .map(|s| {
            let tokens: HashSet<String> = s
                .to_lowercase()
                .split_whitespace()
                .map(|w| w.to_string())
                .collect();
            AnchorPoint {
                statement: s.to_string(),
                tokens,
            }
        })
        .collect()
}

// ── Main Engine ──

pub struct HallucinationDestroyer {
    total_verifications: u64,
    total_clean: u64,
    total_violations: u64,
    total_h_score: f64,
}

impl HallucinationDestroyer {
    pub fn new() -> Self {
        Self {
            total_verifications: 0,
            total_clean: 0,
            total_violations: 0,
            total_h_score: 0.0,
        }
    }

    /// Run the full 5-layer verification pipeline.
    pub fn verify(&mut self, response: &str, confidence: f64) -> VerificationResult {
        let start = std::time::Instant::now();
        let mut all_violations = Vec::new();
        let mut layer_scores = HashMap::new();

        // Layer 1: Syntactic
        let (l1, v1) = check_syntactic(response);
        layer_scores.insert("L1_Syntactic".to_string(), l1);
        all_violations.extend(v1);

        // Layer 2: Semantic
        let (l2, v2) = check_semantic(response);
        layer_scores.insert("L2_Semantic".to_string(), l2);
        all_violations.extend(v2);

        // Layer 3: Logical
        let (l3, v3) = check_logical(response);
        layer_scores.insert("L3_Logical".to_string(), l3);
        all_violations.extend(v3);

        // Layer 4: Cross-Reference
        let (l4, v4, anchored, unanchored) = check_cross_reference(response);
        layer_scores.insert("L4_CrossRef".to_string(), l4);
        all_violations.extend(v4);

        // Layer 5: Calibration
        let (l5, v5) = check_calibration(response, confidence);
        layer_scores.insert("L5_Calibration".to_string(), l5);
        all_violations.extend(v5);

        // Weighted aggregate
        let weights = [
            ("L1_Syntactic", 0.10),
            ("L2_Semantic", 0.25),
            ("L3_Logical", 0.30),
            ("L4_CrossRef", 0.25),
            ("L5_Calibration", 0.10),
        ];
        let grounding: f64 = weights
            .iter()
            .map(|(k, w)| layer_scores.get(*k).copied().unwrap_or(1.0) * w)
            .sum();
        let mut h_score = (1.0 - grounding).clamp(0.0, 1.0);

        // Critical severity boost
        let critical_severity: f64 = all_violations
            .iter()
            .filter(|v| v.severity >= 0.7)
            .map(|v| v.severity)
            .sum();
        if critical_severity > 0.0 {
            h_score = (h_score + critical_severity * 0.5).min(1.0);
        }

        let is_clean = all_violations.is_empty();
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Stats
        self.total_verifications += 1;
        if is_clean {
            self.total_clean += 1;
        }
        self.total_violations += all_violations.len() as u64;
        self.total_h_score += h_score;

        VerificationResult {
            hallucination_score: h_score,
            grounding_score: 1.0 - h_score,
            violations: all_violations,
            layer_scores,
            anchored_claims: anchored,
            unanchored_claims: unanchored,
            total_claims: anchored + unanchored,
            confidence_calibration: l5,
            is_clean,
            duration_ms,
        }
    }

    pub fn get_stats(&self) -> serde_json::Value {
        let avg_h = if self.total_verifications > 0 {
            self.total_h_score / self.total_verifications as f64
        } else {
            0.0
        };

        serde_json::json!({
            "engine": "HallucinationDestroyer",
            "verifications": self.total_verifications,
            "clean": self.total_clean,
            "violations_total": self.total_violations,
            "avg_hallucination_score": avg_h,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_response() {
        let mut hd = HallucinationDestroyer::new();
        let result = hd.verify(
            "The sum of angles in a triangle is 180 degrees. Binary search requires O(log n) time.",
            0.8,
        );
        assert!(result.hallucination_score < 0.5);
    }

    #[test]
    fn test_arithmetic_error() {
        let mut hd = HallucinationDestroyer::new();
        let result = hd.verify("The answer is simple: 2 + 2 = 5. This is proven.", 0.95);
        assert!(result
            .violations
            .iter()
            .any(|v| v.violation_type == HallucinationType::NumericalError));
    }

    #[test]
    fn test_empty_response() {
        let mut hd = HallucinationDestroyer::new();
        let result = hd.verify("", 0.5);
        assert!(result.hallucination_score > 0.5);
    }

    #[test]
    fn test_repetitive_response() {
        let mut hd = HallucinationDestroyer::new();
        let repeated = "This is a test sentence. ".repeat(10);
        let result = hd.verify(&repeated, 0.8);
        assert!(!result.is_clean);
    }
}

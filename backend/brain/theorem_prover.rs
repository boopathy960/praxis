// ─────────────────────────────────────────────────────────────
// Theorem Prover — Resolution, Natural Deduction, Induction
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/theorem_prover.py
// Formal logic proof engine with multiple proof strategies.

use std::collections::HashMap;
use std::fmt;

/// Logical proposition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Proposition {
    Atom(String),
    Not(Box<Proposition>),
    And(Box<Proposition>, Box<Proposition>),
    Or(Box<Proposition>, Box<Proposition>),
    Implies(Box<Proposition>, Box<Proposition>),
    ForAll(String, Box<Proposition>),
    Exists(String, Box<Proposition>),
}

impl fmt::Display for Proposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Atom(s) => write!(f, "{}", s),
            Self::Not(p) => write!(f, "¬{}", p),
            Self::And(a, b) => write!(f, "({} ∧ {})", a, b),
            Self::Or(a, b) => write!(f, "({} ∨ {})", a, b),
            Self::Implies(a, b) => write!(f, "({} → {})", a, b),
            Self::ForAll(v, p) => write!(f, "∀{}.{}", v, p),
            Self::Exists(v, p) => write!(f, "∃{}.{}", v, p),
        }
    }
}

/// A proof step in a formal proof.
#[derive(Debug, Clone)]
pub struct ProofStep {
    pub step_number: usize,
    pub proposition: Proposition,
    pub justification: String,
    pub rule: ProofRule,
}

#[derive(Debug, Clone)]
pub enum ProofRule {
    Premise,
    ModusPonens,
    ModusTollens,
    Resolution,
    Syllogism,
    Induction,
    Contradiction,
    UniversalInstantiation,
    ExistentialGeneralization,
    AndIntroduction,
    AndElimination,
    OrIntroduction,
    Assumption,
    QED,
}

impl ProofRule {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Premise => "Premise",
            Self::ModusPonens => "Modus Ponens",
            Self::ModusTollens => "Modus Tollens",
            Self::Resolution => "Resolution",
            Self::Syllogism => "Syllogism (Barbara)",
            Self::Induction => "Mathematical Induction",
            Self::Contradiction => "Proof by Contradiction",
            Self::UniversalInstantiation => "Universal Instantiation",
            Self::ExistentialGeneralization => "Existential Generalization",
            Self::AndIntroduction => "∧-Introduction",
            Self::AndElimination => "∧-Elimination",
            Self::OrIntroduction => "∨-Introduction",
            Self::Assumption => "Assumption",
            Self::QED => "Q.E.D.",
        }
    }
}

/// Result of a proof attempt.
#[derive(Debug, Clone)]
pub struct ProofResult {
    pub proved: bool,
    pub steps: Vec<ProofStep>,
    pub strategy: String,
    pub confidence: f64,
    pub explanation: String,
}

/// Theorem Prover with multiple proof strategies.
pub struct TheoremProver;

impl TheoremProver {
    pub fn new() -> Self {
        Self
    }

    /// Attempt to prove a syllogism: "All A are B. All B are C. Therefore All A are C."
    pub fn prove_syllogism(
        &self,
        premises: &[(String, String)],
        conclusion: (&str, &str),
    ) -> ProofResult {
        let mut steps = Vec::new();
        let mut step_num = 1;

        // Add premises
        for (subject, predicate) in premises {
            steps.push(ProofStep {
                step_number: step_num,
                proposition: Proposition::Implies(
                    Box::new(Proposition::Atom(subject.clone())),
                    Box::new(Proposition::Atom(predicate.clone())),
                ),
                justification: "Given".into(),
                rule: ProofRule::Premise,
            });
            step_num += 1;
        }

        // Check if we can chain premises to reach conclusion
        let mut chain: HashMap<String, String> = HashMap::new();
        for (s, p) in premises {
            chain.insert(s.clone(), p.clone());
        }

        // Try to build chain from conclusion subject to conclusion predicate
        let mut current = conclusion.0.to_string();
        let mut proof_chain = vec![current.clone()];
        let mut proved = false;

        for _ in 0..premises.len() + 1 {
            if current == conclusion.1 {
                proved = true;
                break;
            }
            if let Some(next) = chain.get(&current) {
                proof_chain.push(next.clone());
                current = next.clone();
            } else {
                break;
            }
        }

        if proved {
            // Add syllogism step
            steps.push(ProofStep {
                step_number: step_num,
                proposition: Proposition::Implies(
                    Box::new(Proposition::Atom(conclusion.0.to_string())),
                    Box::new(Proposition::Atom(conclusion.1.to_string())),
                ),
                justification: format!(
                    "By transitivity through chain: {}",
                    proof_chain.join(" → ")
                ),
                rule: ProofRule::Syllogism,
            });
            step_num += 1;

            steps.push(ProofStep {
                step_number: step_num,
                proposition: Proposition::Atom("■".into()),
                justification: "Conclusion follows from premises".into(),
                rule: ProofRule::QED,
            });
        }

        let explanation = if proved {
            format!(
                "Proved via Barbara Syllogism (AAA-1):\n  Chain: {}\n  \
                 All {} are {} ✓",
                proof_chain.join(" → "),
                conclusion.0,
                conclusion.1
            )
        } else {
            "Could not establish chain of implication between premises.".into()
        };

        ProofResult {
            proved,
            steps,
            strategy: "syllogism".into(),
            confidence: if proved { 1.0 } else { 0.0 },
            explanation,
        }
    }

    /// Attempt Modus Ponens: P, P → Q ⊢ Q
    pub fn modus_ponens(&self, p: &Proposition, implication: &Proposition) -> Option<ProofResult> {
        if let Proposition::Implies(antecedent, consequent) = implication {
            if **antecedent == *p {
                let steps = vec![
                    ProofStep {
                        step_number: 1,
                        proposition: p.clone(),
                        justification: "Given".into(),
                        rule: ProofRule::Premise,
                    },
                    ProofStep {
                        step_number: 2,
                        proposition: implication.clone(),
                        justification: "Given".into(),
                        rule: ProofRule::Premise,
                    },
                    ProofStep {
                        step_number: 3,
                        proposition: *consequent.clone(),
                        justification: "From 1 and 2 by Modus Ponens".into(),
                        rule: ProofRule::ModusPonens,
                    },
                ];
                return Some(ProofResult {
                    proved: true,
                    steps,
                    strategy: "modus_ponens".into(),
                    confidence: 1.0,
                    explanation: format!("P: {}\nP → Q: {}\n∴ Q: {}", p, implication, consequent),
                });
            }
        }
        None
    }

    /// Attempt Modus Tollens: ¬Q, P → Q ⊢ ¬P
    pub fn modus_tollens(
        &self,
        not_q: &Proposition,
        implication: &Proposition,
    ) -> Option<ProofResult> {
        if let (Proposition::Not(q), Proposition::Implies(p, consequent)) = (not_q, implication) {
            if **q == **consequent {
                let conclusion = Proposition::Not(p.clone());
                let steps = vec![
                    ProofStep {
                        step_number: 1,
                        proposition: not_q.clone(),
                        justification: "Given".into(),
                        rule: ProofRule::Premise,
                    },
                    ProofStep {
                        step_number: 2,
                        proposition: implication.clone(),
                        justification: "Given".into(),
                        rule: ProofRule::Premise,
                    },
                    ProofStep {
                        step_number: 3,
                        proposition: conclusion.clone(),
                        justification: "From 1 and 2 by Modus Tollens".into(),
                        rule: ProofRule::ModusTollens,
                    },
                ];
                return Some(ProofResult {
                    proved: true,
                    steps,
                    strategy: "modus_tollens".into(),
                    confidence: 1.0,
                    explanation: format!(
                        "¬Q: {}\nP → Q: {}\n∴ ¬P: {}",
                        not_q, implication, conclusion
                    ),
                });
            }
        }
        None
    }

    /// Proof by Mathematical Induction.
    pub fn prove_by_induction(
        &self,
        property: &str,
        base_case_holds: bool,
        inductive_step_holds: bool,
    ) -> ProofResult {
        let mut steps = Vec::new();

        steps.push(ProofStep {
            step_number: 1,
            proposition: Proposition::Atom(format!("P(0): {}", property)),
            justification: if base_case_holds {
                "Base case verified ✓"
            } else {
                "Base case FAILED ✗"
            }
            .into(),
            rule: ProofRule::Premise,
        });

        steps.push(ProofStep {
            step_number: 2,
            proposition: Proposition::Implies(
                Box::new(Proposition::Atom("P(k)".into())),
                Box::new(Proposition::Atom("P(k+1)".into())),
            ),
            justification: if inductive_step_holds {
                "Inductive step verified ✓"
            } else {
                "Inductive step FAILED ✗"
            }
            .into(),
            rule: ProofRule::Assumption,
        });

        let proved = base_case_holds && inductive_step_holds;
        if proved {
            steps.push(ProofStep {
                step_number: 3,
                proposition: Proposition::ForAll(
                    "n".into(),
                    Box::new(Proposition::Atom(format!("P(n): {}", property))),
                ),
                justification: "By Mathematical Induction on steps 1 and 2".into(),
                rule: ProofRule::Induction,
            });
        }

        ProofResult {
            proved,
            steps,
            strategy: "induction".into(),
            confidence: if proved { 1.0 } else { 0.0 },
            explanation: format!(
                "Induction on '{}': base={}, step={} → {}",
                property,
                base_case_holds,
                inductive_step_holds,
                if proved { "PROVED" } else { "FAILED" }
            ),
        }
    }
}

impl Default for TheoremProver {
    fn default() -> Self {
        Self::new()
    }
}

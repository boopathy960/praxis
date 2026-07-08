//! The Envelope — the decidable constraint language at the heart of Genome OS.
//!
//! The Genome OS concept brief is explicit that the entire bet reduces to one
//! language-design choice: the genome's constraint language must be *expressive*
//! enough to capture real operations, yet *restricted* enough to stay
//! **decidable, refinable, and composable**. This module is that language.
//!
//! It is a deliberately small, quantifier-free fragment: typed operation
//! variables (money, counts, flags, tags, timestamps) and a conjunction of
//! decidable invariant templates over linear arithmetic and set membership.
//! Three operations are then total and decidable on this fragment:
//!
//!   * **Evaluation / admission** (§6.1, control-barrier-function logic) —
//!     [`Envelope::evaluate`] decides whether an operation context sits inside
//!     the safe set. The runtime admits an action only if the resulting context
//!     is admitted, so an agent *cannot* leave the envelope by construction.
//!   * **Refinement** (§6.2, the partial order on the commons) —
//!     [`Envelope::refines`] decides whether one envelope is at least as strong
//!     as another. A valid fork is a refinement: it preserves the parent's
//!     guarantees while adding constraints.
//!   * **Assume-guarantee composition** (§6.2) — [`Envelope::discharges`]
//!     decides whether one genome's guarantees satisfy another's assumptions, so
//!     `A` then `B` is safe exactly when `guarantees(A) => assumptions(B)`.
//!
//! Every envelope also [`compile`](Envelope::compile_to_check)s to a real
//! [`Check`](crate::verification::Check) that runs through the supervised device
//! layer, which is how a genome's Certificate is anchored in the proof economy
//! rather than merely asserted.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::verification::Check;

/// A typed operation value. The fragment is total over these — every term and
/// atom evaluates to a definite truth value on any well-typed context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum OpValue {
    /// A monetary amount (in the genome's currency minor units or a decimal).
    Money(f64),
    /// A non-negative count (retries, items, days overdue).
    Count(i64),
    /// A boolean fact (approved, disputed, eligible).
    Flag(bool),
    /// An enumerated tag (segment, status, channel).
    Tag(String),
    /// A unix-millis timestamp.
    Time(i64),
}

impl OpValue {
    /// Project to a real number for linear arithmetic, where one is defined.
    /// Flags map to 0/1; tags have no numeric value.
    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            OpValue::Money(value) => Some(*value),
            OpValue::Count(value) => Some(*value as f64),
            OpValue::Time(value) => Some(*value as f64),
            OpValue::Flag(flag) => Some(if *flag { 1.0 } else { 0.0 }),
            OpValue::Tag(_) => None,
        }
    }

    #[must_use]
    pub fn as_flag(&self) -> Option<bool> {
        match self {
            OpValue::Flag(flag) => Some(*flag),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_tag(&self) -> Option<&str> {
        match self {
            OpValue::Tag(tag) => Some(tag.as_str()),
            _ => None,
        }
    }
}

/// A binding of operation variables to values — the thing an envelope decides on.
pub type Context = BTreeMap<String, OpValue>;

/// A linear term over operation variables: `Σ scaleᵢ·varᵢ + constant`. Kept
/// linear on purpose — linear arithmetic over the ordered reals is decidable,
/// which is what makes admission and (sound) refinement decidable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Term {
    Var {
        name: String,
    },
    Const {
        value: f64,
    },
    /// Sum of sub-terms.
    Sum {
        terms: Vec<Term>,
    },
    /// A scaled sub-term (`factor · term`), the only multiplication allowed —
    /// scalar-by-term keeps the fragment linear.
    Scale {
        factor: f64,
        term: Box<Term>,
    },
}

impl Term {
    #[must_use]
    pub fn var(name: impl Into<String>) -> Self {
        Term::Var { name: name.into() }
    }

    #[must_use]
    pub fn constant(value: f64) -> Self {
        Term::Const { value }
    }

    /// Evaluate the term against a context. `None` when a referenced variable is
    /// missing or non-numeric — an under-specified context cannot be admitted.
    #[must_use]
    pub fn eval(&self, context: &Context) -> Option<f64> {
        match self {
            Term::Var { name } => context.get(name).and_then(OpValue::as_number),
            Term::Const { value } => Some(*value),
            Term::Sum { terms } => terms
                .iter()
                .try_fold(0.0, |acc, term| Some(acc + term.eval(context)?)),
            Term::Scale { factor, term } => term.eval(context).map(|value| factor * value),
        }
    }

    /// The single variable this term reduces to, if it is exactly `1·var`.
    /// Used by the refinement checker to relate atoms on the same variable.
    fn lone_var(&self) -> Option<&str> {
        match self {
            Term::Var { name } => Some(name.as_str()),
            Term::Scale { factor, term } if (*factor - 1.0).abs() < f64::EPSILON => term.lone_var(),
            _ => None,
        }
    }
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Le,
    Lt,
    Ge,
    Gt,
    Eq,
    Ne,
}

impl Cmp {
    #[must_use]
    fn holds(self, left: f64, right: f64) -> bool {
        match self {
            Cmp::Le => left <= right + EPS,
            Cmp::Lt => left < right - EPS,
            Cmp::Ge => left + EPS >= right,
            Cmp::Gt => left - EPS > right,
            Cmp::Eq => (left - right).abs() <= EPS,
            Cmp::Ne => (left - right).abs() > EPS,
        }
    }

    #[must_use]
    fn symbol(self) -> &'static str {
        match self {
            Cmp::Le => "≤",
            Cmp::Lt => "<",
            Cmp::Ge => "≥",
            Cmp::Gt => ">",
            Cmp::Eq => "=",
            Cmp::Ne => "≠",
        }
    }
}

const EPS: f64 = 1e-9;

/// A decidable atomic predicate — the leaves of the constraint language.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Atom {
    /// A linear comparison `left <cmp> right`.
    Compare { left: Term, op: Cmp, right: Term },
    /// A flag must be true.
    FlagTrue { flag: String },
    /// A flag must be false.
    FlagFalse { flag: String },
    /// A tag variable must be one of an allowed set (eligibility / membership).
    InSet { var: String, allowed: Vec<String> },
    /// A numeric variable must lie in `[lo, hi]`.
    Bound { var: String, lo: f64, hi: f64 },
}

impl Atom {
    /// Decide the atom against a context, returning a human-readable detail.
    #[must_use]
    pub fn evaluate(&self, context: &Context) -> AtomOutcome {
        match self {
            Atom::Compare { left, op, right } => match (left.eval(context), right.eval(context)) {
                (Some(l), Some(r)) => {
                    AtomOutcome::new(op.holds(l, r), format!("{l:.4} {} {r:.4}", op.symbol()))
                }
                _ => AtomOutcome::new(false, "undefined variable in comparison".into()),
            },
            Atom::FlagTrue { flag } => {
                let value = context.get(flag).and_then(OpValue::as_flag);
                AtomOutcome::new(value == Some(true), format!("{flag} is true: {value:?}"))
            }
            Atom::FlagFalse { flag } => {
                let value = context.get(flag).and_then(OpValue::as_flag);
                AtomOutcome::new(value == Some(false), format!("{flag} is false: {value:?}"))
            }
            Atom::InSet { var, allowed } => {
                let value = context
                    .get(var)
                    .and_then(OpValue::as_tag)
                    .map(str::to_string);
                let ok = value
                    .as_deref()
                    .is_some_and(|tag| allowed.iter().any(|a| a == tag));
                AtomOutcome::new(ok, format!("{var}={value:?} ∈ {allowed:?}"))
            }
            Atom::Bound { var, lo, hi } => {
                let value = context.get(var).and_then(OpValue::as_number);
                let ok = value.is_some_and(|v| v + EPS >= *lo && v <= *hi + EPS);
                AtomOutcome::new(ok, format!("{lo:.2} ≤ {var}={value:?} ≤ {hi:.2}"))
            }
        }
    }

    /// Normalise the atom to the set of bound-facts it implies, the canonical
    /// form the refinement checker reasons over.
    fn facts(&self) -> Vec<Fact> {
        match self {
            Atom::Bound { var, lo, hi } => vec![
                Fact::UpperBound {
                    var: var.clone(),
                    bound: *hi,
                },
                Fact::LowerBound {
                    var: var.clone(),
                    bound: *lo,
                },
            ],
            Atom::FlagTrue { flag } => vec![Fact::Flag {
                flag: flag.clone(),
                value: true,
            }],
            Atom::FlagFalse { flag } => vec![Fact::Flag {
                flag: flag.clone(),
                value: false,
            }],
            Atom::InSet { var, allowed } => vec![Fact::Member {
                var: var.clone(),
                allowed: allowed.iter().cloned().collect(),
            }],
            Atom::Compare { left, op, right } => compare_facts(left, *op, right),
        }
    }
}

/// Lower a comparison into canonical upper/lower-bound facts where it reduces to
/// `var <cmp> const` (or `const <cmp> var`). Comparisons between two genuine
/// terms are kept opaque as a [`Fact::Relational`] so composition can still
/// match identical relational guarantees (e.g. money conservation).
fn compare_facts(left: &Term, op: Cmp, right: &Term) -> Vec<Fact> {
    let canon = format!(
        "{}{}{}",
        canonical_term(left),
        op.symbol(),
        canonical_term(right)
    );
    if let (Some(var), Term::Const { value }) = (left.lone_var(), right) {
        match op {
            Cmp::Le | Cmp::Lt => {
                return vec![Fact::UpperBound {
                    var: var.to_string(),
                    bound: *value,
                }];
            }
            Cmp::Ge | Cmp::Gt => {
                return vec![Fact::LowerBound {
                    var: var.to_string(),
                    bound: *value,
                }];
            }
            Cmp::Eq => {
                return vec![
                    Fact::UpperBound {
                        var: var.to_string(),
                        bound: *value,
                    },
                    Fact::LowerBound {
                        var: var.to_string(),
                        bound: *value,
                    },
                ];
            }
            Cmp::Ne => {}
        }
    }
    vec![Fact::Relational { canonical: canon }]
}

fn canonical_term(term: &Term) -> String {
    match term {
        Term::Var { name } => name.clone(),
        Term::Const { value } => format!("{value:.6}"),
        Term::Sum { terms } => terms
            .iter()
            .map(canonical_term)
            .collect::<Vec<_>>()
            .join("+"),
        Term::Scale { factor, term } => format!("{factor:.6}*{}", canonical_term(term)),
    }
}

/// The canonical, comparable form of a guarantee — what refinement is a partial
/// order over.
#[derive(Debug, Clone, PartialEq)]
enum Fact {
    UpperBound {
        var: String,
        bound: f64,
    },
    LowerBound {
        var: String,
        bound: f64,
    },
    Flag {
        flag: String,
        value: bool,
    },
    Member {
        var: String,
        allowed: std::collections::BTreeSet<String>,
    },
    Relational {
        canonical: String,
    },
}

impl Fact {
    /// Does `self` (a child fact) entail `other` (a parent fact)? This is the
    /// per-fact refinement order: a tighter bound, a subset membership, or an
    /// identical flag/relational fact entails the looser parent.
    fn entails(&self, other: &Fact) -> bool {
        match (self, other) {
            (Fact::UpperBound { var: a, bound: x }, Fact::UpperBound { var: b, bound: y }) => {
                a == b && *x <= *y + EPS
            }
            (Fact::LowerBound { var: a, bound: x }, Fact::LowerBound { var: b, bound: y }) => {
                a == b && *x + EPS >= *y
            }
            (Fact::Flag { flag: a, value: x }, Fact::Flag { flag: b, value: y }) => {
                a == b && x == y
            }
            (Fact::Member { var: a, allowed: x }, Fact::Member { var: b, allowed: y }) => {
                a == b && x.is_subset(y)
            }
            (Fact::Relational { canonical: x }, Fact::Relational { canonical: y }) => x == y,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AtomOutcome {
    pub holds: bool,
    pub detail: String,
}

impl AtomOutcome {
    fn new(holds: bool, detail: String) -> Self {
        Self { holds, detail }
    }
}

/// A named, decidable invariant template. These are the vocabulary a business
/// operator (or the compiler) actually composes an envelope from; each lowers to
/// one or more [`Atom`]s. The `Idempotency` template is *structural* — it has no
/// truth value on a single context (it is enforced by the runtime's dedupe by
/// key) and is carried as a guarantee the Certificate records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "invariant", rename_all = "snake_case")]
pub enum Invariant {
    /// Conservation of money: total outflow never exceeds inflow (never refund
    /// more than was paid). The canonical hard invariant of back-office finance.
    MoneyConservation { outflow: String, inflow: String },
    /// A retried action keyed by `key` is applied at most once.
    Idempotency { key: String },
    /// A running counter never exceeds a cap (rate / volume limit).
    RateLimit { counter: String, max: f64 },
    /// An eligibility gate: a tag variable must be one of an allowed set.
    Eligibility { var: String, allowed: Vec<String> },
    /// A numeric variable is confined to `[lo, hi]`.
    Bounded { var: String, lo: f64, hi: f64 },
    /// A flag that must hold (e.g. `owner_approved`).
    Requires { flag: String },
    /// A flag that must not hold (e.g. `account_frozen`).
    Forbids { flag: String },
    /// An escape hatch for any decidable atom, with a human label.
    Predicate { label: String, atom: Atom },
}

impl Invariant {
    /// The decidable atoms this invariant lowers to. Structural invariants
    /// (idempotency) lower to no atoms — they are enforced, not evaluated.
    #[must_use]
    pub fn atoms(&self) -> Vec<Atom> {
        match self {
            Invariant::MoneyConservation { outflow, inflow } => vec![Atom::Compare {
                left: Term::var(outflow.clone()),
                op: Cmp::Le,
                right: Term::var(inflow.clone()),
            }],
            Invariant::Idempotency { .. } => Vec::new(),
            Invariant::RateLimit { counter, max } => vec![Atom::Compare {
                left: Term::var(counter.clone()),
                op: Cmp::Le,
                right: Term::constant(*max),
            }],
            Invariant::Eligibility { var, allowed } => vec![Atom::InSet {
                var: var.clone(),
                allowed: allowed.clone(),
            }],
            Invariant::Bounded { var, lo, hi } => vec![Atom::Bound {
                var: var.clone(),
                lo: *lo,
                hi: *hi,
            }],
            Invariant::Requires { flag } => vec![Atom::FlagTrue { flag: flag.clone() }],
            Invariant::Forbids { flag } => vec![Atom::FlagFalse { flag: flag.clone() }],
            Invariant::Predicate { atom, .. } => vec![atom.clone()],
        }
    }

    /// A short human description for the projected UI and certificates.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Invariant::MoneyConservation { outflow, inflow } => {
                format!("conservation of money: {outflow} ≤ {inflow}")
            }
            Invariant::Idempotency { key } => {
                format!("idempotent on key `{key}` (retries never double-apply)")
            }
            Invariant::RateLimit { counter, max } => format!("rate limit: {counter} ≤ {max:.0}"),
            Invariant::Eligibility { var, allowed } => format!("eligibility: {var} ∈ {allowed:?}"),
            Invariant::Bounded { var, lo, hi } => format!("bound: {lo:.2} ≤ {var} ≤ {hi:.2}"),
            Invariant::Requires { flag } => format!("requires `{flag}`"),
            Invariant::Forbids { flag } => format!("forbids `{flag}`"),
            Invariant::Predicate { label, .. } => label.clone(),
        }
    }

    /// Whether this invariant is structurally enforced rather than evaluated.
    #[must_use]
    pub fn is_structural(&self) -> bool {
        matches!(self, Invariant::Idempotency { .. })
    }

    fn facts(&self) -> Vec<Fact> {
        self.atoms().iter().flat_map(Atom::facts).collect()
    }
}

/// The Envelope (E) — a conjunction of decidable invariants. An operation is
/// admitted only when *every* non-structural invariant holds on its context.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default)]
    pub invariants: Vec<Invariant>,
}

/// The result of deciding an envelope against a context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvelopeReport {
    pub holds: bool,
    pub checked: usize,
    pub violations: Vec<String>,
    pub details: Vec<String>,
}

impl Envelope {
    #[must_use]
    pub fn new(invariants: Vec<Invariant>) -> Self {
        Self { invariants }
    }

    /// Decide the whole envelope on a context (the conjunction of every
    /// non-structural invariant). This is the admission test — §6.1's
    /// control-barrier set membership.
    #[must_use]
    pub fn evaluate(&self, context: &Context) -> EnvelopeReport {
        let mut violations = Vec::new();
        let mut details = Vec::new();
        let mut checked = 0;
        for invariant in &self.invariants {
            if invariant.is_structural() {
                continue;
            }
            for atom in invariant.atoms() {
                checked += 1;
                let outcome = atom.evaluate(context);
                details.push(format!("{}: {}", invariant.describe(), outcome.detail));
                if !outcome.holds {
                    violations.push(format!("{} — {}", invariant.describe(), outcome.detail));
                }
            }
        }
        EnvelopeReport {
            holds: violations.is_empty(),
            checked,
            violations,
            details,
        }
    }

    /// True when the context is inside the safe set. The runtime gates on this.
    #[must_use]
    pub fn admits(&self, context: &Context) -> bool {
        self.evaluate(context).holds
    }

    fn facts(&self) -> Vec<Fact> {
        self.invariants.iter().flat_map(Invariant::facts).collect()
    }

    /// Structural guarantees carried even though they are not point-evaluable
    /// (e.g. idempotency keys), so refinement and composition can reason on them.
    fn structural_keys(&self) -> std::collections::BTreeSet<String> {
        self.invariants
            .iter()
            .filter_map(|invariant| match invariant {
                Invariant::Idempotency { key } => Some(key.clone()),
                _ => None,
            })
            .collect()
    }

    /// **Refinement** (§6.2): does `self` refine `parent`? A refinement is at
    /// least as strong — every parent guarantee is entailed by some child
    /// guarantee, and every structural guarantee of the parent is preserved.
    /// This is sound (a `true` is always a real refinement); it is intentionally
    /// incomplete, the price of decidability on an unrestricted fragment.
    #[must_use]
    pub fn refines(&self, parent: &Envelope) -> bool {
        let child_facts = self.facts();
        let parent_ok = parent.facts().iter().all(|parent_fact| {
            child_facts
                .iter()
                .any(|child_fact| child_fact.entails(parent_fact))
        });
        let structural_ok = parent.structural_keys().is_subset(&self.structural_keys());
        parent_ok && structural_ok
    }

    /// **Assume-guarantee composition** (§6.2): do `self`'s guarantees discharge
    /// `downstream`'s assumptions? `A` composed with `B` is safe exactly when
    /// this holds — `guarantees(A) => assumptions(B)`.
    #[must_use]
    pub fn discharges(&self, downstream_assumptions: &Envelope) -> bool {
        let guarantees = self.facts();
        downstream_assumptions.facts().iter().all(|assumption| {
            guarantees
                .iter()
                .any(|guarantee| guarantee.entails(assumption))
        })
    }

    /// The guarantees of `self` (a parent) that a proposed `child` fails to
    /// preserve — the explanation of why a fork is not a valid refinement. A
    /// structural guarantee (idempotency) is unmet when the child drops its key.
    #[must_use]
    pub fn unmet_by_child(&self, child: &Envelope) -> Vec<String> {
        let child_facts = child.facts();
        let child_structural = child.structural_keys();
        self.invariants
            .iter()
            .filter(|invariant| match invariant {
                Invariant::Idempotency { key } => !child_structural.contains(key),
                other => other.facts().iter().any(|fact| {
                    !child_facts
                        .iter()
                        .any(|child_fact| child_fact.entails(fact))
                }),
            })
            .map(Invariant::describe)
            .collect()
    }

    /// The unmet assumptions, for explaining why a composition is invalid.
    #[must_use]
    pub fn unmet_by(&self, downstream_assumptions: &Envelope) -> Vec<String> {
        let guarantees = self.facts();
        downstream_assumptions
            .invariants
            .iter()
            .filter(|assumption| {
                assumption
                    .facts()
                    .iter()
                    .any(|fact| !guarantees.iter().any(|guarantee| guarantee.entails(fact)))
            })
            .map(Invariant::describe)
            .collect()
    }

    /// Compile the envelope into a real, device-runnable [`Check`] used to anchor
    /// the genome's Certificate in the proof economy. The certificate asserts the
    /// operational truth *"no run of this genome has ever recorded an envelope
    /// violation"* — the runtime appends to `violations_path` on any breach, so an
    /// absent or clean ledger proves the envelope has held in production. A purely
    /// structural envelope (no point-evaluable invariants) still yields a
    /// meaningful clean-ledger check.
    #[must_use]
    pub fn compile_to_check(&self, violations_path: &str) -> Check {
        Check::Not {
            check: Box::new(Check::FileContains {
                path: violations_path.to_string(),
                substring: VIOLATION_MARKER.to_string(),
            }),
        }
    }

    /// Human-readable guarantee list for the projected UI.
    #[must_use]
    pub fn describe(&self) -> Vec<String> {
        self.invariants.iter().map(Invariant::describe).collect()
    }

    /// A coarse description-length (in bits) of the envelope, feeding the
    /// minimum-description-length accounting the commons uses to price a fork by
    /// the size of its deviation (§6.0).
    #[must_use]
    pub fn description_bits(&self) -> f64 {
        self.invariants
            .iter()
            .map(|invariant| 8.0 + invariant.atoms().len() as f64 * 12.0)
            .sum()
    }
}

/// The marker the runtime writes into a genome's violation ledger; the compiled
/// certificate check fails the instant this string appears.
pub const VIOLATION_MARKER: &str = "ENVELOPE_VIOLATION";

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(pairs: &[(&str, OpValue)]) -> Context {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn money_conservation_admits_and_rejects() {
        let env = Envelope::new(vec![Invariant::MoneyConservation {
            outflow: "refund".into(),
            inflow: "paid".into(),
        }]);
        assert!(env.admits(&ctx(&[
            ("refund", OpValue::Money(40.0)),
            ("paid", OpValue::Money(100.0))
        ])));
        assert!(!env.admits(&ctx(&[
            ("refund", OpValue::Money(140.0)),
            ("paid", OpValue::Money(100.0))
        ])));
    }

    #[test]
    fn tighter_bound_refines_looser() {
        let parent = Envelope::new(vec![Invariant::Bounded {
            var: "discount".into(),
            lo: 0.0,
            hi: 20.0,
        }]);
        let child = Envelope::new(vec![Invariant::Bounded {
            var: "discount".into(),
            lo: 0.0,
            hi: 12.0,
        }]);
        assert!(
            child.refines(&parent),
            "a tighter bound must refine a looser one"
        );
        assert!(
            !parent.refines(&child),
            "a looser bound must not refine a tighter one"
        );
    }

    #[test]
    fn fork_preserves_parent_guarantees_and_adds_constraints() {
        let parent = Envelope::new(vec![Invariant::MoneyConservation {
            outflow: "out".into(),
            inflow: "in".into(),
        }]);
        let child = Envelope::new(vec![
            Invariant::MoneyConservation {
                outflow: "out".into(),
                inflow: "in".into(),
            },
            Invariant::Requires {
                flag: "manager_approved".into(),
            },
        ]);
        assert!(child.refines(&parent));
    }

    #[test]
    fn assume_guarantee_composition_decides() {
        // Upstream guarantees the account is verified; downstream assumes it.
        let upstream = Envelope::new(vec![Invariant::Requires {
            flag: "verified".into(),
        }]);
        let downstream_assumptions = Envelope::new(vec![Invariant::Requires {
            flag: "verified".into(),
        }]);
        assert!(upstream.discharges(&downstream_assumptions));
        let unmet = Envelope::new(vec![Invariant::Requires {
            flag: "kyc_passed".into(),
        }]);
        assert!(!upstream.discharges(&unmet));
        assert_eq!(upstream.unmet_by(&unmet).len(), 1);
    }

    #[test]
    fn idempotency_is_structural_and_preserved_under_refinement() {
        let parent = Envelope::new(vec![Invariant::Idempotency {
            key: "invoice_id".into(),
        }]);
        let child_missing = Envelope::new(vec![Invariant::Requires { flag: "x".into() }]);
        assert!(
            !child_missing.refines(&parent),
            "dropping a structural guarantee is not a refinement"
        );
        let child_keeps = Envelope::new(vec![
            Invariant::Idempotency {
                key: "invoice_id".into(),
            },
            Invariant::Requires { flag: "x".into() },
        ]);
        assert!(child_keeps.refines(&parent));
    }
}

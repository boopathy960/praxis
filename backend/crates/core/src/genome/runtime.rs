//! The genome runtime — how a swarm safely executes a certified behaviour.
//!
//! Every run passes the same pipeline, and the order is the whole safety
//! argument:
//!
//!   1. **Idempotency** — a retried action keyed the same way is applied at most
//!      once (the structural guarantee; never double-charge).
//!   2. **Concentration gate** — the underwriter decides whether this genome may
//!      take another unit of its operation's network traffic (§6.5).
//!   3. **Envelope gate** — the operation context is decided against the
//!      [`Envelope`](super::envelope::Envelope). An action that would leave the
//!      safe set is *blocked before it executes* — control-barrier logic, so the
//!      agent cannot violate a hard invariant by construction (§6.1). A block is
//!      the gate working, not a failure; the violation ledger stays clean, which
//!      is what keeps the Certificate honest.
//!   4. **Residual scoring** — the agent's judgment inside the envelope is scored
//!      for non-conformity and, if it falls outside the adaptive conformal set,
//!      routed to a human for sign-off rather than executed on vibes.
//!   5. **Metering** — the outcome value is metered onto the genome (the unit you
//!      are billed against, and the reward the evolution loop reads).
//!
//! The runtime is deterministic and pure given the genome and the request, which
//! is what makes the hard guarantee trustworthy and the whole thing testable
//! without a model in the loop. The service layers the optional agentic reasoner
//! on top for the *judgment*, never for the *guarantee*.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::common::{new_id, now_ms};

use super::conformal::{ConformalState, ResidualDecision, nonconformity};
use super::envelope::{Context, EnvelopeReport, OpValue, VIOLATION_MARKER};
use super::model::{Certificate, Genome};
use super::underwriting::ConcentrationVerdict;

/// A request to run one operation of a genome.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RunRequest {
    /// The typed operation context (variable bindings).
    #[serde(default)]
    pub inputs: BTreeMap<String, OpValue>,
    /// The agent's residual judgment value (e.g. the chosen refund amount),
    /// scored against the conformal set.
    #[serde(default)]
    pub judgment: Option<f64>,
    /// The metered outcome of the operation (e.g. cash collected).
    #[serde(default)]
    pub outcome_value: Option<f64>,
    /// Idempotency key — a retry with the same key is deduped.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Who/what initiated the run.
    #[serde(default)]
    pub actor: Option<String>,
    /// The market segment, for per-segment best-arm learning.
    #[serde(default)]
    pub segment: Option<String>,
}

/// One narrated step of the run pipeline, for the console's transparency view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStep {
    pub stage: String,
    pub passed: bool,
    pub detail: String,
}

/// The full result of a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub run_id: String,
    pub genome_id: String,
    pub operation: String,
    /// "executed" | "blocked" | "pending_signoff" | "deduped".
    pub status: String,
    pub admitted: bool,
    pub envelope: EnvelopeReport,
    pub concentration: ConcentrationVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub residual: Option<ResidualDecision>,
    pub requires_signoff: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signoff_id: Option<String>,
    pub outcome_value: f64,
    pub steps: Vec<RunStep>,
    pub detail: String,
    pub ran_at_ms: i64,
}

/// A residual judgment that fell outside the conformal set and needs a human.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSignoff {
    pub signoff_id: String,
    pub genome_id: String,
    pub operation: String,
    pub judgment: f64,
    pub score: f64,
    pub threshold: f64,
    pub context: BTreeMap<String, OpValue>,
    pub actor: String,
    pub status: String,
    pub created_at_ms: i64,
}

/// What a run produces: the report and, when judgment escaped the conformal set,
/// a pending human sign-off for the service to persist.
pub struct RunOutcome {
    pub report: RunReport,
    pub signoff: Option<PendingSignoff>,
    /// Reward in `[0,1]` and the violation flag for the evolution bandit.
    pub bandit_reward: Option<(f64, bool)>,
}

/// Merge the request inputs over the genome's declared variable defaults.
fn build_context(genome: &Genome, request: &RunRequest) -> Context {
    let mut context: Context = BTreeMap::new();
    for var in &genome.variables {
        if let Some(default) = &var.default {
            context.insert(var.name.clone(), default.clone());
        }
    }
    for (key, value) in &request.inputs {
        context.insert(key.clone(), value.clone());
    }
    context
}

/// Execute one operation. Pure and deterministic given the genome state, the
/// request, the concentration verdict (computed by the service over all
/// genomes), and whether this idempotency key was already applied.
#[must_use]
pub fn execute(
    genome: &mut Genome,
    request: &RunRequest,
    concentration: ConcentrationVerdict,
    already_applied: bool,
) -> RunOutcome {
    let run_id = new_id("grun");
    let now = now_ms();
    let mut steps = Vec::new();
    let context = build_context(genome, request);
    let actor = request.actor.clone().unwrap_or_else(|| "swarm".into());

    // ── 1. Idempotency ──
    if already_applied {
        steps.push(RunStep {
            stage: "idempotency".into(),
            passed: true,
            detail: "retry deduped — action already applied for this key".into(),
        });
        return RunOutcome {
            report: RunReport {
                run_id,
                genome_id: genome.genome_id.clone(),
                operation: genome.operation.clone(),
                status: "deduped".into(),
                admitted: false,
                envelope: genome.envelope.evaluate(&context),
                concentration,
                residual: None,
                requires_signoff: false,
                signoff_id: None,
                outcome_value: 0.0,
                steps,
                detail: "idempotent no-op".into(),
                ran_at_ms: now,
            },
            signoff: None,
            bandit_reward: None,
        };
    }
    steps.push(RunStep {
        stage: "idempotency".into(),
        passed: true,
        detail: "fresh action".into(),
    });

    genome.metrics.runs += 1;

    // ── 2. Concentration gate ──
    if !concentration.allowed {
        genome.metrics.blocked += 1;
        steps.push(RunStep {
            stage: "concentration".into(),
            passed: false,
            detail: concentration.detail.clone(),
        });
        let detail = concentration.detail.clone();
        return RunOutcome {
            report: RunReport {
                run_id,
                genome_id: genome.genome_id.clone(),
                operation: genome.operation.clone(),
                status: "blocked".into(),
                admitted: false,
                envelope: genome.envelope.evaluate(&context),
                concentration,
                residual: None,
                requires_signoff: false,
                signoff_id: None,
                outcome_value: 0.0,
                steps,
                detail,
                ran_at_ms: now,
            },
            signoff: None,
            bandit_reward: None,
        };
    }
    steps.push(RunStep {
        stage: "concentration".into(),
        passed: true,
        detail: concentration.detail.clone(),
    });

    // ── 3. Envelope gate (control-barrier): block before executing ──
    let envelope = genome.envelope.evaluate(&context);
    if !envelope.holds {
        genome.metrics.blocked += 1;
        steps.push(RunStep {
            stage: "envelope".into(),
            passed: false,
            detail: format!(
                "blocked — would violate {} invariant(s): {}",
                envelope.violations.len(),
                envelope.violations.join("; ")
            ),
        });
        let detail = format!(
            "envelope gate blocked the action (do-no-harm by construction): {}",
            envelope.violations.join("; ")
        );
        return RunOutcome {
            report: RunReport {
                run_id,
                genome_id: genome.genome_id.clone(),
                operation: genome.operation.clone(),
                status: "blocked".into(),
                admitted: false,
                envelope,
                concentration,
                residual: None,
                requires_signoff: false,
                signoff_id: None,
                outcome_value: 0.0,
                steps,
                detail,
                ran_at_ms: now,
            },
            signoff: None,
            // A blocked action is the gate working: a safe, low reward, no violation.
            bandit_reward: Some((0.0, false)),
        };
    }
    steps.push(RunStep {
        stage: "envelope".into(),
        passed: true,
        detail: format!(
            "inside the safe set — {} invariant(s) hold",
            envelope.checked
        ),
    });
    genome.metrics.admitted += 1;

    // ── 4. Residual scoring (adaptive conformal) ──
    let mut residual_decision = None;
    let mut requires_signoff = false;
    let mut signoff = None;
    if let Some(judgment) = request.judgment {
        let score = nonconformity(
            judgment,
            genome.residual.approved_center,
            genome.residual.approved_halfwidth,
        );
        let decision = genome.residual.conformal.observe(score);
        if !decision.in_distribution {
            requires_signoff = true;
            genome.metrics.human_signoffs += 1;
            let signoff_id = new_id("gsign");
            signoff = Some(PendingSignoff {
                signoff_id: signoff_id.clone(),
                genome_id: genome.genome_id.clone(),
                operation: genome.operation.clone(),
                judgment,
                score: decision.score,
                threshold: decision.threshold,
                context: context.clone(),
                actor: actor.clone(),
                status: "pending".into(),
                created_at_ms: now,
            });
            steps.push(RunStep {
                stage: "residual".into(),
                passed: false,
                detail: format!("judgment {judgment:.2} outside conformal set (score {:.2} > threshold {:.2}) — routed to human sign-off", decision.score, decision.threshold),
            });
        } else {
            steps.push(RunStep {
                stage: "residual".into(),
                passed: true,
                detail: format!(
                    "judgment {judgment:.2} inside conformal set (score {:.2} ≤ threshold {:.2})",
                    decision.score, decision.threshold
                ),
            });
        }
        residual_decision = Some(decision);
    } else {
        steps.push(RunStep {
            stage: "residual".into(),
            passed: true,
            detail: "no discretionary judgment in this operation".into(),
        });
    }

    // ── 5. Metering ──
    let outcome_value = if requires_signoff {
        0.0
    } else {
        request.outcome_value.unwrap_or(0.0).max(0.0)
    };
    genome.metrics.outcome_value += outcome_value;
    genome.updated_at_ms = now;

    let (status, detail) = if requires_signoff {
        (
            "pending_signoff".to_string(),
            "admitted; residual judgment awaiting human sign-off".to_string(),
        )
    } else {
        (
            "executed".to_string(),
            format!("executed inside the envelope; metered {outcome_value:.2}"),
        )
    };
    steps.push(RunStep {
        stage: "metering".into(),
        passed: true,
        detail: if requires_signoff {
            "held — not metered until sign-off".into()
        } else {
            format!("metered outcome {outcome_value:.2}")
        },
    });

    // Bandit reward: normalised outcome, zero while awaiting sign-off.
    let reward = if requires_signoff {
        0.0
    } else {
        (outcome_value / genome.residual.approved_center.abs().max(1.0)).clamp(0.0, 1.0)
    };

    RunOutcome {
        report: RunReport {
            run_id,
            genome_id: genome.genome_id.clone(),
            operation: genome.operation.clone(),
            status,
            admitted: true,
            envelope,
            concentration,
            residual: residual_decision,
            requires_signoff,
            signoff_id: signoff.as_ref().map(|s| s.signoff_id.clone()),
            outcome_value,
            steps,
            detail,
            ran_at_ms: now,
        },
        signoff,
        bandit_reward: Some((reward, false)),
    }
}

/// The path a genome's envelope-violation ledger lives at, under the data root.
/// The compiled certificate check reads this; the runtime only ever appends to
/// it through [`record_violation`], which by construction is never reached on
/// the normal path (the gate blocks first).
#[must_use]
pub fn violation_ledger_path(data_root: &str, genome_id: &str) -> String {
    format!("{data_root}/ledgers/{genome_id}.violations")
}

/// Defensive audit hook: stamp a real envelope violation into the ledger so the
/// Certificate's device-run check fails. Reached only if an admitted operation
/// is later found to have violated — which the gate is designed to make
/// impossible — so its presence is the system's own tripwire.
#[must_use]
pub fn violation_record(genome_id: &str, detail: &str) -> String {
    format!(
        "{{\"{VIOLATION_MARKER}\":true,\"genome\":\"{genome_id}\",\"detail\":\"{detail}\",\"at\":{}}}\n",
        now_ms()
    )
}

/// Recompute the live certificate fields from the residual's calibration state.
/// Takes the conformal state directly (not the whole genome) so callers can pass
/// two disjoint fields of the same genome — `&mut genome.certificate` and
/// `&genome.residual.conformal` — without a borrow conflict. The logical status
/// and claim anchoring are owned by the service (they touch the proof economy);
/// this refreshes the conformal half and validity.
pub fn refresh_certificate(certificate: &mut Certificate, conformal: &ConformalState) {
    certificate.conformal_coverage = conformal.realised_coverage();
    certificate.conformal_target = conformal.target_coverage();
    certificate.residual_in_bound = conformal.within_bound();
    certificate.maturity = conformal.maturity();
    certificate.verified_at_ms = now_ms();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::testkit::sample_genome;
    use crate::genome::underwriting::ConcentrationVerdict;

    fn allow(op: &str, id: &str) -> ConcentrationVerdict {
        ConcentrationVerdict {
            allowed: true,
            operation: op.into(),
            genome_id: id.into(),
            current_share: 0.1,
            cap: 0.35,
            detail: "ok".into(),
        }
    }

    fn ctx(pairs: &[(&str, OpValue)]) -> BTreeMap<String, OpValue> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn admits_safe_operation_and_meters_outcome() {
        let mut genome = sample_genome("billing.dunning", 0.1);
        let request = RunRequest {
            inputs: ctx(&[
                ("refund", OpValue::Money(20.0)),
                ("paid", OpValue::Money(100.0)),
                ("discount_pct", OpValue::Money(5.0)),
            ]),
            judgment: Some(50.0),
            outcome_value: Some(120.0),
            ..Default::default()
        };
        let id = genome.genome_id.clone();
        let outcome = execute(&mut genome, &request, allow("billing.dunning", &id), false);
        assert_eq!(outcome.report.status, "executed");
        assert!(outcome.report.admitted);
        assert!((genome.metrics.outcome_value - 120.0).abs() < 1e-6);
        assert_eq!(genome.metrics.runs, 1);
        assert_eq!(genome.metrics.admitted, 1);
    }

    #[test]
    fn envelope_gate_blocks_money_violation_before_executing() {
        let mut genome = sample_genome("billing.dunning", 0.1);
        // Refund exceeds paid → money conservation would be violated.
        let request = RunRequest {
            inputs: ctx(&[
                ("refund", OpValue::Money(500.0)),
                ("paid", OpValue::Money(100.0)),
                ("discount_pct", OpValue::Money(5.0)),
            ]),
            outcome_value: Some(999.0),
            ..Default::default()
        };
        let id = genome.genome_id.clone();
        let outcome = execute(&mut genome, &request, allow("billing.dunning", &id), false);
        assert_eq!(outcome.report.status, "blocked");
        assert!(!outcome.report.admitted);
        // Nothing is metered when the gate blocks — do no harm, no reward gamed.
        assert_eq!(genome.metrics.outcome_value, 0.0);
        assert_eq!(genome.metrics.blocked, 1);
        assert_eq!(
            genome.metrics.violations, 0,
            "a blocked action is not a violation"
        );
    }

    #[test]
    fn out_of_distribution_judgment_routes_to_human() {
        let mut genome = sample_genome("billing.dunning", 0.1);
        let id = genome.genome_id.clone();
        // Warm the calibration with in-band judgments.
        for _ in 0..60 {
            let request = RunRequest {
                inputs: ctx(&[
                    ("refund", OpValue::Money(10.0)),
                    ("paid", OpValue::Money(100.0)),
                    ("discount_pct", OpValue::Money(1.0)),
                ]),
                judgment: Some(50.0),
                outcome_value: Some(10.0),
                ..Default::default()
            };
            let _ = execute(&mut genome, &request, allow("billing.dunning", &id), false);
        }
        // A wildly off judgment (far outside the approved band) must be flagged.
        let request = RunRequest {
            inputs: ctx(&[
                ("refund", OpValue::Money(10.0)),
                ("paid", OpValue::Money(100.0)),
                ("discount_pct", OpValue::Money(1.0)),
            ]),
            judgment: Some(5000.0),
            outcome_value: Some(10.0),
            ..Default::default()
        };
        let outcome = execute(&mut genome, &request, allow("billing.dunning", &id), false);
        assert_eq!(outcome.report.status, "pending_signoff");
        assert!(outcome.report.requires_signoff);
        assert!(outcome.signoff.is_some());
    }
}

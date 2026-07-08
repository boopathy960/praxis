//! Self-improvement — constrained best-arm identification with a winner's-curse
//! correction (§6.3).
//!
//! Genome variants are arms in a best-arm-identification bandit: the commons
//! allocates traffic across forks and learns the per-segment winner from
//! evidence no single company could gather. Two failure modes the brief calls
//! out are designed out here:
//!
//!   * **Goodhart / unsafe optimisation.** Improvement is *constrained*: any arm
//!     that ever recorded an envelope violation is disqualified outright. The
//!     loop can never optimise its way past the hard invariants — "do no harm"
//!     is structural, not hoped-for (a constrained-MDP framing).
//!   * **The winner's curse.** When you pick the best of `N` noisy variants, the
//!     winner's measured score is inflated; the expected maximum of `N` noisy
//!     estimates drifts upward like `σ·√(2·ln N)`. Without a post-selection
//!     correction a self-improvement engine confidently promotes noise, over and
//!     over. We subtract that inflation (shrinkage) and gate promotion behind a
//!     minimum sample size and a margin over the incumbent.

use serde::{Deserialize, Serialize};

/// One genome variant as a bandit arm, accumulating outcome rewards per segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arm {
    pub genome_id: String,
    pub label: String,
    pub segment: String,
    pub pulls: u64,
    pub reward_sum: f64,
    pub reward_sq_sum: f64,
    /// Any envelope violation disqualifies the arm — the constraint that makes
    /// improvement safe by construction.
    pub envelope_violations: u64,
}

impl Arm {
    #[must_use]
    pub fn new(genome_id: &str, label: &str, segment: &str) -> Self {
        Self {
            genome_id: genome_id.into(),
            label: label.into(),
            segment: segment.into(),
            pulls: 0,
            reward_sum: 0.0,
            reward_sq_sum: 0.0,
            envelope_violations: 0,
        }
    }

    /// Record an outcome reward (e.g. normalised cash collected) for one pull.
    pub fn record(&mut self, reward: f64, violated: bool) {
        self.pulls += 1;
        self.reward_sum += reward;
        self.reward_sq_sum += reward * reward;
        if violated {
            self.envelope_violations += 1;
        }
    }

    #[must_use]
    pub fn mean(&self) -> f64 {
        if self.pulls == 0 {
            0.0
        } else {
            self.reward_sum / self.pulls as f64
        }
    }

    /// Sample variance of the arm's rewards.
    #[must_use]
    pub fn variance(&self) -> f64 {
        if self.pulls < 2 {
            return 0.0;
        }
        let n = self.pulls as f64;
        let mean = self.mean();
        ((self.reward_sq_sum / n) - mean * mean).max(0.0) * (n / (n - 1.0))
    }

    #[must_use]
    pub fn std(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Standard error of the arm's mean.
    #[must_use]
    pub fn standard_error(&self) -> f64 {
        if self.pulls == 0 {
            return f64::INFINITY;
        }
        self.std() / (self.pulls as f64).sqrt()
    }

    #[must_use]
    pub fn disqualified(&self) -> bool {
        self.envelope_violations > 0
    }
}

/// Minimum pulls before a variant can be promoted — the holdout that protects
/// against promoting a lucky small sample.
const MIN_PROMOTION_PULLS: u64 = 30;
/// UCB exploration coefficient.
const UCB_C: f64 = 1.4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmStat {
    pub genome_id: String,
    pub label: String,
    pub pulls: u64,
    pub mean: f64,
    pub std: f64,
    pub standard_error: f64,
    pub corrected_mean: f64,
    pub ucb: f64,
    pub disqualified: bool,
}

/// The recommendation from one evaluation round on an operation+segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionReport {
    pub operation: String,
    pub segment: String,
    pub arms: Vec<ArmStat>,
    /// The arm with the highest *raw* empirical mean (pre-correction).
    pub raw_best: Option<String>,
    /// The arm that wins after the winner's-curse correction.
    pub corrected_best: Option<String>,
    pub corrected_estimate: f64,
    /// The inflation `σ·√(2·ln N)` subtracted from the selected winner.
    pub winners_curse_inflation: f64,
    pub incumbent: Option<String>,
    /// Whether the corrected winner is safe to promote (enough samples, beats the
    /// incumbent by a standard-error margin, not disqualified).
    pub promotable: bool,
    /// The arm the allocator suggests pulling next (UCB) to resolve uncertainty.
    pub next_to_pull: Option<String>,
    pub recommendation: String,
}

/// Run one best-arm evaluation over the arms of an operation+segment.
#[must_use]
pub fn evaluate(
    arms: &[Arm],
    incumbent: Option<&str>,
    operation: &str,
    segment: &str,
) -> EvolutionReport {
    let qualified: Vec<&Arm> = arms.iter().filter(|arm| !arm.disqualified()).collect();
    let total_pulls: f64 = qualified
        .iter()
        .map(|arm| arm.pulls as f64)
        .sum::<f64>()
        .max(1.0);
    let n = qualified.len().max(1) as f64;

    // Pooled standard error across qualified arms, the scale of the noise we are
    // selecting the maximum over.
    let pooled_se = if qualified.is_empty() {
        0.0
    } else {
        let mean_se: f64 = qualified
            .iter()
            .map(|arm| {
                let se = arm.standard_error();
                if se.is_finite() { se } else { 1.0 }
            })
            .sum::<f64>()
            / qualified.len() as f64;
        mean_se
    };
    // Expected inflation of the max of N noisy estimates.
    let inflation = pooled_se * (2.0 * n.ln().max(0.0)).sqrt();

    let stats: Vec<ArmStat> = arms
        .iter()
        .map(|arm| {
            let mean = arm.mean();
            let ucb = if arm.disqualified() || arm.pulls == 0 {
                f64::NEG_INFINITY
            } else {
                mean + UCB_C * (total_pulls.ln() / arm.pulls as f64).sqrt()
            };
            ArmStat {
                genome_id: arm.genome_id.clone(),
                label: arm.label.clone(),
                pulls: arm.pulls,
                mean,
                std: arm.std(),
                standard_error: arm.standard_error(),
                // Only the selected maximum is curse-corrected; reported per-arm
                // as mean minus its own share is misleading, so we shrink uniformly
                // by the selection inflation for the comparison.
                corrected_mean: mean - inflation,
                ucb,
                disqualified: arm.disqualified(),
            }
        })
        .collect();

    let raw_best = qualified
        .iter()
        .max_by(|a, b| {
            a.mean()
                .partial_cmp(&b.mean())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|arm| arm.genome_id.clone());

    // The corrected winner: the arm whose mean minus the selection inflation is
    // greatest. With few, noisy arms the correction can change the winner.
    let corrected = qualified
        .iter()
        .map(|arm| (arm, arm.mean() - inflation))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let corrected_best = corrected.map(|(arm, _)| arm.genome_id.clone());
    let corrected_estimate = corrected.map(|(_, value)| value).unwrap_or(0.0);

    // UCB allocation: pull the qualified arm with the highest upper bound to
    // resolve uncertainty efficiently.
    let next_to_pull = qualified
        .iter()
        .map(|arm| {
            let ucb = arm.mean() + UCB_C * (total_pulls.ln() / (arm.pulls.max(1)) as f64).sqrt();
            (arm.genome_id.clone(), ucb)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(id, _)| id);

    // Promotion gate: corrected winner must clear the holdout sample size and
    // beat the incumbent by a standard-error margin.
    let incumbent_mean = incumbent
        .and_then(|id| arms.iter().find(|arm| arm.genome_id == id))
        .map(Arm::mean)
        .unwrap_or(f64::NEG_INFINITY);
    let winner_arm = corrected.map(|(arm, _)| arm);
    let promotable = match winner_arm {
        Some(arm) => {
            let margin = arm.standard_error().max(1e-6);
            arm.pulls >= MIN_PROMOTION_PULLS
                && !arm.disqualified()
                && corrected_estimate > incumbent_mean + margin
                && Some(arm.genome_id.as_str()) != incumbent
        }
        None => false,
    };

    let recommendation = match (&corrected_best, promotable) {
        (Some(id), true) => format!(
            "promote {id}: corrected estimate {corrected_estimate:.3} beats incumbent {incumbent_mean:.3} after subtracting winner's-curse inflation {inflation:.3}"
        ),
        (Some(id), false) => format!(
            "hold {id}: needs more evidence (≥{MIN_PROMOTION_PULLS} pulls and a margin over the incumbent after the {inflation:.3} curse correction)"
        ),
        (None, _) => "no qualified variant — all arms disqualified or unsampled".into(),
    };

    EvolutionReport {
        operation: operation.into(),
        segment: segment.into(),
        arms: stats,
        raw_best,
        corrected_best,
        corrected_estimate,
        winners_curse_inflation: inflation,
        incumbent: incumbent.map(str::to_string),
        promotable,
        next_to_pull,
        recommendation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm_with(id: &str, rewards: &[f64], violations: u64) -> Arm {
        let mut arm = Arm::new(id, id, "global");
        for (i, reward) in rewards.iter().enumerate() {
            arm.record(*reward, i == 0 && violations > 0);
        }
        arm.envelope_violations = violations;
        arm
    }

    #[test]
    fn violating_arm_is_disqualified_and_never_promoted() {
        let mut arms = vec![
            arm_with("safe", &[0.5; 40], 0),
            arm_with("greedy", &[0.95; 40], 3),
        ];
        // The greedy arm has the higher mean but violated the envelope.
        arms[1].reward_sum = 0.95 * 40.0;
        let report = evaluate(&arms, Some("safe"), "billing.dunning", "global");
        assert_ne!(report.corrected_best.as_deref(), Some("greedy"));
        assert!(
            report
                .arms
                .iter()
                .find(|a| a.genome_id == "greedy")
                .unwrap()
                .disqualified
        );
    }

    #[test]
    fn winners_curse_correction_subtracts_inflation() {
        let arms = vec![
            arm_with("a", &[0.60, 0.62, 0.58, 0.61, 0.59], 0),
            arm_with("b", &[0.40, 0.42, 0.38, 0.41, 0.39], 0),
        ];
        let report = evaluate(&arms, Some("b"), "op", "global");
        assert!(report.winners_curse_inflation > 0.0);
        // With only 5 pulls each, the winner is not promotable.
        assert!(!report.promotable, "small samples must not be promotable");
    }

    #[test]
    fn clear_winner_with_enough_samples_is_promotable() {
        let arms = vec![
            arm_with("incumbent", &[0.40; 60], 0),
            arm_with("challenger", &[0.80; 60], 0),
        ];
        let report = evaluate(&arms, Some("incumbent"), "op", "global");
        assert_eq!(report.corrected_best.as_deref(), Some("challenger"));
        assert!(report.promotable);
    }
}

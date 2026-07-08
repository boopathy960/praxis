//! The Residual's statistical certificate — adaptive conformal prediction.
//!
//! The envelope is proven logically; the *residual* (was this the wise refund?)
//! can never be proven, only **bounded**. The brief's §6.1 uses conformal
//! prediction: a distribution-free, finite-sample guarantee that the agent's
//! judgment calls fall inside the human-ratified distribution with miscoverage
//! at most `ε`. And because a self-improving system breaks exchangeability the
//! moment it mutates a genome, we use **adaptive conformal inference** (ACI,
//! Gibbs & Candès) to re-maintain coverage under drift — the repair the brief
//! says makes §6.1 and §6.3 "the same system seen twice."
//!
//! Concretely: each judgment yields a non-conformity score `s ∈ [0, ∞)` (how far
//! it sits from approved behaviour). A bounded calibration window holds recent
//! scores; the conformal threshold is their empirical `(1 − αₜ)` quantile. A new
//! judgment is *in-distribution* when its score ≤ threshold. After each
//! observation we nudge `αₜ` by the ACI update `α ← α + γ·(ε − errₜ)`, so when
//! reality drifts and miscoverage creeps up, the threshold widens to pull
//! realised coverage back to the `1 − ε` target.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Persisted, self-contained calibration state for one genome's residual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformalState {
    /// Target miscoverage (e.g. 0.1 ⇒ a 90% coverage guarantee).
    pub epsilon: f64,
    /// Adaptive miscoverage level, updated online by ACI. Starts at `epsilon`.
    pub alpha: f64,
    /// ACI learning rate.
    pub gamma: f64,
    /// Bounded window of recent non-conformity scores (the calibration set).
    pub scores: VecDeque<f64>,
    /// Window capacity.
    pub window: usize,
    /// Rolling record of whether each recent judgment was covered, to report
    /// realised empirical coverage.
    pub coverage_flags: VecDeque<bool>,
    /// Total judgments scored.
    pub observations: u64,
    /// Judgments that fell outside the conformal set (flagged for a human).
    pub flagged: u64,
}

impl ConformalState {
    /// A fresh calibration with target miscoverage `epsilon`.
    #[must_use]
    pub fn new(epsilon: f64) -> Self {
        let epsilon = epsilon.clamp(0.005, 0.5);
        Self {
            epsilon,
            alpha: epsilon,
            gamma: 0.02,
            scores: VecDeque::new(),
            window: 256,
            coverage_flags: VecDeque::new(),
            observations: 0,
            flagged: 0,
        }
    }

    /// The current conformal threshold: the empirical `(1 − αₜ)` quantile of the
    /// calibration window. With an empty window we admit everything (a cold-start
    /// genome carries a weak guarantee until it has run enough — the brief's §6.3
    /// limit, surfaced honestly rather than hidden).
    #[must_use]
    pub fn threshold(&self) -> f64 {
        if self.scores.is_empty() {
            return f64::INFINITY;
        }
        let mut sorted: Vec<f64> = self.scores.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let level = (1.0 - self.alpha).clamp(0.0, 1.0);
        // Finite-sample conformal quantile: the ⌈(n+1)(1−α)⌉-th smallest score.
        let n = sorted.len();
        let rank = (((n + 1) as f64) * level).ceil() as usize;
        let index = rank.clamp(1, n) - 1;
        sorted[index]
    }

    /// Score a judgment and decide whether it is inside the conformal set. The
    /// caller supplies a non-conformity score (see [`nonconformity`]). Mutates
    /// the calibration window and runs the ACI update.
    pub fn observe(&mut self, score: f64) -> ResidualDecision {
        let score = score.max(0.0);
        let threshold = self.threshold();
        let covered = score <= threshold;
        self.observations += 1;
        if !covered {
            self.flagged += 1;
        }

        // ── Adaptive Conformal Inference update ──
        // errₜ = 1 when the realised judgment fell outside the set. Nudging α by
        // γ·(ε − errₜ) drives realised miscoverage toward ε under drift.
        let err = if covered { 0.0 } else { 1.0 };
        self.alpha = (self.alpha + self.gamma * (self.epsilon - err)).clamp(0.005, 0.995);

        // Slide the calibration window and the realised-coverage record.
        self.scores.push_back(score);
        while self.scores.len() > self.window {
            self.scores.pop_front();
        }
        self.coverage_flags.push_back(covered);
        while self.coverage_flags.len() > self.window {
            self.coverage_flags.pop_front();
        }

        ResidualDecision {
            score,
            threshold,
            in_distribution: covered,
            alpha: self.alpha,
            realised_coverage: self.realised_coverage(),
        }
    }

    /// Realised empirical coverage over the window — the fraction of recent
    /// judgments that fell inside the conformal set.
    #[must_use]
    pub fn realised_coverage(&self) -> f64 {
        if self.coverage_flags.is_empty() {
            return 1.0;
        }
        let covered = self.coverage_flags.iter().filter(|c| **c).count();
        covered as f64 / self.coverage_flags.len() as f64
    }

    /// The coverage the certificate targets (`1 − ε`).
    #[must_use]
    pub fn target_coverage(&self) -> f64 {
        1.0 - self.epsilon
    }

    /// Whether the residual certificate currently holds: realised coverage meets
    /// the target within a finite-sample slack of `√(ln 2 / 2n)` (a Hoeffding
    /// tail), so a sparsely-sampled genome is not failed for noise alone.
    #[must_use]
    pub fn within_bound(&self) -> bool {
        let n = self.coverage_flags.len().max(1) as f64;
        let slack = (f64::ln(2.0) / (2.0 * n)).sqrt();
        self.realised_coverage() + slack >= self.target_coverage()
    }

    /// A confidence in `[0, 1]` that grows with the calibration sample size,
    /// surfacing the cold-start weakness explicitly.
    #[must_use]
    pub fn maturity(&self) -> f64 {
        let n = self.scores.len() as f64;
        (n / self.window as f64).clamp(0.0, 1.0)
    }
}

/// The outcome of scoring one judgment against the conformal set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidualDecision {
    pub score: f64,
    pub threshold: f64,
    pub in_distribution: bool,
    pub alpha: f64,
    pub realised_coverage: f64,
}

/// A standard non-conformity score: the absolute deviation of an observed
/// judgment value from the centre of the human-ratified band, normalised by the
/// band's half-width. Zero means dead-centre of approved behaviour; ≥ 1 means at
/// or beyond the edge of what humans ratified.
#[must_use]
pub fn nonconformity(observed: f64, approved_center: f64, approved_halfwidth: f64) -> f64 {
    let half = approved_halfwidth.abs().max(1e-6);
    (observed - approved_center).abs() / half
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_start_admits_then_calibrates() {
        let mut state = ConformalState::new(0.1);
        // First few are admitted (infinite threshold) and seed the window.
        for _ in 0..50 {
            state.observe(0.2);
        }
        // An in-band judgment stays covered.
        let decision = state.observe(0.2);
        assert!(decision.in_distribution);
        assert!(state.realised_coverage() > 0.9);
    }

    #[test]
    fn drift_lowers_alpha_to_widen_the_set_and_recover_coverage() {
        let mut state = ConformalState::new(0.1);
        for _ in 0..100 {
            state.observe(0.1);
        }
        let calm_alpha = state.alpha;
        // A burst of out-of-distribution judgments: realised miscoverage spikes,
        // so ACI must push alpha *down* (raising the 1−α level) to widen the
        // conformal set and pull realised coverage back toward the target.
        for _ in 0..40 {
            state.observe(5.0);
        }
        assert!(
            state.alpha < calm_alpha,
            "alpha must adapt downward under drift to widen the set"
        );
        // And after the threshold catches up to the new regime, coverage recovers.
        for _ in 0..200 {
            state.observe(5.0);
        }
        assert!(
            state.realised_coverage() > 0.8,
            "coverage must recover once the set has widened"
        );
    }

    #[test]
    fn within_bound_holds_for_in_distribution_stream() {
        let mut state = ConformalState::new(0.1);
        for index in 0..200 {
            // Mostly centred with a little noise, all inside the band.
            let jitter = if index % 10 == 0 { 0.4 } else { 0.05 };
            state.observe(jitter);
        }
        assert!(state.within_bound());
    }

    #[test]
    fn nonconformity_normalises() {
        assert!((nonconformity(50.0, 50.0, 10.0)).abs() < 1e-9);
        assert!((nonconformity(60.0, 50.0, 10.0) - 1.0).abs() < 1e-9);
        assert!(nonconformity(80.0, 50.0, 10.0) > 1.0);
    }
}

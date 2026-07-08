//! The cortex — OS control for the FeMAC + p-bit reasoning fabric.
//!
//! Four mechanisms from `os/SPU-OS.md`, all integer math:
//!
//!   * **p-bit sampling** — the fabric's probabilistic tiles, modeled with a
//!     from-scratch xorshift64* generator (deterministically seeded: kernels
//!     don't have entropy sources; the real chip's p-bits are the entropy).
//!   * **Metacognitive stopping rule** (§3) — `n* = z²·p̂(1−p̂)/θ²`: answer
//!     when sampled enough, escalate when certainty is unaffordable,
//!     otherwise keep buying samples.
//!   * **Anytime reasoning** (§3) — `reason_until(deadline)`: best-so-far
//!     answer with monotone confidence, the `reason.until` contract.
//!   * **Economic Switch Quantity** (§5) — `B* = √(2·λ·t_sw/c_d)` batches
//!     mode switches between thinking (FeMAC) and exploring (p-bit).
//!   * **Consolidation utility** (§6) — `U_s = Σ novelty·success`, committed
//!     to the ferroelectric plane only through the Wear-Pacing Invariant.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::SpuDevice;

// ── the p-bit fabric ─────────────────────────────────────────────────────

/// xorshift64* — from-scratch PRNG standing in for physical p-bit noise.
pub struct PBitFabric {
    state: u64,
    pub samples_drawn: u64,
}

impl PBitFabric {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed | 1,
            samples_drawn: 0,
        }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// One p-bit read at probability `p_milli`/1000 of being 1.
    pub fn sample(&mut self, p_milli: u64) -> bool {
        self.samples_drawn += 1;
        (self.next() % 1000) < p_milli
    }
}

// ── the metacognitive stopping rule (§3) ─────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Enough samples: answer with this confidence (per-mille).
    Answer { confidence_milli: u64 },
    /// Certainty is unaffordable: hand off to a bigger model / a human.
    Escalate { samples_needed: u64 },
    /// Keep sampling — each sample still buys uncertainty reduction.
    Continue { samples_needed: u64 },
}

/// `z_milli` = z-score ×1000 (1960 ≈ 95%), `theta_milli` = target CI
/// half-width ×1000. Returns the three-way verdict for `k` successes in `n`
/// samples under a sampling budget.
pub fn stopping_rule(
    n: u64,
    k: u64,
    z_milli: u64,
    theta_milli: u64,
    budget_samples: u64,
) -> Verdict {
    if n == 0 {
        return Verdict::Continue { samples_needed: 1 };
    }
    // Laplace-smoothed estimate p̃ = (k+1)/(n+2): never exactly 0 or 1, so a
    // lucky streak on tiny n cannot fake zero variance and claim certainty.
    let p_milli = (1000 * (k + 1) / (n + 2)).clamp(1, 999);
    let var_micro = p_milli * (1000 - p_milli);
    // n* = z²·p(1−p)/θ²  (z,θ in milli ⇒ divide by 10⁶ once).
    let n_star = ((z_milli as u128 * z_milli as u128 * var_micro as u128)
        / (theta_milli as u128 * theta_milli as u128 * 1_000_000)) as u64;
    let n_star = n_star.max(1);
    if n >= n_star {
        // c = θ√n/(z·σ̂), per-mille, capped at 1000.
        let conf = (1_000_000u128 * theta_milli as u128 * (n as u128).isqrt())
            / (z_milli as u128 * (var_micro as u128).isqrt());
        Verdict::Answer {
            confidence_milli: (conf as u64).min(1000),
        }
    } else if n_star > budget_samples {
        Verdict::Escalate {
            samples_needed: n_star,
        }
    } else {
        Verdict::Continue {
            samples_needed: n_star,
        }
    }
}

// ── anytime reasoning (§3): reason.until(deadline) ──────────────────────

#[derive(Debug, Clone)]
pub struct ReasonReport {
    pub best_candidate: usize,
    pub best_p_milli: u64,
    pub confidence_milli: u64,
    pub samples: u64,
    pub elapsed_ticks: u64,
    pub verdict: Verdict,
}

/// Explore `candidates` (true per-mille qualities, hidden from the caller in
/// real use — here they parameterize the fabric model) until the deadline or
/// the target confidence. Best-so-far is always available and confidence is
/// monotone in samples: the Reflex-Island contract "your best plan in 40ms".
pub fn reason_until(
    fabric: &mut PBitFabric,
    candidates: &[u64],
    deadline_ticks: u64,
    target_conf_milli: u64,
    budget_samples: u64,
    clock: &dyn Fn() -> u64,
) -> ReasonReport {
    let start = clock();
    let mut wins: Vec<u64> = alloc::vec![0; candidates.len()];
    let mut tries: Vec<u64> = alloc::vec![0; candidates.len()];
    let mut verdict = Verdict::Continue { samples_needed: 1 };
    let mut samples = 0u64;

    loop {
        let now = clock();
        if now.saturating_sub(start) >= deadline_ticks || samples >= budget_samples {
            break;
        }
        // UCB-lite: sample the candidate with the highest optimistic estimate
        // (empirical rate + exploration bonus for the under-sampled).
        let pick = (0..candidates.len())
            .max_by_key(|&i| {
                if tries[i] == 0 {
                    return u64::MAX; // never sampled: maximum optimism
                }
                1000 * wins[i] / tries[i] + 1000 / tries[i]
            })
            .unwrap_or(0);
        if fabric.sample(candidates[pick]) {
            wins[pick] += 1;
        }
        tries[pick] += 1;
        samples += 1;

        let best = best_index(&wins, &tries);
        verdict = stopping_rule(tries[best], wins[best], 1960, 100, budget_samples);
        if let Verdict::Answer { confidence_milli } = verdict {
            if confidence_milli >= target_conf_milli {
                break;
            }
        }
        if matches!(verdict, Verdict::Escalate { .. }) {
            break;
        }
    }

    let best = best_index(&wins, &tries);
    let best_p = if tries[best] == 0 {
        0
    } else {
        1000 * wins[best] / tries[best]
    };
    let confidence = match verdict {
        Verdict::Answer { confidence_milli } => confidence_milli,
        _ => 0,
    };
    ReasonReport {
        best_candidate: best,
        best_p_milli: best_p,
        confidence_milli: confidence,
        samples,
        elapsed_ticks: clock().saturating_sub(start),
        verdict,
    }
}

fn best_index(wins: &[u64], tries: &[u64]) -> usize {
    (0..wins.len())
        .max_by_key(|&i| {
            if tries[i] == 0 {
                0
            } else {
                1000 * wins[i] / tries[i]
            }
        })
        .unwrap_or(0)
}

// ── the mode-switching fabric (§5) ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FabricMode {
    /// Deterministic in-memory MAC — thinking.
    Deterministic,
    /// Near-coercive-field p-bit — exploring.
    Stochastic,
}

pub struct ModeFabric {
    pub mode: FabricMode,
    q_det: u64,
    q_stoch: u64,
    /// EWMA arrival rate of stochastic demand (milli per service call).
    lambda_milli: u64,
    /// Mode-switch cost t_sw (ticks) and delay cost c_d (milli/req/tick).
    t_sw: u64,
    c_d_milli: u64,
    pub switches: u64,
    pub served_det: u64,
    pub served_stoch: u64,
}

impl ModeFabric {
    pub fn new() -> Self {
        Self {
            mode: FabricMode::Deterministic,
            q_det: 0,
            q_stoch: 0,
            lambda_milli: 0,
            t_sw: 50,
            c_d_milli: 100,
            switches: 0,
            served_det: 0,
            served_stoch: 0,
        }
    }

    pub fn demand(&mut self, mode: FabricMode, count: u64) {
        match mode {
            FabricMode::Deterministic => self.q_det += count,
            FabricMode::Stochastic => {
                self.q_stoch += count;
                self.lambda_milli = (3 * self.lambda_milli + 1000 * count) / 4;
            }
        }
    }

    /// The Economic Switch Quantity: B* = √(2·λ·t_sw/c_d).
    pub fn batch_size(&self) -> u64 {
        let b = ((2 * self.lambda_milli * self.t_sw) / self.c_d_milli.max(1)).isqrt();
        b.max(1)
    }

    /// Serve one scheduling quantum: work in the current mode up to B*, and
    /// switch only when the other queue holds the hysteresis-weighted
    /// majority — no thrash, provably amortized switching.
    pub fn service(&mut self) -> u64 {
        let batch = self.batch_size();
        let served = match self.mode {
            FabricMode::Deterministic => {
                let n = self.q_det.min(batch);
                self.q_det -= n;
                self.served_det += n;
                n
            }
            FabricMode::Stochastic => {
                let n = self.q_stoch.min(batch);
                self.q_stoch -= n;
                self.served_stoch += n;
                n
            }
        };
        let total = self.q_det + self.q_stoch;
        if total > 0 {
            // Hysteresis: switch when the other side exceeds 60% of demand.
            let other = match self.mode {
                FabricMode::Deterministic => self.q_stoch,
                FabricMode::Stochastic => self.q_det,
            };
            if 1000 * other / total > 600 {
                self.mode = match self.mode {
                    FabricMode::Deterministic => FabricMode::Stochastic,
                    FabricMode::Stochastic => FabricMode::Deterministic,
                };
                self.switches += 1;
            }
        }
        served
    }

    pub fn queued(&self) -> (u64, u64) {
        (self.q_det, self.q_stoch)
    }
}

impl Default for ModeFabric {
    fn default() -> Self {
        Self::new()
    }
}

// ── the Consolidation Engine (§6) ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConsolidationReport {
    pub skill: String,
    pub utility_milli: u64,
    pub committed: bool,
    pub reason: &'static str,
    pub new_level: u32,
}

/// Idle-time distillation of experience into permanent skill, gated by
/// utility (worth learning) and wear pacing (affordable to learn).
pub struct Consolidator {
    traces: Vec<(String, bool, u64)>,
    skill_levels: BTreeMap<String, u32>,
    utility_threshold_milli: u64,
}

impl Consolidator {
    pub fn new() -> Self {
        Self {
            traces: Vec::new(),
            skill_levels: BTreeMap::new(),
            utility_threshold_milli: 1500,
        }
    }

    /// Record an episodic trace: did it succeed, and how novel was it?
    pub fn record(&mut self, skill: impl Into<String>, success: bool, novelty_milli: u64) {
        self.traces
            .push((skill.into(), success, novelty_milli.min(1000)));
    }

    /// U_s = Σ novelty·success. Commit skills with U ≥ θ_c, each commit
    /// spending one paced ferroelectric write. The agent wakes up permanently
    /// better at exactly what was novel, successful, and affordable.
    pub fn consolidate(&mut self, spu: &mut SpuDevice, now: u64) -> Vec<ConsolidationReport> {
        let mut utility: BTreeMap<String, u64> = BTreeMap::new();
        for (skill, success, novelty) in &self.traces {
            if *success {
                *utility.entry(skill.clone()).or_default() += novelty;
            } else {
                utility.entry(skill.clone()).or_default();
            }
        }
        let mut reports = Vec::new();
        let mut committed_skills: Vec<String> = Vec::new();
        for (skill, u) in utility {
            if u < self.utility_threshold_milli {
                reports.push(ConsolidationReport {
                    new_level: self.skill_levels.get(&skill).copied().unwrap_or(0),
                    skill,
                    utility_milli: u,
                    committed: false,
                    reason: "below utility threshold — not worth permanence",
                });
            } else if !spu.admit_fe_write(now) {
                reports.push(ConsolidationReport {
                    new_level: self.skill_levels.get(&skill).copied().unwrap_or(0),
                    skill,
                    utility_milli: u,
                    committed: false,
                    reason: "wear pacing denied — endurance budget exhausted for now",
                });
            } else {
                let level = self.skill_levels.entry(skill.clone()).or_default();
                *level += 1;
                reports.push(ConsolidationReport {
                    new_level: *level,
                    skill: skill.clone(),
                    utility_milli: u,
                    committed: true,
                    reason: "distilled to ferroelectric plane",
                });
                committed_skills.push(skill);
            }
        }
        // Committed traces are consumed; the rest keep accumulating utility.
        self.traces
            .retain(|(skill, _, _)| !committed_skills.contains(skill));
        reports
    }

    pub fn skill_level(&self, skill: &str) -> u32 {
        self.skill_levels.get(skill).copied().unwrap_or(0)
    }

    pub fn pending_traces(&self) -> usize {
        self.traces.len()
    }
}

impl Default for Consolidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    #[test]
    fn stopping_rule_reproduces_the_classic_sample_size() {
        // p̂=0.5, z=1.96, θ=0.05 ⇒ n* ≈ 384 (the textbook number).
        match stopping_rule(100, 50, 1960, 50, 10_000) {
            Verdict::Continue { samples_needed } => {
                assert!((350..=420).contains(&samples_needed), "{samples_needed}");
            }
            other => panic!("expected Continue, got {other:?}"),
        }
        // With 400 samples at p̂=0.5 the rule answers.
        assert!(matches!(
            stopping_rule(400, 200, 1960, 50, 10_000),
            Verdict::Answer { .. }
        ));
        // Unaffordable certainty escalates instead of burning the budget.
        assert!(matches!(
            stopping_rule(10, 5, 1960, 10, 500),
            Verdict::Escalate { .. }
        ));
    }

    #[test]
    fn reason_until_respects_deadline_and_finds_the_better_thought() {
        let mut fabric = PBitFabric::new(42);
        let tick = Cell::new(0u64);
        let clock = || {
            tick.set(tick.get() + 1);
            tick.get()
        };
        // Candidate 2 is clearly best (85% vs 40%/55%).
        let report = reason_until(&mut fabric, &[400, 550, 850], 5_000, 900, 4_000, &clock);
        assert_eq!(report.best_candidate, 2, "{report:?}");
        assert!(report.best_p_milli > 700);
        // The stopping rule answers as soon as certainty is bought — dozens
        // of samples, not thousands. That efficiency IS the mechanism.
        assert!(report.samples >= 10, "{report:?}");
        assert!(report.confidence_milli >= 900, "{report:?}");
        assert!(report.elapsed_ticks <= 5_000);

        // A brutal deadline still yields a best-so-far — the anytime contract.
        let mut fabric = PBitFabric::new(7);
        let quick = reason_until(&mut fabric, &[400, 850], 10, 999, 4_000, &clock);
        assert!(quick.samples <= 10);
        assert!(quick.samples > 0, "always a best-so-far");
    }

    #[test]
    fn esq_batching_amortizes_mode_switches() {
        // Interleaved demand, served with ESQ batching.
        let mut fabric = ModeFabric::new();
        for _ in 0..50 {
            fabric.demand(FabricMode::Deterministic, 10);
            fabric.demand(FabricMode::Stochastic, 10);
        }
        let mut guard = 0;
        while fabric.queued() != (0, 0) && guard < 10_000 {
            fabric.service();
            guard += 1;
        }
        let total = fabric.served_det + fabric.served_stoch;
        assert_eq!(total, 1000);
        // 1000 interleaved requests naively = ~1000 switches; ESQ + hysteresis
        // must amortize far below that.
        assert!(
            fabric.switches < 100,
            "switches={} should be amortized by B*={}",
            fabric.switches,
            fabric.batch_size()
        );
    }

    #[test]
    fn consolidation_requires_utility_and_endurance() {
        let mut spu = SpuDevice::with_endurance(100, 1000);
        let mut consolidator = Consolidator::new();

        // Novel + successful accumulates utility; failures add nothing.
        for _ in 0..3 {
            consolidator.record("grasp-cup", true, 600);
        }
        consolidator.record("open-door", false, 900);

        // Late enough that wear pacing has budget.
        let reports = consolidator.consolidate(&mut spu, 500);
        let grasp = reports.iter().find(|r| r.skill == "grasp-cup").unwrap();
        assert!(grasp.committed, "{grasp:?}");
        assert_eq!(consolidator.skill_level("grasp-cup"), 1);
        let door = reports.iter().find(|r| r.skill == "open-door").unwrap();
        assert!(!door.committed, "failures never become permanent");

        // Early-life wear pacing blocks even worthy skills.
        let mut fresh_spu = SpuDevice::with_endurance(100, 1_000_000);
        let mut c2 = Consolidator::new();
        // Drain the early budget (pace ≈ 0 at t=1, slack=2).
        assert!(fresh_spu.admit_fe_write(1));
        assert!(fresh_spu.admit_fe_write(1));
        c2.record("skill", true, 800);
        c2.record("skill", true, 800);
        let blocked = c2.consolidate(&mut fresh_spu, 1);
        assert!(!blocked[0].committed);
        assert!(blocked[0].reason.contains("wear pacing"));
    }
}

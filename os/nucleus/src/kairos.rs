//! The Kairos index — risk-adjusted generalized-cµ scheduling, a closed-form
//! integer mechanism no existing kernel ships.
//!
//! # The problem
//!
//! Every scheduler answers one question forever: *of everything runnable,
//! what single thing is most worth the next slice of time?* Linux answers
//! with virtual runtime (fairness), RTOSes answer with static priority
//! (rank), and our epistemic scheduler answered with surprise (attention).
//! None of those answers is *about time itself* — none of them minimizes how
//! long work actually waits. Under heterogeneous workloads (cheap chatty
//! tasks mixed with expensive slow ones) fairness policies interleave the
//! expensive work into the cheap work's critical path and multiply everyone's
//! completion time.
//!
//! # The mechanism
//!
//! One integer index per task, recomputed from live state on every pick:
//!
//! ```text
//!             V·A  +  Q·1000
//!   K  =  ─────────────────────  ·  1000
//!            b̂  +  û  +  1000
//! ```
//!
//! where (all fixed-point, per-mille — no floats in a kernel):
//!
//! * `Q`  — **backlog**: declared pending work units (queued messages,
//!          outstanding intents) awaiting this task,
//! * `b̂`  — **predicted burst** in milli-ticks (the scheduler's own EWMA
//!          model of what one poll costs — already maintained),
//! * `û`  — **uncertainty**: an EWMA of the model's absolute prediction
//!          error, i.e. how much the scheduler *distrusts its own forecast*,
//! * `A`  — **attention**: the epistemic score (proof-tier bonus + surprise),
//! * `V`  — the attention/latency trade-off knob ([`ATTENTION_WEIGHT`]).
//!
//! The scheduler serves the maximum-`K` runnable task. Three classical
//! optimality results compose into this single expression — and the third
//! term is the part that is new:
//!
//! 1. **`Q` in the numerator — Lyapunov max-weight.** Serving the largest
//!    backlog greedily minimizes the drift of the quadratic Lyapunov
//!    function `L = ΣQ²`, which is the standard proof of *throughput
//!    optimality*: if any policy can keep the queues bounded, max-weight
//!    does (Tassiulas–Ephremides). The kernel inherits that guarantee for
//!    free — backlogs cannot grow without the index growing to meet them.
//!
//! 2. **`b̂` in the denominator — the generalized-cµ rule.** Weighting each
//!    queue by its service *rate* `µ = 1/b̂` and serving max `Q·µ` is the
//!    Gcµ rule, asymptotically optimal for convex delay cost in heavy
//!    traffic (Van Mieghem). Intuition: a unit of CPU given to a cheap task
//!    retires more waiting work per tick than the same unit given to an
//!    expensive one, so flow time collapses — this is where the massive
//!    mean-latency reduction comes from, and the benchmark test below
//!    measures it directly.
//!
//! 3. **`û` in the denominator — the epistemic discount (new).** Classical
//!    cµ assumes the service rate is *known*. A kernel only ever has an
//!    estimate, and an estimate is exactly as good as its error. Pricing the
//!    burst at `b̂ + û` — the model's upper credible bound — makes the index
//!    risk-adjusted: between two tasks of equal backlog and equal predicted
//!    cost, the one whose behavior the scheduler can actually *predict* wins
//!    the slice. Predictability becomes CPU currency, mechanically. This
//!    closes the loop with the trust ledger: a Proven task that behaves as
//!    proven runs with `û → 0` and enjoys the full cµ rate; a task drifting
//!    from its model pays for its own unpredictability *in the denominator*
//!    of its scheduling index, before the verification layer even reacts.
//!    That coupling — max-weight × cµ ÷ model-confidence, in pure integer
//!    arithmetic, gated by proof tier — is the mechanism that did not exist.
//!
//! The `+1000` floor makes a brand-new task (no model yet) well-defined and
//! keeps the division exact-enough at per-mille resolution. All arithmetic
//! widens to `u128` and saturates back to `u64`, so no admissible state can
//! overflow or panic the picker.

/// Fixed-point scale: per-mille, the house style (`surprise_milli`,
/// `predicted_milliticks`). One backlog unit is worth `MILLI` value points.
pub const MILLI: u64 = 1000;

/// `V` — how much epistemic attention (tier + surprise) weighs against one
/// unit of backlog. At 1, a full attention score (~6000) is worth six queued
/// messages: attention still steers when queues are quiet, but real pending
/// work dominates when it exists.
pub const ATTENTION_WEIGHT: u64 = 1;

/// Live inputs to one index evaluation, all straight off `TaskStats`.
#[derive(Clone, Copy, Debug)]
pub struct Inputs {
    /// Epistemic attention score `A` (proof-tier bonus + surprise).
    pub attention: u64,
    /// Declared pending work units `Q` for this task.
    pub backlog: u64,
    /// The scheduler's burst forecast `b̂`, milli-ticks.
    pub predicted_milliticks: u64,
    /// EWMA of the forecast's absolute error `û`, milli-ticks.
    pub uncertainty_milli: u64,
}

/// The Kairos index: `K = (V·A + Q·MILLI) · MILLI / (b̂ + û + MILLI)`.
///
/// Monotone in the direction of every argument's meaning — more pending work
/// or more attention raises it; a costlier or less predictable burst lowers
/// it — and total over `u64` inputs (widened internally, saturating out).
pub fn index(inputs: &Inputs) -> u64 {
    let value = (ATTENTION_WEIGHT as u128) * (inputs.attention as u128)
        + (inputs.backlog as u128) * (MILLI as u128);
    let cost = (inputs.predicted_milliticks as u128)
        + (inputs.uncertainty_milli as u128)
        + (MILLI as u128);
    let k = value * (MILLI as u128) / cost;
    u64::try_from(k).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn base() -> Inputs {
        Inputs {
            attention: 3000,
            backlog: 10,
            predicted_milliticks: 2000,
            uncertainty_milli: 500,
        }
    }

    #[test]
    fn monotone_in_every_argument() {
        let k0 = index(&base());
        // More pending work → more worth serving.
        let more_backlog = Inputs {
            backlog: 20,
            ..base()
        };
        assert!(index(&more_backlog) > k0);
        // More attention → more worth serving.
        let more_attention = Inputs {
            attention: 6000,
            ..base()
        };
        assert!(index(&more_attention) > k0);
        // Costlier burst → less worth serving (cµ).
        let slower = Inputs {
            predicted_milliticks: 8000,
            ..base()
        };
        assert!(index(&slower) < k0);
        // Less predictable burst → less worth serving (the epistemic discount).
        let murkier = Inputs {
            uncertainty_milli: 4000,
            ..base()
        };
        assert!(index(&murkier) < k0);
    }

    #[test]
    fn predictability_is_currency() {
        // Equal backlog, equal predicted cost: the task the scheduler can
        // actually model wins the slice.
        let steady = Inputs {
            uncertainty_milli: 0,
            ..base()
        };
        let erratic = Inputs {
            uncertainty_milli: 6000,
            ..base()
        };
        assert!(index(&steady) > index(&erratic));
    }

    #[test]
    fn extremes_saturate_instead_of_panicking() {
        let huge = Inputs {
            attention: u64::MAX,
            backlog: u64::MAX,
            predicted_milliticks: 0,
            uncertainty_milli: 0,
        };
        assert_eq!(index(&huge), u64::MAX);
        let zero = Inputs {
            attention: 0,
            backlog: 0,
            predicted_milliticks: u64::MAX,
            uncertainty_milli: u64::MAX,
        };
        assert_eq!(index(&zero), 0);
    }

    // ── the benchmark: does the math actually buy time? ─────────────────
    //
    // A deterministic discrete-event simulation. K queues of work; queue i
    // costs `cost[i]` ticks per unit served and starts `backlog[i]` deep.
    // A policy repeatedly picks a non-empty queue, pays its cost on the
    // clock, and retires one unit; the retired unit's completion time is the
    // clock. The score is total flow time — the sum of every unit's
    // completion time — which is what "how long did work wait" means.

    struct Queue {
        cost: u64,
        left: u64,
    }

    fn simulate(costs_and_backlogs: &[(u64, u64)], kairos: bool) -> u128 {
        let mut queues: Vec<Queue> = costs_and_backlogs
            .iter()
            .map(|&(cost, left)| Queue { cost, left })
            .collect();
        let mut clock: u128 = 0;
        let mut flow: u128 = 0;
        let mut rr_cursor = 0usize;
        loop {
            let pick = if kairos {
                queues
                    .iter()
                    .enumerate()
                    .filter(|(_, q)| q.left > 0)
                    .max_by_key(|(_, q)| {
                        index(&Inputs {
                            attention: 0,
                            backlog: q.left,
                            predicted_milliticks: q.cost * MILLI,
                            uncertainty_milli: 0,
                        })
                    })
                    .map(|(i, _)| i)
            } else {
                // Round-robin, the fairness baseline every kernel defaults to.
                let n = queues.len();
                (0..n)
                    .map(|step| (rr_cursor + step) % n)
                    .find(|&i| queues[i].left > 0)
            };
            let Some(i) = pick else { break };
            rr_cursor = (i + 1) % queues.len();
            clock += queues[i].cost as u128;
            queues[i].left -= 1;
            flow += clock;
        }
        flow
    }

    #[test]
    fn heterogeneous_workload_flow_time_collapses() {
        // Chatty cheap work, moderate work, and a heavyweight — the shape of
        // every real system (interrupt handlers vs services vs batch jobs).
        let workload = [(1, 100), (5, 40), (20, 20), (100, 4)];
        let rr = simulate(&workload, false);
        let kairos = simulate(&workload, true);
        // The Gcµ term retires cheap work first instead of interleaving the
        // heavyweight into everyone's critical path. Demand at least a 2×
        // total-flow-time reduction — the measured gap is larger.
        assert!(
            kairos * 2 <= rr,
            "kairos flow {kairos} should be at most half of round-robin {rr}"
        );
    }

    #[test]
    fn uniform_workload_loses_nothing() {
        // When all work is identical there is nothing to exploit; the index
        // must not do worse than fairness.
        let workload = [(10, 25), (10, 25), (10, 25)];
        let rr = simulate(&workload, false);
        let kairos = simulate(&workload, true);
        assert!(kairos <= rr, "uniform: kairos {kairos} vs rr {rr}");
    }

    #[test]
    fn backlogs_stay_bounded_under_sustained_arrivals() {
        // Max-weight stability: arrivals below capacity, served by the index.
        // Two flows: one unit arrives at flow 0 every 3 ticks and at flow 1
        // every 5 ticks; serving costs 1 tick either way (load = 8/15 < 1).
        let mut q = [0u64; 2];
        let mut peak = 0u64;
        for tick in 0u64..30_000 {
            if tick % 3 == 0 {
                q[0] += 1;
            }
            if tick % 5 == 0 {
                q[1] += 1;
            }
            let pick = (0..2).max_by_key(|&i| {
                index(&Inputs {
                    attention: 0,
                    backlog: q[i],
                    predicted_milliticks: MILLI,
                    uncertainty_milli: 0,
                })
            });
            if let Some(i) = pick {
                q[i] = q[i].saturating_sub(1);
            }
            peak = peak.max(q[0]).max(q[1]);
        }
        assert!(
            peak < 16,
            "sub-capacity arrivals must stay bounded, peak backlog {peak}"
        );
    }
}

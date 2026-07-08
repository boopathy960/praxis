# The Kairos Index — risk-adjusted generalized-cµ scheduling

*kairos (καιρός): the opportune moment — as opposed to chronos, mere elapsed
time. A scheduler that only counts chronos is fair; one that computes kairos
is fast.*

## The problem it deletes

Every mainstream scheduler optimizes a proxy, not time itself:

| policy | maximizes | failure mode |
|---|---|---|
| CFS / virtual runtime | fairness | interleaves a heavyweight into every cheap task's critical path |
| static priority (RTOS) | rank | rank is written once and is wrong forever |
| our epistemic score (v1) | attention | knows *where something is happening*, not *what retires waiting work fastest* |

Under heterogeneous load — chatty interrupt-shaped work mixed with expensive
batch-shaped work, i.e. every real system — fairness multiplies everyone's
completion time. The measured cost on the benchmark workload below is **4.86×**.

## The mechanism

One integer per runnable task, recomputed from live state at every pick;
serve the maximum:

```
        V·A + Q·1000
K  =  ───────────────── · 1000        (all fixed-point per-mille, no floats)
        b̂ + û + 1000
```

| symbol | meaning | source |
|---|---|---|
| `Q` | declared pending work units (messages queued, intents outstanding) | `TaskStats::backlog`, fed by `Scheduler::push_backlog` / `praxsh kairos push` |
| `b̂` | predicted burst per poll, milli-ticks | the scheduler's own EWMA model (already existed) |
| `û` | EWMA of the model's **absolute prediction error** | new — the scheduler's distrust of its own forecast |
| `A` | epistemic attention: proof-tier bonus + surprise | the v1 score, retained |
| `V` | attention/latency trade-off knob | `kairos::ATTENTION_WEIGHT` |

The proof-tier admission structure is untouched: unproven work still only
owns every `UNPROVEN_STRIDE`-th round (the distrust tax), and Kairos ranks
*within* what proof admits.

## Why each term is where it is

**1. `Q` in the numerator — Lyapunov max-weight (throughput optimality).**
Serving the largest backlog greedily minimizes the drift of `L = ΣQ²`. By the
Tassiulas–Ephremides argument, if *any* policy can keep the queues bounded,
this one does. Test: `backlogs_stay_bounded_under_sustained_arrivals` runs
30,000 ticks of sub-capacity arrivals and the peak backlog stays under 16.

**2. `b̂` in the denominator — the generalized-cµ rule (flow-time collapse).**
Weight each queue by its service *rate* `µ = 1/b̂` and serve max `Q·µ`: the
Gcµ rule, asymptotically optimal for convex delay cost in heavy traffic
(Van Mieghem 1995). A tick of CPU given to cheap work retires more waiting
work than the same tick given to expensive work, so total flow time
collapses. This is where the massive time reduction lives.

**3. `û` in the denominator — the epistemic discount. This is the new part.**
Classical cµ assumes the service rate is *known*. A kernel only ever has an
estimate, and an estimate is worth exactly its error. Pricing the burst at
`b̂ + û` — the model's upper credible bound — makes the index risk-adjusted:
between two tasks of equal backlog and equal predicted cost, **the one the
scheduler can actually predict wins the slice**. Predictability becomes CPU
currency, mechanically, before the verification layer even reacts. A Proven
task behaving as proven runs at `û → 0` and enjoys the full cµ rate; a task
drifting from its model pays for its own unpredictability in the denominator
of its own scheduling index. That coupling — max-weight × cµ ÷
model-confidence, in pure integer arithmetic, gated by proof tier — is the
composition that does not exist in any kernel we know of.

## Measured result

Deterministic discrete-event benchmark (`kairos::tests::
heterogeneous_workload_flow_time_collapses`), workload shaped like a real
system — interrupt-ish, service-ish, batch-ish:

```
queues (cost-per-unit, units): (1,100) (5,40) (20,20) (100,4)

round-robin  total flow 141,190   mean completion 860.9
kairos       total flow  29,063   mean completion 177.2

→ 79.4% mean-latency reduction, 4.86× speedup
```

The test asserts ≥2× so it stays robust; the measured gap is 4.86×. Two
guard tests keep the claim honest: `uniform_workload_loses_nothing` (when
there is nothing to exploit, Kairos never does worse than fairness) and the
bounded-backlog stability test above.

## Where it lives

- `nucleus/src/kairos.rs` — the index, the derivation, the benchmark.
- `nucleus/src/sched.rs` — `TaskStats` gains `uncertainty_milli` (EWMA |error|,
  updated per poll) and `backlog` (declared arrivals, one retired per poll);
  `Scheduler::pick` maximizes the Kairos index instead of raw attention.
- `nucleus/src/shell.rs` — `praxsh kairos` prints the live decomposition
  (Q, b̂, û, A, K per task); `kairos push <id> <units>` declares an arrival.

All arithmetic widens to `u128` and saturates to `u64`: no admissible state
can overflow the picker. No floats anywhere — the bare-metal target is
soft-float and a kernel has no business in f64.

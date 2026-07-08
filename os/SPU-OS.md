# SPU-OS — the mathematics of an operating system for the State Processing Unit

The SPU ("SPU chip.pdf") makes the **persistent agent context** the first-class
hardware object: near-memory decode, a sub-µs hardware context switch, a
three-tier memory substrate (SRAM working / DRAM capacity / non-volatile
persistent), the FeMAC + p-bit fabric (one array that both *thinks*
deterministically and *explores* stochastically), Janus dual-plane tiles
(endurance-limited ferroelectric + unlimited-write gain-cell), a Metacognition
Engine (hardware uncertainty counters), a Consolidation Engine (idle-time
experience → weight deltas), and State-Graft (zero-re-prefill context sharing).

A chip like that is wasted under a Linux-shaped OS, whose scheduler optimizes
CPU time for processes. The SPU's scarce resources are different: **tier
residency, ferroelectric write endurance, sampling budget, and mode-switch
bandwidth.** SPU-OS manages those with six new mechanisms, each defined by a
formula the kernel actually computes (integer/fixed-point — see
`nucleus/src/spu/`). The chip itself is simulator-first (the report's own
build philosophy); the OS is built the same way, against a faithful device
model, so hardware and OS meet in the middle.

---

## 1. Thermodynamic context placement — the Value-Density rule

Which contexts deserve the fast tiers? For context `c` with size `S_c` (KB),
define its **heat** as an exponentially-weighted access rate:

```
H_c(t) = (3·H_c(t−1) + 4096·a_c(t)) / 4          (integer EWMA, milli-access units)
```

Each tier `T` has a hold cost `P_T` (energy/KB/tick) and a wake latency `W_T`
(ticks). The expected cost rate of keeping `c` in tier `T` is

```
J_c(T) = P_T · S_c  +  H_c · W_T
         └ rent ┘      └ expected wake tax ┘
```

Minimizing total `Σ J` under tier capacities is a knapsack; the greedy
optimum orders contexts by **value density** — the wake tax saved per KB of
fast-tier space:

```
ρ_c = H_c · (W_slow − W_fast) / S_c
```

**Mechanism:** sort by `ρ_c`, fill SRAM, then DRAM; everything else hibernates
in NV at `P_NV = 0` — the chip's *zero-watt waiting*, now an OS placement
theorem instead of a manual hint. Placement re-runs on demand (`ctx place`) —
a hot context rises tier-by-tier as its heat pays for the space.

## 2. The Wear-Pacing Invariant — Janus flush admission

Ferroelectric cells endure `N_w ≈ 10⁴–10⁷` writes; the KV cache writes every
token. The Janus tile fixes this in hardware (hot state in gain-cells, one
flush per hibernate); the OS must guarantee the flush budget over the device
lifetime `L`. Admission control is one invariant. A flush to plane `p` at
time `t` is admitted iff:

```
writes_p(t) + 1  ≤  N_w · t / L  +  slack
```

with the plane chosen by wear-leveling: `p* = argmin_p writes_p`.

**Theorem (lifetime guarantee):** if every flush passes the invariant, then at
`t = L` no plane exceeds `N_w + slack` writes — the device cannot be worn out
early no matter what userspace does. Endurance stops being a firmware hope and
becomes a kernel-enforced resource, like quota.

## 3. The Metacognitive Stopping Rule — buying certainty by the sample

The p-bit fabric measures its own uncertainty: over `n` samples with success
fraction `p̂`, the variance is `σ̂² = p̂(1−p̂)`. To decide with half-width `θ`
at confidence `z` (CLT interval), the fabric needs

```
n* = z² · p̂(1−p̂) / θ²          samples
```

**Mechanism:** three-way verdict per query —

- `n ≥ n*`                 → **answer**, confidence `c = min(1, θ·√n / (z·σ̂))`
- `n* · e_sample > budget` → **escalate** (bigger model / human) — certainty
  is not affordable here, and the OS knows it *before* burning the budget
- otherwise                → **keep sampling**, attention priced per sample

This turns the Metacognition Engine's counters into a scheduling law: compute
is spent exactly until the marginal sample stops buying uncertainty
reduction. It is also the anytime-reasoning contract: `reason.until(deadline)`
returns best-so-far with a confidence that is monotone in `n`.

## 4. The Graft Threshold — when sharing thought beats re-speaking it

Re-prefilling `n` tokens into another agent's KV cache costs quadratic
attention work; grafting maps the KV subtree copy-on-write:

```
C_prefill(n) = a·n² + b·n           C_graft(n) = g + h·n
```

Grafting wins whenever `n > n₀`, the positive root of `a·n² + (b−h)·n − g`:

```
n₀ = ( (h−b) + √((h−b)² + 4·a·g) ) / 2a
```

With on-die grafting `g` is a few mapping ops and `h ≪ b`, so `n₀` is tiny —
**the OS should never re-prefill on-die**; the formula matters at the
appliance boundary, where `g` includes the compressed-KV network transfer and
the kernel must choose per message. SPU-OS computes `n₀` from measured
constants and routes every inter-agent share through it.

## 5. The Economic Switch Quantity — one fabric, two minds, no thrash

The mode-switching fabric flips tile groups between deterministic MAC
("think") and stochastic p-bit ("explore") at cost `t_sw` per switch. With
stochastic demand arriving at rate `λ` and a delay-cost `c_d` per queued
request per tick, switching after a batch of `B` requests costs
`t_sw/B + c_d·(B−1)/2` per request — minimized (EOQ form) at:

```
B* = √( 2 · λ · t_sw / c_d )
```

plus hysteresis: switch modes only when the queued demand fraction crosses
`½ ± h`. The kernel batches exploration work into `B*`-sized bursts —
maximum fabric utilization, provably no mode thrash.

## 6. Consolidation Utility — deciding what deserves to become permanent

Idle-time consolidation distills episodic traces into ferroelectric weight
deltas — spending endurance (§2) to buy skill. For skill `s` with traces
`j = 1..m`:

```
U_s = Σ_j  novelty_j · success_j        (per-mille novelty × {0,1})
```

Commit iff `U_s ≥ θ_c` **and** the Wear-Pacing Invariant admits the write.
Experience becomes permanent exactly when it is (a) novel, (b) successful,
and (c) affordable in endurance — the OS's answer to "when may the machine
rewrite itself," consistent with Praxis's rule that self-modification is gated,
never ambient.

## 7. The unified dispatch equation

The nucleus scheduler already prices trust (proof-gated cooperation) and
attention (surprise). With the SPU model attached, the dispatch score for
task `i` owning context `c(i)` is:

```
D_i = τ_i · (1000 + min(surprise_i, 3000)) − W_tier(c(i)) · κ
```

`τ_i` — trust coefficient from the ledger (proven 3.0 / partial 1.5 /
unproven 0.3, unproven throttled to every 4th round); `surprise_i` — the
epistemic scheduler's prediction error; `W_tier(c(i))·κ` — the placement-aware
context-wake penalty, so the scheduler naturally runs what is already resident
and lets §1 decide what becomes resident. Proof buys admission, surprise buys
attention, residency buys the tie.

---

### Mapping to the chip

| SPU hardware | SPU-OS mechanism | Formula |
|---|---|---|
| 3-tier substrate, µs wake, zero-watt waiting | thermodynamic placement | §1 |
| Janus tile, flush-on-hibernate | wear-pacing admission | §2 |
| Metacognition Engine counters | stopping/escalation rule | §3 |
| `reason.until(deadline)` | anytime contract, monotone confidence | §3 |
| State-Graft / `context.graft` | graft threshold | §4 |
| `mode.switch` fabric | economic switch quantity | §5 |
| Consolidation Engine | consolidation utility | §6 |
| HW context switch + reasoning loop | unified dispatch | §7 |

### Implementation

All six mechanisms run in the kernel today against the device model in
`nucleus/src/spu/` (integer math only — the kernel is soft-float):
`spu` · `ctx` · `graft` · `reason` · `modes` · `consolidate` in praxsh, tests in
each module, same nucleus on bare metal and hosted. When the real chip (or
its cycle-accurate simulator) exists, the device model's constants
(`W_T, P_T, N_w, t_sw, e_sample, a, b, g, h`) are replaced by measured
values; the formulas — and the kernel that enforces them — do not change.

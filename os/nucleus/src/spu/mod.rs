//! The SPU device model + the OS mechanisms that manage it (see `os/SPU-OS.md`).
//!
//! The State Processing Unit's first-class object is the persistent agent
//! context living in a three-tier substrate (SRAM working / DRAM capacity /
//! non-volatile persistent, with zero-watt waiting and µs wake). The chip is
//! simulator-first; the OS is built the same way — against this device model —
//! so kernel policy and silicon meet in the middle. Constants here are
//! order-of-magnitude stand-ins the real chip (or its cycle-accurate
//! simulator) replaces; the formulas do not change.
//!
//! Implemented mechanisms (all integer math — the kernel is soft-float):
//!   * **Thermodynamic context placement** (SPU-OS §1): value density
//!     `ρ_c = H_c·(W_slow − W_fast)/S_c`, greedy knapsack over tier capacity.
//!   * **Wear-Pacing Invariant** (§2): a ferroelectric flush is admitted iff
//!     `writes_p + 1 ≤ N_w·t/L + slack`, plane chosen by wear-leveling.
//!   * **Graft Threshold** (§4): re-prefill costs `a·n² + b·n`, grafting
//!     `g + h·n`; the kernel grafts beyond the crossover `n₀`.

pub mod cortex;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The three-tier memory substrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemTier {
    /// On-logic-die working tier: fastest, smallest, highest hold cost.
    Sram,
    /// 3D-stacked capacity tier.
    Dram,
    /// Non-volatile tier: zero hold power, 10k+ dormant contexts, µs wake.
    Nv,
}

impl MemTier {
    pub fn name(self) -> &'static str {
        match self {
            MemTier::Sram => "sram",
            MemTier::Dram => "dram",
            MemTier::Nv => "nv",
        }
    }
}

/// Per-tier cost model: capacity, wake latency, hold cost (SPU-OS §1's
/// `W_T` and `P_T`). NV holds at zero — the chip's zero-watt waiting.
struct TierSpec {
    cap_kb: u64,
    wake_ticks: u64,
    hold_milli_per_kb: u64,
}

const TIERS: [TierSpec; 3] = [
    TierSpec {
        cap_kb: 128,
        wake_ticks: 1,
        hold_milli_per_kb: 4,
    },
    TierSpec {
        cap_kb: 4096,
        wake_ticks: 10,
        hold_milli_per_kb: 1,
    },
    TierSpec {
        cap_kb: u64::MAX,
        wake_ticks: 100,
        hold_milli_per_kb: 0,
    },
];

fn tier_spec(tier: MemTier) -> &'static TierSpec {
    match tier {
        MemTier::Sram => &TIERS[0],
        MemTier::Dram => &TIERS[1],
        MemTier::Nv => &TIERS[2],
    }
}

/// A persistent agent/reasoning context — the SPU's first-class object.
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub ctx_id: u64,
    pub name: String,
    pub size_kb: u64,
    /// EWMA access heat in milli-accesses (SPU-OS §1's `H_c`).
    pub heat_milli: u64,
    pub tier: MemTier,
    pub dormant: bool,
    /// KV-cache length in tokens (drives graft economics).
    pub kv_tokens: u64,
    /// KV subtrees grafted in from other contexts (copy-on-write shares).
    pub grafts_in: u32,
    pub last_touch_tick: u64,
}

impl AgentContext {
    /// Value density ρ_c = H_c·(W_nv − W_sram)/S_c — the wake tax this
    /// context saves per KB of fast-tier space (SPU-OS §1).
    pub fn value_density(&self) -> u64 {
        let delta_wake = tier_spec(MemTier::Nv).wake_ticks - tier_spec(MemTier::Sram).wake_ticks;
        self.heat_milli * delta_wake / self.size_kb.max(1)
    }
}

#[derive(Debug, Clone)]
pub struct PlacementReport {
    pub contexts: usize,
    pub migrations: usize,
    pub per_tier: [usize; 3],
    /// Σ J_c(T) = Σ (P_T·S_c + H_c·W_T), in milli cost-rate units.
    pub total_cost_rate_milli: u64,
}

#[derive(Debug, Clone)]
pub struct GraftReport {
    pub tokens: u64,
    /// The crossover n₀ beyond which grafting beats re-prefill.
    pub threshold_tokens: u64,
    pub cost_prefill_milli: u64,
    pub cost_graft_milli: u64,
    pub grafted: bool,
    pub saved_milli: i64,
}

#[derive(Debug, Clone)]
pub struct WearStatus {
    pub endurance: u64,
    pub lifetime_ticks: u64,
    /// The pacing line N_w·t/L at `now`.
    pub pace_now: u64,
    pub plane_writes: Vec<u64>,
    pub flushes_admitted: u64,
    pub flushes_denied: u64,
}

/// Graft/prefill cost constants (milli-ticks): prefill `a·n² + b·n`,
/// graft `g + h·n` (SPU-OS §4).
const PREFILL_A_MILLI: u64 = 10; // 0.01 tick per token²
const PREFILL_B_MILLI: u64 = 500; // 0.5 tick per token
const GRAFT_G_MILLI: u64 = 200_000; // 200-tick fixed mapping/permission cost
const GRAFT_H_MILLI: u64 = 20; // 0.02 tick per token (COW bookkeeping)

/// The modeled SPU: contexts, tiers, and the Janus ferroelectric planes.
pub struct SpuDevice {
    contexts: BTreeMap<u64, AgentContext>,
    next_ctx: u64,
    /// Writes consumed per ferroelectric plane (wear-leveled).
    plane_writes: Vec<u64>,
    /// Ferroelectric endurance N_w (writes/plane) and target lifetime L.
    endurance: u64,
    lifetime_ticks: u64,
    wear_slack: u64,
    flushes_admitted: u64,
    flushes_denied: u64,
    grafts: u64,
    graft_saved_milli: u64,
}

impl SpuDevice {
    /// Default model: 4 planes, 100k-write endurance, long lifetime.
    pub fn model() -> Self {
        Self::with_endurance(100_000, 1_000_000_000)
    }

    pub fn with_endurance(endurance: u64, lifetime_ticks: u64) -> Self {
        Self {
            contexts: BTreeMap::new(),
            next_ctx: 1,
            plane_writes: alloc::vec![0; 4],
            endurance,
            lifetime_ticks: lifetime_ticks.max(1),
            wear_slack: 2,
            flushes_admitted: 0,
            flushes_denied: 0,
            grafts: 0,
            graft_saved_milli: 0,
        }
    }

    // ── contexts ─────────────────────────────────────────────────────────

    /// Contexts are born dormant in NV — existence is free until heat earns
    /// a faster tier (§1).
    pub fn create_context(&mut self, name: impl Into<String>, size_kb: u64, kv_tokens: u64) -> u64 {
        let ctx_id = self.next_ctx;
        self.next_ctx += 1;
        self.contexts.insert(
            ctx_id,
            AgentContext {
                ctx_id,
                name: name.into(),
                size_kb: size_kb.max(1),
                heat_milli: 0,
                tier: MemTier::Nv,
                dormant: true,
                kv_tokens,
                grafts_in: 0,
                last_touch_tick: 0,
            },
        );
        ctx_id
    }

    /// An access: wake if dormant (paying the tier's wake tax) and bump the
    /// heat EWMA `H = (3H + 4096)/4`.
    pub fn touch(&mut self, ctx_id: u64, now: u64) -> Option<u64> {
        let ctx = self.contexts.get_mut(&ctx_id)?;
        let wake_cost = if ctx.dormant {
            ctx.dormant = false;
            tier_spec(ctx.tier).wake_ticks
        } else {
            0
        };
        ctx.heat_milli = (3 * ctx.heat_milli + 4096) / 4;
        ctx.last_touch_tick = now;
        Some(wake_cost)
    }

    /// Cool every context one EWMA step (`H ← 3H/4`) — called by placement
    /// so heat is a rate, not a lifetime counter.
    fn cool_all(&mut self) {
        for ctx in self.contexts.values_mut() {
            ctx.heat_milli = 3 * ctx.heat_milli / 4;
        }
    }

    /// Thermodynamic placement (SPU-OS §1): order by value density, fill
    /// SRAM then DRAM, hibernate the rest in NV at zero hold power.
    pub fn place(&mut self, now: u64) -> PlacementReport {
        self.cool_all();
        let mut order: Vec<u64> = self.contexts.keys().copied().collect();
        order.sort_by_key(|id| {
            let ctx = &self.contexts[id];
            (u64::MAX - ctx.value_density(), *id)
        });

        let mut migrations = 0;
        let mut per_tier = [0usize; 3];
        let mut used = [0u64; 3];
        let mut total_cost_rate_milli = 0u64;

        for id in order {
            let ctx = self.contexts.get_mut(&id).expect("known id");
            let want = if used[0] + ctx.size_kb <= tier_spec(MemTier::Sram).cap_kb
                && ctx.heat_milli > 0
            {
                MemTier::Sram
            } else if used[1] + ctx.size_kb <= tier_spec(MemTier::Dram).cap_kb && ctx.heat_milli > 0
            {
                MemTier::Dram
            } else {
                MemTier::Nv
            };
            let slot = match want {
                MemTier::Sram => 0,
                MemTier::Dram => 1,
                MemTier::Nv => 2,
            };
            used[slot] += ctx.size_kb;
            per_tier[slot] += 1;
            if ctx.tier != want {
                ctx.tier = want;
                migrations += 1;
                if want == MemTier::Nv {
                    ctx.dormant = true;
                }
            }
            // J_c(T) = P_T·S_c + H_c·W_T
            let spec = tier_spec(want);
            total_cost_rate_milli +=
                spec.hold_milli_per_kb * ctx.size_kb + ctx.heat_milli * spec.wake_ticks / 1000;
            ctx.last_touch_tick = ctx.last_touch_tick.max(now);
        }

        PlacementReport {
            contexts: self.contexts.len(),
            migrations,
            per_tier,
            total_cost_rate_milli,
        }
    }

    // ── Janus wear pacing (SPU-OS §2) ───────────────────────────────────

    /// Admission control for a ferroelectric write. Pacing is **global** —
    /// `total_writes + 1 ≤ planes·N_w·t/L + slack` — and wear-leveling only
    /// chooses *where* an admitted write lands (the least-worn plane), never
    /// how many are admitted. Enforcing this at every flush guarantees the
    /// fabric cannot be worn out before the lifetime `L`, no matter what
    /// userspace does: endurance as a kernel-enforced resource.
    pub fn admit_fe_write(&mut self, now: u64) -> bool {
        let pace = ((self.endurance as u128 * now as u128) / self.lifetime_ticks as u128) as u64;
        let budget = pace * self.plane_writes.len() as u64 + self.wear_slack;
        let total: u64 = self.plane_writes.iter().sum();
        if total < budget {
            let plane = self
                .plane_writes
                .iter()
                .copied()
                .enumerate()
                .min_by_key(|(_, w)| *w)
                .map(|(i, _)| i)
                .expect("planes exist");
            self.plane_writes[plane] += 1;
            self.flushes_admitted += 1;
            true
        } else {
            self.flushes_denied += 1;
            false
        }
    }

    /// Suspend a context: flush-on-hibernate wants one ferroelectric write.
    /// If pacing denies it, the context still hibernates — in DRAM (volatile,
    /// no wear) instead of NV. Returns `(hibernated_to, fe_write_admitted)`.
    pub fn suspend(&mut self, ctx_id: u64, now: u64) -> Option<(MemTier, bool)> {
        let admitted = self.admit_fe_write(now);
        let ctx = self.contexts.get_mut(&ctx_id)?;
        ctx.dormant = true;
        ctx.tier = if admitted { MemTier::Nv } else { MemTier::Dram };
        Some((ctx.tier, admitted))
    }

    pub fn wear_status(&self, now: u64) -> WearStatus {
        WearStatus {
            endurance: self.endurance,
            lifetime_ticks: self.lifetime_ticks,
            pace_now: ((self.endurance as u128 * now as u128) / self.lifetime_ticks as u128) as u64,
            plane_writes: self.plane_writes.clone(),
            flushes_admitted: self.flushes_admitted,
            flushes_denied: self.flushes_denied,
        }
    }

    // ── State-Graft (SPU-OS §4) ─────────────────────────────────────────

    /// The crossover n₀ = ((h−b) + √((h−b)² + 4ag)) / 2a beyond which a
    /// graft beats re-prefill.
    pub fn graft_threshold_tokens() -> u64 {
        let a = PREFILL_A_MILLI as i128;
        let hb = GRAFT_H_MILLI as i128 - PREFILL_B_MILLI as i128;
        let disc = (hb * hb + 4 * a * GRAFT_G_MILLI as i128) as u128;
        let root = disc.isqrt() as i128;
        ((hb + root) / (2 * a)).max(0) as u64
    }

    /// Share `tokens` of `src`'s KV subtree into `dst` copy-on-write —
    /// zero re-prefill. Grafts when past the threshold; otherwise reports
    /// that re-speaking is genuinely cheaper (small n).
    pub fn graft(&mut self, src: u64, dst: u64, tokens: u64) -> Option<GraftReport> {
        if !self.contexts.contains_key(&src) || src == dst {
            return None;
        }
        let n = tokens as u128;
        let cost_prefill = (PREFILL_A_MILLI as u128 * n * n + PREFILL_B_MILLI as u128 * n) as u64;
        let cost_graft = GRAFT_G_MILLI + GRAFT_H_MILLI * tokens;
        let threshold = Self::graft_threshold_tokens();
        let grafted = tokens > threshold;
        let saved = cost_prefill as i64 - cost_graft as i64;
        if grafted {
            let dst_ctx = self.contexts.get_mut(&dst)?;
            dst_ctx.kv_tokens += tokens;
            dst_ctx.grafts_in += 1;
            self.grafts += 1;
            self.graft_saved_milli += saved.max(0) as u64;
        }
        Some(GraftReport {
            tokens,
            threshold_tokens: threshold,
            cost_prefill_milli: cost_prefill,
            cost_graft_milli: cost_graft,
            grafted,
            saved_milli: saved,
        })
    }

    // ── introspection ───────────────────────────────────────────────────

    pub fn contexts(&self) -> impl Iterator<Item = &AgentContext> {
        self.contexts.values()
    }

    pub fn get(&self, ctx_id: u64) -> Option<&AgentContext> {
        self.contexts.get(&ctx_id)
    }

    pub fn graft_stats(&self) -> (u64, u64) {
        (self.grafts, self.graft_saved_milli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_contexts_earn_sram_cold_ones_hibernate() {
        let mut spu = SpuDevice::model();
        let hot = spu.create_context("hot-agent", 32, 4000);
        let cold = spu.create_context("cold-agent", 32, 4000);
        for t in 0..20 {
            spu.touch(hot, t);
        }
        spu.touch(cold, 0);
        let report = spu.place(21);
        assert_eq!(spu.get(hot).unwrap().tier, MemTier::Sram);
        assert!(spu.get(hot).unwrap().value_density() > spu.get(cold).unwrap().value_density());
        assert!(report.per_tier[0] >= 1);

        // Never touched again: cooling drains the cold context back to NV.
        let mut report = spu.place(100);
        for t in 0..20 {
            report = spu.place(200 + t);
        }
        assert_eq!(spu.get(cold).unwrap().tier, MemTier::Nv);
        assert!(spu.get(cold).unwrap().dormant, "{report:?}");
    }

    #[test]
    fn wear_pacing_invariant_bounds_early_writes() {
        // 100-write endurance over 1000 ticks: pace = t/10.
        let mut spu = SpuDevice::with_endurance(100, 1000);
        let ctx = spu.create_context("agent", 8, 100);

        // At t=10 the global budget is planes·pace + slack = 4·1 + 2 = 6;
        // hammering suspend must hit denials rather than eating endurance.
        let mut denied = 0;
        for _ in 0..10 {
            let (_, admitted) = spu.suspend(ctx, 10).unwrap();
            if !admitted {
                denied += 1;
            }
        }
        assert!(
            denied >= 4,
            "early flush spam must be paced, denied={denied}"
        );

        // Late in life the budget has accrued.
        let (tier, admitted) = spu.suspend(ctx, 900).unwrap();
        assert!(admitted);
        assert_eq!(tier, MemTier::Nv);

        // The invariant held throughout: no plane exceeds pace + slack.
        let status = spu.wear_status(1000);
        for writes in &status.plane_writes {
            assert!(*writes <= status.endurance + 2);
        }
    }

    #[test]
    fn graft_beats_prefill_beyond_the_crossover() {
        let mut spu = SpuDevice::model();
        let a = spu.create_context("alice", 16, 5000);
        let b = spu.create_context("bob", 16, 1000);
        let n0 = SpuDevice::graft_threshold_tokens();
        assert!(n0 > 10 && n0 < 10_000, "sane crossover, got {n0}");

        // Large share: graft wins, quadratically.
        let big = spu.graft(a, b, 4000).unwrap();
        assert!(big.grafted);
        assert!(big.saved_milli > 0);
        assert!(big.cost_prefill_milli > 10 * big.cost_graft_milli);
        assert_eq!(spu.get(b).unwrap().grafts_in, 1);

        // Tiny share below n₀: re-speaking is cheaper; kernel refuses to graft.
        let small = spu.graft(a, b, n0 / 2).unwrap();
        assert!(!small.grafted);
        assert!(small.saved_milli < 0);
    }
}

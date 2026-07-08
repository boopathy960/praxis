//! The in-kernel trust ledger — execution privilege as live state.
//!
//! This is the nucleus-resident distillation of the Astra proof economy: each
//! task carries claims (memory safety, capability bounds, resource bounds,
//! speculation safety), the ledger computes the tier those claims buy, and a
//! claim that *drifts* (stops holding) demotes the task on the very next
//! scheduling decision — recorded at tick precision in the transition log. In
//! the full system the mint/attack lifecycle lives in the userspace economy;
//! the nucleus consumes its verdicts the way Linux consumes page tables.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

pub type TaskId = u64;
pub type ClaimId = u64;

/// Execution privilege. Lower rank = less caged = cheaper to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Full claim set: runs uncaged and unthrottled, trusted to cooperate.
    Proven,
    /// Partial claims: runs with per-crossing guard checks.
    Partial,
    /// No standing proof: pays the distrust tax (scheduler throttle).
    Unproven,
}

impl Tier {
    pub fn rank(self) -> u8 {
        match self {
            Tier::Proven => 0,
            Tier::Partial => 1,
            Tier::Unproven => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tier::Proven => "proven",
            Tier::Partial => "partial",
            Tier::Unproven => "unproven",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimClass {
    MemorySafety,
    CapabilityBound,
    ResourceBound,
    SpeculationSafety,
}

impl ClaimClass {
    pub fn name(self) -> &'static str {
        match self {
            ClaimClass::MemorySafety => "memory_safety",
            ClaimClass::CapabilityBound => "capability_bound",
            ClaimClass::ResourceBound => "resource_bound",
            ClaimClass::SpeculationSafety => "speculation_safety",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Claim {
    pub claim_id: ClaimId,
    pub task: TaskId,
    pub class: ClaimClass,
    /// Whether the claim currently describes reality. Drift flips this.
    pub holding: bool,
}

/// The moment a task's execution privilege moved, stamped with the kernel tick.
#[derive(Debug, Clone)]
pub struct Transition {
    pub task: TaskId,
    pub from: Tier,
    pub to: Tier,
    pub reason: String,
    pub at_tick: u64,
}

#[derive(Default)]
pub struct TrustLedger {
    claims: BTreeMap<ClaimId, Claim>,
    by_task: BTreeMap<TaskId, Vec<ClaimId>>,
    transitions: Vec<Transition>,
    next_claim: ClaimId,
}

impl TrustLedger {
    pub const fn new() -> Self {
        Self {
            claims: BTreeMap::new(),
            by_task: BTreeMap::new(),
            transitions: Vec::new(),
            next_claim: 1,
        }
    }

    /// Record a claim for a task (in the full system this mirrors a minted
    /// economy claim; here it is the kernel-resident verdict).
    pub fn grant(&mut self, task: TaskId, class: ClaimClass) -> ClaimId {
        let claim_id = self.next_claim;
        self.next_claim += 1;
        self.claims.insert(
            claim_id,
            Claim {
                claim_id,
                task,
                class,
                holding: true,
            },
        );
        self.by_task.entry(task).or_default().push(claim_id);
        claim_id
    }

    /// A claim stopped describing reality (the sentinel's verdict, in kernel).
    pub fn drift(&mut self, claim_id: ClaimId) -> Option<&Claim> {
        let claim = self.claims.get_mut(&claim_id)?;
        claim.holding = false;
        Some(claim)
    }

    /// A drifted claim was re-proven.
    pub fn restore(&mut self, claim_id: ClaimId) -> Option<&Claim> {
        let claim = self.claims.get_mut(&claim_id)?;
        claim.holding = true;
        Some(claim)
    }

    /// What the standing claims buy right now.
    pub fn tier_of(&self, task: TaskId) -> Tier {
        let has = |class: ClaimClass| {
            self.by_task
                .get(&task)
                .map(|ids| {
                    ids.iter().any(|id| {
                        self.claims
                            .get(id)
                            .map(|c| c.class == class && c.holding)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        };
        if has(ClaimClass::MemorySafety)
            && has(ClaimClass::CapabilityBound)
            && has(ClaimClass::ResourceBound)
        {
            Tier::Proven
        } else if has(ClaimClass::MemorySafety) || has(ClaimClass::CapabilityBound) {
            Tier::Partial
        } else {
            Tier::Unproven
        }
    }

    pub fn record_transition(&mut self, transition: Transition) {
        self.transitions.push(transition);
    }

    pub fn transitions(&self) -> &[Transition] {
        &self.transitions
    }

    pub fn claims_of(&self, task: TaskId) -> Vec<&Claim> {
        self.by_task
            .get(&task)
            .map(|ids| ids.iter().filter_map(|id| self.claims.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn all_claims(&self) -> impl Iterator<Item = &Claim> {
        self.claims.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_set_is_proven_and_drift_demotes() {
        let mut ledger = TrustLedger::new();
        let task = 7;
        let mem = ledger.grant(task, ClaimClass::MemorySafety);
        ledger.grant(task, ClaimClass::CapabilityBound);
        ledger.grant(task, ClaimClass::ResourceBound);
        assert_eq!(ledger.tier_of(task), Tier::Proven);

        ledger.drift(mem);
        assert_eq!(ledger.tier_of(task), Tier::Partial);

        ledger.restore(mem);
        assert_eq!(ledger.tier_of(task), Tier::Proven);
    }

    #[test]
    fn no_claims_means_unproven() {
        let ledger = TrustLedger::new();
        assert_eq!(ledger.tier_of(42), Tier::Unproven);
    }
}

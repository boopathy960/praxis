//! Preemptive multitasking — where the time-slice is a function of proof.
//!
//! Cooperative scheduling (the nucleus [executor](crate::sched)) trusts code to
//! yield. Preemption exists because you cannot trust arbitrary code to do that:
//! a timer interrupt forcibly takes the CPU back so one runaway program cannot
//! freeze the machine. Every OS levies that as a flat tax — the same quantum
//! for everyone, trusted or not. Praxis prices it by proof instead:
//!
//!   * **Proven** code has carried a termination / resource-bound proof through
//!     the economy — it has *proven* it will not hog the CPU — so it earns a
//!     long time-slice and is rarely interrupted.
//!   * **Partially proven** code earns a medium slice.
//!   * **Unproven** code gets a short slice and is preempted aggressively,
//!     because nothing guarantees it ever yields. A runaway `while true {}` in
//!     an unproven process therefore *cannot* hang the system — it is forced
//!     off the CPU every few ticks — while a proven compute kernel runs almost
//!     uninterrupted.
//!
//! So the quantum, like every other privilege in Praxis, is earned: **trust
//! buys uninterrupted CPU; distrust is preempted.** The result is both faster
//! (proven work suffers fewer context switches) and safer (unproven work is
//! contained) than a flat quantum.
//!
//! This module is the scheduling *policy*, driven one timer tick at a time and
//! fully host-testable. The register-level switch it directs is
//! [`switch_context`](../../boot) on bare metal.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

use crate::process::Pid;
use crate::proof::Tier;

/// The time-slice, in timer ticks, that each proof tier earns. This single
/// function is the whole mechanism: privilege over the CPU, priced by proof.
#[must_use]
pub fn quantum_for(tier: Tier) -> u64 {
    match tier {
        Tier::Proven => 10,  // proven to behave → run long, rarely interrupted
        Tier::Partial => 5,  // partly trusted → medium slice
        Tier::Unproven => 2, // untrusted → short slice, preempted aggressively
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
}

#[derive(Debug, Clone)]
pub struct Thread {
    pub pid: Pid,
    pub tier: Tier,
    pub state: ThreadState,
    /// Ticks left in the current slice.
    pub remaining: u64,
    /// Total CPU ticks this thread has consumed.
    pub cpu_ticks: u64,
    /// Times it was forcibly preempted (quantum expired mid-work).
    pub preemptions: u64,
    /// Times it voluntarily gave up the CPU.
    pub yields: u64,
    pub turns: u64,
}

/// What one timer tick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// Nothing runnable.
    Idle,
    /// The current thread keeps running; this many ticks left in its slice.
    Running { pid: Pid, remaining: u64 },
    /// The quantum expired: the CPU was taken from `from` and given to `to`.
    /// On bare metal this is where `switch_context(from, to)` fires.
    Preempted { from: Pid, to: Pid },
    /// A thread's slice ended but it was the only one runnable, so it simply
    /// starts a fresh slice (no switch).
    Renewed { pid: Pid },
}

/// The preemptive scheduler. Round-robin *turns*, but the CPU *share* per turn
/// is set by proof, so proven work accrues more CPU while unproven work is
/// contained.
pub struct PreemptiveScheduler {
    threads: BTreeMap<Pid, Thread>,
    /// Ready threads awaiting a turn (round-robin order), excluding `current`.
    queue: VecDeque<Pid>,
    current: Option<Pid>,
    ticks: u64,
    switches: u64,
    preemptions: u64,
}

impl PreemptiveScheduler {
    pub fn new() -> Self {
        Self {
            threads: BTreeMap::new(),
            queue: VecDeque::new(),
            current: None,
            ticks: 0,
            switches: 0,
            preemptions: 0,
        }
    }

    /// Admit a thread at its proof tier. It joins the back of the run queue.
    pub fn admit(&mut self, pid: Pid, tier: Tier) {
        if self.threads.contains_key(&pid) {
            return;
        }
        self.threads.insert(
            pid,
            Thread {
                pid,
                tier,
                state: ThreadState::Ready,
                remaining: quantum_for(tier),
                cpu_ticks: 0,
                preemptions: 0,
                yields: 0,
                turns: 0,
            },
        );
        self.queue.push_back(pid);
    }

    /// Remove a thread (it exited). If it was running, the CPU is freed for the
    /// next tick to dispatch.
    pub fn remove(&mut self, pid: Pid) -> bool {
        let existed = self.threads.remove(&pid).is_some();
        self.queue.retain(|p| *p != pid);
        if self.current == Some(pid) {
            self.current = None;
        }
        existed
    }

    /// Advance one timer tick: charge the running thread, and preempt it if its
    /// proof-sized slice has run out. This is the function the timer IRQ calls.
    pub fn on_tick(&mut self) -> Tick {
        self.ticks += 1;
        let Some(cur) = self.current else {
            return self.dispatch_next(None);
        };
        // Charge the current thread one tick of CPU.
        let (expired, tier) = {
            let thread = self.threads.get_mut(&cur).expect("current exists");
            thread.cpu_ticks += 1;
            thread.remaining = thread.remaining.saturating_sub(1);
            (thread.remaining == 0, thread.tier)
        };
        if !expired {
            return Tick::Running {
                pid: cur,
                remaining: self.threads[&cur].remaining,
            };
        }
        // Quantum expired. If someone else is waiting, preempt; else renew.
        if self.queue.is_empty() {
            let thread = self.threads.get_mut(&cur).expect("current exists");
            thread.remaining = quantum_for(tier);
            thread.turns += 1;
            return Tick::Renewed { pid: cur };
        }
        // Preempt: current goes to the back of the queue, next comes forward.
        {
            let thread = self.threads.get_mut(&cur).expect("current exists");
            thread.preemptions += 1;
            thread.remaining = quantum_for(tier); // refilled for its next turn
            thread.state = ThreadState::Ready;
        }
        self.queue.push_back(cur);
        self.preemptions += 1;
        match self.dispatch_next(Some(cur)) {
            Tick::Running { pid, .. } => Tick::Preempted { from: cur, to: pid },
            other => other,
        }
    }

    /// The running thread voluntarily gives up the rest of its slice.
    pub fn yield_current(&mut self) -> Tick {
        let Some(cur) = self.current else {
            return Tick::Idle;
        };
        {
            let thread = self.threads.get_mut(&cur).expect("current exists");
            thread.yields += 1;
            thread.remaining = quantum_for(thread.tier);
            thread.state = ThreadState::Ready;
        }
        if self.queue.is_empty() {
            return Tick::Renewed { pid: cur };
        }
        self.queue.push_back(cur);
        match self.dispatch_next(Some(cur)) {
            Tick::Running { pid, .. } => Tick::Preempted { from: cur, to: pid },
            other => other,
        }
    }

    /// Block the running (or a named) thread — it leaves the run queue until
    /// unblocked (e.g. waiting on IO).
    pub fn block(&mut self, pid: Pid) {
        if let Some(thread) = self.threads.get_mut(&pid) {
            thread.state = ThreadState::Blocked;
        }
        self.queue.retain(|p| *p != pid);
        if self.current == Some(pid) {
            self.current = None;
        }
    }

    /// Return a blocked thread to the run queue.
    pub fn unblock(&mut self, pid: Pid) {
        if let Some(thread) = self.threads.get_mut(&pid) {
            if thread.state == ThreadState::Blocked {
                thread.state = ThreadState::Ready;
                thread.remaining = quantum_for(thread.tier);
                self.queue.push_back(pid);
            }
        }
    }

    /// Pop the next ready thread and make it current. `_yielded` is the thread
    /// that just gave up the CPU (already requeued), kept for clarity.
    fn dispatch_next(&mut self, _yielded: Option<Pid>) -> Tick {
        if let Some(prev) = self.current.take() {
            if let Some(thread) = self.threads.get_mut(&prev) {
                if thread.state == ThreadState::Running {
                    thread.state = ThreadState::Ready;
                }
            }
        }
        while let Some(pid) = self.queue.pop_front() {
            if self.threads.get(&pid).map(|t| t.state) == Some(ThreadState::Blocked) {
                continue;
            }
            if let Some(thread) = self.threads.get_mut(&pid) {
                thread.state = ThreadState::Running;
                thread.turns += 1;
                self.current = Some(pid);
                self.switches += 1;
                let remaining = thread.remaining;
                return Tick::Running { pid, remaining };
            }
        }
        self.current = None;
        Tick::Idle
    }

    /// Run `n` timer ticks, collecting the preemption events (for the shell's
    /// `quantum` demo and for tests).
    pub fn advance(&mut self, n: u64) -> Vec<Tick> {
        let mut events = Vec::new();
        for _ in 0..n {
            let tick = self.on_tick();
            if matches!(tick, Tick::Preempted { .. } | Tick::Idle) {
                events.push(tick);
            }
        }
        events
    }

    // ── introspection ────────────────────────────────────────────────────

    pub fn current(&self) -> Option<Pid> {
        self.current
    }
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
    pub fn switches(&self) -> u64 {
        self.switches
    }
    pub fn total_preemptions(&self) -> u64 {
        self.preemptions
    }
    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Threads sorted by pid, for a `top`-style view.
    pub fn threads(&self) -> Vec<&Thread> {
        self.threads.values().collect()
    }

    /// The share of CPU (per mille) a thread has received — the observable
    /// result of proof-weighted quanta.
    pub fn cpu_share_milli(&self, pid: Pid) -> u64 {
        let total: u64 = self.threads.values().map(|t| t.cpu_ticks).sum();
        if total == 0 {
            return 0;
        }
        self.threads
            .get(&pid)
            .map(|t| t.cpu_ticks * 1000 / total)
            .unwrap_or(0)
    }
}

impl Default for PreemptiveScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantum_is_a_function_of_proof() {
        assert!(quantum_for(Tier::Proven) > quantum_for(Tier::Partial));
        assert!(quantum_for(Tier::Partial) > quantum_for(Tier::Unproven));
    }

    #[test]
    fn a_lone_thread_runs_its_full_slice_then_renews() {
        let mut sched = PreemptiveScheduler::new();
        sched.admit(1, Tier::Proven);
        // It runs uninterrupted until its proven quantum is exhausted, then —
        // with no rival waiting — simply starts a fresh slice (no switch).
        let mut renewed = false;
        for _ in 0..14 {
            match sched.on_tick() {
                Tick::Running { pid: 1, .. } => {}
                Tick::Renewed { pid: 1 } => {
                    renewed = true;
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(renewed, "the lone proven thread should renew its slice");
        assert_eq!(sched.total_preemptions(), 0, "no rival, no preemption");
    }

    #[test]
    fn unproven_runaway_cannot_starve_a_proven_thread() {
        let mut sched = PreemptiveScheduler::new();
        sched.admit(1, Tier::Proven); // a well-behaved compute kernel
        sched.admit(2, Tier::Unproven); // a runaway `while true {}`

        // Run a long time; both are always CPU-hungry (never yield).
        sched.advance(600);

        let proven = sched.cpu_share_milli(1);
        let unproven = sched.cpu_share_milli(2);
        // Proof-weighted: proven gets ~10/(10+2)=83%, unproven ~17%.
        assert!(proven > 700, "proven share {proven}‰ should dominate");
        assert!(unproven > 50, "unproven still makes progress: {unproven}‰");
        // Crucially, the unproven runaway is preempted over and over — it can
        // never hang the machine.
        let runaway = sched.threads().into_iter().find(|t| t.pid == 2).unwrap();
        assert!(runaway.preemptions > 30, "runaway forced off repeatedly");
    }

    #[test]
    fn proven_work_suffers_fewer_context_switches() {
        // Two proven threads vs two unproven threads, same count, same run.
        let mut proven = PreemptiveScheduler::new();
        proven.admit(1, Tier::Proven);
        proven.admit(2, Tier::Proven);
        proven.advance(200);

        let mut unproven = PreemptiveScheduler::new();
        unproven.admit(1, Tier::Unproven);
        unproven.admit(2, Tier::Unproven);
        unproven.advance(200);

        // Longer proven quanta → far fewer switches for the same CPU time.
        assert!(
            proven.switches() < unproven.switches(),
            "proven {} vs unproven {} switches",
            proven.switches(),
            unproven.switches()
        );
    }

    #[test]
    fn yield_gives_up_the_slice_and_blocking_removes_from_rotation() {
        let mut sched = PreemptiveScheduler::new();
        sched.admit(1, Tier::Proven);
        sched.admit(2, Tier::Proven);
        sched.on_tick(); // dispatch pid 1
        assert_eq!(sched.current(), Some(1));
        // pid 1 yields early → pid 2 runs, and pid 1 recorded a voluntary yield.
        let outcome = sched.yield_current();
        assert!(matches!(outcome, Tick::Preempted { from: 1, to: 2 }));
        assert_eq!(
            sched
                .threads()
                .into_iter()
                .find(|t| t.pid == 1)
                .unwrap()
                .yields,
            1
        );

        // Block pid 2 → only pid 1 remains runnable.
        sched.block(2);
        sched.on_tick();
        assert_eq!(sched.current(), Some(1));
        // Unblock returns it to rotation.
        sched.unblock(2);
        assert_eq!(sched.thread_count(), 2);
    }

    #[test]
    fn removing_the_running_thread_frees_the_cpu() {
        let mut sched = PreemptiveScheduler::new();
        sched.admit(1, Tier::Unproven);
        sched.on_tick();
        assert_eq!(sched.current(), Some(1));
        assert!(sched.remove(1));
        // Next tick finds nothing to run.
        assert_eq!(sched.on_tick(), Tick::Idle);
    }
}

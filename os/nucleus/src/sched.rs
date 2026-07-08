//! The epistemic scheduler — two mechanisms no mainstream kernel has:
//!
//! 1. **Proof-gated cooperative scheduling.** Preemption is a distrust tax: a
//!    timer interrupt exists because the kernel cannot trust code to yield.
//!    Here, tasks whose ledger tier is Proven carry resource/termination
//!    claims, so they run cooperatively with zero preemption machinery, and
//!    *unproven* tasks pay the tax explicitly — a scheduler throttle that only
//!    admits them every [`UNPROVEN_STRIDE`]-th round. Trust is the cheap path;
//!    distrust is priced, not universal.
//!
//! 2. **Surprise-driven attention (scheduling as active inference).** The
//!    scheduler keeps a predictive model of every task's burst length (an
//!    integer EWMA) and measures its own prediction error — *surprise* — on
//!    each poll. CPU attention flows toward surprise: a task behaving exactly
//!    as modeled needs no scrutiny and schedules on its prediction; a task
//!    deviating from its model is exactly where something is happening, so it
//!    is boosted, observed, and re-modeled. Linux profiles everything or
//!    nothing; an epistemic scheduler spends observation where the model is
//!    wrong. Chronic surprise in a Proven task is also a *drift signal* — the
//!    resource-bound claim may no longer describe reality — surfaced via
//!    [`Scheduler::suspects`].
//!
//! The executor itself is a from-scratch cooperative future runner: own raw
//! waker, own yield point, no async runtime underneath.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::ptr;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::kairos;
use crate::proof::{Tier, Transition, TrustLedger};

/// Unproven tasks run only every N-th scheduler round — the distrust tax,
/// levied on exactly the code that has not proven it deserves better.
pub const UNPROVEN_STRIDE: u64 = 4;

/// EWMA weight: prediction = (3*old + new) / 4. Integer math only — the
/// bare-metal target is soft-float and a kernel has no business in f64.
const EWMA_OLD: u64 = 3;
const EWMA_DIV: u64 = 4;

/// Surprise (per-mille prediction error) above which a Proven task becomes a
/// drift suspect for the verification layer to re-check.
pub const SUSPECT_SURPRISE_MILLI: u64 = 1500;

pub struct TaskStats {
    pub polls: u64,
    /// Predicted burst per poll, in milli-ticks (fixed-point EWMA — integer
    /// math that still resolves sub-tick bursts).
    pub predicted_milliticks: u64,
    pub last_ticks: u64,
    /// Per-mille prediction error on the last poll.
    pub surprise_milli: u64,
    /// EWMA of the burst model's *absolute* error, milli-ticks — how much
    /// the scheduler distrusts its own forecast. The `û` of the Kairos index.
    pub uncertainty_milli: u64,
    /// Declared pending work units (queued messages, outstanding intents)
    /// awaiting this task. The `Q` of the Kairos index; each poll retires one.
    pub backlog: u64,
    pub total_ticks: u64,
}

struct Task {
    id: u64,
    name: String,
    future: Pin<Box<dyn Future<Output = ()>>>,
    tier: Tier,
    done: bool,
    stats: TaskStats,
}

/// A read-only view for shells and telemetry.
pub struct TaskView<'a> {
    pub id: u64,
    pub name: &'a str,
    pub tier: Tier,
    pub done: bool,
    pub stats: &'a TaskStats,
}

pub struct Scheduler {
    tasks: Vec<Task>,
    round: u64,
    next_id: u64,
    clock: Box<dyn Fn() -> u64>,
}

impl Scheduler {
    /// `clock` is the platform's monotonic tick source (TSC on bare metal,
    /// `Instant` on the hosted runner).
    pub fn new(clock: Box<dyn Fn() -> u64>) -> Self {
        Self {
            tasks: Vec::new(),
            round: 0,
            next_id: 1,
            clock,
        }
    }

    pub fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Admit a task at the tier its ledger claims currently buy.
    pub fn spawn(
        &mut self,
        name: impl Into<String>,
        ledger: &TrustLedger,
        future: impl Future<Output = ()> + 'static,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let tier = ledger.tier_of(id);
        self.tasks.push(Task {
            id,
            name: name.into(),
            future: Box::pin(future),
            tier,
            done: false,
            stats: TaskStats {
                polls: 0,
                predicted_milliticks: 0,
                last_ticks: 0,
                surprise_milli: 0,
                uncertainty_milli: 0,
                backlog: 0,
                total_ticks: 0,
            },
        });
        id
    }

    /// The id the *next* spawn will get — lets callers grant ledger claims to
    /// a task before admitting it, so it is born at the right tier.
    pub fn peek_next_id(&self) -> u64 {
        self.next_id
    }

    /// Re-read the ledger and move tasks between tiers while they run. Every
    /// move lands in the ledger's transition log at tick precision.
    pub fn resync(&mut self, ledger: &mut TrustLedger) -> usize {
        let now = self.now();
        let mut moved = 0;
        for task in &mut self.tasks {
            let to = ledger.tier_of(task.id);
            if to != task.tier {
                ledger.record_transition(Transition {
                    task: task.id,
                    from: task.tier,
                    to,
                    reason: if to.rank() > task.tier.rank() {
                        String::from("claim drifted")
                    } else {
                        String::from("claim restored")
                    },
                    at_tick: now,
                });
                task.tier = to;
                moved += 1;
            }
        }
        moved
    }

    /// Run up to `max_polls` scheduling decisions. Each round picks the
    /// highest-attention runnable task, polls it once, measures the burst
    /// against the model, and updates prediction + surprise.
    pub fn run_slice(&mut self, max_polls: usize) -> usize {
        let mut polled = 0;
        for _ in 0..max_polls {
            self.round += 1;
            let tax_round = self.round.is_multiple_of(UNPROVEN_STRIDE);
            let Some(index) = self.pick(tax_round) else {
                break;
            };
            self.poll_one(index);
            polled += 1;
        }
        polled
    }

    /// Epistemic attention `A`: proof buys admission, surprise buys attention.
    pub fn attention(tier: Tier, surprise_milli: u64) -> u64 {
        let tier_bonus: u64 = match tier {
            Tier::Proven => 3000,
            Tier::Partial => 1500,
            Tier::Unproven => 300,
        };
        tier_bonus + surprise_milli.min(3000)
    }

    /// The Kairos index of one task — see [`kairos`] for the derivation.
    /// Attention steers when queues are quiet; declared backlog dominates
    /// when real work is pending; the predicted burst and the model's own
    /// uncertainty discount both sit in the denominator, so cheap
    /// *predictable* work wins the slice.
    fn score(task: &Task) -> u64 {
        kairos::index(&kairos::Inputs {
            attention: Self::attention(task.tier, task.stats.surprise_milli),
            backlog: task.stats.backlog,
            predicted_milliticks: task.stats.predicted_milliticks,
            uncertainty_milli: task.stats.uncertainty_milli,
        })
    }

    /// Declare `units` of pending work for task `id` — an arrival in the
    /// Kairos queueing model (a message routed to it, an intent awaiting it).
    /// Returns false if no live task has that id.
    pub fn push_backlog(&mut self, id: u64, units: u64) -> bool {
        match self.tasks.iter_mut().find(|t| t.id == id && !t.done) {
            Some(task) => {
                task.stats.backlog = task.stats.backlog.saturating_add(units);
                true
            }
            None => false,
        }
    }

    /// Every `UNPROVEN_STRIDE`-th round *belongs* to unproven work — that is
    /// its whole CPU share when contended (the tax, exactly priced). All other
    /// rounds go to proven/partial tasks. Neither side ever starves: when only
    /// one kind of work exists, it runs regardless of the round.
    fn pick(&self, tax_round: bool) -> Option<usize> {
        let best = |want_unproven: bool| {
            self.tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.done && (t.tier == Tier::Unproven) == want_unproven)
                .max_by_key(|(index, t)| {
                    // Least-recently-polled breaks score ties so one hot task
                    // cannot starve its tier peers.
                    (
                        Self::score(t),
                        u64::MAX - t.stats.polls,
                        usize::MAX - *index,
                    )
                })
                .map(|(index, _)| index)
        };
        if tax_round {
            best(true).or_else(|| best(false))
        } else {
            best(false).or_else(|| best(true))
        }
    }

    fn poll_one(&mut self, index: usize) {
        let task = &mut self.tasks[index];
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let before = (self.clock)();
        let poll = task.future.as_mut().poll(&mut cx);
        let elapsed = (self.clock)().saturating_sub(before);

        let stats = &mut task.stats;
        stats.polls += 1;
        stats.last_ticks = elapsed;
        stats.total_ticks += elapsed;
        // A poll is one service opportunity: it retires one declared unit.
        stats.backlog = stats.backlog.saturating_sub(1);
        let actual_milli = elapsed.saturating_mul(1000);
        let predicted = stats.predicted_milliticks;
        let error_milli = actual_milli.abs_diff(predicted);
        stats.surprise_milli = error_milli * 1000 / (predicted + 1000);
        stats.uncertainty_milli = (EWMA_OLD * stats.uncertainty_milli + error_milli) / EWMA_DIV;
        stats.predicted_milliticks = (EWMA_OLD * predicted + actual_milli) / EWMA_DIV;

        if poll.is_ready() {
            task.done = true;
            stats.backlog = 0;
        }
    }

    /// Proven tasks whose behavior chronically defies their model — the
    /// epistemic drift signal the verification layer should re-check first.
    pub fn suspects(&self) -> Vec<TaskView<'_>> {
        self.tasks
            .iter()
            .filter(|t| {
                !t.done
                    && t.tier == Tier::Proven
                    && t.stats.polls > 3
                    && t.stats.surprise_milli >= SUSPECT_SURPRISE_MILLI
            })
            .map(Task::view)
            .collect()
    }

    pub fn tasks(&self) -> Vec<TaskView<'_>> {
        self.tasks.iter().map(Task::view).collect()
    }

    pub fn runnable(&self) -> usize {
        self.tasks.iter().filter(|t| !t.done).count()
    }

    pub fn round(&self) -> u64 {
        self.round
    }
}

impl Task {
    fn view(&self) -> TaskView<'_> {
        TaskView {
            id: self.id,
            name: &self.name,
            tier: self.tier,
            done: self.done,
            stats: &self.stats,
        }
    }
}

// ── the from-scratch waker and yield point ──────────────────────────────

fn noop_raw_waker() -> RawWaker {
    RawWaker::new(ptr::null(), &NOOP_VTABLE)
}

static NOOP_VTABLE: RawWakerVTable =
    RawWakerVTable::new(|_| noop_raw_waker(), |_| {}, |_| {}, |_| {});

fn noop_waker() -> Waker {
    // Safety: every vtable entry is a no-op; the data pointer is never read.
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

/// Cooperative yield: return control to the scheduler once.
pub fn yield_now() -> YieldNow {
    YieldNow { yielded: false }
}

pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proof::ClaimClass;
    use alloc::rc::Rc;
    use core::cell::RefCell;

    fn test_clock() -> Box<dyn Fn() -> u64> {
        // A deterministic, strictly-advancing clock.
        let tick = Rc::new(RefCell::new(0u64));
        Box::new(move || {
            *tick.borrow_mut() += 1;
            *tick.borrow()
        })
    }

    async fn count_to(n: u64, out: Rc<RefCell<u64>>) {
        for _ in 0..n {
            *out.borrow_mut() += 1;
            yield_now().await;
        }
    }

    #[test]
    fn cooperative_tasks_run_to_completion() {
        let ledger = TrustLedger::new();
        let mut sched = Scheduler::new(test_clock());
        let a = Rc::new(RefCell::new(0));
        let b = Rc::new(RefCell::new(0));
        sched.spawn("a", &ledger, count_to(5, a.clone()));
        sched.spawn("b", &ledger, count_to(3, b.clone()));
        sched.run_slice(200);
        assert_eq!(*a.borrow(), 5);
        assert_eq!(*b.borrow(), 3);
        assert_eq!(sched.runnable(), 0);
    }

    #[test]
    fn unproven_tasks_pay_the_distrust_tax() {
        let mut ledger = TrustLedger::new();
        let mut sched = Scheduler::new(test_clock());

        // A proven task and an unproven one, same workload.
        let proven_id = sched.peek_next_id();
        ledger.grant(proven_id, ClaimClass::MemorySafety);
        ledger.grant(proven_id, ClaimClass::CapabilityBound);
        ledger.grant(proven_id, ClaimClass::ResourceBound);
        let pa = Rc::new(RefCell::new(0));
        let ua = Rc::new(RefCell::new(0));
        sched.spawn("proven", &ledger, count_to(1000, pa.clone()));
        sched.spawn("unproven", &ledger, count_to(1000, ua.clone()));

        sched.run_slice(100);
        let proven_progress = *pa.borrow();
        let unproven_progress = *ua.borrow();
        assert!(
            proven_progress > unproven_progress * 2,
            "proven {proven_progress} should far outpace unproven {unproven_progress}"
        );
        assert!(unproven_progress > 0, "throttled, never starved");
    }

    #[test]
    fn drift_demotes_live_and_resync_records_it() {
        let mut ledger = TrustLedger::new();
        let mut sched = Scheduler::new(test_clock());
        let id = sched.peek_next_id();
        let mem = ledger.grant(id, ClaimClass::MemorySafety);
        ledger.grant(id, ClaimClass::CapabilityBound);
        ledger.grant(id, ClaimClass::ResourceBound);
        let progress = Rc::new(RefCell::new(0));
        sched.spawn("watched", &ledger, count_to(10_000, progress.clone()));
        sched.run_slice(10);
        assert_eq!(sched.tasks()[0].tier, Tier::Proven);

        // The proof breaks mid-flight; the task keeps running, demoted.
        ledger.drift(mem);
        assert_eq!(sched.resync(&mut ledger), 1);
        assert_eq!(sched.tasks()[0].tier, Tier::Partial);
        assert_eq!(ledger.transitions().len(), 1);
        assert!(ledger.transitions()[0].at_tick > 0);
        sched.run_slice(10);
        assert!(*progress.borrow() > 10, "demoted but still running");
    }

    #[test]
    fn backlog_pulls_the_scheduler_toward_pending_work() {
        // Two identical proven tasks; one has 60 units of declared pending
        // work (arrivals routed to it). The Kairos index must route the
        // slices where the work actually is — not split them evenly.
        let mut ledger = TrustLedger::new();
        let mut sched = Scheduler::new(test_clock());
        for offset in 0..2 {
            let id = sched.peek_next_id() + offset;
            ledger.grant(id, ClaimClass::MemorySafety);
            ledger.grant(id, ClaimClass::CapabilityBound);
            ledger.grant(id, ClaimClass::ResourceBound);
        }
        let idle = Rc::new(RefCell::new(0));
        let busy = Rc::new(RefCell::new(0));
        sched.spawn("idle", &ledger, count_to(1000, idle.clone()));
        let busy_id = sched.spawn("busy", &ledger, count_to(1000, busy.clone()));
        assert!(sched.push_backlog(busy_id, 60));

        sched.run_slice(60);
        let idle_progress = *idle.borrow();
        let busy_progress = *busy.borrow();
        assert!(
            busy_progress > idle_progress * 3,
            "backlogged task got {busy_progress}, idle peer got {idle_progress}"
        );
        assert!(idle_progress > 0, "steered, never starved");
    }

    #[test]
    fn scheduler_learns_burst_predictions() {
        let ledger = TrustLedger::new();
        let mut sched = Scheduler::new(test_clock());
        let out = Rc::new(RefCell::new(0));
        sched.spawn("steady", &ledger, count_to(50, out.clone()));
        sched.run_slice(60);
        let tasks = sched.tasks();
        let stats = tasks[0].stats;
        // A steady task under a steady clock becomes predictable: the model
        // converges and surprise collapses.
        assert!(stats.polls > 10);
        assert!(stats.predicted_milliticks > 0);
        assert!(
            stats.surprise_milli < 500,
            "steady task should be well-modeled, surprise={} pred={}",
            stats.surprise_milli,
            stats.predicted_milliticks
        );
    }
}

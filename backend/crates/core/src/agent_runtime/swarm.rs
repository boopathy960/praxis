// ─────────────────────────────────────────────────────────────
// Swarm Scheduler — Cooperative Agent Multiplexing
// ─────────────────────────────────────────────────────────────
// Runs hundreds-to-thousands of agent executions on a handful of CPU cores
// by scheduling at tool-step granularity instead of binding one agent to one
// OS thread for its whole pipeline. An agent in this runtime is queue state
// (~KBs), not a thread; the scarce resource is lanes (worker threads), and
// lanes are governed by measurement, not guesses. Four formulas:
//
//   1. Lane Expansion Law (worker sizing). A step alternates s ms of compute
//      with b = β·s ms blocked on I/O. Saturating C cores requires
//
//          R*(t) = clamp(⌈C · (1 + β̂(t))⌉, 1, R_max)
//
//      lanes, where β̂ is an EWMA of the measured blocked-to-service time
//      ratio. Derivation is Little's law applied to the lane pool: lane
//      utilization U = R·s / (C·(s+b)); setting U = 1 gives R = C(1+β).
//      Pure-compute swarms collapse to R* = C (more threads would only
//      context-switch); I/O-heavy swarms expand so cores stay busy while
//      lanes wait on the network.
//
//   2. Virtual-finish fairness (weighted fair queueing over cost units).
//      Each agent i with weight w_i (its cost budget) carries a virtual
//      finish time updated per step of cost c:
//
//          F_i ← max(V, F_i) + c / w_i
//
//      and the dispatcher always runs the smallest F_i (V is the global
//      virtual clock). Between two consecutive steps of any agent, every
//      other resident agent advances at most one step, so the longest wait
//      for service is bounded by (R_resident − 1) · s_max — no agent can be
//      starved no matter how many are spawned.
//
//   3. Memory-envelope admission. With swarm budget B bytes and ŝ an EWMA
//      of observed per-agent state size, the admission cap is
//
//          M(t) = ⌊B / ŝ(t)⌋
//
//      enforced incrementally: an agent is admitted only while
//      resident_bytes + estimate ≤ B, so resident memory is bounded by
//      construction rather than by hope.
//
//   4. Throughput ceiling (reported, never promised):
//
//          X̂ = R_active / (s̄ · (1 + β̂))   steps per millisecond
//
//      the physical limit of the current configuration. No scheduler can
//      exceed it; this one converges toward it.
//
// None of this manufactures CPU cycles. It converts time agents would spend
// blocked on I/O or waiting for a whole free thread into interleaved
// progress, with bounded memory and provable fairness.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms, sha3_hex};
use crate::search_intelligence::SearchIntelligenceService;

use super::{
    AgentExecutionReceipt, AgentExecutionStep, AgentRuntimeStore, BlockingReasoner,
    GeneratedAgentManifest, GeneratedToolManifest, ToolContext, builtin_tool_cost, canonical_tool,
    execute_tool,
};

/// Hard cap on `replicate` for a single swarm spec.
pub const MAX_REPLICATE: usize = 10_000;
/// Idle lanes exit after this long with an empty run queue.
const LANE_IDLE_EXIT: Duration = Duration::from_millis(200);
/// Fixed per-task bookkeeping overhead added to the state-size estimate.
const TASK_OVERHEAD_BYTES: usize = 768;

#[derive(Debug, Clone)]
pub struct SwarmConfig {
    /// Hard ceiling on worker lanes regardless of the expansion law.
    pub max_worker_threads: usize,
    /// Hard ceiling on resident (queued + running) tasks.
    pub max_resident_tasks: usize,
    /// Memory envelope for all resident task state, in bytes.
    pub memory_budget_bytes: usize,
    /// EWMA smoothing factor for β̂, s̄, and ŝ.
    pub ewma_alpha: f64,
    /// Overrides detected core count (useful for tests and pinned deploys).
    pub cores_override: Option<usize>,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            max_worker_threads: 32,
            max_resident_tasks: 50_000,
            memory_budget_bytes: 128 * 1024 * 1024,
            ewma_alpha: 0.2,
            cores_override: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SwarmAgentSpec {
    pub manifest_id: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub requested_tools: Vec<String>,
    /// Number of identical agents to spawn from this spec (clamped to
    /// [1, MAX_REPLICATE]).
    #[serde(default = "default_replicate")]
    pub replicate: usize,
}

fn default_replicate() -> usize {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpawnSwarmRequest {
    pub agents: Vec<SwarmAgentSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwarmReceipt {
    pub swarm_id: String,
    pub admitted: usize,
    /// Tasks refused by the memory envelope or residency cap.
    pub rejected: usize,
    pub worker_target: usize,
    /// Current memory-envelope admission cap M(t) in tasks.
    pub memory_cap_tasks: usize,
    pub execution_ids: Vec<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwarmStatus {
    pub swarm_id: String,
    pub total: usize,
    pub completed: usize,
    pub completed_with_warnings: usize,
    pub failed: usize,
    pub in_flight: usize,
    /// EWMA blocked-to-service ratio β̂ driving the Lane Expansion Law.
    pub beta: f64,
    /// EWMA per-step service time in milliseconds.
    pub service_ms_ewma: f64,
    pub worker_threads: usize,
    pub worker_target: usize,
    /// Throughput ceiling X̂ for the current configuration, steps/second.
    pub steps_per_second_estimate: f64,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

/// Internal spec handed over by the runtime service after validation.
pub(super) struct AdmitSpec {
    pub manifest: GeneratedAgentManifest,
    pub tools: Vec<String>,
    pub input: serde_json::Value,
    pub replicate: usize,
}

// ── Governing formulas ──

/// Lane Expansion Law: R* = clamp(⌈C · (1 + β̂)⌉, 1, R_max).
fn residency_target(cores: usize, beta: f64, max_threads: usize) -> usize {
    let expanded = (cores as f64 * (1.0 + beta.max(0.0))).ceil() as usize;
    expanded.clamp(1, max_threads.max(1))
}

/// Virtual-finish update: F ← max(V, F_prev) + c / w.
fn virtual_finish(virtual_now: f64, previous_finish: f64, step_cost: f64, weight: f64) -> f64 {
    virtual_now.max(previous_finish) + step_cost / weight.max(1.0)
}

/// Memory-envelope admission cap: M = ⌊B / ŝ⌋.
fn memory_admission_cap(budget_bytes: usize, state_bytes_ewma: f64) -> usize {
    (budget_bytes as f64 / state_bytes_ewma.max(1.0)) as usize
}

/// Throughput ceiling X̂ = R_active / (s̄ · (1 + β̂)), converted to steps/sec.
fn throughput_ceiling(active_lanes: usize, service_ms: f64, beta: f64) -> f64 {
    if service_ms <= f64::EPSILON {
        return 0.0;
    }
    active_lanes as f64 * 1000.0 / (service_ms * (1.0 + beta.max(0.0)))
}

fn ewma(previous: f64, sample: f64, alpha: f64) -> f64 {
    previous + alpha * (sample - previous)
}

fn estimate_task_bytes(input_len: usize, manifest: &GeneratedAgentManifest) -> usize {
    input_len
        + manifest.goal.len()
        + manifest
            .tool_allowlist
            .iter()
            .map(String::len)
            .sum::<usize>()
        + TASK_OVERHEAD_BYTES
}

// ── Task and scheduler state ──

struct SwarmTask {
    swarm_id: String,
    execution_id: String,
    manifest: GeneratedAgentManifest,
    custom_tools: Arc<HashMap<String, GeneratedToolManifest>>,
    research: Option<SearchIntelligenceService>,
    device: Option<Arc<crate::device_agent::DeviceCapabilities>>,
    reasoner: Option<BlockingReasoner>,
    workspace: PathBuf,
    actor: String,
    remaining: VecDeque<String>,
    working_input: serde_json::Value,
    steps: Vec<AgentExecutionStep>,
    request_hash: String,
    cost_spent: u32,
    warnings: bool,
    weight: f64,
    /// Wall-clock budget starts at first dispatch, not admission, so queue
    /// wait in a large swarm does not consume the agent's time budget.
    deadline: Option<Instant>,
    approx_bytes: usize,
    created_at_ms: i64,
}

struct TaskEntry {
    vfinish: f64,
    seq: u64,
    task: SwarmTask,
}

impl PartialEq for TaskEntry {
    fn eq(&self, other: &Self) -> bool {
        self.seq == other.seq
    }
}
impl Eq for TaskEntry {}
impl PartialOrd for TaskEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TaskEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.vfinish
            .total_cmp(&other.vfinish)
            .then(self.seq.cmp(&other.seq))
    }
}

struct SwarmState {
    total: usize,
    completed: usize,
    completed_with_warnings: usize,
    failed: usize,
    created_at_ms: i64,
    completed_at_ms: Option<i64>,
}

struct SchedState {
    runqueue: BinaryHeap<Reverse<TaskEntry>>,
    virtual_now: f64,
    seq: u64,
    resident_tasks: usize,
    resident_bytes: usize,
    /// EWMA per-step compute time (ms).
    service_ms: f64,
    /// EWMA per-step blocked time (ms); β̂ = blocked_ms / service_ms.
    blocked_ms: f64,
    /// EWMA per-task state size (bytes) feeding the admission cap.
    state_bytes: f64,
    swarms: HashMap<String, SwarmState>,
}

impl SchedState {
    fn beta(&self) -> f64 {
        self.blocked_ms / self.service_ms.max(0.001)
    }
}

struct SwarmInner {
    config: SwarmConfig,
    cores: usize,
    store: Shared<AgentRuntimeStore>,
    sched: Mutex<SchedState>,
    work_cv: Condvar,
    done_cv: Condvar,
    workers: AtomicUsize,
}

#[derive(Clone)]
pub(super) struct SwarmScheduler {
    inner: Arc<SwarmInner>,
}

impl SwarmScheduler {
    pub(super) fn new(store: Shared<AgentRuntimeStore>) -> Self {
        Self::with_config(store, SwarmConfig::default())
    }

    pub(super) fn with_config(store: Shared<AgentRuntimeStore>, config: SwarmConfig) -> Self {
        let cores = config.cores_override.unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(2, std::num::NonZero::get)
        });
        Self {
            inner: Arc::new(SwarmInner {
                config,
                cores,
                store,
                sched: Mutex::new(SchedState {
                    runqueue: BinaryHeap::new(),
                    virtual_now: 0.0,
                    seq: 0,
                    resident_tasks: 0,
                    resident_bytes: 0,
                    service_ms: 1.0,
                    blocked_ms: 0.0,
                    state_bytes: TASK_OVERHEAD_BYTES as f64,
                    swarms: HashMap::new(),
                }),
                work_cv: Condvar::new(),
                done_cv: Condvar::new(),
                workers: AtomicUsize::new(0),
            }),
        }
    }

    pub(super) fn admit(
        &self,
        specs: Vec<AdmitSpec>,
        custom_tools: Arc<HashMap<String, GeneratedToolManifest>>,
        research: Option<SearchIntelligenceService>,
        device: Option<Arc<crate::device_agent::DeviceCapabilities>>,
        reasoner: Option<BlockingReasoner>,
        workspace_root: &Arc<PathBuf>,
        actor: &str,
    ) -> SwarmReceipt {
        let swarm_id = new_id("swarm");
        let created_at_ms = now_ms();
        let mut admitted_ids = Vec::new();
        let mut rejected = 0_usize;

        let worker_target;
        let memory_cap_tasks;
        {
            let mut state = self.inner.sched.lock();
            for spec in specs {
                let input_json = spec.input.to_string();
                let request_hash = sha3_hex(input_json.as_bytes());
                let approx = estimate_task_bytes(input_json.len(), &spec.manifest);
                for _ in 0..spec.replicate {
                    if state.resident_tasks >= self.inner.config.max_resident_tasks
                        || state.resident_bytes + approx > self.inner.config.memory_budget_bytes
                    {
                        rejected += 1;
                        continue;
                    }
                    let execution_id = new_id("agent_exec");
                    let weight = f64::from(spec.manifest.cost_budget.max(1));
                    let task = SwarmTask {
                        swarm_id: swarm_id.clone(),
                        execution_id: execution_id.clone(),
                        manifest: spec.manifest.clone(),
                        custom_tools: custom_tools.clone(),
                        research: research.clone(),
                        device: device.clone(),
                        reasoner: reasoner.clone(),
                        workspace: workspace_root.join(&spec.manifest.manifest_id),
                        actor: actor.into(),
                        remaining: spec.tools.iter().cloned().collect(),
                        working_input: spec.input.clone(),
                        steps: Vec::new(),
                        request_hash: request_hash.clone(),
                        cost_spent: 0,
                        warnings: false,
                        weight,
                        deadline: None,
                        approx_bytes: approx,
                        created_at_ms: now_ms(),
                    };
                    state.seq += 1;
                    let entry = TaskEntry {
                        vfinish: virtual_finish(state.virtual_now, 0.0, 1.0, weight),
                        seq: state.seq,
                        task,
                    };
                    state.runqueue.push(Reverse(entry));
                    state.resident_tasks += 1;
                    state.resident_bytes += approx;
                    state.state_bytes = ewma(
                        state.state_bytes,
                        approx as f64,
                        self.inner.config.ewma_alpha,
                    );
                    admitted_ids.push(execution_id);
                }
            }
            let completed_at_ms = if admitted_ids.is_empty() {
                Some(now_ms())
            } else {
                None
            };
            state.swarms.insert(
                swarm_id.clone(),
                SwarmState {
                    total: admitted_ids.len(),
                    completed: 0,
                    completed_with_warnings: 0,
                    failed: 0,
                    created_at_ms,
                    completed_at_ms,
                },
            );
            worker_target = residency_target(
                self.inner.cores,
                state.beta(),
                self.inner.config.max_worker_threads,
            )
            .min(state.runqueue.len());
            memory_cap_tasks =
                memory_admission_cap(self.inner.config.memory_budget_bytes, state.state_bytes);
        }

        spawn_workers(&self.inner, worker_target);
        self.inner.work_cv.notify_all();
        SwarmReceipt {
            swarm_id,
            admitted: admitted_ids.len(),
            rejected,
            worker_target,
            memory_cap_tasks,
            execution_ids: admitted_ids,
            created_at_ms,
        }
    }

    pub(super) fn status(&self, swarm_id: &str) -> Result<SwarmStatus, AppError> {
        let state = self.inner.sched.lock();
        self.status_locked(&state, swarm_id)
            .ok_or_else(|| AppError::NotFound(format!("swarm {swarm_id}")))
    }

    pub(super) fn wait(&self, swarm_id: &str, timeout: Duration) -> Result<SwarmStatus, AppError> {
        let deadline = Instant::now() + timeout;
        let mut state = self.inner.sched.lock();
        loop {
            let Some(status) = self.status_locked(&state, swarm_id) else {
                return Err(AppError::NotFound(format!("swarm {swarm_id}")));
            };
            if status.completed_at_ms.is_some() {
                return Ok(status);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(status);
            }
            self.inner.done_cv.wait_for(&mut state, deadline - now);
        }
    }

    fn status_locked(&self, state: &SchedState, swarm_id: &str) -> Option<SwarmStatus> {
        let swarm = state.swarms.get(swarm_id)?;
        let workers = self.inner.workers.load(Ordering::Relaxed);
        let beta = state.beta();
        let worker_target =
            residency_target(self.inner.cores, beta, self.inner.config.max_worker_threads);
        let finished = swarm.completed + swarm.completed_with_warnings + swarm.failed;
        Some(SwarmStatus {
            swarm_id: swarm_id.into(),
            total: swarm.total,
            completed: swarm.completed,
            completed_with_warnings: swarm.completed_with_warnings,
            failed: swarm.failed,
            in_flight: swarm.total - finished,
            beta,
            service_ms_ewma: state.service_ms,
            worker_threads: workers,
            worker_target,
            steps_per_second_estimate: throughput_ceiling(workers.max(1), state.service_ms, beta),
            created_at_ms: swarm.created_at_ms,
            completed_at_ms: swarm.completed_at_ms,
        })
    }
}

fn spawn_workers(inner: &Arc<SwarmInner>, target: usize) {
    loop {
        let current = inner.workers.load(Ordering::Relaxed);
        if current >= target {
            break;
        }
        if inner
            .workers
            .compare_exchange(current, current + 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            continue;
        }
        let lane = Arc::clone(inner);
        if std::thread::Builder::new()
            .name("astra-swarm-lane".into())
            .spawn(move || worker_loop(&lane))
            .is_err()
        {
            inner.workers.fetch_sub(1, Ordering::Relaxed);
            break;
        }
    }
}

fn worker_loop(inner: &Arc<SwarmInner>) {
    loop {
        let mut entry = {
            let mut state = inner.sched.lock();
            loop {
                if let Some(Reverse(entry)) = state.runqueue.pop() {
                    state.virtual_now = state.virtual_now.max(entry.vfinish);
                    break Some(entry);
                }
                let timed_out = inner
                    .work_cv
                    .wait_for(&mut state, LANE_IDLE_EXIT)
                    .timed_out();
                if timed_out && state.runqueue.is_empty() {
                    break None;
                }
            }
        };
        let Some(ref mut popped) = entry else {
            inner.workers.fetch_sub(1, Ordering::Relaxed);
            return;
        };

        let bytes_before = popped.task.approx_bytes;
        let (finished, timing) = run_one_step(&mut popped.task);

        let worker_target = {
            let mut state = inner.sched.lock();
            if let Some(timing) = timing {
                if timing.blocking {
                    state.blocked_ms =
                        ewma(state.blocked_ms, timing.elapsed_ms, inner.config.ewma_alpha);
                } else {
                    state.service_ms =
                        ewma(state.service_ms, timing.elapsed_ms, inner.config.ewma_alpha);
                    state.blocked_ms = ewma(state.blocked_ms, 0.0, inner.config.ewma_alpha);
                }
            }
            let entry = entry.take().expect("entry present");
            if finished {
                state.resident_tasks -= 1;
                state.resident_bytes = state.resident_bytes.saturating_sub(bytes_before);
                finalize_task(inner, &mut state, entry.task);
            } else {
                let task = entry.task;
                state.resident_bytes = state
                    .resident_bytes
                    .saturating_sub(bytes_before)
                    .saturating_add(task.approx_bytes);
                state.state_bytes = ewma(
                    state.state_bytes,
                    task.approx_bytes as f64,
                    inner.config.ewma_alpha,
                );
                let step_cost = task
                    .steps
                    .last()
                    .map_or(1.0, |step| f64::from(step.cost_units.max(1)));
                let weight = task.weight;
                state.seq += 1;
                let next = TaskEntry {
                    vfinish: virtual_finish(state.virtual_now, entry.vfinish, step_cost, weight),
                    seq: state.seq,
                    task,
                };
                state.runqueue.push(Reverse(next));
            }
            residency_target(inner.cores, state.beta(), inner.config.max_worker_threads)
                .min(state.runqueue.len().max(1))
        };
        if !finished {
            inner.work_cv.notify_one();
        }
        spawn_workers(inner, worker_target);
    }
}

struct StepTiming {
    elapsed_ms: f64,
    blocking: bool,
}

/// Executes exactly one tool step of the task, mirroring the budget and
/// pipelining semantics of `create_execution`. Returns whether the task is
/// finished plus the timing sample feeding β̂ and s̄.
fn run_one_step(task: &mut SwarmTask) -> (bool, Option<StepTiming>) {
    let Some(tool) = task.remaining.pop_front() else {
        return (true, None);
    };
    let deadline = *task.deadline.get_or_insert_with(|| {
        Instant::now() + Duration::from_millis(task.manifest.time_budget_ms)
    });
    let step_started_ms = now_ms();
    let input_json = task.working_input.to_string();
    let input_hash = sha3_hex(input_json.as_bytes());
    let tool_cost = builtin_tool_cost(&tool);

    if Instant::now() >= deadline {
        task.warnings = true;
        task.steps.push(AgentExecutionStep {
            step_id: new_id("agent_step"),
            tool,
            input_hash,
            output_hash: String::new(),
            status: "skipped_time_budget_exhausted".into(),
            output_summary: None,
            cost_units: 0,
            started_at_ms: step_started_ms,
            completed_at_ms: now_ms(),
        });
        return (task.remaining.is_empty(), None);
    }
    if task.cost_spent + tool_cost > task.manifest.cost_budget {
        task.warnings = true;
        task.steps.push(AgentExecutionStep {
            step_id: new_id("agent_step"),
            tool,
            input_hash,
            output_hash: String::new(),
            status: "skipped_cost_budget_exhausted".into(),
            output_summary: None,
            cost_units: 0,
            started_at_ms: step_started_ms,
            completed_at_ms: now_ms(),
        });
        return (task.remaining.is_empty(), None);
    }

    let context = ToolContext {
        workspace: task.workspace.clone(),
        custom_tools: task.custom_tools.clone(),
        research: task.research.clone(),
        device: task.device.clone(),
        reasoner: task.reasoner.clone(),
    };
    // Both deep_research and reason block on the network, so they feed the
    // blocked-time EWMA that drives lane expansion.
    let blocking = matches!(canonical_tool(&tool), "deep_research" | "reason");
    let started = Instant::now();
    let result = execute_tool(&tool, &task.working_input, &context);
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

    match result {
        Ok(output) => {
            task.cost_spent += tool_cost;
            let output_json = output.to_string();
            let summary: String = output_json.chars().take(160).collect();
            task.approx_bytes = estimate_task_bytes(output_json.len(), &task.manifest);
            task.steps.push(AgentExecutionStep {
                step_id: new_id("agent_step"),
                tool,
                input_hash,
                output_hash: sha3_hex(output_json.as_bytes()),
                status: "completed".into(),
                output_summary: Some(summary),
                cost_units: tool_cost,
                started_at_ms: step_started_ms,
                completed_at_ms: now_ms(),
            });
            task.working_input = output;
        }
        Err(reason) => {
            task.warnings = true;
            task.steps.push(AgentExecutionStep {
                step_id: new_id("agent_step"),
                tool,
                input_hash,
                output_hash: String::new(),
                status: format!("failed: {reason}"),
                output_summary: None,
                cost_units: 0,
                started_at_ms: step_started_ms,
                completed_at_ms: now_ms(),
            });
        }
    }
    (
        task.remaining.is_empty(),
        Some(StepTiming {
            elapsed_ms,
            blocking,
        }),
    )
}

/// Builds the standard execution receipt, persists it in the shared runtime
/// store, and updates the swarm's counters. Called with the scheduler lock
/// held; the store has its own lock and is written first-acquired-last to
/// keep lock ordering consistent (sched → store, never store → sched).
fn finalize_task(inner: &Arc<SwarmInner>, state: &mut SchedState, task: SwarmTask) {
    let execution_status = if task.steps.iter().all(|step| step.status == "completed") {
        "completed"
    } else if task.warnings && task.steps.iter().any(|step| step.status == "completed") {
        "completed_with_warnings"
    } else {
        "failed"
    };
    let receipt_hash = sha3_hex(
        serde_json::json!({
            "execution_id": task.execution_id,
            "manifest_id": task.manifest.manifest_id,
            "actor_principal_id": task.actor,
            "request_hash": task.request_hash,
            "steps": &task.steps,
        })
        .to_string()
        .as_bytes(),
    );
    let receipt = AgentExecutionReceipt {
        execution_id: task.execution_id.clone(),
        manifest_id: task.manifest.manifest_id.clone(),
        tenant_scope: task.manifest.tenant_scope.clone(),
        actor_principal_id: task.actor.clone(),
        request_hash: task.request_hash.clone(),
        receipt_hash,
        status: execution_status.into(),
        steps: task.steps,
        resource_limits: BTreeMap::new(),
        created_at_ms: task.created_at_ms,
        completed_at_ms: now_ms(),
        chain_block_height: None,
        chain_block_hash: None,
        chain_receipt_id: None,
        sandbox: None,
    };
    inner
        .store
        .write()
        .executions
        .insert(task.execution_id, receipt);

    if let Some(swarm) = state.swarms.get_mut(&task.swarm_id) {
        match execution_status {
            "completed" => swarm.completed += 1,
            "completed_with_warnings" => swarm.completed_with_warnings += 1,
            _ => swarm.failed += 1,
        }
        let finished = swarm.completed + swarm.completed_with_warnings + swarm.failed;
        if finished >= swarm.total {
            swarm.completed_at_ms = Some(now_ms());
            inner.done_cv.notify_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::{AgentRuntimeService, CreateGeneratedAgentRequest, RiskTier};
    use crate::common::TenantScope;

    fn two_lane_service() -> AgentRuntimeService {
        AgentRuntimeService::new().with_swarm_config(SwarmConfig {
            max_worker_threads: 2,
            cores_override: Some(2),
            ..SwarmConfig::default()
        })
    }

    fn manifest_request(tools: Vec<&str>, cost_budget: u32) -> CreateGeneratedAgentRequest {
        CreateGeneratedAgentRequest {
            tenant_scope: TenantScope::Global,
            goal: "swarm load test".into(),
            tool_allowlist: tools.into_iter().map(str::to_string).collect(),
            output_schema: serde_json::json!({"type": "object"}),
            cost_budget,
            time_budget_ms: 30_000,
            risk_tier: RiskTier::Low,
            escalation_policy: "block".into(),
        }
    }

    #[test]
    fn lane_expansion_law_matches_derivation() {
        // Pure compute: β = 0 collapses to the core count.
        assert_eq!(residency_target(2, 0.0, 64), 2);
        // β = 3 (steps blocked 3x longer than they compute): R* = C(1+β) = 8.
        assert_eq!(residency_target(2, 3.0, 64), 8);
        // The ceiling always wins.
        assert_eq!(residency_target(2, 100.0, 16), 16);
        // Never zero lanes.
        assert_eq!(residency_target(1, 0.0, 1), 1);
    }

    #[test]
    fn virtual_finish_is_monotonic_and_weight_inverse() {
        // Heavier weight ⇒ smaller virtual-time increment ⇒ more service share.
        let light = virtual_finish(0.0, 0.0, 2.0, 1.0);
        let heavy = virtual_finish(0.0, 0.0, 2.0, 4.0);
        assert!(heavy < light);
        // The clock never runs backwards.
        assert!(virtual_finish(10.0, 3.0, 1.0, 1.0) >= 10.0);
        assert!(virtual_finish(3.0, 10.0, 1.0, 1.0) >= 10.0);
    }

    #[test]
    fn memory_envelope_cap_is_budget_over_state_size() {
        assert_eq!(memory_admission_cap(1024, 128.0), 8);
        assert_eq!(memory_admission_cap(1024, 0.0), 1024);
    }

    #[test]
    fn thousand_agents_complete_on_two_lanes() {
        let service = two_lane_service();
        let manifest = service
            .create_agent(manifest_request(vec!["summarizer", "word_counter"], 10))
            .expect("manifest");
        let receipt = service
            .spawn_swarm(
                SpawnSwarmRequest {
                    agents: vec![SwarmAgentSpec {
                        manifest_id: manifest.manifest_id.clone(),
                        input: serde_json::json!("alpha beta gamma delta epsilon"),
                        requested_tools: Vec::new(),
                        replicate: 1000,
                    }],
                },
                "tester",
            )
            .expect("swarm receipt");
        assert_eq!(receipt.admitted, 1000);
        assert_eq!(receipt.rejected, 0);
        assert!(receipt.worker_target <= 2);

        let status = service
            .wait_for_swarm(&receipt.swarm_id, Duration::from_secs(60))
            .expect("swarm status");
        assert_eq!(status.completed, 1000, "all agents must finish: {status:?}");
        assert_eq!(status.failed, 0);
        assert_eq!(status.in_flight, 0);
        assert!(status.completed_at_ms.is_some());
        // Every receipt is a real, fully-stepped execution in the shared store.
        let executions = service.list_executions();
        assert_eq!(executions.len(), 1000);
        assert!(executions.iter().all(|r| r.status == "completed"));
        assert!(executions.iter().all(|r| r.steps.len() == 2));
    }

    #[test]
    fn cost_budget_exhaustion_is_preserved_under_swarm_scheduling() {
        let service = two_lane_service();
        // summarizer costs 2; budget 2 leaves nothing for the counter.
        let manifest = service
            .create_agent(manifest_request(vec!["summarizer", "word_counter"], 2))
            .expect("manifest");
        let receipt = service
            .spawn_swarm(
                SpawnSwarmRequest {
                    agents: vec![SwarmAgentSpec {
                        manifest_id: manifest.manifest_id.clone(),
                        input: serde_json::json!("budget test"),
                        requested_tools: Vec::new(),
                        replicate: 10,
                    }],
                },
                "tester",
            )
            .expect("swarm receipt");
        let status = service
            .wait_for_swarm(&receipt.swarm_id, Duration::from_secs(30))
            .expect("swarm status");
        assert_eq!(status.completed_with_warnings, 10);
        for execution_id in &receipt.execution_ids {
            let receipt = service.get_execution(execution_id).expect("receipt");
            assert_eq!(receipt.status, "completed_with_warnings");
            assert_eq!(receipt.steps[1].status, "skipped_cost_budget_exhausted");
        }
    }

    #[test]
    fn memory_envelope_rejects_when_budget_is_exhausted() {
        let service = AgentRuntimeService::new().with_swarm_config(SwarmConfig {
            max_worker_threads: 2,
            cores_override: Some(2),
            memory_budget_bytes: 1,
            ..SwarmConfig::default()
        });
        let manifest = service
            .create_agent(manifest_request(vec!["summarizer"], 10))
            .expect("manifest");
        let receipt = service
            .spawn_swarm(
                SpawnSwarmRequest {
                    agents: vec![SwarmAgentSpec {
                        manifest_id: manifest.manifest_id,
                        input: serde_json::json!("x"),
                        requested_tools: Vec::new(),
                        replicate: 5,
                    }],
                },
                "tester",
            )
            .expect("swarm receipt");
        assert_eq!(receipt.admitted, 0);
        assert_eq!(receipt.rejected, 5);
        // An empty swarm is born completed; waiting must not hang.
        let status = service
            .wait_for_swarm(&receipt.swarm_id, Duration::from_secs(5))
            .expect("swarm status");
        assert!(status.completed_at_ms.is_some());
    }

    #[test]
    fn swarm_rejects_tools_outside_the_allowlist() {
        let service = two_lane_service();
        let manifest = service
            .create_agent(manifest_request(vec!["summarizer"], 10))
            .expect("manifest");
        let error = service
            .spawn_swarm(
                SpawnSwarmRequest {
                    agents: vec![SwarmAgentSpec {
                        manifest_id: manifest.manifest_id,
                        input: serde_json::json!("x"),
                        requested_tools: vec!["file_writer".into()],
                        replicate: 1,
                    }],
                },
                "tester",
            )
            .expect_err("disallowed tool must be rejected");
        assert!(error.to_string().contains("not allowed"));
    }
}

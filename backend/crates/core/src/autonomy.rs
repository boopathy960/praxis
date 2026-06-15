// ─────────────────────────────────────────────────────────────
// Autonomy Worker — continuous, self-running agent execution
// ─────────────────────────────────────────────────────────────
// Turns the per-request agent runtime into a genuinely autonomous worker:
//
//   * Objectives are enqueued and persisted as jobs.
//   * A background tick (driven by a worker thread in the server, or manually
//     via the API) pulls the next queued job, fabricates a tailored agent for
//     it, and runs an execution — which can call the deep_research engine,
//     compose custom tools, and read/write its jailed workspace.
//   * Results and failures are recorded so progress survives across ticks and
//     is queryable.
//
// The worker drives the same governed agent runtime as the request path, so
// every safety gate (sandbox, jailed workspace, forbidden-capability blocks)
// still applies — autonomy changes *when* work runs, not *what is allowed*.

use std::collections::{BTreeMap, VecDeque};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::agent_runtime::{AgentRuntimeService, CreateAgentExecutionRequest, FabricateAgentRequest};
use crate::chronicle::{ChronicleService, EpisodeKind, RecordEpisodeRequest};
use crate::common::{AppError, TenantScope, new_id, now_ms};

/// Upper bound on jobs processed in a single tick, so one tick can't run away.
const MAX_JOBS_PER_TICK: usize = 8;
/// Total queued+history cap to bound memory.
const MAX_JOBS: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyJob {
    pub job_id: String,
    pub tenant_scope: TenantScope,
    pub objective: String,
    pub status: JobStatus,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub execution_id: Option<String>,
    #[serde(default)]
    pub tools_used: Vec<String>,
    #[serde(default)]
    pub result_summary: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    pub attempts: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnqueueJobRequest {
    pub objective: String,
    #[serde(default = "default_scope")]
    pub tenant_scope: TenantScope,
}

fn default_scope() -> TenantScope {
    TenantScope::Global
}

#[derive(Debug, Clone, Serialize)]
pub struct AutonomyStats {
    pub total_jobs: usize,
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TickReport {
    pub processed: usize,
    pub jobs: Vec<AutonomyJob>,
}

#[derive(Default)]
struct AutonomyStore {
    jobs: BTreeMap<String, AutonomyJob>,
    queue: VecDeque<String>,
}

#[derive(Clone)]
pub struct AutonomyService {
    store: Shared<AutonomyStore>,
    agent_runtime: AgentRuntimeService,
    chronicle: Option<ChronicleService>,
}

impl AutonomyService {
    #[must_use]
    pub fn new(agent_runtime: AgentRuntimeService) -> Self {
        Self {
            store: Shared::new(RwLock::new(AutonomyStore::default())),
            agent_runtime,
            chronicle: None,
        }
    }

    /// Wires the chronicle in so every finished job is remembered with
    /// provenance — memory accrues with zero user effort.
    #[must_use]
    pub fn with_chronicle(mut self, chronicle: ChronicleService) -> Self {
        self.chronicle = Some(chronicle);
        self
    }

    pub fn enqueue(&self, request: EnqueueJobRequest) -> Result<AutonomyJob, AppError> {
        let objective = request.objective.trim();
        if objective.is_empty() {
            return Err(AppError::Validation(
                "autonomy objective must not be empty".into(),
            ));
        }
        if objective.len() > 2_048 {
            return Err(AppError::Validation("autonomy objective is too long".into()));
        }
        let mut store = self.store.write();
        if store.jobs.len() >= MAX_JOBS {
            return Err(AppError::Validation(format!(
                "autonomy job cap of {MAX_JOBS} reached"
            )));
        }
        let now = now_ms();
        let job = AutonomyJob {
            job_id: new_id("job"),
            tenant_scope: request.tenant_scope,
            objective: objective.to_string(),
            status: JobStatus::Queued,
            agent_id: None,
            execution_id: None,
            tools_used: Vec::new(),
            result_summary: None,
            error: None,
            attempts: 0,
            created_at_ms: now,
            updated_at_ms: now,
        };
        store.jobs.insert(job.job_id.clone(), job.clone());
        store.queue.push_back(job.job_id.clone());
        Ok(job)
    }

    #[must_use]
    pub fn list_jobs(&self) -> Vec<AutonomyJob> {
        let mut jobs: Vec<_> = self.store.read().jobs.values().cloned().collect();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at_ms));
        jobs
    }

    pub fn get_job(&self, job_id: &str) -> Result<AutonomyJob, AppError> {
        self.store
            .read()
            .jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("autonomy job {job_id}")))
    }

    #[must_use]
    pub fn stats(&self) -> AutonomyStats {
        let store = self.store.read();
        let mut stats = AutonomyStats {
            total_jobs: store.jobs.len(),
            queued: 0,
            running: 0,
            completed: 0,
            failed: 0,
        };
        for job in store.jobs.values() {
            match job.status {
                JobStatus::Queued => stats.queued += 1,
                JobStatus::Running => stats.running += 1,
                JobStatus::Completed => stats.completed += 1,
                JobStatus::Failed => stats.failed += 1,
            }
        }
        stats
    }

    /// Processes up to `MAX_JOBS_PER_TICK` queued jobs. Returns the jobs touched.
    pub fn tick(&self, max: usize) -> TickReport {
        let max = max.clamp(1, MAX_JOBS_PER_TICK);
        let mut processed = Vec::new();
        for _ in 0..max {
            match self.process_next() {
                Some(job) => processed.push(job),
                None => break,
            }
        }
        TickReport {
            processed: processed.len(),
            jobs: processed,
        }
    }

    /// Atomically claims the next queued job and runs it to completion.
    fn process_next(&self) -> Option<AutonomyJob> {
        // Claim under the lock, then release before doing heavy work.
        let job_id = {
            let mut store = self.store.write();
            let job_id = store.queue.pop_front()?;
            if let Some(job) = store.jobs.get_mut(&job_id) {
                job.status = JobStatus::Running;
                job.attempts += 1;
                job.updated_at_ms = now_ms();
            }
            job_id
        };

        let (objective, tenant_scope) = {
            let store = self.store.read();
            let job = store.jobs.get(&job_id)?;
            (job.objective.clone(), job.tenant_scope.clone())
        };

        let outcome = self.run_agent_objective(&job_id, &objective, tenant_scope);

        let mut store = self.store.write();
        let job = store.jobs.get_mut(&job_id)?;
        match outcome {
            Ok(result) => {
                job.status = JobStatus::Completed;
                job.agent_id = Some(result.agent_id);
                job.execution_id = result.execution_id;
                job.tools_used = result.tools_used;
                job.result_summary = Some(result.summary);
                job.error = None;
            }
            Err(error) => {
                job.status = JobStatus::Failed;
                job.error = Some(error.to_string());
            }
        }
        job.updated_at_ms = now_ms();
        let finished = store.jobs.get(&job_id).cloned();
        drop(store);

        // Remember the outcome. Recording is best-effort: a full or failing
        // chronicle must never fail the job itself.
        if let (Some(chronicle), Some(job)) = (self.chronicle.as_ref(), finished.as_ref()) {
            let failed = job.status == JobStatus::Failed;
            let detail = if failed {
                job.error.clone().unwrap_or_default()
            } else {
                job.result_summary.clone().unwrap_or_default()
            };
            let mut tags = vec!["autonomy".to_string()];
            if failed {
                tags.push("failure".into());
            }
            let _ = chronicle.record(RecordEpisodeRequest {
                kind: EpisodeKind::Event,
                content: format!("autonomy job '{}': {detail}", job.objective),
                rationale: None,
                source: Some("autonomy".into()),
                source_ref: Some(job.job_id.clone()),
                tags,
                importance: Some(if failed { 0.7 } else { 0.5 }),
                due_at_ms: None,
                tenant_scope: job.tenant_scope.clone(),
            });
        }
        finished
    }

    /// Fabricates a tailored agent for the objective and runs one execution.
    /// The agent is attributed to the job so each spawned worker has its own
    /// identity in the runtime and audit trail.
    fn run_agent_objective(
        &self,
        job_id: &str,
        objective: &str,
        tenant_scope: TenantScope,
    ) -> Result<ObjectiveOutcome, AppError> {
        let fabrication = self.agent_runtime.fabricate_for_objective(FabricateAgentRequest {
            tenant_scope,
            objective: objective.to_string(),
        })?;
        // Empty requested_tools => the runtime uses the full fabricated
        // allowlist (deep_research, custom blueprint tool, file tools, ...).
        let receipt = self.agent_runtime.create_execution(
            &fabrication.agent.manifest_id,
            &format!("autonomy:{job_id}"),
            CreateAgentExecutionRequest {
                input: serde_json::json!({ "objective": objective }),
                requested_tools: Vec::new(),
                resource_limits: BTreeMap::new(),
            },
        )?;
        let tools_used = receipt.steps.iter().map(|step| step.tool.clone()).collect();
        let completed = receipt
            .steps
            .iter()
            .filter(|step| step.status == "completed")
            .count();
        let summary = format!(
            "status={}, {}/{} steps completed; tools: {}",
            receipt.status,
            completed,
            receipt.steps.len(),
            receipt
                .steps
                .iter()
                .map(|step| step.tool.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ok(ObjectiveOutcome {
            agent_id: fabrication.agent.manifest_id,
            execution_id: Some(receipt.execution_id),
            tools_used,
            summary,
        })
    }
}

struct ObjectiveOutcome {
    agent_id: String,
    execution_id: Option<String>,
    tools_used: Vec<String>,
    summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> AutonomyService {
        let research = crate::search_intelligence::SearchIntelligenceService::new();
        let runtime = AgentRuntimeService::new().with_research(research);
        AutonomyService::new(runtime)
    }

    #[test]
    fn enqueue_validates_objective() {
        let service = service();
        assert!(
            service
                .enqueue(EnqueueJobRequest {
                    objective: "   ".into(),
                    tenant_scope: TenantScope::Global,
                })
                .is_err()
        );
    }

    #[test]
    fn tick_processes_queued_job_through_fabricated_agent() {
        let service = service();
        let job = service
            .enqueue(EnqueueJobRequest {
                objective: "research compliance evidence and write a findings report file".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("enqueue");
        assert_eq!(job.status, JobStatus::Queued);

        let report = service.tick(4);
        assert_eq!(report.processed, 1);

        let done = service.get_job(&job.job_id).expect("job");
        assert_eq!(done.status, JobStatus::Completed);
        assert!(done.agent_id.is_some());
        assert!(done.execution_id.is_some());
        // The fabricated agent actually used the deep_research engine.
        assert!(
            done.tools_used.iter().any(|tool| tool.contains("deep_research")),
            "expected deep_research in tools, got {:?}",
            done.tools_used
        );
        assert!(done.result_summary.is_some());
    }

    #[test]
    fn tick_on_empty_queue_is_a_noop() {
        let service = service();
        let report = service.tick(4);
        assert_eq!(report.processed, 0);
    }

    #[test]
    fn stats_track_job_lifecycle() {
        let service = service();
        service
            .enqueue(EnqueueJobRequest {
                objective: "summarize the onboarding workflow".into(),
                tenant_scope: TenantScope::Global,
            })
            .expect("enqueue");
        let before = service.stats();
        assert_eq!(before.queued, 1);
        assert_eq!(before.completed, 0);
        service.tick(4);
        let after = service.stats();
        assert_eq!(after.queued, 0);
        assert_eq!(after.completed, 1);
        assert_eq!(after.total_jobs, 1);
    }

    #[test]
    fn each_job_is_processed_once() {
        let service = service();
        for index in 0..3 {
            service
                .enqueue(EnqueueJobRequest {
                    objective: format!("analyze dataset {index} and summarize"),
                    tenant_scope: TenantScope::Global,
                })
                .expect("enqueue");
        }
        let first = service.tick(8);
        assert_eq!(first.processed, 3);
        // Nothing left to do on the next tick.
        let second = service.tick(8);
        assert_eq!(second.processed, 0);
    }
}

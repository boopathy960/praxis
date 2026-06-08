// Job Scheduler — Port of backend/agents/scheduler.py
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub id: String,
    pub name: String,
    pub interval: Duration,
    pub last_run: Option<Instant>,
    pub run_count: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJobStatus {
    pub id: String,
    pub name: String,
    pub interval_secs: u64,
    pub run_count: u64,
    pub enabled: bool,
    pub due: bool,
    pub seconds_until_due: u64,
}

pub struct JobScheduler {
    jobs: HashMap<String, ScheduledJob>,
    next_id: u64,
}
impl JobScheduler {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            next_id: 0,
        }
    }
    pub fn schedule(&mut self, name: &str, interval: Duration) -> String {
        let id = format!("job_{}", self.next_id);
        self.next_id += 1;
        self.jobs.insert(
            id.clone(),
            ScheduledJob {
                id: id.clone(),
                name: name.to_string(),
                interval,
                last_run: None,
                run_count: 0,
                enabled: true,
            },
        );
        id
    }
    pub fn due_jobs(&self) -> Vec<&ScheduledJob> {
        self.jobs
            .values()
            .filter(|j| {
                j.enabled
                    && match j.last_run {
                        None => true,
                        Some(last) => last.elapsed() >= j.interval,
                    }
            })
            .collect()
    }
    pub fn due_job_ids(&self) -> Vec<String> {
        self.due_jobs()
            .into_iter()
            .map(|job| job.id.clone())
            .collect()
    }
    pub fn mark_run(&mut self, id: &str) {
        if let Some(j) = self.jobs.get_mut(id) {
            j.last_run = Some(Instant::now());
            j.run_count += 1;
        }
    }
    pub fn disable(&mut self, id: &str) {
        if let Some(j) = self.jobs.get_mut(id) {
            j.enabled = false;
        }
    }
    pub fn enable(&mut self, id: &str) {
        if let Some(j) = self.jobs.get_mut(id) {
            j.enabled = true;
        }
    }
    pub fn total_jobs(&self) -> usize {
        self.jobs.len()
    }
    pub fn job_statuses(&self) -> Vec<ScheduledJobStatus> {
        let mut statuses = self
            .jobs
            .values()
            .map(|job| {
                let (due, seconds_until_due) = match job.last_run {
                    None => (true, 0),
                    Some(last) => {
                        let elapsed = last.elapsed();
                        if elapsed >= job.interval {
                            (true, 0)
                        } else {
                            (false, (job.interval - elapsed).as_secs())
                        }
                    }
                };

                ScheduledJobStatus {
                    id: job.id.clone(),
                    name: job.name.clone(),
                    interval_secs: job.interval.as_secs(),
                    run_count: job.run_count,
                    enabled: job.enabled,
                    due,
                    seconds_until_due,
                }
            })
            .collect::<Vec<_>>();
        statuses.sort_by(|left, right| left.name.cmp(&right.name));
        statuses
    }
}
impl Default for JobScheduler {
    fn default() -> Self {
        Self::new()
    }
}

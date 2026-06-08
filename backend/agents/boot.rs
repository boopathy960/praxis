use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootJobRequest {
    pub agent_id: String,
    pub workspace_dir: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootJob {
    pub job_id: String,
    pub agent_id: String,
    pub workspace_dir: String,
    pub session_key: String,
    pub prompt: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootServiceSummary {
    pub enabled: bool,
    pub queued_jobs: usize,
    pub recent_jobs: Vec<BootJob>,
}

pub struct OpenClawBootService {
    enabled: bool,
    jobs: Arc<Mutex<Vec<BootJob>>>,
}

impl OpenClawBootService {
    #[must_use]
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn queue_boot_job(&self, request: BootJobRequest, session_key: String) -> BootJob {
        let prompt = build_boot_prompt(&request.content);
        let job = BootJob {
            job_id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
            agent_id: request.agent_id,
            workspace_dir: request.workspace_dir,
            session_key,
            prompt,
            status: if self.enabled { "queued" } else { "disabled" }.into(),
            created_at: Utc::now(),
        };
        let mut jobs = self.jobs.lock().expect("boot jobs mutex poisoned");
        jobs.push(job.clone());
        if jobs.len() > 128 {
            let overflow = jobs.len() - 128;
            jobs.drain(0..overflow);
        }
        job
    }

    #[must_use]
    pub fn summary(&self) -> BootServiceSummary {
        let jobs = self.jobs.lock().expect("boot jobs mutex poisoned");
        let recent_jobs = jobs.iter().rev().take(20).cloned().collect::<Vec<_>>();
        BootServiceSummary {
            enabled: self.enabled,
            queued_jobs: jobs.iter().filter(|job| job.status == "queued").count(),
            recent_jobs,
        }
    }
}

fn build_boot_prompt(content: &str) -> String {
    [
        "You are running a boot check. Follow BOOT.md instructions exactly.",
        "",
        "BOOT.md:",
        content,
        "",
        "If BOOT.md asks you to send a message, use the message tool with an explicit target.",
        "If nothing needs attention, return only the silent acknowledgement token.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_service_queues_jobs() {
        let service = OpenClawBootService::new(true);
        let job = service.queue_boot_job(
            BootJobRequest {
                agent_id: "default".into(),
                workspace_dir: "workspace".into(),
                content: "Run startup checks".into(),
            },
            "agent/default/main".into(),
        );
        assert_eq!(job.status, "queued");
        assert!(service.summary().queued_jobs >= 1);
    }
}

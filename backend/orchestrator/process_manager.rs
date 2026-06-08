// Process Manager — Port of backend/agents/process_manager.py
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone)]
pub struct ManagedProcess {
    pub id: String,
    pub name: String,
    pub status: ProcessStatus,
    pub started_at: Instant,
    pub output: String,
}
pub struct ProcessManager {
    processes: HashMap<String, ManagedProcess>,
    next_id: u64,
    max_concurrent: usize,
    completed_count: u64,
}
impl ProcessManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            processes: HashMap::new(),
            next_id: 0,
            max_concurrent,
            completed_count: 0,
        }
    }
    pub fn spawn(&mut self, name: &str) -> Option<String> {
        let active = self
            .processes
            .values()
            .filter(|p| p.status == ProcessStatus::Running)
            .count();
        if active >= self.max_concurrent {
            return None;
        }
        let id = format!("proc_{}", self.next_id);
        self.next_id += 1;
        self.processes.insert(
            id.clone(),
            ManagedProcess {
                id: id.clone(),
                name: name.to_string(),
                status: ProcessStatus::Running,
                started_at: Instant::now(),
                output: String::new(),
            },
        );
        Some(id)
    }
    pub fn complete(&mut self, id: &str, output: &str, success: bool) {
        if let Some(p) = self.processes.get_mut(id) {
            p.status = if success {
                ProcessStatus::Completed
            } else {
                ProcessStatus::Failed
            };
            p.output = output.to_string();
            self.completed_count += 1;
        }
    }
    pub fn cancel(&mut self, id: &str) {
        if let Some(p) = self.processes.get_mut(id) {
            p.status = ProcessStatus::Cancelled;
        }
    }
    pub fn active_count(&self) -> usize {
        self.processes
            .values()
            .filter(|p| p.status == ProcessStatus::Running)
            .count()
    }
    pub fn get(&self, id: &str) -> Option<&ManagedProcess> {
        self.processes.get(id)
    }
}
impl Default for ProcessManager {
    fn default() -> Self {
        Self::new(10)
    }
}

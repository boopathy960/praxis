// ─────────────────────────────────────────────────────────────
// Hierarchical Task Planner — Goal Decomposition Engine
// ─────────────────────────────────────────────────────────────
// Breaks complex goals into executable action plans with
// dependency tracking, parallel execution, and replanning.

use crate::crypto::hash::sha3_256_hex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Blocked,
    Replanning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskPriority {
    Critical,
    High,
    Normal,
    Low,
    Background,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub depends_on: Vec<String>,
    pub subtasks: Vec<String>,
    pub parent: Option<String>,
    pub depth: usize,
    pub estimated_cost: f64,
    pub actual_cost: f64,
    pub confidence: f64,
    pub retry_count: u32,
    pub max_retries: u32,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

impl PlanTask {
    pub fn new(name: &str, description: &str, depth: usize, parent: Option<String>) -> Self {
        Self {
            id: sha3_256_hex(
                format!(
                    "task:{}:{}",
                    name,
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                )
                .as_bytes(),
            )[..16]
                .to_string(),
            name: name.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            priority: TaskPriority::Normal,
            depends_on: Vec::new(),
            subtasks: Vec::new(),
            parent,
            depth,
            estimated_cost: 0.0,
            actual_cost: 0.0,
            confidence: 0.5,
            retry_count: 0,
            max_retries: 3,
            result: None,
            error: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            completed_at: None,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status, TaskStatus::Completed | TaskStatus::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: String,
    pub goal: String,
    pub tasks: Vec<String>,
    pub execution_order: Vec<Vec<String>>, // Parallel waves
    pub total_estimated_cost: f64,
    pub progress: f64,
    pub status: TaskStatus,
}

#[allow(dead_code)]
pub struct HierarchicalPlanner {
    tasks: HashMap<String, PlanTask>,
    plans: HashMap<String, ExecutionPlan>,
    max_depth: usize,
    max_tasks_per_plan: usize,
    total_plans: u64,
    total_completed: u64,
}

impl HierarchicalPlanner {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            plans: HashMap::new(),
            max_depth: 6,
            max_tasks_per_plan: 50,
            total_plans: 0,
            total_completed: 0,
        }
    }

    /// Create an execution plan for a high-level goal.
    pub fn plan(&mut self, goal: &str, context: &serde_json::Value) -> ExecutionPlan {
        self.total_plans += 1;
        let plan_id = sha3_256_hex(
            format!("plan:{}:{}", goal, chrono::Utc::now().timestamp_millis()).as_bytes(),
        )[..16]
            .to_string();

        // Decompose goal into tasks
        let root_tasks = self.decompose_goal(goal, context, 0, None);
        let all_task_ids: Vec<String> = root_tasks.iter().map(|t| t.id.clone()).collect();

        // Store tasks
        for task in root_tasks {
            self.tasks.insert(task.id.clone(), task);
        }

        // Compute execution order (topological sort into parallel waves)
        let execution_order = self.compute_execution_order(&all_task_ids);

        let total_cost: f64 = all_task_ids
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .map(|t| t.estimated_cost)
            .sum();

        let plan = ExecutionPlan {
            id: plan_id.clone(),
            goal: goal.to_string(),
            tasks: all_task_ids,
            execution_order,
            total_estimated_cost: total_cost,
            progress: 0.0,
            status: TaskStatus::Ready,
        };

        self.plans.insert(plan_id, plan.clone());
        plan
    }

    /// Decompose a goal into component tasks.
    fn decompose_goal(
        &mut self,
        goal: &str,
        _context: &serde_json::Value,
        depth: usize,
        parent: Option<String>,
    ) -> Vec<PlanTask> {
        if depth >= self.max_depth {
            let mut leaf = PlanTask::new(goal, "Atomic action", depth, parent);
            leaf.estimated_cost = 0.1;
            leaf.confidence = 0.8;
            return vec![leaf];
        }

        let lower = goal.to_lowercase();
        let mut tasks = Vec::new();

        // Analyze goal complexity and decompose
        if lower.contains("and") || lower.contains(",") || lower.contains("then") {
            // Sequential compound goal
            // Sequential compound goal — split on commas, then on " and "
            let parts: Vec<&str> = goal
                .split(',')
                .flat_map(|segment| segment.split(" and "))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .take(4)
                .collect();

            let mut prev_id: Option<String> = None;
            for (i, part) in parts.iter().enumerate() {
                let mut task =
                    PlanTask::new(&format!("Step {}", i + 1), part, depth, parent.clone());
                task.estimated_cost = 0.2 + (i as f64 * 0.1);
                if let Some(ref dep) = prev_id {
                    task.depends_on.push(dep.clone());
                }
                prev_id = Some(task.id.clone());
                tasks.push(task);
            }
        } else if lower.contains("analyze")
            || lower.contains("research")
            || lower.contains("investigate")
        {
            // Analysis: gather → process → synthesize
            let gather = PlanTask::new(
                "Gather Data",
                &format!("Collect data for: {}", goal),
                depth + 1,
                parent.clone(),
            );
            let mut process = PlanTask::new(
                "Process",
                "Analyze and score collected data",
                depth + 1,
                parent.clone(),
            );
            process.depends_on.push(gather.id.clone());
            let mut synthesize = PlanTask::new(
                "Synthesize",
                "Produce final analysis report",
                depth + 1,
                parent.clone(),
            );
            synthesize.depends_on.push(process.id.clone());

            tasks.extend([gather, process, synthesize]);
        } else if lower.contains("build") || lower.contains("create") || lower.contains("implement")
        {
            // Build: design → implement → test → deploy
            let design = PlanTask::new(
                "Design",
                &format!("Design solution for: {}", goal),
                depth + 1,
                parent.clone(),
            );
            let mut implement = PlanTask::new(
                "Implement",
                "Build the designed solution",
                depth + 1,
                parent.clone(),
            );
            implement.depends_on.push(design.id.clone());
            let mut test = PlanTask::new(
                "Test",
                "Verify implementation correctness",
                depth + 1,
                parent.clone(),
            );
            test.depends_on.push(implement.id.clone());
            let mut deploy = PlanTask::new(
                "Deploy",
                "Ship the tested solution",
                depth + 1,
                parent.clone(),
            );
            deploy.depends_on.push(test.id.clone());

            tasks.extend([design, implement, test, deploy]);
        } else {
            // Default: just create a single task
            let mut task = PlanTask::new(goal, "Execute goal directly", depth, parent);
            task.estimated_cost = 0.3;
            tasks.push(task);
        }

        // Assign costs
        for task in tasks.iter_mut() {
            task.estimated_cost = 0.1 * (1.0 + depth as f64 * 0.5);
        }

        tasks
    }

    /// Topological sort into parallel execution waves.
    fn compute_execution_order(&self, task_ids: &[String]) -> Vec<Vec<String>> {
        let mut waves: Vec<Vec<String>> = Vec::new();
        let mut completed: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut remaining: Vec<String> = task_ids.to_vec();

        for _ in 0..20 {
            if remaining.is_empty() {
                break;
            }

            let mut wave = Vec::new();
            let mut next_remaining = Vec::new();

            for id in &remaining {
                if let Some(task) = self.tasks.get(id) {
                    let deps_met = task.depends_on.iter().all(|d| completed.contains(d));
                    if deps_met {
                        wave.push(id.clone());
                    } else {
                        next_remaining.push(id.clone());
                    }
                }
            }

            if wave.is_empty() {
                // Circular dependency — force remaining into final wave
                waves.push(remaining);
                break;
            }

            completed.extend(wave.iter().cloned());
            waves.push(wave);
            remaining = next_remaining;
        }

        waves
    }

    /// Mark a task as completed.
    pub fn complete_task(&mut self, task_id: &str, result: serde_json::Value) -> bool {
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Completed;
            task.result = Some(result);
            task.completed_at = Some(chrono::Utc::now().timestamp_millis());
            self.update_plan_progress(task_id);
            true
        } else {
            false
        }
    }

    /// Mark a task as failed and attempt replan.
    pub fn fail_task(&mut self, task_id: &str, error: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(task_id) {
            if task.retry_count < task.max_retries {
                task.retry_count += 1;
                task.status = TaskStatus::Replanning;
                task.error = Some(error.to_string());
                true
            } else {
                task.status = TaskStatus::Failed;
                task.error = Some(error.to_string());
                false
            }
        } else {
            false
        }
    }

    fn update_plan_progress(&mut self, _task_id: &str) {
        for plan in self.plans.values_mut() {
            let total = plan.tasks.len() as f64;
            if total == 0.0 {
                continue;
            }
            let completed = plan
                .tasks
                .iter()
                .filter(|id| {
                    self.tasks
                        .get(*id)
                        .map(|t| t.status == TaskStatus::Completed)
                        .unwrap_or(false)
                })
                .count() as f64;
            plan.progress = completed / total;

            if (plan.progress - 1.0).abs() < f64::EPSILON {
                plan.status = TaskStatus::Completed;
                self.total_completed += 1;
            }
        }
    }

    /// Get ready-to-execute tasks (all dependencies met).
    pub fn get_ready_tasks(&self) -> Vec<&PlanTask> {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::Ready)
            .filter(|t| {
                t.depends_on.iter().all(|dep| {
                    self.tasks
                        .get(dep)
                        .map(|d| d.status == TaskStatus::Completed)
                        .unwrap_or(true)
                })
            })
            .collect()
    }

    pub fn task_snapshot(&self, task_id: &str) -> Option<PlanTask> {
        self.tasks.get(task_id).cloned()
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_plans": self.total_plans,
            "completed_plans": self.total_completed,
            "active_tasks": self.tasks.values().filter(|t| !t.is_terminal()).count(),
            "completed_tasks": self.tasks.values().filter(|t| t.status == TaskStatus::Completed).count(),
            "failed_tasks": self.tasks.values().filter(|t| t.status == TaskStatus::Failed).count(),
        })
    }
}

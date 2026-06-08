// ─────────────────────────────────────────────────────────────
// Autonomous Execution Engine — DAG Goal Decomposition
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/autonomous_execution_engine.py

use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Retrying,
    RolledBack,
}

#[derive(Debug, Clone)]
pub struct ExecutionTask {
    pub id: String,
    pub name: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
    pub result: Option<String>,
    pub retries: usize,
    pub max_retries: usize,
    pub priority: f64,
}

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub goal: String,
    pub tasks: Vec<ExecutionTask>,
    pub execution_order: Vec<String>,
    pub total_duration_ms: f64,
    pub success: bool,
}

/// Autonomous Execution Engine — DAG-based goal decomposition with retry/rollback.
pub struct AutonomousExecutionEngine {
    max_retries: usize,
    execution_history: Vec<ExecutionPlan>,
    next_task_id: u64,
}

impl AutonomousExecutionEngine {
    pub fn new() -> Self {
        Self {
            max_retries: 3,
            execution_history: Vec::new(),
            next_task_id: 0,
        }
    }

    /// Decompose a goal into a DAG of tasks.
    pub fn plan(&mut self, goal: &str) -> ExecutionPlan {
        let tasks = self.decompose_goal(goal);
        let execution_order = self.topological_sort(&tasks);

        ExecutionPlan {
            goal: goal.to_string(),
            tasks,
            execution_order,
            total_duration_ms: 0.0,
            success: false,
        }
    }

    /// Execute a plan by running tasks in topological order.
    pub fn execute<F>(&mut self, plan: &mut ExecutionPlan, executor: F) -> bool
    where
        F: Fn(&str, &str) -> Result<String, String>,
    {
        let start = Instant::now();
        let order = plan.execution_order.clone();

        for task_id in &order {
            let task_idx = plan.tasks.iter().position(|t| &t.id == task_id);
            if let Some(idx) = task_idx {
                // Check dependencies are completed
                let deps_met = plan.tasks[idx].dependencies.iter().all(|dep_id| {
                    plan.tasks
                        .iter()
                        .any(|t| &t.id == dep_id && t.status == TaskStatus::Completed)
                });

                if !deps_met {
                    plan.tasks[idx].status = TaskStatus::Failed;
                    plan.tasks[idx].result = Some("Dependencies not met".into());
                    continue;
                }

                plan.tasks[idx].status = TaskStatus::Running;

                // Execute with retry
                let mut success = false;
                for retry in 0..=plan.tasks[idx].max_retries {
                    plan.tasks[idx].retries = retry;
                    match executor(&plan.tasks[idx].name, &plan.tasks[idx].description) {
                        Ok(result) => {
                            plan.tasks[idx].status = TaskStatus::Completed;
                            plan.tasks[idx].result = Some(result);
                            success = true;
                            break;
                        }
                        Err(err) => {
                            if retry < plan.tasks[idx].max_retries {
                                plan.tasks[idx].status = TaskStatus::Retrying;
                            } else {
                                plan.tasks[idx].status = TaskStatus::Failed;
                                plan.tasks[idx].result =
                                    Some(format!("Failed after {} retries: {}", retry + 1, err));
                            }
                        }
                    }
                }

                if !success {
                    // Rollback dependent tasks
                    self.rollback(plan, task_id);
                    break;
                }
            }
        }

        plan.total_duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        plan.success = plan.tasks.iter().all(|t| t.status == TaskStatus::Completed);
        self.execution_history.push(plan.clone());
        plan.success
    }

    /// Rollback tasks that depend on a failed task.
    fn rollback(&self, plan: &mut ExecutionPlan, failed_id: &str) {
        for task in &mut plan.tasks {
            if task.dependencies.contains(&failed_id.to_string())
                && task.status != TaskStatus::Pending
            {
                task.status = TaskStatus::RolledBack;
                task.result = Some(format!("Rolled back due to failure of {}", failed_id));
            }
        }
    }

    /// Decompose a goal into sub-tasks.
    fn decompose_goal(&mut self, goal: &str) -> Vec<ExecutionTask> {
        let goal_lower = goal.to_lowercase();
        let mut tasks = Vec::new();

        // Analysis task (always first)
        let analysis_id = self.new_task_id();
        tasks.push(ExecutionTask {
            id: analysis_id.clone(),
            name: "analyze_requirements".into(),
            description: format!("Analyze: {}", goal),
            dependencies: Vec::new(),
            status: TaskStatus::Pending,
            result: None,
            retries: 0,
            max_retries: self.max_retries,
            priority: 1.0,
        });

        // Domain-specific tasks
        if goal_lower.contains("build")
            || goal_lower.contains("create")
            || goal_lower.contains("implement")
        {
            let design_id = self.new_task_id();
            tasks.push(ExecutionTask {
                id: design_id.clone(),
                name: "design_architecture".into(),
                description: "Design system architecture and interfaces".into(),
                dependencies: vec![analysis_id.clone()],
                status: TaskStatus::Pending,
                result: None,
                retries: 0,
                max_retries: self.max_retries,
                priority: 0.9,
            });

            let impl_id = self.new_task_id();
            tasks.push(ExecutionTask {
                id: impl_id.clone(),
                name: "implement_core".into(),
                description: "Implement core functionality".into(),
                dependencies: vec![design_id.clone()],
                status: TaskStatus::Pending,
                result: None,
                retries: 0,
                max_retries: self.max_retries,
                priority: 0.8,
            });

            let test_id = self.new_task_id();
            tasks.push(ExecutionTask {
                id: test_id.clone(),
                name: "test_and_verify".into(),
                description: "Run tests and verify correctness".into(),
                dependencies: vec![impl_id],
                status: TaskStatus::Pending,
                result: None,
                retries: 0,
                max_retries: self.max_retries,
                priority: 0.7,
            });
        } else {
            let execute_id = self.new_task_id();
            tasks.push(ExecutionTask {
                id: execute_id.clone(),
                name: "execute_task".into(),
                description: format!("Execute: {}", goal),
                dependencies: vec![analysis_id.clone()],
                status: TaskStatus::Pending,
                result: None,
                retries: 0,
                max_retries: self.max_retries,
                priority: 0.8,
            });

            tasks.push(ExecutionTask {
                id: self.new_task_id(),
                name: "verify_result".into(),
                description: "Verify execution result".into(),
                dependencies: vec![execute_id],
                status: TaskStatus::Pending,
                result: None,
                retries: 0,
                max_retries: self.max_retries,
                priority: 0.7,
            });
        }

        tasks
    }

    /// Topological sort of tasks by DAG dependencies.
    fn topological_sort(&self, tasks: &[ExecutionTask]) -> Vec<String> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

        for task in tasks {
            in_degree.entry(task.id.clone()).or_insert(0);
            for dep in &task.dependencies {
                adjacency
                    .entry(dep.clone())
                    .or_default()
                    .push(task.id.clone());
                *in_degree.entry(task.id.clone()).or_insert(0) += 1;
            }
        }

        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();
        queue.sort(); // deterministic order
        let mut order = Vec::new();

        while let Some(current) = queue.pop() {
            order.push(current.clone());
            if let Some(neighbors) = adjacency.get(&current) {
                for next in neighbors {
                    if let Some(deg) = in_degree.get_mut(next) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(next.clone());
                        }
                    }
                }
            }
            queue.sort();
        }

        order
    }

    fn new_task_id(&mut self) -> String {
        let id = format!("task_{}", self.next_task_id);
        self.next_task_id += 1;
        id
    }

    pub fn execution_count(&self) -> usize {
        self.execution_history.len()
    }
}

impl Default for AutonomousExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

// Task Compiler — Port of backend/agents/compiler.py
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CompiledTask {
    pub intent: String,
    pub entities: Vec<String>,
    pub constraints: Vec<String>,
    pub context: HashMap<String, String>,
    pub complexity: f64,
}
pub struct TaskCompiler;
impl TaskCompiler {
    pub fn new() -> Self {
        Self
    }
    pub fn compile(&self, prompt: &str, context: &HashMap<String, String>) -> CompiledTask {
        let prompt_lower = prompt.to_lowercase();
        let intent = if prompt_lower.contains("create") || prompt_lower.contains("build") {
            "create"
        } else if prompt_lower.contains("fix") || prompt_lower.contains("debug") {
            "fix"
        } else if prompt_lower.contains("explain") || prompt_lower.contains("how") {
            "explain"
        } else if prompt_lower.contains("search") || prompt_lower.contains("find") {
            "search"
        } else {
            "general"
        };
        let entities: Vec<String> = prompt
            .split_whitespace()
            .filter(|w| w.len() > 4 && w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false))
            .map(|w| w.to_string())
            .collect();
        let complexity = (prompt.len() as f64 / 100.0).min(1.0);
        CompiledTask {
            intent: intent.to_string(),
            entities,
            constraints: Vec::new(),
            context: context.clone(),
            complexity,
        }
    }
}
impl Default for TaskCompiler {
    fn default() -> Self {
        Self::new()
    }
}

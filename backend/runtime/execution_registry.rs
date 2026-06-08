// Execution Registry — Port of claw-code-main/src/execution_registry.py
use std::collections::HashMap;

pub struct ExecutionRegistry {
    command_executors: HashMap<String, Box<dyn Fn(&str) -> String + Send + Sync>>,
    tool_executors: HashMap<String, Box<dyn Fn(&str) -> String + Send + Sync>>,
}

impl ExecutionRegistry {
    pub fn new() -> Self {
        Self {
            command_executors: HashMap::new(),
            tool_executors: HashMap::new(),
        }
    }

    pub fn register_command<F>(&mut self, name: &str, executor: F)
    where
        F: Fn(&str) -> String + Send + Sync + 'static,
    {
        self.command_executors
            .insert(name.to_string(), Box::new(executor));
    }

    pub fn register_tool<F>(&mut self, name: &str, executor: F)
    where
        F: Fn(&str) -> String + Send + Sync + 'static,
    {
        self.tool_executors
            .insert(name.to_string(), Box::new(executor));
    }

    pub fn execute_command(&self, name: &str, input: &str) -> Option<String> {
        self.command_executors.get(name).map(|f| f(input))
    }

    pub fn execute_tool(&self, name: &str, input: &str) -> Option<String> {
        self.tool_executors.get(name).map(|f| f(input))
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.command_executors.contains_key(name)
    }
    pub fn has_tool(&self, name: &str) -> bool {
        self.tool_executors.contains_key(name)
    }
}

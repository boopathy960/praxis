// Tool Registry — Port of claw-code-main/src/tools.py
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    pub source: String,
}

pub struct RuntimeToolRegistry {
    tools: HashMap<String, ToolEntry>,
}

impl RuntimeToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }
    pub fn register(&mut self, name: &str, desc: &str, source: &str) {
        self.tools.insert(
            name.to_string(),
            ToolEntry {
                name: name.into(),
                description: desc.into(),
                source: source.into(),
            },
        );
    }
    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.get(name)
    }
    pub fn list(&self) -> Vec<&ToolEntry> {
        self.tools.values().collect()
    }
    pub fn count(&self) -> usize {
        self.tools.len()
    }
}
impl Default for RuntimeToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

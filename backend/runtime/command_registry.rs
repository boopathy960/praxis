// Command Registry — Port of claw-code-main/src/commands.py
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub name: String,
    pub description: String,
    pub source: String,
}

pub struct CommandRegistry {
    commands: HashMap<String, CommandEntry>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }
    pub fn register(&mut self, name: &str, desc: &str, source: &str) {
        self.commands.insert(
            name.to_string(),
            CommandEntry {
                name: name.into(),
                description: desc.into(),
                source: source.into(),
            },
        );
    }
    pub fn get(&self, name: &str) -> Option<&CommandEntry> {
        self.commands.get(name)
    }
    pub fn list(&self) -> Vec<&CommandEntry> {
        self.commands.values().collect()
    }
    pub fn count(&self) -> usize {
        self.commands.len()
    }
}
impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

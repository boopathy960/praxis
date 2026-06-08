// Agent Forge — Dynamic Agent Creation — Port of backend/agents/agent_forge.py
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AgentBlueprint {
    pub name: String,
    pub specialization: String,
    pub capabilities: Vec<String>,
    pub max_iterations: usize,
    pub tool_whitelist: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct ForgedAgent {
    pub id: String,
    pub blueprint: AgentBlueprint,
    pub active: bool,
    pub tasks_completed: u64,
}
pub struct AgentForge {
    agents: HashMap<String, ForgedAgent>,
    next_id: u64,
    templates: HashMap<String, AgentBlueprint>,
}
impl AgentForge {
    pub fn new() -> Self {
        let mut templates = HashMap::new();
        templates.insert(
            "researcher".into(),
            AgentBlueprint {
                name: "Researcher".into(),
                specialization: "research".into(),
                capabilities: vec!["web_search".into(), "deep_research".into()],
                max_iterations: 10,
                tool_whitelist: vec!["web_search".into(), "read_url".into()],
            },
        );
        templates.insert(
            "coder".into(),
            AgentBlueprint {
                name: "Coder".into(),
                specialization: "coding".into(),
                capabilities: vec!["code_gen".into(), "code_review".into()],
                max_iterations: 15,
                tool_whitelist: vec!["code_executor".into(), "file_ops".into()],
            },
        );
        templates.insert(
            "analyst".into(),
            AgentBlueprint {
                name: "Analyst".into(),
                specialization: "analysis".into(),
                capabilities: vec!["data_analysis".into(), "visualization".into()],
                max_iterations: 8,
                tool_whitelist: vec!["data_analyzer".into(), "calculator".into()],
            },
        );
        Self {
            agents: HashMap::new(),
            next_id: 0,
            templates,
        }
    }
    pub fn forge(&mut self, template_name: &str) -> Option<String> {
        let blueprint = self.templates.get(template_name)?.clone();
        let id = format!("agent_{}", self.next_id);
        self.next_id += 1;
        self.agents.insert(
            id.clone(),
            ForgedAgent {
                id: id.clone(),
                blueprint,
                active: true,
                tasks_completed: 0,
            },
        );
        Some(id)
    }
    pub fn forge_custom(&mut self, blueprint: AgentBlueprint) -> String {
        let id = format!("agent_{}", self.next_id);
        self.next_id += 1;
        self.agents.insert(
            id.clone(),
            ForgedAgent {
                id: id.clone(),
                blueprint,
                active: true,
                tasks_completed: 0,
            },
        );
        id
    }
    pub fn decommission(&mut self, id: &str) -> bool {
        self.agents
            .get_mut(id)
            .map(|a| {
                a.active = false;
                true
            })
            .unwrap_or(false)
    }
    pub fn active_agents(&self) -> Vec<&ForgedAgent> {
        self.agents.values().filter(|a| a.active).collect()
    }
    pub fn total_agents(&self) -> usize {
        self.agents.len()
    }
}
impl Default for AgentForge {
    fn default() -> Self {
        Self::new()
    }
}

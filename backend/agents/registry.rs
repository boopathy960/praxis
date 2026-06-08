// Agent Registry — lifecycle management
use super::base::{AgentInfo, AgentStats};
use std::collections::HashMap;

pub struct AgentRegistry {
    agents: HashMap<String, AgentInfo>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: &str, agent_type: &str, caps: Vec<String>) {
        self.agents
            .insert(id.to_string(), AgentInfo::new(id, agent_type, caps));
        log::info!("🤖 Agent registered: {} ({})", id, agent_type);
    }

    pub fn get(&self, id: &str) -> Option<&AgentInfo> {
        self.agents.get(id)
    }
    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentInfo> {
        self.agents.get_mut(id)
    }

    pub fn deactivate(&mut self, id: &str) {
        if let Some(a) = self.agents.get_mut(id) {
            a.is_active = false;
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.agents.remove(id).is_some()
    }

    pub fn active_agents(&self) -> Vec<&AgentInfo> {
        self.agents.values().filter(|a| a.is_active).collect()
    }

    pub fn all_stats(&self) -> Vec<AgentStats> {
        self.agents.values().map(|a| a.stats()).collect()
    }

    pub fn count(&self) -> usize {
        self.agents.len()
    }

    pub fn active_count(&self) -> usize {
        self.agents.values().filter(|a| a.is_active).count()
    }
}

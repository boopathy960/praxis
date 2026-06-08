// Agent Accountability Engine
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct AccountabilityRecord {
    pub agent_id: String,
    pub action: String,
    pub timestamp: f64,
    pub outcome: String,
    pub penalty: f64,
}

pub struct AccountabilityEngine {
    records: HashMap<String, Vec<AccountabilityRecord>>,
    penalties: HashMap<String, f64>,
}

impl AccountabilityEngine {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            penalties: HashMap::new(),
        }
    }

    pub fn record(&mut self, agent_id: &str, action: &str, outcome: &str, penalty: f64) {
        let rec = AccountabilityRecord {
            agent_id: agent_id.to_string(),
            action: action.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
            outcome: outcome.to_string(),
            penalty,
        };
        self.records
            .entry(agent_id.to_string())
            .or_default()
            .push(rec);
        *self.penalties.entry(agent_id.to_string()).or_insert(0.0) += penalty;
    }

    pub fn get_penalty(&self, agent_id: &str) -> f64 {
        self.penalties.get(agent_id).copied().unwrap_or(0.0)
    }
    pub fn get_history(&self, agent_id: &str) -> Vec<AccountabilityRecord> {
        self.records.get(agent_id).cloned().unwrap_or_default()
    }
    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({"agents_tracked": self.records.len(), "total_penalties": self.penalties.values().sum::<f64>()})
    }
}

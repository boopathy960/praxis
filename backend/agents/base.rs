// ─────────────────────────────────────────────────────────────
// Agent System — Base Types
// ─────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

/// A solution proposal from an agent: y_{a,t}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProposal {
    pub agent_id: String,
    pub solution: Vec<f64>,
    pub confidence: f64,         // c_{a,t} ∈ [0,1]
    pub resource_footprint: f64, // ρ_{a,t}
    pub timestamp: f64,
    pub metadata: serde_json::Value,
}

/// Agent statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStats {
    pub agent_id: String,
    pub agent_type: String,
    pub total_proposals: u64,
    pub accepted_proposals: u64,
    pub acceptance_rate: f64,
    pub avg_confidence: f64,
    pub avg_resource: f64,
    pub current_divergence: f64,
    pub is_active: bool,
}

/// Agent info for registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub agent_type: String,
    pub capabilities: Vec<String>,
    pub is_active: bool,
    pub total_proposals: u64,
    pub accepted_proposals: u64,
    pub cumulative_confidence: f64,
    pub cumulative_resource: f64,
    pub divergence_history: Vec<f64>,
}

impl AgentInfo {
    pub fn new(id: &str, agent_type: &str, capabilities: Vec<String>) -> Self {
        Self {
            agent_id: id.to_string(),
            agent_type: agent_type.to_string(),
            capabilities,
            is_active: true,
            total_proposals: 0,
            accepted_proposals: 0,
            cumulative_confidence: 0.0,
            cumulative_resource: 0.0,
            divergence_history: Vec::new(),
        }
    }

    pub fn record_proposal(&mut self, confidence: f64, resource: f64) {
        self.total_proposals += 1;
        self.cumulative_confidence += confidence;
        self.cumulative_resource += resource;
    }

    pub fn record_acceptance(&mut self) {
        self.accepted_proposals += 1;
    }

    pub fn record_divergence(&mut self, delta: f64) {
        if self.divergence_history.len() >= 500 {
            self.divergence_history.remove(0);
        }
        self.divergence_history.push(delta);
    }

    pub fn current_divergence(&self) -> f64 {
        self.divergence_history.last().copied().unwrap_or(0.0)
    }

    pub fn avg_confidence(&self) -> f64 {
        if self.total_proposals == 0 {
            0.5
        } else {
            self.cumulative_confidence / self.total_proposals as f64
        }
    }

    pub fn avg_resource(&self) -> f64 {
        if self.total_proposals == 0 {
            1.0
        } else {
            self.cumulative_resource / self.total_proposals as f64
        }
    }

    pub fn stats(&self) -> AgentStats {
        AgentStats {
            agent_id: self.agent_id.clone(),
            agent_type: self.agent_type.clone(),
            total_proposals: self.total_proposals,
            accepted_proposals: self.accepted_proposals,
            acceptance_rate: if self.total_proposals > 0 {
                self.accepted_proposals as f64 / self.total_proposals as f64
            } else {
                0.0
            },
            avg_confidence: self.avg_confidence(),
            avg_resource: self.avg_resource(),
            current_divergence: self.current_divergence(),
            is_active: self.is_active,
        }
    }
}

/// Production-facing description of a built-in specialist agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilityProfile {
    pub agent_id: String,
    pub display_name: String,
    pub agent_type: String,
    pub specialization: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub preferred_tools: Vec<String>,
    pub safety_lane: String,
    pub latency_tier: String,
    pub max_concurrency: u32,
    pub reliability_score: f64,
}

/// Mission-time view of how a tool will be used.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionToolAssignment {
    pub name: String,
    pub purpose: String,
    pub required: bool,
    pub risk_level: String,
}

/// Mission-time assignment of an agent to a user task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionAgentAssignment {
    pub agent_id: String,
    pub display_name: String,
    pub role: String,
    pub reason: String,
    pub priority: u8,
    pub confidence: f64,
    pub tools: Vec<MissionToolAssignment>,
}

/// Coordinated mission plan describing the execution lane for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionAssignmentPlan {
    pub task: String,
    pub summary: String,
    pub execution_mode: String,
    pub privacy_lane: String,
    pub delivery_mode: String,
    pub escalation_policy: String,
    pub estimated_duration_ms: u64,
    pub agents: Vec<MissionAgentAssignment>,
    pub safeguards: Vec<String>,
    pub follow_up_actions: Vec<String>,
}

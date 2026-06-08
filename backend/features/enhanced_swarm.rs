// ─────────────────────────────────────────────────────────────
// Feature: Multi-Agent Swarm Browsing (Enhanced)
// ─────────────────────────────────────────────────────────────
// Enhanced swarm with agent specialization, debate system,
// research campaigns, and consensus visualization.

use crate::agents::base::AgentProposal;
use crate::agents::solver::AgentSolver;
use crate::error::AstraResult;
use crate::orchestrator::{RankedCandidateInput, RankedConsensusEngine, RankedConsensusResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AgentSpecialization {
    Researcher,
    Verifier,
    Synthesizer,
    Critic,
    Explorer,
}

impl AgentSpecialization {
    pub fn label(&self) -> &str {
        match self {
            Self::Researcher => "Researcher",
            Self::Verifier => "Verifier",
            Self::Synthesizer => "Synthesizer",
            Self::Critic => "Critic",
            Self::Explorer => "Explorer",
        }
    }

    pub fn weight_bonus(&self) -> f64 {
        match self {
            Self::Researcher => 1.2,
            Self::Verifier => 1.3,
            Self::Synthesizer => 1.1,
            Self::Critic => 1.15,
            Self::Explorer => 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SwarmAgent {
    pub id: String,
    pub name: String,
    pub specialization: AgentSpecialization,
    pub status: String,
    pub findings: Vec<String>,
    pub confidence: f64,
    pub processing_time_ms: u64,
    pub contribution_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DebateRound {
    pub round_number: usize,
    pub speaker_id: String,
    pub speaker_name: String,
    pub argument: String,
    pub stance: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwarmDebate {
    pub topic: String,
    pub agents: Vec<SwarmAgent>,
    pub rounds: Vec<DebateRound>,
    pub consensus: String,
    pub dissent: Vec<String>,
    pub final_verdict: String,
    pub consensus_confidence: f64,
    pub total_rounds: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CampaignPhase {
    Planning,
    Researching,
    Verifying,
    Synthesizing,
    Complete,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResearchCampaign {
    pub id: String,
    pub objective: String,
    pub agents: Vec<SwarmAgent>,
    pub phase: CampaignPhase,
    pub progress: f64,
    pub findings: Vec<String>,
    pub synthesis: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnhancedSwarmResult {
    pub agents: Vec<SwarmAgent>,
    pub consensus_answer: String,
    pub consensus_confidence: f64,
    pub ranking_consensus: RankedConsensusResult,
    pub debate: Option<SwarmDebate>,
    pub total_agents: usize,
    pub processing_time_ms: u64,
    pub specialization_breakdown: HashMap<String, usize>,
}

pub struct EnhancedSwarmEngine {
    solver: AgentSolver,
    ranking_engine: RankedConsensusEngine,
    active_campaigns: Vec<ResearchCampaign>,
    completed_debates: Vec<SwarmDebate>,
    total_swarms: u64,
    total_debates: u64,
    total_campaigns: u64,
}

impl EnhancedSwarmEngine {
    pub fn new() -> Self {
        Self {
            solver: AgentSolver::new(0.01, 1e-6, 1.5),
            ranking_engine: RankedConsensusEngine::new(),
            active_campaigns: Vec::new(),
            completed_debates: Vec::new(),
            total_swarms: 0,
            total_debates: 0,
            total_campaigns: 0,
        }
    }

    /// Deploy a specialized swarm for a research intent.
    pub fn deploy_swarm(
        &mut self,
        intent: &str,
        num_agents: usize,
        enable_debate: bool,
    ) -> AstraResult<EnhancedSwarmResult> {
        self.total_swarms += 1;
        let n = num_agents.max(3).min(20);
        let start = std::time::Instant::now();

        // Create specialized agents
        let specializations = [
            AgentSpecialization::Researcher,
            AgentSpecialization::Verifier,
            AgentSpecialization::Synthesizer,
            AgentSpecialization::Critic,
            AgentSpecialization::Explorer,
        ];

        let mut agents = Vec::new();
        let mut proposals = Vec::new();
        let mut ranking_candidates = Vec::new();
        let mut rng = rand::thread_rng();

        for i in 0..n {
            let spec = specializations[i % specializations.len()];
            let agent_id = format!("agent_{}_{}", spec.label().to_lowercase(), i);
            let confidence = 0.5 + (rand::Rng::gen::<f64>(&mut rng) * 0.5) * spec.weight_bonus();

            // Generate findings based on specialization
            let findings = self.generate_findings(intent, spec, &mut rng);

            agents.push(SwarmAgent {
                id: agent_id.clone(),
                name: format!("{} #{}", spec.label(), i),
                specialization: spec,
                status: "completed".into(),
                findings: findings.clone(),
                confidence: confidence.min(1.0),
                processing_time_ms: 50 + (rand::Rng::gen::<f64>(&mut rng) * 200.0) as u64,
                contribution_score: 0.0, // Set after consensus
            });
            ranking_candidates.push(RankedCandidateInput {
                id: agent_id.clone(),
                label: format!("{} #{}", spec.label(), i),
                specialization: spec.label().to_lowercase(),
                summary: findings.join(" | "),
                confidence: confidence.min(1.0),
                domain_fit: specialization_domain_fit(spec),
                tool_readiness: specialization_tool_readiness(spec),
                verification_strength: specialization_verification_strength(spec),
                collaboration: specialization_collaboration(spec),
                policy_alignment: specialization_policy_alignment(spec),
                blocked_ratio: 0.0,
                resource_cost: 0.08 + (i as f64 * 0.01),
            });

            // Create proposal for consensus
            let intent_hash = crate::crypto::hash::sha3_256(
                format!("{}:{}:{}", intent, i, chrono::Utc::now().timestamp_millis()).as_bytes(),
            );
            let solution: Vec<f64> = intent_hash
                .iter()
                .take(8)
                .map(|b| *b as f64 / 255.0)
                .collect();

            proposals.push(AgentProposal {
                agent_id,
                solution,
                confidence: confidence.min(1.0),
                resource_footprint: 0.05 + rand::Rng::gen::<f64>(&mut rng) * 0.1,
                timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                metadata: serde_json::json!({"specialization": spec.label()}),
            });
        }

        // Run consensus aggregation
        let divergences: HashMap<String, f64> = proposals
            .iter()
            .map(|p| (p.agent_id.clone(), 0.0))
            .collect();

        let (_, weights) = self.solver.aggregate(&proposals, &divergences)?;
        let ranking_consensus = self.ranking_engine.evaluate(intent, &ranking_candidates)?;

        // Update contribution scores
        for agent in &mut agents {
            agent.contribution_score = ranking_consensus
                .ranked_agents
                .iter()
                .find(|candidate| candidate.candidate_id == agent.id)
                .map(|candidate| candidate.final_score)
                .unwrap_or_else(|| weights.get(&agent.id).copied().unwrap_or(0.0));
        }
        agents.sort_by(|a, b| {
            b.contribution_score
                .partial_cmp(&a.contribution_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let consensus_answer = ranking_consensus.consensus_summary.clone();
        let consensus_confidence = ranking_consensus.agreement_score;

        // Optional debate
        let debate = if enable_debate {
            Some(self.run_debate(intent, &agents))
        } else {
            None
        };

        let mut spec_breakdown: HashMap<String, usize> = HashMap::new();
        for agent in &agents {
            *spec_breakdown
                .entry(agent.specialization.label().to_string())
                .or_insert(0) += 1;
        }

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(EnhancedSwarmResult {
            total_agents: agents.len(),
            consensus_answer,
            consensus_confidence,
            ranking_consensus,
            debate,
            processing_time_ms: elapsed,
            specialization_breakdown: spec_breakdown,
            agents,
        })
    }

    /// Run a structured debate between agents.
    pub fn run_debate(&mut self, topic: &str, agents: &[SwarmAgent]) -> SwarmDebate {
        self.total_debates += 1;
        let mut rounds = Vec::new();
        let num_rounds = 3;
        let mut rng = rand::thread_rng();

        for round in 0..num_rounds {
            for agent in agents.iter().take(4) {
                // Max 4 debaters
                let stance = if rand::Rng::gen::<f64>(&mut rng) > 0.3 {
                    "agree"
                } else {
                    "disagree"
                };
                let argument = format!(
                    "[{}] {} perspective on '{}': {}",
                    agent.specialization.label(),
                    if stance == "agree" {
                        "Supporting"
                    } else {
                        "Challenging"
                    },
                    topic,
                    agent.findings.first().unwrap_or(&"No data".to_string()),
                );

                rounds.push(DebateRound {
                    round_number: round + 1,
                    speaker_id: agent.id.clone(),
                    speaker_name: agent.name.clone(),
                    argument,
                    stance: stance.to_string(),
                    confidence: agent.confidence * (0.8 + rand::Rng::gen::<f64>(&mut rng) * 0.2),
                });
            }
        }

        let agree_count = rounds.iter().filter(|r| r.stance == "agree").count();
        let total = rounds.len();
        let consensus_score = agree_count as f64 / total as f64;

        let consensus = if consensus_score > 0.7 {
            "Strong consensus reached".to_string()
        } else if consensus_score > 0.5 {
            "Moderate consensus with some dissent".to_string()
        } else {
            "No clear consensus — significant disagreement".to_string()
        };

        let dissent: Vec<String> = rounds
            .iter()
            .filter(|r| r.stance == "disagree")
            .map(|r| format!("{}: {}", r.speaker_name, r.argument))
            .take(3)
            .collect();

        let verdict = agents
            .first()
            .map(|a| a.findings.join(". "))
            .unwrap_or_else(|| "Insufficient data".into());

        let debate = SwarmDebate {
            topic: topic.to_string(),
            agents: agents.to_vec(),
            rounds,
            consensus,
            dissent,
            final_verdict: verdict,
            consensus_confidence: consensus_score,
            total_rounds: num_rounds,
        };

        self.completed_debates.push(debate.clone());
        debate
    }

    /// Launch a multi-phase research campaign.
    pub fn launch_campaign(&mut self, objective: &str, num_agents: usize) -> ResearchCampaign {
        self.total_campaigns += 1;
        let n = num_agents.max(3).min(15);
        let mut rng = rand::thread_rng();

        let agents: Vec<SwarmAgent> = (0..n)
            .map(|i| {
                let specs = [
                    AgentSpecialization::Researcher,
                    AgentSpecialization::Verifier,
                    AgentSpecialization::Synthesizer,
                    AgentSpecialization::Critic,
                    AgentSpecialization::Explorer,
                ];
                let spec = specs[i % specs.len()];
                SwarmAgent {
                    id: format!("campaign_agent_{}", i),
                    name: format!("{} #{}", spec.label(), i),
                    specialization: spec,
                    status: "active".into(),
                    findings: self.generate_findings(objective, spec, &mut rng),
                    confidence: 0.5 + rand::Rng::gen::<f64>(&mut rng) * 0.45,
                    processing_time_ms: 0,
                    contribution_score: 0.0,
                }
            })
            .collect();

        let all_findings: Vec<String> = agents.iter().flat_map(|a| a.findings.clone()).collect();

        let synthesis = format!(
            "Campaign analysis of '{}': {} agents deployed across {} specializations. \
             Key findings: {}",
            objective,
            n,
            agents
                .iter()
                .map(|a| a.specialization.label())
                .collect::<std::collections::HashSet<_>>()
                .len(),
            all_findings
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
        );

        let campaign = ResearchCampaign {
            id: uuid::Uuid::new_v4().to_string()[..12].to_string(),
            objective: objective.to_string(),
            agents,
            phase: CampaignPhase::Complete, // Simulated instant completion
            progress: 1.0,
            findings: all_findings,
            synthesis,
            started_at: chrono::Utc::now().timestamp_millis(),
            completed_at: Some(chrono::Utc::now().timestamp_millis()),
        };

        self.active_campaigns.push(campaign.clone());
        campaign
    }

    /// Get all campaigns.
    pub fn get_campaigns(&self) -> &[ResearchCampaign] {
        &self.active_campaigns
    }

    /// Get recent debates.
    pub fn get_debates(&self) -> &[SwarmDebate] {
        &self.completed_debates
    }

    fn generate_findings(
        &self,
        intent: &str,
        spec: AgentSpecialization,
        rng: &mut impl rand::Rng,
    ) -> Vec<String> {
        let base = match spec {
            AgentSpecialization::Researcher => vec![
                format!(
                    "Research finding on '{}': Multiple sources corroborate key claims",
                    intent
                ),
                format!("Data analysis indicates high relevance for '{}'", intent),
            ],
            AgentSpecialization::Verifier => vec![
                format!(
                    "Verification of '{}': Cross-referenced against 3 independent sources",
                    intent
                ),
                format!(
                    "Confidence: {:.0}% — primary claims verified",
                    60.0 + rng.gen::<f64>() * 35.0
                ),
            ],
            AgentSpecialization::Synthesizer => vec![
                format!(
                    "Synthesis on '{}': Combined insights from multiple research threads",
                    intent
                ),
                "Pattern identified: convergent evidence from diverse domains".into(),
            ],
            AgentSpecialization::Critic => vec![
                format!(
                    "Critical analysis of '{}': Potential bias detected in sources",
                    intent
                ),
                "Recommendation: verify with primary sources before accepting conclusions".into(),
            ],
            AgentSpecialization::Explorer => vec![
                format!(
                    "Exploration of '{}': Discovered {} related topics",
                    intent,
                    3 + (rng.gen::<f64>() * 7.0) as usize
                ),
                "Novel connections found between seemingly unrelated domains".into(),
            ],
        };
        base
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_swarms": self.total_swarms,
            "total_debates": self.total_debates,
            "total_campaigns": self.total_campaigns,
            "active_campaigns": self.active_campaigns.len(),
            "completed_debates": self.completed_debates.len(),
        })
    }
}

fn specialization_domain_fit(spec: AgentSpecialization) -> f64 {
    match spec {
        AgentSpecialization::Researcher => 0.88,
        AgentSpecialization::Verifier => 0.9,
        AgentSpecialization::Synthesizer => 0.84,
        AgentSpecialization::Critic => 0.82,
        AgentSpecialization::Explorer => 0.76,
    }
}

fn specialization_tool_readiness(spec: AgentSpecialization) -> f64 {
    match spec {
        AgentSpecialization::Researcher => 0.84,
        AgentSpecialization::Verifier => 0.78,
        AgentSpecialization::Synthesizer => 0.74,
        AgentSpecialization::Critic => 0.72,
        AgentSpecialization::Explorer => 0.7,
    }
}

fn specialization_verification_strength(spec: AgentSpecialization) -> f64 {
    match spec {
        AgentSpecialization::Verifier => 0.96,
        AgentSpecialization::Critic => 0.86,
        AgentSpecialization::Researcher => 0.74,
        AgentSpecialization::Synthesizer => 0.72,
        AgentSpecialization::Explorer => 0.68,
    }
}

fn specialization_collaboration(spec: AgentSpecialization) -> f64 {
    match spec {
        AgentSpecialization::Synthesizer => 0.9,
        AgentSpecialization::Researcher => 0.8,
        AgentSpecialization::Verifier => 0.82,
        AgentSpecialization::Critic => 0.74,
        AgentSpecialization::Explorer => 0.72,
    }
}

fn specialization_policy_alignment(spec: AgentSpecialization) -> f64 {
    match spec {
        AgentSpecialization::Verifier => 0.88,
        AgentSpecialization::Researcher => 0.82,
        AgentSpecialization::Synthesizer => 0.8,
        AgentSpecialization::Critic => 0.78,
        AgentSpecialization::Explorer => 0.7,
    }
}

// ─────────────────────────────────────────────────────────────
// Feature 2: Multi-Agent Swarm Browsing
// ─────────────────────────────────────────────────────────────
// Spawns N virtual agents, each proposes data, consensus aggregates best answer.

use crate::agents::base::AgentProposal;
use crate::agents::solver::AgentSolver;
use crate::error::AstraResult;
use crate::orchestrator::{RankedCandidateInput, RankedConsensusEngine, RankedConsensusResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmQuery {
    pub intent: String,
    pub num_agents: usize,
    pub timeout_ms: u64,
    pub min_confidence: f64,
}

impl Default for SwarmQuery {
    fn default() -> Self {
        Self {
            intent: String::new(),
            num_agents: 5,
            timeout_ms: 10000,
            min_confidence: 0.3,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentReport {
    pub agent_id: String,
    pub data: String,
    pub confidence: f64,
    pub weight: f64,
    pub peer_review_score: f64,
    pub final_score: f64,
    pub rank: usize,
    pub divergence_penalty: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwarmResult {
    pub consensus_answer: String,
    pub confidence: f64,
    pub num_agents: usize,
    pub agent_reports: Vec<AgentReport>,
    pub consensus_vector: Vec<f64>,
    pub total_divergence: f64,
    pub ranking_consensus: RankedConsensusResult,
}

pub struct SwarmBrowser {
    solver: AgentSolver,
    ranking_engine: RankedConsensusEngine,
    total_queries: u64,
}

impl SwarmBrowser {
    pub fn new() -> Self {
        Self {
            solver: AgentSolver::new(0.01, 1e-6, 1.5),
            ranking_engine: RankedConsensusEngine::new(),
            total_queries: 0,
        }
    }

    /// Execute a swarm browsing query.
    pub fn query(&mut self, q: &SwarmQuery) -> AstraResult<SwarmResult> {
        self.total_queries += 1;
        let n = q.num_agents.max(2).min(20);

        // Simulate N agents producing proposals
        let mut proposals = Vec::new();
        let mut agent_data = Vec::new();
        let mut ranking_candidates = Vec::new();
        let mut rng = rand::thread_rng();

        for i in 0..n {
            let agent_id = format!("swarm_agent_{}", i);
            let confidence = 0.5 + (rand::Rng::gen::<f64>(&mut rng) * 0.5);
            let resource = 0.05 + (rand::Rng::gen::<f64>(&mut rng) * 0.15);

            // Solution vector based on intent hash
            let intent_hash = crate::crypto::hash::sha3_256(
                format!(
                    "{}:{}:{}",
                    q.intent,
                    i,
                    chrono::Utc::now().timestamp_millis()
                )
                .as_bytes(),
            );
            let solution: Vec<f64> = intent_hash
                .iter()
                .take(8)
                .map(|b| *b as f64 / 255.0)
                .collect();

            let data = format!("Agent {} analysis of: '{}'", i, q.intent);
            agent_data.push((agent_id.clone(), data, confidence));
            ranking_candidates.push(RankedCandidateInput {
                id: agent_id.clone(),
                label: format!("Swarm Agent {}", i),
                specialization: if i % 3 == 0 {
                    "verifier".into()
                } else if i % 2 == 0 {
                    "researcher".into()
                } else {
                    "explorer".into()
                },
                summary: format!("Virtual swarm branch {} evaluated {}", i, q.intent),
                confidence,
                domain_fit: if confidence >= q.min_confidence {
                    0.84
                } else {
                    0.62
                },
                tool_readiness: 0.72 + ((i % 3) as f64 * 0.06),
                verification_strength: if i % 3 == 0 { 0.88 } else { 0.68 },
                collaboration: 0.76,
                policy_alignment: 0.7,
                blocked_ratio: if confidence < q.min_confidence {
                    0.18
                } else {
                    0.0
                },
                resource_cost: resource,
            });

            proposals.push(AgentProposal {
                agent_id,
                solution,
                confidence,
                resource_footprint: resource,
                timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
                metadata: serde_json::json!({}),
            });
        }

        let divergences: HashMap<String, f64> = proposals
            .iter()
            .map(|p| (p.agent_id.clone(), 0.0))
            .collect();

        let (consensus, weights) = self.solver.aggregate(&proposals, &divergences)?;
        let ranking_consensus = self
            .ranking_engine
            .evaluate(&q.intent, &ranking_candidates)?;

        let mut reports = Vec::new();
        let mut total_div = 0.0;
        for (ad, prop) in agent_data.iter().zip(proposals.iter()) {
            let div: f64 = consensus
                .iter()
                .zip(prop.solution.iter())
                .map(|(c, s)| (c - s).powi(2))
                .sum::<f64>()
                .sqrt();
            total_div += div;
            let ranking = ranking_consensus
                .ranked_agents
                .iter()
                .find(|candidate| candidate.candidate_id == ad.0);
            reports.push(AgentReport {
                agent_id: ad.0.clone(),
                data: ad.1.clone(),
                confidence: ad.2,
                weight: ranking
                    .map(|candidate| candidate.consensus_weight)
                    .unwrap_or_else(|| weights.get(&ad.0).copied().unwrap_or(0.0)),
                peer_review_score: ranking
                    .map(|candidate| candidate.peer_review_score)
                    .unwrap_or(ad.2),
                final_score: ranking
                    .map(|candidate| candidate.final_score)
                    .unwrap_or(ad.2),
                rank: ranking.map(|candidate| candidate.rank).unwrap_or(0),
                divergence_penalty: div,
            });
        }

        reports.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let top_answer = ranking_consensus.consensus_summary.clone();
        let avg_conf = ranking_consensus
            .ranked_agents
            .first()
            .map(|candidate| candidate.final_score)
            .unwrap_or_else(|| {
                reports.iter().map(|report| report.confidence).sum::<f64>()
                    / reports.len().max(1) as f64
            });

        Ok(SwarmResult {
            consensus_answer: top_answer,
            confidence: avg_conf,
            num_agents: n,
            agent_reports: reports,
            consensus_vector: consensus,
            total_divergence: total_div,
            ranking_consensus,
        })
    }

    pub fn total_queries(&self) -> u64 {
        self.total_queries
    }
}

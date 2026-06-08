use std::cmp::Ordering;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::agents::base::AgentProposal;
use crate::agents::solver::AgentSolver;
use crate::error::{AstraError, AstraResult};

#[derive(Debug, Clone)]
pub struct RankedCandidateInput {
    pub id: String,
    pub label: String,
    pub specialization: String,
    pub summary: String,
    pub confidence: f64,
    pub domain_fit: f64,
    pub tool_readiness: f64,
    pub verification_strength: f64,
    pub collaboration: f64,
    pub policy_alignment: f64,
    pub blocked_ratio: f64,
    pub resource_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedPeerReview {
    pub reviewer_id: String,
    pub target_id: String,
    pub score: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedAgentScore {
    pub candidate_id: String,
    pub label: String,
    pub specialization: String,
    pub summary: String,
    pub base_confidence: f64,
    pub peer_review_score: f64,
    pub consensus_weight: f64,
    pub final_score: f64,
    pub rank: usize,
    pub strengths: Vec<String>,
    pub cautions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedConsensusResult {
    pub champion_id: String,
    pub champion_label: String,
    pub champion_summary: String,
    pub agreement_score: f64,
    pub cohort_grade: String,
    pub ranked_agents: Vec<RankedAgentScore>,
    pub peer_reviews: Vec<RankedPeerReview>,
    pub consensus_summary: String,
}

pub struct RankedConsensusEngine {
    solver: AgentSolver,
}

impl RankedConsensusEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            solver: AgentSolver::new(0.01, 1e-6, 1.2),
        }
    }

    pub fn evaluate(
        &self,
        objective: &str,
        candidates: &[RankedCandidateInput],
    ) -> AstraResult<RankedConsensusResult> {
        if candidates.is_empty() {
            return Err(AstraError::NoProposals);
        }

        let peer_reviews = build_peer_reviews(candidates);
        let peer_scores = average_peer_scores(candidates, &peer_reviews);
        let divergences = review_divergence(candidates, &peer_reviews, &peer_scores);
        let proposals = candidates.iter().map(build_proposal).collect::<Vec<_>>();
        let (_, weights) = self.solver.aggregate(&proposals, &divergences)?;

        let max_weight = weights.values().copied().fold(0.0_f64, f64::max).max(1.0);

        let mut ranked_agents = candidates
            .iter()
            .map(|candidate| {
                let peer_score = peer_scores.get(&candidate.id).copied().unwrap_or(0.5);
                let consensus_weight =
                    weights.get(&candidate.id).copied().unwrap_or(0.0) / max_weight;
                let final_score = compute_final_score(candidate, peer_score, consensus_weight);

                RankedAgentScore {
                    candidate_id: candidate.id.clone(),
                    label: candidate.label.clone(),
                    specialization: candidate.specialization.clone(),
                    summary: candidate.summary.clone(),
                    base_confidence: clamp01(candidate.confidence),
                    peer_review_score: peer_score,
                    consensus_weight,
                    final_score,
                    rank: 0,
                    strengths: build_strengths(candidate, peer_score, consensus_weight),
                    cautions: build_cautions(candidate),
                }
            })
            .collect::<Vec<_>>();

        ranked_agents.sort_by(|left, right| {
            right
                .final_score
                .partial_cmp(&left.final_score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.label.cmp(&right.label))
        });

        for (index, score) in ranked_agents.iter_mut().enumerate() {
            score.rank = index + 1;
        }

        let champion = ranked_agents
            .first()
            .ok_or_else(|| AstraError::Internal("ranked consensus produced no champion".into()))?;
        let agreement_score = compute_agreement_score(&ranked_agents);
        let top_labels = ranked_agents
            .iter()
            .take(3)
            .map(|agent| format!("{}#{:.2}", agent.label, agent.final_score))
            .collect::<Vec<_>>();

        Ok(RankedConsensusResult {
            champion_id: champion.candidate_id.clone(),
            champion_label: champion.label.clone(),
            champion_summary: champion.summary.clone(),
            agreement_score,
            cohort_grade: cohort_grade(agreement_score, champion.final_score),
            consensus_summary: format!(
                "Champion {} leads the ranked cohort for `{}`. Top contributors: {}.",
                champion.label,
                truncate_text(objective, 96),
                top_labels.join(", ")
            ),
            ranked_agents,
            peer_reviews,
        })
    }
}

impl Default for RankedConsensusEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn build_peer_reviews(candidates: &[RankedCandidateInput]) -> Vec<RankedPeerReview> {
    let mut reviews = Vec::new();

    for reviewer in candidates {
        for target in candidates {
            if reviewer.id == target.id {
                continue;
            }

            let complementarity =
                specialization_complementarity(&reviewer.specialization, &target.specialization);
            let score = clamp01(
                target.confidence * 0.22
                    + target.domain_fit * 0.18
                    + target.tool_readiness * 0.16
                    + target.verification_strength * 0.16
                    + target.collaboration * 0.12
                    + target.policy_alignment * 0.08
                    + complementarity * 0.16
                    - target.blocked_ratio * 0.22,
            );

            reviews.push(RankedPeerReview {
                reviewer_id: reviewer.id.clone(),
                target_id: target.id.clone(),
                score,
                rationale: review_rationale(target, complementarity, score),
            });
        }
    }

    reviews
}

fn average_peer_scores(
    candidates: &[RankedCandidateInput],
    peer_reviews: &[RankedPeerReview],
) -> HashMap<String, f64> {
    candidates
        .iter()
        .map(|candidate| {
            let reviews = peer_reviews
                .iter()
                .filter(|review| review.target_id == candidate.id)
                .collect::<Vec<_>>();
            let average = if reviews.is_empty() {
                clamp01(candidate.confidence)
            } else {
                reviews.iter().map(|review| review.score).sum::<f64>() / reviews.len() as f64
            };
            (candidate.id.clone(), average)
        })
        .collect()
}

fn review_divergence(
    candidates: &[RankedCandidateInput],
    peer_reviews: &[RankedPeerReview],
    peer_scores: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    candidates
        .iter()
        .map(|candidate| {
            let target_reviews = peer_reviews
                .iter()
                .filter(|review| review.target_id == candidate.id)
                .collect::<Vec<_>>();
            let average_peer = peer_scores.get(&candidate.id).copied().unwrap_or(0.5);
            let disagreement = if target_reviews.is_empty() {
                0.0
            } else {
                target_reviews
                    .iter()
                    .map(|review| (review.score - average_peer).abs())
                    .sum::<f64>()
                    / target_reviews.len() as f64
            };
            let divergence = clamp01(
                disagreement * 0.6
                    + (candidate.confidence - average_peer).abs() * 0.3
                    + candidate.blocked_ratio * 0.25,
            );
            (candidate.id.clone(), divergence)
        })
        .collect()
}

fn build_proposal(candidate: &RankedCandidateInput) -> AgentProposal {
    AgentProposal {
        agent_id: candidate.id.clone(),
        solution: vec![
            clamp01(candidate.confidence),
            clamp01(candidate.domain_fit),
            clamp01(candidate.tool_readiness),
            clamp01(candidate.verification_strength),
            clamp01(candidate.collaboration),
            clamp01(candidate.policy_alignment),
            clamp01(1.0 - candidate.blocked_ratio),
        ],
        confidence: clamp01(candidate.confidence),
        resource_footprint: candidate.resource_cost.max(0.05),
        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
        metadata: serde_json::json!({
            "label": candidate.label,
            "specialization": candidate.specialization,
        }),
    }
}

fn compute_final_score(
    candidate: &RankedCandidateInput,
    peer_score: f64,
    consensus_weight: f64,
) -> f64 {
    clamp01(
        candidate.confidence * 0.22
            + peer_score * 0.22
            + consensus_weight * 0.18
            + candidate.domain_fit * 0.12
            + candidate.tool_readiness * 0.10
            + candidate.verification_strength * 0.08
            + candidate.collaboration * 0.05
            + candidate.policy_alignment * 0.07
            - candidate.blocked_ratio * 0.14,
    )
}

fn build_strengths(
    candidate: &RankedCandidateInput,
    peer_score: f64,
    consensus_weight: f64,
) -> Vec<String> {
    let mut strengths = Vec::new();
    if candidate.domain_fit >= 0.75 {
        strengths.push("strong domain fit".into());
    }
    if candidate.verification_strength >= 0.75 {
        strengths.push("strong verification posture".into());
    }
    if candidate.tool_readiness >= 0.75 {
        strengths.push("high tool readiness".into());
    }
    if peer_score >= 0.72 {
        strengths.push("trusted by peer cohort".into());
    }
    if consensus_weight >= 0.72 {
        strengths.push("high solver weight".into());
    }
    if strengths.is_empty() {
        strengths.push("balanced baseline contribution".into());
    }
    strengths
}

fn build_cautions(candidate: &RankedCandidateInput) -> Vec<String> {
    let mut cautions = Vec::new();
    if candidate.blocked_ratio > 0.2 {
        cautions.push("tool policy blocks part of the execution plan".into());
    }
    if candidate.policy_alignment < 0.45 {
        cautions.push("weak alignment with the current adaptive policy".into());
    }
    if candidate.tool_readiness < 0.5 {
        cautions.push("limited tool support for this candidate".into());
    }
    cautions
}

fn compute_agreement_score(ranked_agents: &[RankedAgentScore]) -> f64 {
    if ranked_agents.len() <= 1 {
        return ranked_agents
            .first()
            .map(|agent| clamp01(agent.final_score))
            .unwrap_or(0.0);
    }

    let champion = &ranked_agents[0];
    let spread = ranked_agents
        .iter()
        .skip(1)
        .map(|agent| (champion.final_score - agent.final_score).abs())
        .sum::<f64>()
        / (ranked_agents.len() - 1) as f64;

    clamp01(champion.peer_review_score * 0.55 + (1.0 - spread) * 0.45)
}

fn cohort_grade(agreement_score: f64, champion_score: f64) -> String {
    if agreement_score >= 0.84 && champion_score >= 0.82 {
        "enterprise_ready".into()
    } else if agreement_score >= 0.72 && champion_score >= 0.74 {
        "production_candidate".into()
    } else if agreement_score >= 0.6 {
        "needs_review".into()
    } else {
        "unstable".into()
    }
}

fn specialization_complementarity(reviewer: &str, target: &str) -> f64 {
    let reviewer = reviewer.to_lowercase();
    let target = target.to_lowercase();
    if reviewer == target {
        return 0.58;
    }
    if reviewer.contains("verifier") || reviewer.contains("safety") {
        return 0.9;
    }
    if reviewer.contains("planner") || reviewer.contains("coordinator") {
        return 0.86;
    }
    if reviewer.contains("research") && target.contains("browser") {
        return 0.82;
    }
    if reviewer.contains("coder") && target.contains("planner") {
        return 0.8;
    }
    0.72
}

fn review_rationale(target: &RankedCandidateInput, complementarity: f64, score: f64) -> String {
    let strongest_signal = if target.verification_strength >= target.domain_fit
        && target.verification_strength >= target.tool_readiness
    {
        "verification posture"
    } else if target.tool_readiness >= target.domain_fit {
        "tool readiness"
    } else {
        "domain fit"
    };

    if target.blocked_ratio > 0.2 {
        format!(
            "Support is moderated by blocked directives, but {} and peer complementarity {:.2} keep the candidate viable at {:.2}.",
            strongest_signal, complementarity, score
        )
    } else {
        format!(
            "Support is driven by {} with peer complementarity {:.2}; review score {:.2}.",
            strongest_signal, complementarity, score
        )
    }
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn truncate_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }

    let mut truncated = value
        .chars()
        .take(limit.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranked_consensus_produces_champion_and_peer_reviews() {
        let engine = RankedConsensusEngine::new();
        let result = engine
            .evaluate(
                "ship a production-grade fix",
                &[
                    RankedCandidateInput {
                        id: "planner".into(),
                        label: "Planner".into(),
                        specialization: "planner".into(),
                        summary: "Breaks work into ordered execution stages.".into(),
                        confidence: 0.82,
                        domain_fit: 0.8,
                        tool_readiness: 0.75,
                        verification_strength: 0.62,
                        collaboration: 0.9,
                        policy_alignment: 0.78,
                        blocked_ratio: 0.0,
                        resource_cost: 0.18,
                    },
                    RankedCandidateInput {
                        id: "verifier".into(),
                        label: "Verifier".into(),
                        specialization: "verifier".into(),
                        summary: "Checks completeness and safety.".into(),
                        confidence: 0.88,
                        domain_fit: 0.76,
                        tool_readiness: 0.7,
                        verification_strength: 0.96,
                        collaboration: 0.8,
                        policy_alignment: 0.84,
                        blocked_ratio: 0.0,
                        resource_cost: 0.16,
                    },
                ],
            )
            .expect("ranked consensus should succeed");

        assert_eq!(result.ranked_agents.len(), 2);
        assert!(!result.peer_reviews.is_empty());
        assert!(!result.champion_id.is_empty());
        assert!(result.agreement_score > 0.0);
    }
}

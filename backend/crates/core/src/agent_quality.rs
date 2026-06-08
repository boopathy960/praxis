use serde::{Deserialize, Serialize};

use crate::common::sha3_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    Researcher,
    SourceRanker,
    Extractor,
    Verifier,
    Safety,
    Synthesizer,
}

impl AgentRole {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Researcher => "researcher",
            Self::SourceRanker => "source_ranker",
            Self::Extractor => "extractor",
            Self::Verifier => "verifier",
            Self::Safety => "safety",
            Self::Synthesizer => "synthesizer",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQualityInput {
    pub objective: String,
    pub candidates: Vec<AgentQualityCandidate>,
    pub source_coverage: f64,
    pub contradiction_findings: Vec<String>,
    pub policy_violations: Vec<String>,
    pub synthesis_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQualityCandidate {
    pub role: AgentRole,
    pub summary: String,
    pub confidence: f64,
    pub source_quality: f64,
    pub tool_readiness: f64,
    pub policy_compliance: f64,
    pub evidence_coverage: f64,
    pub verifier_strength: f64,
    pub disagreement: f64,
    pub resource_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPeerReview {
    pub reviewer_role: AgentRole,
    pub target_role: AgentRole,
    pub score: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCandidateScore {
    pub role: AgentRole,
    pub confidence: f64,
    pub peer_review_score: f64,
    pub consensus_weight: f64,
    pub verifier_score: f64,
    pub evidence_score: f64,
    pub final_score: f64,
    pub rank: usize,
    pub strengths: Vec<String>,
    pub cautions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityTrace {
    pub champion_role: AgentRole,
    pub champion_summary: String,
    pub agreement_score: f64,
    pub quality_grade: String,
    pub ranked_agents: Vec<AgentCandidateScore>,
    pub peer_reviews: Vec<AgentPeerReview>,
    pub source_coverage: f64,
    pub contradiction_findings: Vec<String>,
    pub policy_violations: Vec<String>,
    pub synthesis_notes: Vec<String>,
    pub trace_hash: String,
}

#[derive(Debug, Default, Clone)]
pub struct AgentQualityEngine;

impl AgentQualityEngine {
    #[must_use]
    pub fn evaluate(input: AgentQualityInput) -> QualityTrace {
        let candidates = if input.candidates.is_empty() {
            default_candidates(&input.objective, input.source_coverage)
        } else {
            input.candidates
        };
        let peer_reviews = build_peer_reviews(&candidates);
        let mut ranked_agents = candidates
            .iter()
            .map(|candidate| {
                let peer_review_score = average_peer_score(candidate.role, &peer_reviews)
                    .unwrap_or_else(|| clamp01(candidate.confidence));
                let consensus_weight = consensus_weight(candidate);
                let verifier_score = clamp01(
                    candidate.verifier_strength * 0.6
                        + candidate.policy_compliance * 0.25
                        + (1.0 - candidate.disagreement) * 0.15,
                );
                let evidence_score =
                    clamp01(candidate.evidence_coverage * 0.62 + candidate.source_quality * 0.38);
                let final_score = clamp01(
                    candidate.confidence * 0.16
                        + peer_review_score * 0.17
                        + consensus_weight * 0.14
                        + verifier_score * 0.16
                        + evidence_score * 0.17
                        + candidate.tool_readiness * 0.08
                        + candidate.policy_compliance * 0.08
                        - candidate.disagreement * 0.12
                        - normalized_cost(candidate.resource_cost) * 0.04,
                );
                AgentCandidateScore {
                    role: candidate.role,
                    confidence: round4(clamp01(candidate.confidence)),
                    peer_review_score: round4(peer_review_score),
                    consensus_weight: round4(consensus_weight),
                    verifier_score: round4(verifier_score),
                    evidence_score: round4(evidence_score),
                    final_score: round4(final_score),
                    rank: 0,
                    strengths: strengths(candidate, peer_review_score, consensus_weight),
                    cautions: cautions(candidate),
                }
            })
            .collect::<Vec<_>>();

        ranked_agents.sort_by(|left, right| {
            right
                .final_score
                .total_cmp(&left.final_score)
                .then_with(|| left.role.as_str().cmp(right.role.as_str()))
        });
        for (index, score) in ranked_agents.iter_mut().enumerate() {
            score.rank = index + 1;
        }

        let champion = ranked_agents
            .first()
            .cloned()
            .unwrap_or(AgentCandidateScore {
                role: AgentRole::Planner,
                confidence: 0.5,
                peer_review_score: 0.5,
                consensus_weight: 0.5,
                verifier_score: 0.5,
                evidence_score: 0.5,
                final_score: 0.5,
                rank: 1,
                strengths: vec!["fallback baseline".into()],
                cautions: vec!["no agent candidates were supplied".into()],
            });
        let agreement_score = round4(agreement_score(&ranked_agents));
        let grade = quality_grade(
            champion.final_score,
            agreement_score,
            input.source_coverage,
            !input.contradiction_findings.is_empty(),
            !input.policy_violations.is_empty(),
        );
        let champion_summary = format!(
            "{} led the cohort with score {:.3}",
            champion.role.as_str(),
            champion.final_score
        );
        let trace_hash = sha3_hex(
            serde_json::json!({
                "objective": input.objective,
                "champion": champion.role,
                "agreement_score": agreement_score,
                "grade": grade,
                "ranked_agents": ranked_agents,
                "peer_reviews": peer_reviews,
                "source_coverage": input.source_coverage,
                "contradictions": input.contradiction_findings,
                "policy_violations": input.policy_violations,
                "synthesis_notes": input.synthesis_notes,
            })
            .to_string()
            .as_bytes(),
        );

        QualityTrace {
            champion_role: champion.role,
            champion_summary,
            agreement_score,
            quality_grade: grade,
            ranked_agents,
            peer_reviews,
            source_coverage: round4(input.source_coverage),
            contradiction_findings: input.contradiction_findings,
            policy_violations: input.policy_violations,
            synthesis_notes: input.synthesis_notes,
            trace_hash,
        }
    }
}

fn build_peer_reviews(candidates: &[AgentQualityCandidate]) -> Vec<AgentPeerReview> {
    let mut reviews = Vec::new();
    for reviewer in candidates {
        for target in candidates {
            if reviewer.role == target.role {
                continue;
            }
            let complementarity = role_complementarity(reviewer.role, target.role);
            let score = clamp01(
                target.confidence * 0.18
                    + target.source_quality * 0.16
                    + target.evidence_coverage * 0.19
                    + target.verifier_strength * 0.16
                    + target.tool_readiness * 0.10
                    + target.policy_compliance * 0.13
                    + complementarity * 0.12
                    - target.disagreement * 0.18,
            );
            reviews.push(AgentPeerReview {
                reviewer_role: reviewer.role,
                target_role: target.role,
                score: round4(score),
                rationale: review_rationale(target, complementarity, score),
            });
        }
    }
    reviews
}

fn average_peer_score(role: AgentRole, peer_reviews: &[AgentPeerReview]) -> Option<f64> {
    let reviews = peer_reviews
        .iter()
        .filter(|review| review.target_role == role)
        .collect::<Vec<_>>();
    if reviews.is_empty() {
        return None;
    }
    Some(reviews.iter().map(|review| review.score).sum::<f64>() / reviews.len() as f64)
}

fn consensus_weight(candidate: &AgentQualityCandidate) -> f64 {
    let efficiency = clamp01(candidate.confidence) / (candidate.resource_cost.max(0.05) + 0.05);
    let trust = (-1.2 * clamp01(candidate.disagreement)).exp();
    clamp01((efficiency * trust) / 4.0)
}

fn agreement_score(ranked_agents: &[AgentCandidateScore]) -> f64 {
    if ranked_agents.is_empty() {
        return 0.0;
    }
    if ranked_agents.len() == 1 {
        return ranked_agents[0].final_score;
    }
    let champion = ranked_agents[0].final_score;
    let spread = ranked_agents
        .iter()
        .skip(1)
        .map(|score| (champion - score.final_score).abs())
        .sum::<f64>()
        / (ranked_agents.len() - 1) as f64;
    let peer_mean = ranked_agents
        .iter()
        .map(|score| score.peer_review_score)
        .sum::<f64>()
        / ranked_agents.len() as f64;
    clamp01(peer_mean * 0.56 + (1.0 - spread) * 0.44)
}

fn quality_grade(
    champion_score: f64,
    agreement_score: f64,
    source_coverage: f64,
    has_contradictions: bool,
    has_policy_violations: bool,
) -> String {
    if has_policy_violations {
        return "blocked_by_policy".into();
    }
    if source_coverage <= 0.0 {
        return "insufficient_evidence".into();
    }
    if has_contradictions && champion_score < 0.78 {
        return "needs_review".into();
    }
    if champion_score >= 0.80 && agreement_score >= 0.72 && source_coverage >= 0.72 {
        "enterprise_ready".into()
    } else if champion_score >= 0.70 && agreement_score >= 0.64 {
        "production_candidate".into()
    } else if champion_score >= 0.55 {
        "needs_review".into()
    } else {
        "insufficient_evidence".into()
    }
}

fn strengths(
    candidate: &AgentQualityCandidate,
    peer_review_score: f64,
    consensus_weight: f64,
) -> Vec<String> {
    let mut strengths = Vec::new();
    if candidate.evidence_coverage >= 0.72 {
        strengths.push("strong evidence coverage".into());
    }
    if candidate.source_quality >= 0.72 {
        strengths.push("high source quality".into());
    }
    if candidate.verifier_strength >= 0.72 {
        strengths.push("strong verifier posture".into());
    }
    if peer_review_score >= 0.72 {
        strengths.push("trusted by peer cohort".into());
    }
    if consensus_weight >= 0.62 {
        strengths.push("high consensus weight".into());
    }
    if strengths.is_empty() {
        strengths.push("balanced baseline contribution".into());
    }
    strengths
}

fn cautions(candidate: &AgentQualityCandidate) -> Vec<String> {
    let mut cautions = Vec::new();
    if candidate.policy_compliance < 0.8 {
        cautions.push("policy compliance requires review".into());
    }
    if candidate.evidence_coverage < 0.45 {
        cautions.push("limited evidence coverage".into());
    }
    if candidate.disagreement > 0.35 {
        cautions.push("cohort disagreement is elevated".into());
    }
    if candidate.tool_readiness < 0.5 {
        cautions.push("limited tool readiness".into());
    }
    cautions
}

fn review_rationale(target: &AgentQualityCandidate, complementarity: f64, score: f64) -> String {
    format!(
        "{} reviewed as {:.3}; evidence={:.3}, verifier={:.3}, complementarity={:.3}",
        target.role.as_str(),
        round4(score),
        round4(target.evidence_coverage),
        round4(target.verifier_strength),
        round4(complementarity)
    )
}

fn role_complementarity(reviewer: AgentRole, target: AgentRole) -> f64 {
    match (reviewer, target) {
        (AgentRole::Verifier | AgentRole::Safety, AgentRole::Researcher | AgentRole::Extractor) => {
            0.86
        }
        (AgentRole::SourceRanker, AgentRole::Researcher | AgentRole::Synthesizer) => 0.82,
        (AgentRole::Planner, AgentRole::Synthesizer) => 0.78,
        (AgentRole::Synthesizer, AgentRole::Verifier | AgentRole::Safety) => 0.75,
        _ => 0.62,
    }
}

fn default_candidates(objective: &str, source_coverage: f64) -> Vec<AgentQualityCandidate> {
    let objective_factor = if objective.trim().is_empty() {
        0.45
    } else {
        0.62
    };
    [
        AgentRole::Planner,
        AgentRole::Researcher,
        AgentRole::SourceRanker,
        AgentRole::Extractor,
        AgentRole::Verifier,
        AgentRole::Safety,
        AgentRole::Synthesizer,
    ]
    .into_iter()
    .map(|role| AgentQualityCandidate {
        role,
        summary: format!("{} baseline execution candidate", role.as_str()),
        confidence: objective_factor,
        source_quality: source_coverage,
        tool_readiness: 0.55,
        policy_compliance: 0.9,
        evidence_coverage: source_coverage,
        verifier_strength: if matches!(role, AgentRole::Verifier | AgentRole::Safety) {
            0.75
        } else {
            0.58
        },
        disagreement: 1.0 - source_coverage,
        resource_cost: 0.4,
    })
    .collect()
}

fn normalized_cost(cost: f64) -> f64 {
    clamp01(cost / 3.0)
}

fn clamp01(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn round4(value: f64) -> f64 {
    (clamp01(value) * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_is_deterministic_and_selects_verifiable_champion() {
        let input = AgentQualityInput {
            objective: "verify source-backed answer".into(),
            source_coverage: 0.9,
            contradiction_findings: Vec::new(),
            policy_violations: Vec::new(),
            synthesis_notes: vec!["all claims cited".into()],
            candidates: vec![
                AgentQualityCandidate {
                    role: AgentRole::Researcher,
                    summary: "broad source acquisition".into(),
                    confidence: 0.78,
                    source_quality: 0.86,
                    tool_readiness: 0.82,
                    policy_compliance: 0.95,
                    evidence_coverage: 0.88,
                    verifier_strength: 0.70,
                    disagreement: 0.10,
                    resource_cost: 0.45,
                },
                AgentQualityCandidate {
                    role: AgentRole::Verifier,
                    summary: "claim verification".into(),
                    confidence: 0.82,
                    source_quality: 0.82,
                    tool_readiness: 0.76,
                    policy_compliance: 0.98,
                    evidence_coverage: 0.86,
                    verifier_strength: 0.94,
                    disagreement: 0.06,
                    resource_cost: 0.40,
                },
            ],
        };
        let first = AgentQualityEngine::evaluate(input.clone());
        let second = AgentQualityEngine::evaluate(input);

        assert_eq!(first.trace_hash, second.trace_hash);
        assert_eq!(first.ranked_agents[0].role, AgentRole::Verifier);
        assert_eq!(first.quality_grade, "production_candidate");
    }

    #[test]
    fn policy_violations_block_quality_grade() {
        let trace = AgentQualityEngine::evaluate(AgentQualityInput {
            objective: "unsafe fetch".into(),
            candidates: Vec::new(),
            source_coverage: 0.7,
            contradiction_findings: Vec::new(),
            policy_violations: vec!["private host blocked".into()],
            synthesis_notes: Vec::new(),
        });

        assert_eq!(trace.quality_grade, "blocked_by_policy");
    }
}

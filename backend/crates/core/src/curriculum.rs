use serde::{Deserialize, Serialize};

use crate::active_inference::AttentionPlan;
use crate::evals::EvalScorecard;
use crate::learning::{LearningEpisode, LearningSignal};
use crate::proof_economy::Claim;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurriculumTask {
    pub task_id: String,
    pub title: String,
    pub rationale: String,
    pub source: String,
    pub priority: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CurriculumRequest {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurriculumPlan {
    pub tasks: Vec<CurriculumTask>,
    pub generated_from: Vec<String>,
}

#[derive(Clone, Default)]
pub struct CurriculumService;

impl CurriculumService {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn propose(
        &self,
        learning: &[LearningEpisode],
        open_claims: &[Claim],
        eval_runs: &[EvalScorecard],
        attention: Option<&AttentionPlan>,
        limit: usize,
    ) -> CurriculumPlan {
        let mut tasks = Vec::new();
        for episode in learning.iter().take(50) {
            let priority = match episode.signal {
                LearningSignal::Failure | LearningSignal::Refutation => 0.92,
                LearningSignal::CapabilityGap => 0.88,
                LearningSignal::Surprise => 0.75,
                LearningSignal::SafetyBlock => 0.7,
                LearningSignal::Insight => 0.55,
                LearningSignal::Success => 0.25,
            };
            if priority >= 0.55 {
                tasks.push(CurriculumTask {
                    task_id: format!("learn:{}", episode.episode_id),
                    title: format!("Investigate {}", episode.organ),
                    rationale: episode.summary.clone(),
                    source: "learning".into(),
                    priority,
                });
            }
        }
        for claim in open_claims.iter().take(25) {
            tasks.push(CurriculumTask {
                task_id: format!("claim:{}", claim.claim_id),
                title: "Resolve open proof claim".into(),
                rationale: claim.statement.clone(),
                source: "proof_economy".into(),
                priority: 0.82 + (1.0 - claim.confidence).clamp(0.0, 0.15),
            });
        }
        for run in eval_runs.iter().filter(|run| !run.pass).take(25) {
            tasks.push(CurriculumTask {
                task_id: format!("eval:{}", run.run_id),
                title: "Repair failing eval suite".into(),
                rationale: format!(
                    "suite {} passed {}/{} cases",
                    run.suite_id, run.passed, run.total
                ),
                source: "evals".into(),
                priority: 0.86,
            });
        }
        if let Some(attention) = attention {
            for item in attention.candidates.iter().take(10) {
                tasks.push(CurriculumTask {
                    task_id: format!("belief:{}", item.belief_id),
                    title: "Sample high-surprise belief".into(),
                    rationale: item.statement.clone(),
                    source: "active_inference".into(),
                    priority: item.salience.clamp(0.0, 1.0),
                });
            }
        }
        tasks.sort_by(|left, right| {
            right
                .priority
                .total_cmp(&left.priority)
                .then_with(|| left.task_id.cmp(&right.task_id))
        });
        tasks.truncate(limit.clamp(1, 100));
        CurriculumPlan {
            tasks,
            generated_from: vec![
                "learning".into(),
                "proof_economy".into(),
                "evals".into(),
                "active_inference".into(),
            ],
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Credit Assignment Engine — Span-Level Credit Attribution
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/credit_assignment.py

use super::trace_store::{SpanType, TrajectoryTrace};

/// Credit assignment result for a single span.
#[derive(Debug, Clone)]
pub struct SpanCredit {
    pub span_id: String,
    pub credit: f64,
    pub normalized_credit: f64,
    pub was_helpful: bool,
    pub reasoning: String,
}

/// Credit Assignment Engine — assigns credit to individual spans.
/// Uses temporal difference and counterfactual analysis.
pub struct CreditAssignmentEngine {
    gamma: f64, // Discount factor for temporal credit
    total_assignments: u64,
}

impl CreditAssignmentEngine {
    pub fn new(gamma: f64) -> Self {
        Self {
            gamma,
            total_assignments: 0,
        }
    }

    /// Assign credit to all spans in a trajectory.
    pub fn assign_credit(&mut self, trace: &TrajectoryTrace) -> Vec<SpanCredit> {
        self.total_assignments += 1;
        let final_reward = trace.final_reward;
        let n = trace.spans.len();

        if n == 0 {
            return Vec::new();
        }

        let mut credits = Vec::new();

        for (i, span) in trace.spans.iter().enumerate() {
            // Temporal credit: spans closer to the reward get more credit
            let temporal_weight = self.gamma.powi((n - 1 - i) as i32);

            // Span-type weight: some span types matter more
            let type_weight = match span.span_type {
                SpanType::Verification => 1.5,
                SpanType::Reasoning => 1.3,
                SpanType::Hypothesis => 1.2,
                SpanType::ToolCall => 1.1,
                SpanType::Metacognition => 0.8,
                SpanType::Reward => 0.5,
                _ => 1.0,
            };

            // Duration-based: longer spans may have contributed more
            let duration_weight = (span.duration_ms / 100.0).min(2.0).max(0.5);

            let raw_credit = final_reward * temporal_weight * type_weight * duration_weight;

            // Counterfactual: was this span's contribution positive?
            let was_helpful = raw_credit > 0.0 && span.reward >= 0.0;

            credits.push(SpanCredit {
                span_id: span.span_id.clone(),
                credit: raw_credit,
                normalized_credit: 0.0, // Will normalize below
                was_helpful,
                reasoning: format!(
                    "temporal={:.3} type={:.1} duration={:.2} -> raw={:.4}",
                    temporal_weight, type_weight, duration_weight, raw_credit
                ),
            });
        }

        // Normalize credits to sum to final_reward
        let total_credit: f64 = credits.iter().map(|c| c.credit.abs()).sum();
        if total_credit > 0.0 {
            for credit in &mut credits {
                credit.normalized_credit = credit.credit / total_credit * final_reward;
            }
        }

        credits
    }

    /// Identify the most impactful spans (top N by credit).
    pub fn top_contributors<'a>(&self, credits: &'a [SpanCredit], n: usize) -> Vec<&'a SpanCredit> {
        let mut sorted: Vec<&SpanCredit> = credits.iter().collect();
        sorted.sort_by(|a, b| {
            b.credit
                .partial_cmp(&a.credit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(n);
        sorted
    }

    /// Identify spans that hurt performance (negative credit).
    pub fn negative_contributors<'a>(&self, credits: &'a [SpanCredit]) -> Vec<&'a SpanCredit> {
        credits.iter().filter(|c| c.credit < 0.0).collect()
    }

    pub fn total_assignments(&self) -> u64 {
        self.total_assignments
    }
}

impl Default for CreditAssignmentEngine {
    fn default() -> Self {
        Self::new(0.95)
    }
}

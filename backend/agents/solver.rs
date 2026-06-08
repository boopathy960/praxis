// ─────────────────────────────────────────────────────────────
// Agent-Solver — Weighted Consensus Functional
// ─────────────────────────────────────────────────────────────
// Φ_t = argmin_y Σ w_{a,t} · ℓ(y, y_{a,t}) + λ‖y‖²
// w_{a,t} = (c_{a,t} / (ρ_{a,t} + ε)) · exp(-κ · Δ_{a,t})

use super::base::AgentProposal;
use crate::error::{AstraError, AstraResult};
use std::collections::HashMap;

/// Agent-Solver: aggregates proposals into consensus Φ_t.
pub struct AgentSolver {
    pub lambda_reg: f64, // λ — regularization
    pub epsilon: f64,    // ε — prevents div by zero
    pub kappa: f64,      // κ — divergence penalty
}

impl AgentSolver {
    pub fn new(lambda_reg: f64, epsilon: f64, kappa: f64) -> Self {
        Self {
            lambda_reg,
            epsilon,
            kappa,
        }
    }

    /// Compute agent weight: w = (c / (ρ + ε)) · exp(-κΔ)
    pub fn compute_weight(&self, confidence: f64, resource: f64, divergence: f64) -> f64 {
        let efficiency = confidence / (resource + self.epsilon);
        let trust = (-self.kappa * divergence).exp();
        efficiency * trust
    }

    /// Aggregate proposals into consensus (closed-form for squared loss).
    /// Φ_t = (Σ w · y_a) / (Σ w + λ)
    pub fn aggregate(
        &self,
        proposals: &[AgentProposal],
        divergences: &HashMap<String, f64>,
    ) -> AstraResult<(Vec<f64>, HashMap<String, f64>)> {
        if proposals.is_empty() {
            return Err(AstraError::NoProposals);
        }

        let dim = proposals[0].solution.len();
        let mut weights = HashMap::new();

        for prop in proposals {
            let delta = divergences.get(&prop.agent_id).copied().unwrap_or(0.0);
            let w = self.compute_weight(prop.confidence, prop.resource_footprint, delta);
            weights.insert(prop.agent_id.clone(), w);
        }

        let mut weighted_sum = vec![0.0f64; dim];
        let mut total_weight: f64 = 0.0;

        for prop in proposals {
            let w = weights[&prop.agent_id];
            for (i, val) in prop.solution.iter().enumerate() {
                if i < dim {
                    weighted_sum[i] += w * val;
                }
            }
            total_weight += w;
        }

        // Φ_t = (Σ w · y_a) / (Σ w + λ)
        let denom = total_weight + self.lambda_reg;
        let consensus: Vec<f64> = weighted_sum.iter().map(|v| v / denom).collect();

        Ok((consensus, weights))
    }

    /// Compute consensus objective loss.
    pub fn compute_loss(
        &self,
        y: &[f64],
        proposals: &[AgentProposal],
        weights: &HashMap<String, f64>,
    ) -> f64 {
        let mut loss = 0.0;
        for prop in proposals {
            let w = weights.get(&prop.agent_id).copied().unwrap_or(0.0);
            let diff_sq: f64 = y
                .iter()
                .zip(prop.solution.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum();
            loss += w * diff_sq;
        }
        let y_sq: f64 = y.iter().map(|v| v.powi(2)).sum();
        loss += self.lambda_reg * y_sq;
        loss
    }

    /// Iterative gradient descent for general loss functions.
    pub fn iterative_aggregate(
        &self,
        proposals: &[AgentProposal],
        divergences: &HashMap<String, f64>,
        max_iter: usize,
        lr: f64,
        tol: f64,
    ) -> AstraResult<(Vec<f64>, HashMap<String, f64>)> {
        let (mut y, weights) = self.aggregate(proposals, divergences)?;
        let dim = y.len();

        for _ in 0..max_iter {
            let mut grad = vec![0.0f64; dim];
            for prop in proposals {
                let w = weights.get(&prop.agent_id).copied().unwrap_or(0.0);
                for i in 0..dim.min(prop.solution.len()) {
                    grad[i] += 2.0 * w * (y[i] - prop.solution[i]);
                }
            }
            for i in 0..dim {
                grad[i] += 2.0 * self.lambda_reg * y[i];
            }

            let grad_norm: f64 = grad.iter().map(|g| g.powi(2)).sum::<f64>().sqrt();
            if grad_norm < tol {
                break;
            }

            for i in 0..dim {
                y[i] -= lr * grad[i];
            }
        }

        Ok((y, weights))
    }
}

impl Default for AgentSolver {
    fn default() -> Self {
        Self::new(0.01, 1e-6, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consensus_two_agents() {
        let solver = AgentSolver::default();
        let proposals = vec![
            AgentProposal {
                agent_id: "a1".into(),
                solution: vec![1.0, 0.0],
                confidence: 0.9,
                resource_footprint: 0.1,
                timestamp: 0.0,
                metadata: serde_json::json!({}),
            },
            AgentProposal {
                agent_id: "a2".into(),
                solution: vec![0.0, 1.0],
                confidence: 0.8,
                resource_footprint: 0.2,
                timestamp: 0.0,
                metadata: serde_json::json!({}),
            },
        ];
        let divs = HashMap::new();
        let (consensus, weights) = solver.aggregate(&proposals, &divs).unwrap();
        assert_eq!(consensus.len(), 2);
        assert!(weights.contains_key("a1"));
        assert!(weights.contains_key("a2"));
        // a1 has higher confidence and lower resource → higher weight
        assert!(weights["a1"] > weights["a2"]);
    }
}

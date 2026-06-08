// ─────────────────────────────────────────────────────────────
// Agentic Verification Calculus
// ─────────────────────────────────────────────────────────────
// E_t(y) = Σ w·ℓ(y,y_a) + λ‖y‖² + ζ·Risk(y)

use super::base::AgentProposal;
use std::collections::HashMap;

/// Verification calculus engine.
pub struct AgenticVerification {
    pub lambda_reg: f64,
    pub zeta: f64,
    pub omega: f64,
    pub beta_smooth: f64,
}

impl AgenticVerification {
    pub fn new(lambda_reg: f64, zeta: f64, omega: f64, beta_smooth: f64) -> Self {
        Self {
            lambda_reg,
            zeta,
            omega,
            beta_smooth,
        }
    }

    /// Compute verification energy E_t(y).
    pub fn verification_energy(
        &self,
        y: &[f64],
        proposals: &[AgentProposal],
        weights: &HashMap<String, f64>,
        state: Option<&[f64]>,
    ) -> f64 {
        let wl: f64 = proposals
            .iter()
            .map(|p| {
                let w = weights.get(&p.agent_id).copied().unwrap_or(0.0);
                let ds: f64 = y
                    .iter()
                    .zip(p.solution.iter())
                    .map(|(a, b)| (a - b).powi(2))
                    .sum();
                w * ds
            })
            .sum();

        let reg: f64 = self.lambda_reg * y.iter().map(|v| v.powi(2)).sum::<f64>();
        let risk = self.compute_risk(y, state);
        wl + reg + self.zeta * risk
    }

    /// Find optimal solution y* = argmin E_t(y) via gradient descent.
    pub fn find_optimal(
        &self,
        proposals: &[AgentProposal],
        weights: &HashMap<String, f64>,
        state: Option<&[f64]>,
        max_iter: usize,
        lr: f64,
        tol: f64,
    ) -> (Vec<f64>, f64) {
        let dim = proposals[0].solution.len();
        let total_w: f64 = weights.values().sum::<f64>() + 1e-12;
        let mut y = vec![0.0; dim];
        for p in proposals {
            let w = weights.get(&p.agent_id).copied().unwrap_or(0.0);
            for (i, v) in p.solution.iter().enumerate() {
                if i < dim {
                    y[i] += w * v;
                }
            }
        }
        for v in y.iter_mut() {
            *v /= total_w + self.lambda_reg;
        }

        for _ in 0..max_iter {
            let grad = self.energy_gradient(&y, proposals, weights, state);
            let gn: f64 = grad.iter().map(|g| g.powi(2)).sum::<f64>().sqrt();
            if gn < tol {
                break;
            }
            for i in 0..dim {
                y[i] -= lr * grad[i];
            }
        }

        let energy = self.verification_energy(&y, proposals, weights, state);
        (y, energy)
    }

    /// Check Certified Lane Dominance (Lemma 1).
    pub fn check_lane_dominance(
        &self,
        y_star: &[f64],
        proposal: &AgentProposal,
        weight: f64,
    ) -> (bool, f64) {
        let diff_norm: f64 = y_star
            .iter()
            .zip(proposal.solution.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        let grad: Vec<f64> = y_star
            .iter()
            .zip(proposal.solution.iter())
            .map(|(ys, ya)| 2.0 * weight * (ya - ys) + 2.0 * self.lambda_reg * ya)
            .collect();
        let gn: f64 = grad.iter().map(|g| g.powi(2)).sum::<f64>().sqrt();
        let bound = if self.lambda_reg > 0.0 {
            gn / self.lambda_reg
        } else {
            f64::INFINITY
        };
        (diff_norm <= bound, diff_norm / (bound + 1e-12))
    }

    /// Check stability: ‖F(x,y*,0) - F(x,y,0)‖ ≤ ω‖y* - y‖
    pub fn check_stability(&self, y_star: &[f64], y_t: &[f64], state: &[f64]) -> bool {
        let dim = state.len().min(y_star.len()).min(y_t.len());
        let lhs: f64 = (0..dim)
            .map(|i| ((state[i] + y_star[i]) - (state[i] + y_t[i])).powi(2))
            .sum::<f64>()
            .sqrt();
        let rhs: f64 = self.omega
            * (0..dim)
                .map(|i| (y_star[i] - y_t[i]).powi(2))
                .sum::<f64>()
                .sqrt();
        lhs <= rhs + 1e-10
    }

    fn compute_risk(&self, y: &[f64], state: Option<&[f64]>) -> f64 {
        let yn: f64 = y.iter().map(|v| v.powi(2)).sum::<f64>().sqrt();
        match state {
            Some(s) => {
                let dim = y.len().min(s.len());
                let sn: f64 = s[..dim].iter().map(|v| v.powi(2)).sum::<f64>().sqrt();
                yn * (1.0 + yn / (sn + 1e-12))
            }
            None => yn,
        }
    }

    fn energy_gradient(
        &self,
        y: &[f64],
        proposals: &[AgentProposal],
        weights: &HashMap<String, f64>,
        _state: Option<&[f64]>,
    ) -> Vec<f64> {
        let dim = y.len();
        let mut grad = vec![0.0; dim];
        for p in proposals {
            let w = weights.get(&p.agent_id).copied().unwrap_or(0.0);
            for i in 0..dim.min(p.solution.len()) {
                grad[i] += 2.0 * w * (y[i] - p.solution[i]);
            }
        }
        for i in 0..dim {
            grad[i] += 2.0 * self.lambda_reg * y[i];
        }

        let yn: f64 = y.iter().map(|v| v.powi(2)).sum::<f64>().sqrt();
        if yn > 1e-12 {
            for i in 0..dim {
                grad[i] += self.zeta * y[i] / yn;
            }
        }
        grad
    }
}

impl Default for AgenticVerification {
    fn default() -> Self {
        Self::new(0.01, 0.1, 0.5, 1.0)
    }
}

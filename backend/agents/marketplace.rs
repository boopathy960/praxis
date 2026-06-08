// ─────────────────────────────────────────────────────────────
// Problem-Solving Marketplace
// ─────────────────────────────────────────────────────────────
use crate::crypto::hash::sha3_256_hex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProblemType {
    Optimization,
    MachineLearning,
    Cryptographic,
    DataAnalysis,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProblemStatus {
    Open,
    InProgress,
    Solved,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub id: String,
    pub problem_type: ProblemType,
    pub description: String,
    pub bounty: f64,
    pub poster: String,
    pub deadline: f64,
    pub min_confidence: f64,
    pub status: ProblemStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub id: String,
    pub problem_id: String,
    pub agent_id: String,
    pub result: serde_json::Value,
    pub confidence: f64,
    pub computation_time: f64,
    pub resource_cost: f64,
    pub quality_score: f64,
    pub verified: bool,
}

pub struct ProblemMarketplace {
    problems: HashMap<String, Problem>,
    solutions: HashMap<String, Vec<Solution>>,
    fee_rate: f64,
    total_paid: f64,
    total_solved: usize,
}

impl ProblemMarketplace {
    pub fn new(fee_rate: f64) -> Self {
        Self {
            problems: HashMap::new(),
            solutions: HashMap::new(),
            fee_rate,
            total_paid: 0.0,
            total_solved: 0,
        }
    }

    pub fn post_problem(&mut self, mut problem: Problem) -> String {
        if problem.id.is_empty() {
            problem.id =
                sha3_256_hex(format!("{}:{}", problem.poster, problem.description).as_bytes())
                    [..16]
                    .to_string();
        }
        let id = problem.id.clone();
        self.solutions.insert(id.clone(), Vec::new());
        self.problems.insert(id.clone(), problem);
        id
    }

    pub fn submit_solution(&mut self, mut sol: Solution) -> bool {
        let problem = match self.problems.get_mut(&sol.problem_id) {
            Some(p) => p,
            None => return false,
        };
        if problem.status != ProblemStatus::Open && problem.status != ProblemStatus::InProgress {
            return false;
        }

        sol.quality_score = sol.confidence
            * (1.0 / sol.computation_time.max(0.001))
            * (1.0 / sol.resource_cost.max(0.001));
        sol.verified = sol.confidence >= problem.min_confidence;
        problem.status = ProblemStatus::InProgress;
        self.solutions
            .entry(sol.problem_id.clone())
            .or_default()
            .push(sol);
        true
    }

    pub fn finalize(&mut self, problem_id: &str) -> Option<serde_json::Value> {
        let problem = self.problems.get_mut(problem_id)?;
        let sols = self.solutions.get(problem_id)?;
        let winner = sols.iter().filter(|s| s.verified).max_by(|a, b| {
            a.quality_score
                .partial_cmp(&b.quality_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        let payout = problem.bounty * (1.0 - self.fee_rate);
        problem.status = ProblemStatus::Solved;
        self.total_paid += payout;
        self.total_solved += 1;
        Some(serde_json::json!({
            "winner": winner.agent_id, "payout": payout,
            "quality_score": winner.quality_score, "submissions": sols.len(),
        }))
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_problems": self.problems.len(), "solved": self.total_solved,
            "total_paid": self.total_paid, "fee_rate": self.fee_rate,
        })
    }
}

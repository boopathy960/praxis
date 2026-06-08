// ─────────────────────────────────────────────────────────────
// Byzantine Consensus Engine — Multi-Path Verified Reasoning
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/consensus_engine.py
// Multiple independent reasoning paths with BFT-style agreement.

use std::collections::HashMap;
use std::time::Instant;

/// A reasoning path result.
#[derive(Debug, Clone)]
pub struct ReasoningPath {
    pub id: usize,
    pub strategy: String,
    pub answer: String,
    pub confidence: f64,
    pub proof_chain: Vec<String>,
    pub duration_ms: f64,
}

/// Consensus result from multiple reasoning paths.
#[derive(Debug, Clone)]
pub struct ConsensusResult {
    pub consensus_answer: String,
    pub agreement_ratio: f64,
    pub total_paths: usize,
    pub agreeing_paths: usize,
    pub divergent_answers: Vec<(String, usize)>,
    pub proof_chain: Vec<String>,
    pub confidence: f64,
}

/// Byzantine Consensus Engine — multi-path verified reasoning.
/// Requires 2f+1 agreement from 3f+1 paths for BFT tolerance.
pub struct ByzantineConsensusEngine {
    num_paths: usize,
    min_agreement: f64,
    strategies: Vec<String>,
}

impl ByzantineConsensusEngine {
    pub fn new(num_paths: usize) -> Self {
        Self {
            num_paths,
            min_agreement: 0.67, // 2/3 BFT threshold
            strategies: vec![
                "direct_analysis".into(),
                "structural_decomposition".into(),
                "analogical_reasoning".into(),
                "adversarial_challenge".into(),
                "constraint_satisfaction".into(),
            ],
        }
    }

    /// Execute multiple reasoning paths and find consensus.
    pub fn find_consensus<F>(&self, problem: &str, solver: F) -> ConsensusResult
    where
        F: Fn(&str, &str) -> (String, f64),
    {
        let _start = Instant::now();
        let mut paths = Vec::new();

        // Execute each reasoning path
        for i in 0..self.num_paths {
            let strategy = &self.strategies[i % self.strategies.len()];
            let path_start = Instant::now();
            let (answer, confidence) = solver(problem, strategy);

            paths.push(ReasoningPath {
                id: i,
                strategy: strategy.clone(),
                answer,
                confidence,
                proof_chain: vec![format!("Path {} via {}", i, strategy)],
                duration_ms: path_start.elapsed().as_secs_f64() * 1000.0,
            });
        }

        // Count votes for each answer
        let mut vote_counts: HashMap<String, Vec<usize>> = HashMap::new();
        for path in &paths {
            let normalized = self.normalize_answer(&path.answer);
            vote_counts.entry(normalized).or_default().push(path.id);
        }

        // Find majority answer
        let mut answers_sorted: Vec<(String, Vec<usize>)> = vote_counts.into_iter().collect();
        answers_sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        let (consensus_answer, agreeing_ids) = answers_sorted
            .first()
            .map(|(a, ids)| (a.clone(), ids.clone()))
            .unwrap_or(("No consensus reached".into(), Vec::new()));

        let agreement_ratio = agreeing_ids.len() as f64 / self.num_paths as f64;

        let divergent: Vec<(String, usize)> = answers_sorted
            .iter()
            .skip(1)
            .map(|(a, ids)| (a.clone(), ids.len()))
            .collect();

        // Build proof chain from agreeing paths
        let proof_chain: Vec<String> = paths
            .iter()
            .filter(|p| agreeing_ids.contains(&p.id))
            .flat_map(|p| p.proof_chain.clone())
            .collect();

        let confidence = if agreement_ratio >= self.min_agreement {
            agreement_ratio
                * paths
                    .iter()
                    .filter(|p| agreeing_ids.contains(&p.id))
                    .map(|p| p.confidence)
                    .sum::<f64>()
                / agreeing_ids.len().max(1) as f64
        } else {
            0.3 // Below BFT threshold
        };

        ConsensusResult {
            consensus_answer,
            agreement_ratio,
            total_paths: self.num_paths,
            agreeing_paths: agreeing_ids.len(),
            divergent_answers: divergent,
            proof_chain,
            confidence,
        }
    }

    fn normalize_answer(&self, answer: &str) -> String {
        answer
            .trim()
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
    }
}

impl Default for ByzantineConsensusEngine {
    fn default() -> Self {
        Self::new(5)
    }
}

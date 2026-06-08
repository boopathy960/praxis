// ─────────────────────────────────────────────────────────────
// Recursive Reasoning — Self-Evolving Meta-Reasoning
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/recursive_reasoning.py

use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ReasoningNode {
    pub id: usize,
    pub content: String,
    pub confidence: f64,
    pub depth: usize,
    pub children: Vec<usize>,
    pub strategy: String,
}

#[derive(Debug, Clone)]
pub struct ReasoningResult {
    pub conclusion: String,
    pub confidence: f64,
    pub depth_reached: usize,
    pub nodes_explored: usize,
    pub reasoning_chain: Vec<String>,
    pub duration_ms: f64,
}

/// Recursive Reasoning Engine — self-evolving meta-reasoning strategies.
#[allow(dead_code)]
pub struct RecursiveReasoningEngine {
    max_depth: usize,
    nodes: Vec<ReasoningNode>,
    strategy_weights: HashMap<String, f64>,
}

impl RecursiveReasoningEngine {
    pub fn new(max_depth: usize) -> Self {
        let mut weights = HashMap::new();
        weights.insert("divide_conquer".into(), 1.0);
        weights.insert("analogy".into(), 0.8);
        weights.insert("abstraction".into(), 0.9);
        weights.insert("specialization".into(), 0.7);
        Self {
            max_depth,
            nodes: Vec::new(),
            strategy_weights: weights,
        }
    }

    /// Reason recursively about a problem.
    pub fn reason(&mut self, problem: &str) -> ReasoningResult {
        let start = Instant::now();
        self.nodes.clear();

        let root = self.create_node(problem, 0, "root");
        let result = self.recurse(root, problem, 0);

        let chain: Vec<String> = self
            .nodes
            .iter()
            .map(|n| {
                format!(
                    "[D{}] {} (conf={:.2})",
                    n.depth,
                    &n.content.chars().take(80).collect::<String>(),
                    n.confidence
                )
            })
            .collect();

        ReasoningResult {
            conclusion: result.0,
            confidence: result.1,
            depth_reached: self.nodes.iter().map(|n| n.depth).max().unwrap_or(0),
            nodes_explored: self.nodes.len(),
            reasoning_chain: chain,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    fn recurse(&mut self, node_id: usize, problem: &str, depth: usize) -> (String, f64) {
        if depth >= self.max_depth {
            let leaf_answer = format!(
                "Base analysis at depth {}: {}",
                depth,
                &problem.chars().take(100).collect::<String>()
            );
            self.nodes[node_id].confidence = 0.5;
            return (leaf_answer, 0.5);
        }

        // Divide and conquer
        let sub_problems = self.decompose(problem);
        let mut sub_results = Vec::new();

        for sub in &sub_problems {
            let child_id = self.create_node(sub, depth + 1, "divide_conquer");
            self.nodes[node_id].children.push(child_id);
            let result = self.recurse(child_id, sub, depth + 1);
            sub_results.push(result);
        }

        // Synthesize from sub-results
        let combined_confidence = if sub_results.is_empty() {
            0.5
        } else {
            sub_results.iter().map(|(_, c)| c).sum::<f64>() / sub_results.len() as f64
        };

        let conclusion = format!(
            "Synthesized from {} sub-analyses at depth {}: combined confidence {:.3}",
            sub_results.len(),
            depth,
            combined_confidence
        );

        self.nodes[node_id].confidence = combined_confidence;
        (conclusion, combined_confidence)
    }

    fn decompose(&self, problem: &str) -> Vec<String> {
        // Simple heuristic: split by sentence, or create aspects to analyze
        let sentences: Vec<&str> = problem
            .split(['.', '?', '!', ';'])
            .filter(|s| s.trim().len() > 3)
            .collect();

        if sentences.len() > 1 {
            sentences.iter().map(|s| s.trim().to_string()).collect()
        } else {
            vec![
                format!(
                    "Core semantics: {}",
                    &problem.chars().take(50).collect::<String>()
                ),
                format!("Implications analysis"),
                format!("Edge cases and constraints"),
            ]
        }
    }

    fn create_node(&mut self, content: &str, depth: usize, strategy: &str) -> usize {
        let id = self.nodes.len();
        self.nodes.push(ReasoningNode {
            id,
            content: content.to_string(),
            confidence: 0.0,
            depth,
            children: Vec::new(),
            strategy: strategy.to_string(),
        });
        id
    }
}

impl Default for RecursiveReasoningEngine {
    fn default() -> Self {
        Self::new(5)
    }
}

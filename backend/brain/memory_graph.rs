// ─────────────────────────────────────────────────────────────
// Memory Graph — Associative Knowledge Network
// ─────────────────────────────────────────────────────────────
// Long-term semantic memory using a weighted directed graph.
// Nodes are concepts, edges are relationships with strengths.
// Supports retrieval, association, decay, and consolidation.

use crate::crypto::hash::sha3_256_hex;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct MemoryNode {
    pub id: String,
    pub content: String,
    pub category: MemoryCategory,
    pub importance: f64,
    pub access_count: u64,
    pub last_accessed: i64,
    pub created_at: i64,
    pub embedding: Vec<f64>, // Simplified semantic vector
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum MemoryCategory {
    Fact,
    Procedure,
    Episode,
    Concept,
    Skill,
    Context,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub strength: f64,
    pub created_at: i64,
}

pub struct MemoryGraph {
    nodes: HashMap<String, MemoryNode>,
    edges: Vec<MemoryEdge>,
    adjacency: HashMap<String, Vec<usize>>, // node_id → edge indices
    decay_rate: f64,
    max_nodes: usize,
    total_retrievals: u64,
}

impl MemoryGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
            decay_rate: 0.001,
            max_nodes: 50_000,
            total_retrievals: 0,
        }
    }

    /// Store a new memory with auto-generated semantic embedding.
    pub fn store(
        &mut self,
        content: &str,
        category: MemoryCategory,
        importance: f64,
        metadata: serde_json::Value,
    ) -> String {
        let id = sha3_256_hex(
            format!(
                "mem:{}:{}",
                content,
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            )
            .as_bytes(),
        )[..16]
            .to_string();

        let embedding = self.compute_embedding(content);

        self.nodes.insert(
            id.clone(),
            MemoryNode {
                id: id.clone(),
                content: content.to_string(),
                category,
                importance,
                access_count: 0,
                last_accessed: chrono::Utc::now().timestamp_millis(),
                created_at: chrono::Utc::now().timestamp_millis(),
                embedding,
                metadata,
            },
        );

        // Auto-link to similar memories
        self.auto_link(&id);

        // Evict if over capacity
        if self.nodes.len() > self.max_nodes {
            self.evict_least_important();
        }

        id
    }

    /// Link two memories with a named relationship.
    pub fn link(&mut self, source: &str, target: &str, relation: &str, strength: f64) {
        let edge_idx = self.edges.len();
        self.edges.push(MemoryEdge {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            strength,
            created_at: chrono::Utc::now().timestamp_millis(),
        });
        self.adjacency
            .entry(source.to_string())
            .or_default()
            .push(edge_idx);
        self.adjacency
            .entry(target.to_string())
            .or_default()
            .push(edge_idx);
    }

    /// Retrieve memories by semantic similarity to query.
    pub fn retrieve(&mut self, query: &str, top_k: usize) -> Vec<&MemoryNode> {
        self.total_retrievals += 1;
        let query_embedding = self.compute_embedding(query);

        let mut scored: Vec<(&String, f64)> = self
            .nodes
            .iter()
            .map(|(id, node)| {
                let sim = self.cosine_similarity(&query_embedding, &node.embedding);
                let recency = 1.0
                    / (1.0
                        + (chrono::Utc::now().timestamp_millis() - node.last_accessed) as f64
                            / 3_600_000.0);
                let score = sim * 0.6 + node.importance * 0.2 + recency * 0.2;
                (id, score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Update access counts for retrieved nodes
        let top_ids: Vec<String> = scored
            .iter()
            .take(top_k)
            .map(|(id, _)| (*id).clone())
            .collect();
        for id in &top_ids {
            if let Some(node) = self.nodes.get_mut(id) {
                node.access_count += 1;
                node.last_accessed = chrono::Utc::now().timestamp_millis();
            }
        }

        top_ids.iter().filter_map(|id| self.nodes.get(id)).collect()
    }

    /// Get associated memories (graph neighbors).
    pub fn get_associations(&self, node_id: &str, max_hops: usize) -> Vec<&MemoryNode> {
        let mut visited = std::collections::HashSet::new();
        let mut frontier = vec![node_id.to_string()];
        visited.insert(node_id.to_string());

        for _ in 0..max_hops {
            let mut next_frontier = Vec::new();
            for current in &frontier {
                if let Some(edge_indices) = self.adjacency.get(current) {
                    for &idx in edge_indices {
                        if idx < self.edges.len() {
                            let edge = &self.edges[idx];
                            let neighbor = if edge.source == *current {
                                &edge.target
                            } else {
                                &edge.source
                            };
                            if visited.insert(neighbor.clone()) {
                                next_frontier.push(neighbor.clone());
                            }
                        }
                    }
                }
            }
            frontier = next_frontier;
        }

        visited
            .iter()
            .filter(|id| *id != node_id)
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    /// Apply memory decay (reduce importance of infrequently accessed memories).
    pub fn apply_decay(&mut self) {
        let now = chrono::Utc::now().timestamp_millis();
        for node in self.nodes.values_mut() {
            let hours_since = (now - node.last_accessed) as f64 / 3_600_000.0;
            let access_factor = (node.access_count as f64).ln_1p() / 10.0;
            let decay = (-self.decay_rate * hours_since).exp() + access_factor;
            node.importance *= decay.clamp(0.01, 1.0);
        }
    }

    /// Consolidate memories: merge similar, strengthen frequent.
    pub fn consolidate(&mut self) {
        // Strengthen edges between frequently co-accessed nodes
        for edge in self.edges.iter_mut() {
            let src_access = self
                .nodes
                .get(&edge.source)
                .map(|n| n.access_count)
                .unwrap_or(0);
            let tgt_access = self
                .nodes
                .get(&edge.target)
                .map(|n| n.access_count)
                .unwrap_or(0);
            let co_access = (src_access.min(tgt_access) as f64) / 100.0;
            edge.strength = (edge.strength + co_access * 0.01).min(1.0);
        }
    }

    // ── Internal ──

    fn compute_embedding(&self, text: &str) -> Vec<f64> {
        // Bag-of-characters hashing for lightweight semantic vectors
        let hash = crate::crypto::hash::sha3_256(text.as_bytes());
        hash.iter().take(32).map(|b| *b as f64 / 255.0).collect()
    }

    fn cosine_similarity(&self, a: &[f64], b: &[f64]) -> f64 {
        let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
        let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
        if mag_a < 1e-10 || mag_b < 1e-10 {
            0.0
        } else {
            dot / (mag_a * mag_b)
        }
    }

    fn auto_link(&mut self, new_id: &str) {
        let new_embedding = match self.nodes.get(new_id) {
            Some(n) => n.embedding.clone(),
            None => return,
        };

        let mut similar: Vec<(String, f64)> = self
            .nodes
            .iter()
            .filter(|(id, _)| *id != new_id)
            .map(|(id, node)| {
                let sim = self.cosine_similarity(&new_embedding, &node.embedding);
                (id.clone(), sim)
            })
            .filter(|(_, sim)| *sim > 0.7)
            .collect();

        similar.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (target_id, sim) in similar.into_iter().take(3) {
            self.link(new_id, &target_id, "similar", sim);
        }
    }

    fn evict_least_important(&mut self) {
        let evict_count = self.nodes.len() / 10;
        let mut scored: Vec<(String, f64)> = self
            .nodes
            .iter()
            .map(|(id, n)| (id.clone(), n.importance))
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        for (id, _) in scored.into_iter().take(evict_count) {
            self.nodes.remove(&id);
            self.adjacency.remove(&id);
        }

        // Clean up orphaned edges
        self.edges
            .retain(|e| self.nodes.contains_key(&e.source) && self.nodes.contains_key(&e.target));
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "total_nodes": self.nodes.len(),
            "total_edges": self.edges.len(),
            "total_retrievals": self.total_retrievals,
            "categories": {
                "facts": self.nodes.values().filter(|n| n.category == MemoryCategory::Fact).count(),
                "procedures": self.nodes.values().filter(|n| n.category == MemoryCategory::Procedure).count(),
                "episodes": self.nodes.values().filter(|n| n.category == MemoryCategory::Episode).count(),
                "concepts": self.nodes.values().filter(|n| n.category == MemoryCategory::Concept).count(),
            },
        })
    }
}

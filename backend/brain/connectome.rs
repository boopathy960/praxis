// ═══════════════════════════════════════════════════════════════
// KNOWLEDGE CONNECTOME v3.0 — Neuromorphic Knowledge Architecture
// ═══════════════════════════════════════════════════════════════
//
// Beyond DOM foraging — a full knowledge network:
//   1. Concept nodes with typed relationships
//   2. Spreading activation for associative retrieval
//   3. Ontology construction from browsing data
//   4. Cross-domain concept mapping
//   5. MCTS-based DOM planning (upgraded from v1)
//   6. Semantic similarity via TF-IDF (no embeddings needed)
//   7. Hebbian learning: "neurons that fire together wire together"
//
// The connectome IS the browser's knowledge — not a cache,
// but a living graph that grows with every interaction.

use crate::crypto::hash::sha3_256_hex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

// ═══════════════════════════════════════════════════════════════
// CONCEPT NODE — A node in the knowledge graph
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptNode {
    pub id: String,
    pub label: String,
    pub domain: String,
    pub activation: f64,      // Current activation level
    pub base_activation: f64, // Resting activation
    pub access_count: u64,
    pub last_accessed: i64,
    pub created_at: i64,
    pub properties: HashMap<String, String>,
    pub tf_idf_vector: HashMap<String, f64>, // Term frequencies
}

impl ConceptNode {
    pub fn new(label: &str, domain: &str) -> Self {
        let id = sha3_256_hex(
            format!(
                "concept:{}:{}:{}",
                label,
                domain,
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
            )
            .as_bytes(),
        )[..12]
            .to_string();
        let now = chrono::Utc::now().timestamp_millis();

        // Build TF-IDF vector from label
        let mut tf = HashMap::new();
        for word in label
            .split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|w| w.len() > 2)
        {
            *tf.entry(word).or_insert(0.0) += 1.0;
        }

        Self {
            id,
            label: label.to_string(),
            domain: domain.to_string(),
            activation: 0.5,
            base_activation: 0.3,
            access_count: 0,
            last_accessed: now,
            created_at: now,
            properties: HashMap::new(),
            tf_idf_vector: tf,
        }
    }

    /// Decay activation toward base level.
    pub fn decay(&mut self, rate: f64) {
        self.activation += (self.base_activation - self.activation) * rate;
    }

    /// Boost activation (e.g., when accessed or receiving spread).
    pub fn activate(&mut self, boost: f64) {
        self.activation = (self.activation + boost).min(1.0);
        self.access_count += 1;
        self.last_accessed = chrono::Utc::now().timestamp_millis();
        // Hebbian: increase base activation for frequently accessed concepts
        self.base_activation = (self.base_activation + 0.01).min(0.8);
    }
}

// ═══════════════════════════════════════════════════════════════
// TYPED RELATIONSHIPS
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    IsA,
    HasPart,
    CausedBy,
    Causes,
    SimilarTo,
    OppositeOf,
    UsedFor,
    LocatedIn,
    TemporalBefore,
    TemporalAfter,
    DependsOn,
    Implements,
    ExampleOf,
    PropertyOf,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub source_id: String,
    pub target_id: String,
    pub relation: RelationType,
    pub weight: f64,
    pub evidence_count: u32,
    pub created_at: i64,
}

// ═══════════════════════════════════════════════════════════════
// DOM NODE (kept from v1 for backward compatibility)
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNode {
    pub id: String,
    pub tag: String,
    pub text_content: String,
    pub attributes: HashMap<String, String>,
    pub is_interactive: bool,
    pub is_visible: bool,
    pub semantic_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConnectomeAction {
    Click(String),
    ExtractText(String),
    ScrollDown,
    NavigateBack,
    FormFill(String, String),
    Wait(u64),
    Success,
    GiveUp,
}

// ═══════════════════════════════════════════════════════════════
// KNOWLEDGE CONNECTOME v3.0
// ═══════════════════════════════════════════════════════════════

pub struct DomForager {
    // Knowledge graph
    concepts: HashMap<String, ConceptNode>,
    relationships: Vec<Relationship>,
    // Reverse index: concept_id -> connected relationship indices
    adjacency: HashMap<String, Vec<usize>>,
    // DOM planning (MCTS pheromones)
    pheromones: HashMap<String, f64>,
    // IDF (inverse document frequency) for TF-IDF similarity
    doc_freq: HashMap<String, u32>,
    total_docs: u32,
    // Stats
    total_thoughts: u64,
    total_concepts_created: u64,
    total_relationships_created: u64,
}

impl DomForager {
    pub fn new() -> Self {
        Self {
            concepts: HashMap::new(),
            relationships: Vec::new(),
            adjacency: HashMap::new(),
            pheromones: HashMap::new(),
            doc_freq: HashMap::new(),
            total_docs: 0,
            total_thoughts: 0,
            total_concepts_created: 0,
            total_relationships_created: 0,
        }
    }

    // ═══════════════════════════════════════════════════════════
    // KNOWLEDGE GRAPH OPERATIONS
    // ═══════════════════════════════════════════════════════════

    /// Add a concept to the knowledge graph.
    pub fn add_concept(&mut self, label: &str, domain: &str) -> String {
        // Check if concept already exists
        if let Some(existing) = self
            .concepts
            .values()
            .find(|c| c.label.to_lowercase() == label.to_lowercase() && c.domain == domain)
        {
            let id = existing.id.clone();
            if let Some(c) = self.concepts.get_mut(&id) {
                c.activate(0.2);
            }
            return id;
        }

        let concept = ConceptNode::new(label, domain);
        let id = concept.id.clone();

        // Update IDF
        self.total_docs += 1;
        for word in concept.tf_idf_vector.keys() {
            *self.doc_freq.entry(word.clone()).or_insert(0) += 1;
        }

        self.concepts.insert(id.clone(), concept);
        self.total_concepts_created += 1;
        id
    }

    /// Add a relationship between two concepts.
    pub fn add_relationship(
        &mut self,
        source_id: &str,
        target_id: &str,
        relation: RelationType,
        weight: f64,
    ) {
        // Check for duplicate
        let exists = self.relationships.iter().any(|r| {
            r.source_id == source_id && r.target_id == target_id && r.relation == relation
        });
        if exists {
            // Strengthen existing
            if let Some(r) = self.relationships.iter_mut().find(|r| {
                r.source_id == source_id && r.target_id == target_id && r.relation == relation
            }) {
                r.weight = (r.weight + weight * 0.3).min(1.0);
                r.evidence_count += 1;
            }
            return;
        }

        let idx = self.relationships.len();
        self.relationships.push(Relationship {
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            relation,
            weight,
            evidence_count: 1,
            created_at: chrono::Utc::now().timestamp_millis(),
        });
        self.adjacency
            .entry(source_id.to_string())
            .or_default()
            .push(idx);
        self.adjacency
            .entry(target_id.to_string())
            .or_default()
            .push(idx);
        self.total_relationships_created += 1;
    }

    /// Extract concepts and relationships from text content.
    pub fn ingest_text(&mut self, text: &str, domain: &str, source_url: &str) {
        let sentences: Vec<&str> = text
            .split(&['.', '!', '?', '\n'])
            .filter(|s| s.len() > 15 && s.len() < 500)
            .collect();

        let mut prev_concept_id: Option<String> = None;

        for sentence in sentences.iter().take(20) {
            let words: Vec<String> = sentence
                .split_whitespace()
                .map(|w| {
                    w.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .filter(|w| w.len() > 3)
                .collect();

            if words.is_empty() {
                continue;
            }

            // Create concept from sentence
            let label = words.iter().take(5).cloned().collect::<Vec<_>>().join(" ");
            let concept_id = self.add_concept(&label, domain);

            // Add source property
            if let Some(c) = self.concepts.get_mut(&concept_id) {
                c.properties
                    .insert("source".to_string(), source_url.to_string());
            }

            // Link to previous concept (temporal sequence)
            if let Some(prev_id) = &prev_concept_id {
                self.add_relationship(prev_id, &concept_id, RelationType::TemporalAfter, 0.5);
            }

            // Detect causal relationships
            let lower = sentence.to_lowercase();
            if lower.contains("because") || lower.contains("causes") || lower.contains("due to") {
                if let Some(prev_id) = &prev_concept_id {
                    self.add_relationship(&concept_id, prev_id, RelationType::CausedBy, 0.7);
                }
            }
            if lower.contains("is a") || lower.contains("are a") || lower.contains("type of") {
                if let Some(prev_id) = &prev_concept_id {
                    self.add_relationship(&concept_id, prev_id, RelationType::IsA, 0.6);
                }
            }

            prev_concept_id = Some(concept_id);
        }
    }

    /// Spreading activation: activate a concept and propagate.
    pub fn spread_activation(&mut self, concept_id: &str, initial_boost: f64, max_depth: usize) {
        let mut queue: VecDeque<(String, f64, usize)> = VecDeque::new();
        queue.push_back((concept_id.to_string(), initial_boost, 0));
        let mut visited = HashSet::new();

        while let Some((current_id, energy, depth)) = queue.pop_front() {
            if depth >= max_depth || energy < 0.05 || visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            if let Some(concept) = self.concepts.get_mut(&current_id) {
                concept.activate(energy);
            }

            // Spread to neighbors
            if let Some(rel_indices) = self.adjacency.get(&current_id).cloned() {
                for idx in rel_indices {
                    if let Some(rel) = self.relationships.get(idx) {
                        let neighbor_id = if rel.source_id == current_id {
                            &rel.target_id
                        } else {
                            &rel.source_id
                        };
                        let spread_energy = energy * rel.weight * 0.6;
                        if spread_energy > 0.05 {
                            queue.push_back((neighbor_id.clone(), spread_energy, depth + 1));
                        }
                    }
                }
            }
        }
    }

    /// TF-IDF cosine similarity between a query and all concepts.
    pub fn semantic_search(&self, query: &str, top_k: usize) -> Vec<(&ConceptNode, f64)> {
        let query_tf: HashMap<String, f64> = {
            let mut tf = HashMap::new();
            for word in query
                .split_whitespace()
                .map(|w| {
                    w.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .filter(|w| w.len() > 2)
            {
                *tf.entry(word).or_insert(0.0) += 1.0;
            }
            tf
        };

        let total = self.total_docs.max(1) as f64;
        // Build query TF-IDF
        let query_tfidf: HashMap<&String, f64> = query_tf
            .iter()
            .map(|(word, tf)| {
                let df = *self.doc_freq.get(word).unwrap_or(&1) as f64;
                let idf = (total / df).ln().max(0.0);
                (word, tf * idf)
            })
            .collect();

        let query_norm: f64 = query_tfidf
            .values()
            .map(|v| v * v)
            .sum::<f64>()
            .sqrt()
            .max(1e-10);

        let mut scored: Vec<_> = self
            .concepts
            .values()
            .map(|concept| {
                let concept_tfidf: HashMap<&String, f64> = concept
                    .tf_idf_vector
                    .iter()
                    .map(|(word, tf)| {
                        let df = *self.doc_freq.get(word).unwrap_or(&1) as f64;
                        let idf = (total / df).ln().max(0.0);
                        (word, tf * idf)
                    })
                    .collect();

                let concept_norm: f64 = concept_tfidf
                    .values()
                    .map(|v| v * v)
                    .sum::<f64>()
                    .sqrt()
                    .max(1e-10);

                let dot: f64 = query_tfidf
                    .iter()
                    .map(|(word, q_val)| {
                        let c_val = concept_tfidf.get(word).unwrap_or(&0.0);
                        q_val * c_val
                    })
                    .sum();

                let cosine = dot / (query_norm * concept_norm);
                // Blend with activation level
                let final_score = cosine * 0.7 + concept.activation * 0.3;
                (concept, final_score)
            })
            .filter(|(_, s)| *s > 0.01)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    /// Decay all concept activations.
    pub fn global_decay(&mut self, rate: f64) {
        for concept in self.concepts.values_mut() {
            concept.decay(rate);
        }
    }

    /// Get most active concepts.
    pub fn get_active_concepts(&self, top_k: usize) -> Vec<&ConceptNode> {
        let mut sorted: Vec<_> = self.concepts.values().collect();
        sorted.sort_by(|a, b| {
            b.activation
                .partial_cmp(&a.activation)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(top_k);
        sorted
    }

    /// Get all relationships for a concept.
    pub fn get_relationships(&self, concept_id: &str) -> Vec<&Relationship> {
        self.adjacency
            .get(concept_id)
            .map(|indices| {
                indices
                    .iter()
                    .filter_map(|i| self.relationships.get(*i))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find path between two concepts (BFS).
    pub fn find_path(&self, from_id: &str, to_id: &str, max_depth: usize) -> Option<Vec<String>> {
        let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::new();
        queue.push_back((from_id.to_string(), vec![from_id.to_string()]));
        let mut visited = HashSet::new();

        while let Some((current, path)) = queue.pop_front() {
            if current == to_id {
                return Some(path);
            }
            if path.len() > max_depth || visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            if let Some(rel_indices) = self.adjacency.get(&current) {
                for idx in rel_indices {
                    if let Some(rel) = self.relationships.get(*idx) {
                        let neighbor = if rel.source_id == current {
                            &rel.target_id
                        } else {
                            &rel.source_id
                        };
                        if !visited.contains(neighbor) {
                            let mut new_path = path.clone();
                            new_path.push(neighbor.clone());
                            queue.push_back((neighbor.clone(), new_path));
                        }
                    }
                }
            }
        }
        None
    }

    // ═══════════════════════════════════════════════════════════
    // DOM PLANNING (MCTS — preserved from v1)
    // ═══════════════════════════════════════════════════════════

    /// Plan best action for DOM interaction via MCTS.
    pub fn contemplate(&mut self, goal: &str, dom_nodes: &[DomNode]) -> ConnectomeAction {
        self.total_thoughts += 1;
        let possible = self.extract_affordances(dom_nodes);
        if possible.is_empty() {
            return ConnectomeAction::ScrollDown;
        }
        let state_hash = self.hash_state(goal, dom_nodes);

        // Quick MCTS
        let mut scores: Vec<(ConnectomeAction, f64)> = possible
            .iter()
            .map(|action| {
                let mut total = 0.0;
                for _ in 0..500 {
                    total += self.simulate_action(action, goal, dom_nodes, &state_hash);
                }
                (action.clone(), total / 500.0)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
            .first()
            .map(|(a, _)| a.clone())
            .unwrap_or(ConnectomeAction::GiveUp)
    }

    pub fn reinforce(
        &mut self,
        goal: &str,
        dom_nodes: &[DomNode],
        action: &ConnectomeAction,
        reward: f64,
    ) {
        let state_hash = self.hash_state(goal, dom_nodes);
        let key = format!("{}::{:?}", state_hash, action);
        let current = self.pheromones.get(&key).copied().unwrap_or(0.0);
        self.pheromones
            .insert(key, current + 0.1 * (reward - current));
    }

    fn extract_affordances(&self, nodes: &[DomNode]) -> Vec<ConnectomeAction> {
        let mut actions = vec![ConnectomeAction::ScrollDown, ConnectomeAction::NavigateBack];
        for node in nodes {
            if node.is_interactive && node.is_visible {
                if node.tag == "input" || node.tag == "textarea" {
                    actions.push(ConnectomeAction::FormFill(node.id.clone(), String::new()));
                } else {
                    actions.push(ConnectomeAction::Click(node.id.clone()));
                }
            }
            if !node.text_content.trim().is_empty() && node.is_visible {
                actions.push(ConnectomeAction::ExtractText(node.id.clone()));
            }
        }
        actions
    }

    fn simulate_action(
        &self,
        action: &ConnectomeAction,
        goal: &str,
        dom_nodes: &[DomNode],
        state_hash: &str,
    ) -> f64 {
        let key = format!("{}::{:?}", state_hash, action);
        let pheromone = self.pheromones.get(&key).copied().unwrap_or(0.0);
        let mut score = 0.1;

        let node_id = match action {
            ConnectomeAction::ExtractText(id) | ConnectomeAction::Click(id) => Some(id),
            ConnectomeAction::ScrollDown => return (0.15 + pheromone).clamp(0.0, 1.0),
            ConnectomeAction::NavigateBack => return (0.05 + pheromone).clamp(0.0, 1.0),
            _ => None,
        };

        if let Some(id) = node_id {
            if let Some(node) = dom_nodes.iter().find(|n| &n.id == id) {
                let keywords: HashSet<String> = goal
                    .to_lowercase()
                    .split_whitespace()
                    .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
                    .filter(|w| w.len() > 3)
                    .collect();

                let node_text = node.text_content.to_lowercase();
                let attrs: String = node
                    .attributes
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();

                for k in &keywords {
                    if node_text.contains(k) {
                        score += 0.2;
                    }
                    if attrs.contains(k) {
                        score += 0.1;
                    }
                }

                let is_extract = matches!(action, ConnectomeAction::ExtractText(_));
                if is_extract && score > 0.3 {
                    score += 0.4;
                }
                if !is_extract && (node.tag == "a" || node.tag == "button") && score > 0.1 {
                    score += 0.1;
                }
            }
        }
        (score + pheromone).clamp(0.0, 1.0)
    }

    fn hash_state(&self, goal: &str, nodes: &[DomNode]) -> String {
        let mut ids: Vec<String> = nodes
            .iter()
            .filter(|n| {
                (n.is_interactive && n.is_visible)
                    || (n.is_visible && !n.text_content.trim().is_empty())
            })
            .map(|n| n.id.clone())
            .collect();
        ids.sort();
        sha3_256_hex(format!("{}::{}", goal, ids.join(",")).as_bytes())[..16].to_string()
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "KnowledgeConnectome v3.0",
            "total_concepts": self.concepts.len(),
            "total_relationships": self.relationships.len(),
            "total_thoughts": self.total_thoughts,
            "learned_pheromones": self.pheromones.len(),
            "total_concepts_created": self.total_concepts_created,
            "total_relationships_created": self.total_relationships_created,
            "idf_vocabulary_size": self.doc_freq.len(),
        })
    }
}

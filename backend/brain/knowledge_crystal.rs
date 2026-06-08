// ─────────────────────────────────────────────────────────────
// Knowledge Crystal — Self-Growing Knowledge Base
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/knowledge_crystal.py

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// A knowledge fact stored in the crystal.
#[derive(Debug, Clone)]
pub struct KnowledgeFact {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f64,
    pub source: String,
    pub created_at: f64,
    pub access_count: u64,
    pub last_accessed: f64,
    pub tags: Vec<String>,
}

/// Query result from the knowledge crystal.
#[derive(Debug, Clone)]
pub struct KnowledgeQuery {
    pub facts: Vec<KnowledgeFact>,
    pub total_matches: usize,
    pub query_time_ms: f64,
}

/// Knowledge Crystal — self-growing knowledge base with associative recall.
pub struct KnowledgeCrystal {
    facts: HashMap<String, KnowledgeFact>,
    subject_index: HashMap<String, Vec<String>>,
    predicate_index: HashMap<String, Vec<String>>,
    tag_index: HashMap<String, Vec<String>>,
    next_id: u64,
}

impl KnowledgeCrystal {
    pub fn new() -> Self {
        Self {
            facts: HashMap::new(),
            subject_index: HashMap::new(),
            predicate_index: HashMap::new(),
            tag_index: HashMap::new(),
            next_id: 0,
        }
    }

    /// Add a knowledge fact (subject-predicate-object triple).
    pub fn add_fact(
        &mut self,
        subject: &str,
        predicate: &str,
        object: &str,
        confidence: f64,
        source: &str,
        tags: Vec<String>,
    ) -> String {
        let id = format!("fact_{}", self.next_id);
        self.next_id += 1;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let fact = KnowledgeFact {
            id: id.clone(),
            subject: subject.to_string(),
            predicate: predicate.to_string(),
            object: object.to_string(),
            confidence,
            source: source.to_string(),
            created_at: now,
            access_count: 0,
            last_accessed: now,
            tags: tags.clone(),
        };

        // Update indices
        self.subject_index
            .entry(subject.to_lowercase())
            .or_default()
            .push(id.clone());
        self.predicate_index
            .entry(predicate.to_lowercase())
            .or_default()
            .push(id.clone());
        for tag in &tags {
            self.tag_index
                .entry(tag.to_lowercase())
                .or_default()
                .push(id.clone());
        }

        self.facts.insert(id.clone(), fact);
        id
    }

    /// Query facts by subject.
    pub fn query_by_subject(&mut self, subject: &str) -> Vec<&KnowledgeFact> {
        let ids = self
            .subject_index
            .get(&subject.to_lowercase())
            .cloned()
            .unwrap_or_default();
        self.access_facts(&ids)
    }

    /// Query facts by predicate.
    pub fn query_by_predicate(&mut self, predicate: &str) -> Vec<&KnowledgeFact> {
        let ids = self
            .predicate_index
            .get(&predicate.to_lowercase())
            .cloned()
            .unwrap_or_default();
        self.access_facts(&ids)
    }

    /// Query facts by tag.
    pub fn query_by_tag(&mut self, tag: &str) -> Vec<&KnowledgeFact> {
        let ids = self
            .tag_index
            .get(&tag.to_lowercase())
            .cloned()
            .unwrap_or_default();
        self.access_facts(&ids)
    }

    /// Free-text search across all facts.
    pub fn search(&mut self, query: &str) -> Vec<&KnowledgeFact> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(String, f64)> = self
            .facts
            .iter()
            .map(|(id, fact)| {
                let text = format!(
                    "{} {} {} {}",
                    fact.subject,
                    fact.predicate,
                    fact.object,
                    fact.tags.join(" ")
                )
                .to_lowercase();
                let score = query_words.iter().filter(|w| text.contains(*w)).count() as f64
                    / query_words.len().max(1) as f64;
                (id.clone(), score * fact.confidence)
            })
            .filter(|(_, s)| *s > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top_ids: Vec<String> = scored.iter().take(20).map(|(id, _)| id.clone()).collect();
        self.access_facts(&top_ids)
    }

    /// Get the most accessed facts (hot knowledge).
    pub fn hot_facts(&self, limit: usize) -> Vec<&KnowledgeFact> {
        let mut sorted: Vec<&KnowledgeFact> = self.facts.values().collect();
        sorted.sort_by(|a, b| b.access_count.cmp(&a.access_count));
        sorted.truncate(limit);
        sorted
    }

    /// Crystallize: merge similar facts to increase confidence.
    pub fn crystallize(&mut self) {
        let fact_ids: Vec<String> = self.facts.keys().cloned().collect();
        let mut merged = Vec::new();

        for i in 0..fact_ids.len() {
            for j in (i + 1)..fact_ids.len() {
                let a = &self.facts[&fact_ids[i]];
                let b = &self.facts[&fact_ids[j]];
                if a.subject == b.subject && a.predicate == b.predicate && a.object == b.object {
                    merged.push((fact_ids[i].clone(), fact_ids[j].clone()));
                }
            }
        }

        for (keep_id, remove_id) in merged {
            if let Some(removed) = self.facts.remove(&remove_id) {
                if let Some(kept) = self.facts.get_mut(&keep_id) {
                    kept.confidence = (kept.confidence + removed.confidence).min(1.0);
                    kept.access_count += removed.access_count;
                    for tag in removed.tags {
                        if !kept.tags.contains(&tag) {
                            kept.tags.push(tag);
                        }
                    }
                }
            }
        }
    }

    fn access_facts(&mut self, ids: &[String]) -> Vec<&KnowledgeFact> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        for id in ids {
            if let Some(fact) = self.facts.get_mut(id) {
                fact.access_count += 1;
                fact.last_accessed = now;
            }
        }
        ids.iter().filter_map(|id| self.facts.get(id)).collect()
    }

    pub fn total_facts(&self) -> usize {
        self.facts.len()
    }
}

impl Default for KnowledgeCrystal {
    fn default() -> Self {
        Self::new()
    }
}

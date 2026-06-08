// ═══════════════════════════════════════════════════════════════
// BM25 Memory — Keyword-Based Retrieval + Bug Diary
// ═══════════════════════════════════════════════════════════════
// Provides BM25 (Best Matching 25) text retrieval over a memory
// store of past experiences, bug patterns, and failure modes.
//
// BM25 Score: score(D,Q) = Σ IDF(q) · (tf(q,D) · (k1+1)) / (tf(q,D) + k1 · (1 - b + b · |D|/avgdl))
//
// Features:
//   - BM25 ranked retrieval
//   - Bug diary — tracks past failures and their fixes
//   - Failure pattern memory — learns from mistakes
//   - Automatic term frequency index

use crate::crypto::hash::sha3_256_hex;
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;

// ── Memory Entry ──

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub category: MemoryCategory,
    pub tokens: Vec<String>,
    pub token_freq: HashMap<String, u32>,
    pub created_at: i64,
    pub access_count: u64,
    pub importance: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MemoryCategory {
    Experience,
    Bug,
    Fix,
    Failure,
    Lesson,
    Pattern,
}

// ── Bug Diary Entry ──

#[derive(Debug, Clone, Serialize)]
pub struct BugDiaryEntry {
    pub id: String,
    pub error_type: String,
    pub description: String,
    pub root_cause: String,
    pub fix: String,
    pub module: String,
    pub occurrences: u32,
    pub first_seen: i64,
    pub last_seen: i64,
    pub resolved: bool,
}

// ── BM25 Memory Engine ──

pub struct BM25Memory {
    entries: Vec<MemoryEntry>,
    bug_diary: Vec<BugDiaryEntry>,
    /// BM25 parameters
    k1: f64,
    b: f64,
    /// Document frequency: term → number of docs containing term
    doc_freq: HashMap<String, u32>,
    /// Total number of documents
    total_docs: u32,
    /// Average document length
    avg_doc_len: f64,
    /// Total token count
    total_tokens: u64,
    /// Maximum entries
    max_entries: usize,
}

impl BM25Memory {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            bug_diary: Vec::new(),
            k1: 1.5,
            b: 0.75,
            doc_freq: HashMap::new(),
            total_docs: 0,
            avg_doc_len: 0.0,
            total_tokens: 0,
            max_entries: 50000,
        }
    }

    /// Store a memory with automatic BM25 indexing.
    pub fn store(
        &mut self,
        content: &str,
        category: MemoryCategory,
        importance: f64,
        metadata: serde_json::Value,
    ) -> String {
        let id = sha3_256_hex(
            format!(
                "bm25:{}:{}",
                content,
                Utc::now().timestamp_nanos_opt().unwrap_or(0)
            )
            .as_bytes(),
        )[..16]
            .to_string();

        let tokens = tokenize(content);
        let mut token_freq = HashMap::new();
        for t in &tokens {
            *token_freq.entry(t.clone()).or_insert(0u32) += 1;
        }

        // Update document frequency index
        let unique_tokens: std::collections::HashSet<&String> = tokens.iter().collect();
        for t in &unique_tokens {
            *self.doc_freq.entry((*t).clone()).or_insert(0) += 1;
        }

        self.total_docs += 1;
        self.total_tokens += tokens.len() as u64;
        self.avg_doc_len = self.total_tokens as f64 / self.total_docs as f64;

        let entry = MemoryEntry {
            id: id.clone(),
            content: content.to_string(),
            category,
            tokens,
            token_freq,
            created_at: Utc::now().timestamp(),
            access_count: 0,
            importance,
            metadata,
        };

        self.entries.push(entry);

        // Evict if needed (keep most important)
        if self.entries.len() > self.max_entries {
            self.entries.sort_by(|a, b| {
                b.importance
                    .partial_cmp(&a.importance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.entries.truncate(self.max_entries);
        }

        id
    }

    /// BM25 retrieval — returns top-k entries ranked by relevance.
    pub fn search(&mut self, query: &str, top_k: usize) -> Vec<BM25Result> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() || self.entries.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<BM25Result> = self
            .entries
            .iter()
            .map(|entry| {
                let score = self.bm25_score(&query_tokens, entry);
                BM25Result {
                    id: entry.id.clone(),
                    content: entry.content.clone(),
                    category: entry.category,
                    score,
                    importance: entry.importance,
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(top_k);

        // Update access counts
        for result in &scored {
            if let Some(entry) = self.entries.iter_mut().find(|e| e.id == result.id) {
                entry.access_count += 1;
            }
        }

        scored
    }

    /// Compute BM25 score for a query against a document.
    fn bm25_score(&self, query_tokens: &[String], entry: &MemoryEntry) -> f64 {
        let doc_len = entry.tokens.len() as f64;
        let mut score: f64 = 0.0;

        for qt in query_tokens {
            let tf = entry.token_freq.get(qt).copied().unwrap_or(0) as f64;
            let df = self.doc_freq.get(qt).copied().unwrap_or(0) as f64;

            // IDF = ln((N - df + 0.5) / (df + 0.5) + 1)
            let idf = ((self.total_docs as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

            // TF component with length normalization
            let tf_norm = (tf * (self.k1 + 1.0))
                / (tf + self.k1 * (1.0 - self.b + self.b * doc_len / self.avg_doc_len.max(1.0)));

            score += idf * tf_norm;
        }

        // Importance boost
        score *= 1.0 + entry.importance * 0.2;

        score
    }

    // ── Bug Diary ──

    /// Record a bug/failure for future reference.
    pub fn record_bug(
        &mut self,
        error_type: &str,
        description: &str,
        root_cause: &str,
        fix: &str,
        module: &str,
    ) -> String {
        // Check if same bug exists — increment occurrences
        for bug in &mut self.bug_diary {
            if bug.error_type == error_type && bug.module == module {
                bug.occurrences += 1;
                bug.last_seen = Utc::now().timestamp();
                if !fix.is_empty() {
                    bug.fix = fix.to_string();
                    bug.resolved = true;
                }
                return bug.id.clone();
            }
        }

        let id = format!("BUG-{:04}", self.bug_diary.len() + 1);
        let entry = BugDiaryEntry {
            id: id.clone(),
            error_type: error_type.to_string(),
            description: description.to_string(),
            root_cause: root_cause.to_string(),
            fix: fix.to_string(),
            module: module.to_string(),
            occurrences: 1,
            first_seen: Utc::now().timestamp(),
            last_seen: Utc::now().timestamp(),
            resolved: !fix.is_empty(),
        };

        // Also store in BM25 index
        self.store(
            &format!(
                "Bug: {} in {} — {} Fix: {}",
                error_type, module, description, fix
            ),
            MemoryCategory::Bug,
            0.8,
            serde_json::json!({"bug_id": &id}),
        );

        self.bug_diary.push(entry);
        id
    }

    /// Search bug diary for matching errors.
    pub fn search_bugs(&self, error_type: &str) -> Vec<&BugDiaryEntry> {
        let lower = error_type.to_lowercase();
        self.bug_diary
            .iter()
            .filter(|b| {
                b.error_type.to_lowercase().contains(&lower)
                    || b.description.to_lowercase().contains(&lower)
            })
            .collect()
    }

    /// Record a lesson learned from a failure.
    pub fn record_lesson(&mut self, lesson: &str, context: &str) -> String {
        self.store(
            &format!("Lesson: {} Context: {}", lesson, context),
            MemoryCategory::Lesson,
            0.9, // lessons are high importance
            serde_json::json!({"type": "lesson"}),
        )
    }

    pub fn get_stats(&self) -> serde_json::Value {
        let categories: HashMap<String, usize> = {
            let mut m = HashMap::new();
            for e in &self.entries {
                *m.entry(format!("{:?}", e.category)).or_insert(0) += 1;
            }
            m
        };

        serde_json::json!({
            "engine": "BM25Memory",
            "total_entries": self.entries.len(),
            "total_bugs": self.bug_diary.len(),
            "resolved_bugs": self.bug_diary.iter().filter(|b| b.resolved).count(),
            "total_tokens": self.total_tokens,
            "avg_doc_length": self.avg_doc_len,
            "unique_terms": self.doc_freq.len(),
            "categories": categories,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BM25Result {
    pub id: String,
    pub content: String,
    pub category: MemoryCategory,
    pub score: f64,
    pub importance: f64,
}

/// Simple tokenizer: lowercase, split on whitespace/punctuation, filter short tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_and_search() {
        let mut mem = BM25Memory::new();
        mem.store(
            "quicksort has O(n log n) average time complexity",
            MemoryCategory::Experience,
            0.8,
            serde_json::json!({}),
        );
        mem.store(
            "merge sort has O(n log n) worst case guarantee",
            MemoryCategory::Experience,
            0.7,
            serde_json::json!({}),
        );
        mem.store(
            "bubble sort has O(n^2) time complexity",
            MemoryCategory::Experience,
            0.5,
            serde_json::json!({}),
        );

        let results = mem.search("quicksort time complexity", 2);
        assert!(!results.is_empty());
        // quicksort entry should rank highest
        assert!(results[0].content.contains("quicksort"));
    }

    #[test]
    fn test_bug_diary() {
        let mut mem = BM25Memory::new();
        let id = mem.record_bug(
            "IndexOutOfBounds",
            "Array access at invalid index during reasoning",
            "Off-by-one in loop bound",
            "Changed < to <=",
            "intelligence/reasoning.rs",
        );
        assert!(id.starts_with("BUG-"));

        // Search for it
        let bugs = mem.search_bugs("IndexOutOfBounds");
        assert_eq!(bugs.len(), 1);
        assert!(bugs[0].resolved);
    }

    #[test]
    fn test_bm25_ranking() {
        let mut mem = BM25Memory::new();
        mem.store(
            "Rust is a systems programming language focused on safety",
            MemoryCategory::Experience,
            0.6,
            serde_json::json!({}),
        );
        mem.store(
            "Python is a high-level programming language",
            MemoryCategory::Experience,
            0.6,
            serde_json::json!({}),
        );
        mem.store(
            "Rust programming with memory safety guarantees",
            MemoryCategory::Experience,
            0.6,
            serde_json::json!({}),
        );

        let results = mem.search("Rust programming safety", 3);
        assert!(results.len() >= 2);
        // Rust entries should rank higher than Python
        assert!(results[0].content.contains("Rust"));
    }
}

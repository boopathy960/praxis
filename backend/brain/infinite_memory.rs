// ─────────────────────────────────────────────────────────────
// Infinite Memory Engine — 4-Tier Hierarchical Memory
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/infinite_memory_engine.py

use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Memory item stored in a tier.
#[derive(Debug, Clone)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub importance: f64,
    pub access_count: u64,
    pub created_at: f64,
    pub last_accessed: f64,
    pub decay_rate: f64,
    pub associations: Vec<String>,
    pub tags: Vec<String>,
}

impl MemoryItem {
    pub fn new(id: &str, content: &str, importance: f64) -> Self {
        let now_ts = now();
        Self {
            id: id.to_string(),
            content: content.to_string(),
            importance,
            access_count: 0,
            created_at: now_ts,
            last_accessed: now_ts,
            decay_rate: 0.01,
            associations: Vec::new(),
            tags: Vec::new(),
        }
    }

    /// Current effective importance (decays over time).
    pub fn effective_importance(&self) -> f64 {
        let elapsed = now() - self.last_accessed;
        let decay = (-self.decay_rate * elapsed).exp();
        self.importance * decay * (1.0 + (self.access_count as f64).ln().max(0.0))
    }
}

/// The four memory tiers.
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryTier {
    Working,      // Fast, small, volatile
    ShortTerm,    // Medium, decays
    LongTerm,     // Large, persistent
    Crystallized, // Permanent, high-confidence
}

/// 4-Tier Hierarchical Memory System.
/// Working → ShortTerm → LongTerm → Crystallized
pub struct InfiniteMemoryEngine {
    working: VecDeque<MemoryItem>,             // Tier 1: ~20 items
    short_term: VecDeque<MemoryItem>,          // Tier 2: ~200 items
    long_term: HashMap<String, MemoryItem>,    // Tier 3: ~10K items
    crystallized: HashMap<String, MemoryItem>, // Tier 4: unlimited, permanent
    working_capacity: usize,
    short_term_capacity: usize,
    long_term_capacity: usize,
    next_id: u64,
    consolidation_threshold: f64,
    crystallization_threshold: u64,
}

impl InfiniteMemoryEngine {
    pub fn new() -> Self {
        Self {
            working: VecDeque::new(),
            short_term: VecDeque::new(),
            long_term: HashMap::new(),
            crystallized: HashMap::new(),
            working_capacity: 20,
            short_term_capacity: 200,
            long_term_capacity: 10000,
            next_id: 0,
            consolidation_threshold: 0.5,
            crystallization_threshold: 50,
        }
    }

    /// Store a new memory item in working memory.
    pub fn store(&mut self, content: &str, importance: f64, tags: Vec<String>) -> String {
        let id = format!("mem_{}", self.next_id);
        self.next_id += 1;

        let mut item = MemoryItem::new(&id, content, importance);
        item.tags = tags;

        // High-importance items go directly to short-term
        if importance > 0.8 {
            self.short_term.push_front(item);
        } else {
            self.working.push_front(item);
        }

        // Evict if over capacity
        self.enforce_capacity();
        id
    }

    /// Recall memory by searching across all tiers.
    pub fn recall(&mut self, query: &str, max_results: usize) -> Vec<MemoryItem> {
        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower.split_whitespace().collect();
        let mut scored: Vec<(MemoryItem, f64)> = Vec::new();

        // Search all tiers (crystallized first for priority)
        let all_items: Vec<&MemoryItem> = self
            .crystallized
            .values()
            .chain(self.long_term.values())
            .chain(self.short_term.iter())
            .chain(self.working.iter())
            .collect();

        for item in all_items {
            let text = format!("{} {}", item.content, item.tags.join(" ")).to_lowercase();
            let keyword_matches = keywords.iter().filter(|kw| text.contains(*kw)).count();

            if keyword_matches > 0 {
                let relevance = keyword_matches as f64 / keywords.len().max(1) as f64;
                let score = relevance * item.effective_importance();
                scored.push((item.clone(), score));
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(max_results);

        // Update access counts
        let recalled_ids: Vec<String> = scored.iter().map(|(item, _)| item.id.clone()).collect();
        self.touch_items(&recalled_ids);

        scored.into_iter().map(|(item, _)| item).collect()
    }

    /// Associate two memory items.
    pub fn associate(&mut self, id_a: &str, id_b: &str) {
        // Find and update in any tier
        for item in self.working.iter_mut().chain(self.short_term.iter_mut()) {
            if item.id == id_a && !item.associations.contains(&id_b.to_string()) {
                item.associations.push(id_b.to_string());
            }
            if item.id == id_b && !item.associations.contains(&id_a.to_string()) {
                item.associations.push(id_a.to_string());
            }
        }
        for map in [&mut self.long_term, &mut self.crystallized] {
            if let Some(item) = map.get_mut(id_a) {
                if !item.associations.contains(&id_b.to_string()) {
                    item.associations.push(id_b.to_string());
                }
            }
            if let Some(item) = map.get_mut(id_b) {
                if !item.associations.contains(&id_a.to_string()) {
                    item.associations.push(id_a.to_string());
                }
            }
        }
    }

    /// Consolidate: promote items from lower tiers to higher tiers.
    pub fn consolidate(&mut self) {
        // Working → Short-term (if importance is above threshold)
        let mut promote_to_st = Vec::new();
        self.working.retain(|item| {
            if item.effective_importance() > self.consolidation_threshold {
                promote_to_st.push(item.clone());
                false
            } else {
                true
            }
        });
        for item in promote_to_st {
            self.short_term.push_back(item);
        }

        // Short-term → Long-term (if accessed enough)
        let mut promote_to_lt = Vec::new();
        self.short_term.retain(|item| {
            if item.access_count >= 5 {
                promote_to_lt.push(item.clone());
                false
            } else {
                true
            }
        });
        for item in promote_to_lt {
            self.long_term.insert(item.id.clone(), item);
        }

        // Long-term → Crystallized (if highly accessed)
        let mut promote_to_crystal = Vec::new();
        self.long_term.retain(|_, item| {
            if item.access_count >= self.crystallization_threshold {
                promote_to_crystal.push(item.clone());
                false
            } else {
                true
            }
        });
        for mut item in promote_to_crystal {
            item.decay_rate = 0.0; // Crystallized items don't decay
            self.crystallized.insert(item.id.clone(), item);
        }

        self.enforce_capacity();
    }

    fn enforce_capacity(&mut self) {
        while self.working.len() > self.working_capacity {
            self.working.pop_back();
        }
        while self.short_term.len() > self.short_term_capacity {
            self.short_term.pop_back();
        }
        if self.long_term.len() > self.long_term_capacity {
            // Evict least important
            let mut items: Vec<(String, f64)> = self
                .long_term
                .iter()
                .map(|(id, item)| (id.clone(), item.effective_importance()))
                .collect();
            items.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            let to_remove = self.long_term.len() - self.long_term_capacity;
            for (id, _) in items.into_iter().take(to_remove) {
                self.long_term.remove(&id);
            }
        }
    }

    fn touch_items(&mut self, ids: &[String]) {
        let now_ts = now();
        for item in self.working.iter_mut().chain(self.short_term.iter_mut()) {
            if ids.contains(&item.id) {
                item.access_count += 1;
                item.last_accessed = now_ts;
            }
        }
        for map in [&mut self.long_term, &mut self.crystallized] {
            for id in ids {
                if let Some(item) = map.get_mut(id) {
                    item.access_count += 1;
                    item.last_accessed = now_ts;
                }
            }
        }
    }

    pub fn stats(&self) -> HashMap<String, usize> {
        let mut s = HashMap::new();
        s.insert("working".into(), self.working.len());
        s.insert("short_term".into(), self.short_term.len());
        s.insert("long_term".into(), self.long_term.len());
        s.insert("crystallized".into(), self.crystallized.len());
        s.insert(
            "total".into(),
            self.working.len()
                + self.short_term.len()
                + self.long_term.len()
                + self.crystallized.len(),
        );
        s
    }
}

impl Default for InfiniteMemoryEngine {
    fn default() -> Self {
        Self::new()
    }
}

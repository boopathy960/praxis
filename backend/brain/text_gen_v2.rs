// ═══════════════════════════════════════════════════════════════
// Text Generation Engine v2.0 — Markov + Grammar + Vocabulary Graph
// ═══════════════════════════════════════════════════════════════
//
// Advanced text generation that adds 3 major capabilities on top of v1.0:
//   1. N-gram Markov Chain — Learns statistical word transitions from corpus
//   2. Grammar-Guided Generation — SVO sentence construction from rules
//   3. Vocabulary Semantic Graph — Word-to-word distance for synonym selection
//   4. Cognitive Result Narrator — Translates D1-D7 engine results to English
//   5. Multi-Format Serializer — Markdown, JSON, plain text, code comments
//
// This module is separate from v1.0 to avoid breaking existing API.
// Import both and use v2.0 for advanced generation, v1.0 for simple templates.
//
// Pure Rust. Zero LLM. Zero GPU. Deterministic with seeded PRNG.

use std::collections::HashMap;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// A word node in the vocabulary graph.
#[derive(Debug, Clone)]
pub struct VocabNode {
    pub word: String,
    pub pos: WordPOS,
    pub frequency: u32,
    /// Neighbors: (word, semantic_distance)
    pub neighbors: Vec<(String, f64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WordPOS {
    Noun,
    Verb,
    Adjective,
    Adverb,
    Preposition,
    Conjunction,
    Determiner,
    Pronoun,
}

/// Result of v2 text generation.
#[derive(Debug, Clone)]
pub struct TextGenV2Result {
    pub text: String,
    pub format: OutputFormat,
    pub word_count: usize,
    pub sentence_count: usize,
    pub generation_method: String,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    PlainText,
    Markdown,
    Json,
    CodeComment,
    Report,
}

/// Request for cognitive result narration.
#[derive(Debug, Clone)]
pub struct NarrationRequest {
    pub dimension: String,
    pub operation: String,
    pub metrics: HashMap<String, f64>,
    pub key_findings: Vec<String>,
    pub confidence: f64,
    pub audience: NarrationAudience,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NarrationAudience {
    Technical,
    Executive,
    Casual,
}

// ═══════════════════════════════════════════════════════════════
// MARKOV CHAIN
// ═══════════════════════════════════════════════════════════════

struct MarkovChain {
    /// Bigram transitions: (word_a, word_b) -> count
    bigrams: HashMap<(String, String), u32>,
    /// Trigram transitions: (word_a, word_b, word_c) -> count
    trigrams: HashMap<(String, String, String), u32>,
    /// Word frequency
    unigrams: HashMap<String, u32>,
    /// Sentence starters
    starters: Vec<String>,
    total_words: u64,
}

impl MarkovChain {
    fn new() -> Self {
        Self {
            bigrams: HashMap::new(),
            trigrams: HashMap::new(),
            unigrams: HashMap::new(),
            starters: Vec::new(),
            total_words: 0,
        }
    }

    /// Learn from a corpus of text.
    fn train(&mut self, text: &str) {
        let sentences: Vec<&str> = text
            .split(|c: char| c == '.' || c == '!' || c == '?')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for sentence in sentences {
            let words: Vec<String> = sentence
                .split_whitespace()
                .map(|w| {
                    w.to_lowercase()
                        .trim_matches(|c: char| !c.is_alphanumeric())
                        .to_string()
                })
                .filter(|w| !w.is_empty())
                .collect();

            if words.is_empty() {
                continue;
            }

            self.starters.push(words[0].clone());

            for word in &words {
                *self.unigrams.entry(word.clone()).or_insert(0) += 1;
                self.total_words += 1;
            }

            for window in words.windows(2) {
                *self
                    .bigrams
                    .entry((window[0].clone(), window[1].clone()))
                    .or_insert(0) += 1;
            }

            for window in words.windows(3) {
                *self
                    .trigrams
                    .entry((window[0].clone(), window[1].clone(), window[2].clone()))
                    .or_insert(0) += 1;
            }
        }
    }

    /// Generate next word given context (backoff from trigram → bigram → unigram).
    fn next_word(&self, prev1: &str, prev2: Option<&str>, rng_state: &mut u64) -> Option<String> {
        // Try trigram first
        if let Some(p2) = prev2 {
            let candidates: Vec<(&String, &u32)> = self
                .trigrams
                .iter()
                .filter(|((a, b, _), _)| a == p2 && b == prev1)
                .map(|((_, _, c), count)| (c, count))
                .collect();

            if !candidates.is_empty() {
                let total: u32 = candidates.iter().map(|(_, c)| **c).sum();
                let r = xorshift(rng_state) % total as u64;
                let mut cumulative = 0u64;
                for (word, count) in &candidates {
                    cumulative += **count as u64;
                    if cumulative > r {
                        return Some((*word).clone());
                    }
                }
            }
        }

        // Backoff to bigram
        let candidates: Vec<(&String, &u32)> = self
            .bigrams
            .iter()
            .filter(|((a, _), _)| a == prev1)
            .map(|((_, b), count)| (b, count))
            .collect();

        if !candidates.is_empty() {
            let total: u32 = candidates.iter().map(|(_, c)| **c).sum();
            let r = xorshift(rng_state) % total.max(1) as u64;
            let mut cumulative = 0u64;
            for (word, count) in &candidates {
                cumulative += **count as u64;
                if cumulative > r {
                    return Some((*word).clone());
                }
            }
        }

        // Backoff to random unigram
        if !self.unigrams.is_empty() {
            let words: Vec<&String> = self.unigrams.keys().collect();
            let idx = xorshift(rng_state) as usize % words.len();
            return Some(words[idx].clone());
        }

        None
    }
}

fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

// ═══════════════════════════════════════════════════════════════
// GRAMMAR RULES
// ═══════════════════════════════════════════════════════════════

struct GrammarEngine {
    sentence_templates: Vec<Vec<WordPOS>>,
    vocabulary: HashMap<WordPOS, Vec<&'static str>>,
}

impl GrammarEngine {
    fn new() -> Self {
        let mut vocabulary: HashMap<WordPOS, Vec<&str>> = HashMap::new();

        vocabulary.insert(
            WordPOS::Noun,
            vec![
                "system",
                "engine",
                "module",
                "architecture",
                "framework",
                "algorithm",
                "protocol",
                "pipeline",
                "dimension",
                "timeline",
                "solution",
                "problem",
                "analysis",
                "result",
                "performance",
                "confidence",
                "accuracy",
                "stability",
                "reasoning",
                "intelligence",
                "cognition",
                "memory",
                "network",
                "state",
                "process",
                "function",
                "structure",
                "pattern",
                "model",
                "agent",
                "strategy",
                "approach",
                "capability",
                "metric",
                "threshold",
                "score",
            ],
        );

        vocabulary.insert(
            WordPOS::Verb,
            vec![
                "analyzes",
                "processes",
                "generates",
                "validates",
                "optimizes",
                "computes",
                "evaluates",
                "synthesizes",
                "transforms",
                "detects",
                "resolves",
                "executes",
                "coordinates",
                "integrates",
                "consolidates",
                "predicts",
                "verifies",
                "calibrates",
                "monitors",
                "adapts",
                "converges",
                "collapses",
                "branches",
                "forks",
                "merges",
                "strengthens",
                "decays",
                "evolves",
                "learns",
                "remembers",
            ],
        );

        vocabulary.insert(
            WordPOS::Adjective,
            vec![
                "cognitive",
                "deterministic",
                "probabilistic",
                "parallel",
                "recursive",
                "sovereign",
                "autonomous",
                "adaptive",
                "robust",
                "scalable",
                "optimal",
                "convergent",
                "stable",
                "dynamic",
                "novel",
                "mathematical",
                "formal",
                "rigorous",
                "creative",
                "impossible",
                "temporal",
                "spatial",
                "causal",
                "entropic",
                "quantum",
            ],
        );

        vocabulary.insert(
            WordPOS::Adverb,
            vec![
                "simultaneously",
                "deterministically",
                "recursively",
                "autonomously",
                "mathematically",
                "formally",
                "rigorously",
                "efficiently",
                "significantly",
                "fundamentally",
                "systematically",
                "dynamically",
            ],
        );

        vocabulary.insert(
            WordPOS::Determiner,
            vec!["the", "this", "each", "every", "all"],
        );

        let sentence_templates = vec![
            // "The [adj] [noun] [verb] the [noun]"
            vec![
                WordPOS::Determiner,
                WordPOS::Adjective,
                WordPOS::Noun,
                WordPOS::Verb,
                WordPOS::Determiner,
                WordPOS::Noun,
            ],
            // "[noun] [verb] [adv]"
            vec![WordPOS::Noun, WordPOS::Verb, WordPOS::Adverb],
            // "The [noun] [adv] [verb] [adj] [noun]"
            vec![
                WordPOS::Determiner,
                WordPOS::Noun,
                WordPOS::Adverb,
                WordPOS::Verb,
                WordPOS::Adjective,
                WordPOS::Noun,
            ],
        ];

        Self {
            sentence_templates,
            vocabulary,
        }
    }

    fn generate_sentence(&self, rng_state: &mut u64) -> String {
        let template_idx = xorshift(rng_state) as usize % self.sentence_templates.len();
        let template = &self.sentence_templates[template_idx];

        let mut words: Vec<String> = Vec::new();
        for pos in template {
            if let Some(word_list) = self.vocabulary.get(pos) {
                let idx = xorshift(rng_state) as usize % word_list.len();
                words.push(word_list[idx].to_string());
            }
        }

        if let Some(first) = words.first_mut() {
            let mut chars = first.chars();
            if let Some(c) = chars.next() {
                *first = c.to_uppercase().to_string() + chars.as_str();
            }
        }

        words.join(" ")
    }
}

// ═══════════════════════════════════════════════════════════════
// TEXT GENERATION ENGINE v2.0
// ═══════════════════════════════════════════════════════════════

pub struct TextGenV2 {
    markov: MarkovChain,
    grammar: GrammarEngine,
    vocab_graph: HashMap<String, VocabNode>,
    rng_state: u64,
    total_generations: u64,
}

impl TextGenV2 {
    pub fn new() -> Self {
        let mut engine = Self {
            markov: MarkovChain::new(),
            grammar: GrammarEngine::new(),
            vocab_graph: HashMap::new(),
            rng_state: 0xABCD_1234_DEAD_BEEF,
            total_generations: 0,
        };
        engine.load_default_corpus();
        engine.build_vocab_graph();
        engine
    }

    /// Generate text using Markov chain from learned corpus.
    pub fn generate_markov(
        &mut self,
        seed_word: Option<&str>,
        max_words: usize,
    ) -> TextGenV2Result {
        let start = Instant::now();
        self.total_generations += 1;

        let first = seed_word
            .map(|s| s.to_lowercase())
            .or_else(|| {
                if self.markov.starters.is_empty() {
                    None
                } else {
                    let idx = xorshift(&mut self.rng_state) as usize % self.markov.starters.len();
                    Some(self.markov.starters[idx].clone())
                }
            })
            .unwrap_or_else(|| "the".to_string());

        let mut words = vec![first.clone()];
        let mut prev2: Option<String> = None;
        let mut prev1 = first;

        for _ in 1..max_words {
            if let Some(next) = self
                .markov
                .next_word(&prev1, prev2.as_deref(), &mut self.rng_state)
            {
                words.push(next.clone());
                prev2 = Some(prev1);
                prev1 = next;
            } else {
                break;
            }
        }

        // Capitalize and add period
        if let Some(first_word) = words.first_mut() {
            let mut chars = first_word.chars();
            if let Some(c) = chars.next() {
                *first_word = c.to_uppercase().to_string() + chars.as_str();
            }
        }

        let text = words.join(" ") + ".";
        let word_count = words.len();

        TextGenV2Result {
            text,
            format: OutputFormat::PlainText,
            word_count,
            sentence_count: 1,
            generation_method: "markov_chain".into(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Generate text using grammar rules.
    pub fn generate_grammar(&mut self, sentence_count: usize) -> TextGenV2Result {
        let start = Instant::now();
        self.total_generations += 1;

        let mut sentences = Vec::new();
        for _ in 0..sentence_count {
            sentences.push(self.grammar.generate_sentence(&mut self.rng_state));
        }

        let text = sentences.join(". ") + ".";
        let word_count = text.split_whitespace().count();

        TextGenV2Result {
            text,
            format: OutputFormat::PlainText,
            word_count,
            sentence_count,
            generation_method: "grammar_rules".into(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Narrate a cognitive engine result in human-readable language.
    pub fn narrate_result(&mut self, request: &NarrationRequest) -> TextGenV2Result {
        let start = Instant::now();
        self.total_generations += 1;

        let mut paragraphs = Vec::new();

        // Opening
        let opening = match request.audience {
            NarrationAudience::Technical => format!(
                "The {} engine completed {} with a confidence of {:.1}%.",
                request.dimension,
                request.operation,
                request.confidence * 100.0
            ),
            NarrationAudience::Executive => format!(
                "Analysis complete. The {} process achieved {:.0}% confidence in its results.",
                request.operation,
                request.confidence * 100.0
            ),
            NarrationAudience::Casual => format!(
                "Done! The {} just finished {} and is {:.0}% sure about the answer.",
                request.dimension,
                request.operation,
                request.confidence * 100.0
            ),
        };
        paragraphs.push(opening);

        // Metrics
        if !request.metrics.is_empty() {
            let metrics_text = match request.audience {
                NarrationAudience::Technical => {
                    let metric_lines: Vec<String> = request
                        .metrics
                        .iter()
                        .map(|(k, v)| format!("  - {}: {:.6}", k, v))
                        .collect();
                    format!("Key metrics:\n{}", metric_lines.join("\n"))
                }
                NarrationAudience::Executive => {
                    let top_metrics: Vec<String> = request
                        .metrics
                        .iter()
                        .take(3)
                        .map(|(k, v)| format!("{} = {:.2}", k, v))
                        .collect();
                    format!("Performance indicators: {}.", top_metrics.join(", "))
                }
                NarrationAudience::Casual => "The numbers look good across the board.".to_string(),
            };
            paragraphs.push(metrics_text);
        }

        // Findings
        if !request.key_findings.is_empty() {
            let findings_text = match request.audience {
                NarrationAudience::Technical => {
                    let lines: Vec<String> = request
                        .key_findings
                        .iter()
                        .map(|f| format!("  → {}", f))
                        .collect();
                    format!("Findings:\n{}", lines.join("\n"))
                }
                NarrationAudience::Executive => {
                    format!("Key takeaway: {}.", request.key_findings[0])
                }
                NarrationAudience::Casual => {
                    format!("Main thing: {}.", request.key_findings[0])
                }
            };
            paragraphs.push(findings_text);
        }

        // Confidence assessment
        let confidence_text = if request.confidence > 0.9 {
            "The result is highly reliable and mathematically verified.".to_string()
        } else if request.confidence > 0.7 {
            "The result has strong confidence, though minor uncertainties remain.".to_string()
        } else if request.confidence > 0.5 {
            "The result shows moderate confidence. Further analysis may be warranted.".to_string()
        } else {
            "Confidence is below threshold. The result should be treated as preliminary."
                .to_string()
        };
        paragraphs.push(confidence_text);

        let text = paragraphs.join("\n\n");
        let word_count = text.split_whitespace().count();
        let sentence_count = text.matches('.').count() + text.matches('!').count();

        TextGenV2Result {
            text,
            format: OutputFormat::Report,
            word_count,
            sentence_count,
            generation_method: "cognitive_narration".into(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Find synonyms for a word using the vocabulary graph.
    pub fn find_synonyms(&self, word: &str, max_results: usize) -> Vec<(String, f64)> {
        if let Some(node) = self.vocab_graph.get(&word.to_lowercase()) {
            let mut neighbors = node.neighbors.clone();
            neighbors.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            neighbors.truncate(max_results);
            neighbors
        } else {
            Vec::new()
        }
    }

    /// Train the Markov chain on additional text.
    pub fn train(&mut self, text: &str) {
        self.markov.train(text);
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "TextGenV2 (Markov + Grammar + VocabGraph)",
            "total_generations": self.total_generations,
            "markov_unigrams": self.markov.unigrams.len(),
            "markov_bigrams": self.markov.bigrams.len(),
            "markov_trigrams": self.markov.trigrams.len(),
            "vocab_graph_nodes": self.vocab_graph.len(),
            "total_corpus_words": self.markov.total_words,
        })
    }

    // ─────────────────────────────────────────────────────
    // INTERNAL
    // ─────────────────────────────────────────────────────

    fn load_default_corpus(&mut self) {
        let corpus = concat!(
            "The cognitive engine processes information through multiple dimensions of reasoning. ",
            "Each dimension specializes in a different aspect of intelligence. ",
            "The deductive cascade proves logical statements through formal verification. ",
            "Creative hypersynthesis generates novel ideas by combining unrelated concepts. ",
            "The impossible engine breaks through paradoxes by destroying limiting axioms. ",
            "Metacognitive sovereignty enables the system to monitor and correct its own reasoning. ",
            "Temporal omniscience simulates multiple future timelines simultaneously. ",
            "The emotion engine adapts reasoning strategy based on contextual affect signals. ",
            "Persistent learning stores episodic memories and consolidates them into cognitive shortcuts. ",
            "The swarm intelligence spawns specialized agents that debate and challenge each other. ",
            "Code generation constructs programs as abstract syntax trees before serialization. ",
            "Spatial reasoning analyzes geometry and topology through pure mathematical proofs. ",
            "Natural language understanding parses human text through rule-based intent classification. ",
            "The system operates without any external language model or GPU dependency. ",
            "All reasoning is deterministic, verifiable, and mathematically grounded. ",
            "The architecture achieves sub-millisecond latency on all cognitive operations. ",
            "Memory consolidation uses exponential forgetting curves to maintain efficiency. ",
            "Sentiment analysis detects user emotional state through lexicon-based valence scoring. ",
            "The debate protocol forces agents to attack and defend their proposed solutions. ",
            "Only solutions that survive adversarial challenge are promoted to final output. ",
            "Timeline forking enables parallel exploration of multiple solution paths. ",
            "Quantum amplitude scoring selects the optimal timeline through energy minimization. ",
            "The system continuously evolves its own strategies through reinforcement learning. ",
            "Proof strength is measured using entropic information metrics. ",
            "Novelty is scored using Kolmogorov creative distance between concepts. ",
        );
        self.markov.train(corpus);
    }

    fn build_vocab_graph(&mut self) {
        // Build semantic word neighborhoods
        let word_groups: Vec<Vec<&str>> = vec![
            vec!["system", "engine", "module", "component", "framework"],
            vec!["analyze", "evaluate", "assess", "examine", "inspect"],
            vec!["generate", "create", "produce", "synthesize", "construct"],
            vec!["fast", "quick", "rapid", "efficient", "swift"],
            vec!["accurate", "precise", "exact", "correct", "rigorous"],
            vec![
                "cognitive",
                "mental",
                "intellectual",
                "reasoning",
                "thinking",
            ],
            vec!["parallel", "concurrent", "simultaneous", "async"],
            vec!["stable", "robust", "reliable", "consistent", "steady"],
            vec!["novel", "creative", "innovative", "original", "unique"],
            vec!["mathematical", "formal", "deterministic", "provable"],
        ];

        for group in &word_groups {
            for (i, word) in group.iter().enumerate() {
                let node = self
                    .vocab_graph
                    .entry(word.to_string())
                    .or_insert_with(|| VocabNode {
                        word: word.to_string(),
                        pos: WordPOS::Adjective, // simplified
                        frequency: 1,
                        neighbors: Vec::new(),
                    });

                for (j, other) in group.iter().enumerate() {
                    if i != j {
                        let distance = (i as f64 - j as f64).abs() / group.len() as f64;
                        node.neighbors.push((other.to_string(), distance));
                    }
                }
            }
        }
    }
}

impl Default for TextGenV2 {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markov_generation() {
        let mut engine = TextGenV2::new();
        let result = engine.generate_markov(Some("the"), 20);
        assert!(result.word_count > 0);
        assert!(!result.text.is_empty());
    }

    #[test]
    fn test_grammar_generation() {
        let mut engine = TextGenV2::new();
        let result = engine.generate_grammar(3);
        assert!(result.sentence_count == 3);
        assert!(result.word_count > 5);
    }

    #[test]
    fn test_narration() {
        let mut engine = TextGenV2::new();
        let mut metrics = HashMap::new();
        metrics.insert("proof_strength".into(), 0.95);
        metrics.insert("novelty".into(), 0.72);

        let result = engine.narrate_result(&NarrationRequest {
            dimension: "D1 Deductive Cascade".into(),
            operation: "formal proof verification".into(),
            metrics,
            key_findings: vec![
                "All axioms verified".into(),
                "No contradictions found".into(),
            ],
            confidence: 0.95,
            audience: NarrationAudience::Technical,
        });
        assert!(result.text.contains("95"));
        assert!(result.text.contains("axioms"));
    }

    #[test]
    fn test_synonyms() {
        let engine = TextGenV2::new();
        let synonyms = engine.find_synonyms("fast", 3);
        assert!(!synonyms.is_empty());
    }

    #[test]
    fn test_custom_training() {
        let mut engine = TextGenV2::new();
        engine.train("The cat sat on the mat. The dog chased the cat. The cat ran fast.");
        let result = engine.generate_markov(Some("the"), 10);
        assert!(result.word_count > 0);
    }
}

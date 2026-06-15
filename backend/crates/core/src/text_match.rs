//! Deterministic text similarity for ledger recall.
//!
//! With the local ML brain removed, there is no embedding model on this host,
//! so "semantic" recall here is an honest **lexical** measure: a cosine over
//! content-word term frequencies. It is a real improvement over exact-string
//! matching — robust to word order, extra/filler words, punctuation, and
//! partial overlap — but it will NOT match paraphrases that share no vocabulary.
//! It is dependency-free and fully deterministic, which is what lets recall be
//! cheap and testable. A real embedding model could be swapped in behind the
//! same `similarity` signature later.

use std::collections::BTreeMap;

/// Words too generic to carry meaning; excluded so content words dominate.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "had", "her", "was",
    "one", "our", "out", "has", "him", "his", "how", "man", "new", "now", "old", "see", "two",
    "way", "who", "boy", "did", "its", "let", "put", "say", "she", "too", "use", "that", "this",
    "with", "have", "from", "they", "will", "would", "there", "their", "what", "about", "which",
    "when", "make", "like", "time", "just", "know", "take", "into", "your", "some", "could",
    "them", "than", "then", "look", "only", "come", "over", "also", "back", "after", "use",
    "does", "must", "should", "shall", "been", "being", "were", "such", "very", "much",
];

/// Cosine similarity in [0, 1] over content-word term frequencies.
#[must_use]
pub fn similarity(a: &str, b: &str) -> f64 {
    let va = term_freq(a);
    let vb = term_freq(b);
    cosine(&va, &vb)
}

fn term_freq(text: &str) -> BTreeMap<String, f64> {
    let mut tf: BTreeMap<String, f64> = BTreeMap::new();
    for token in text
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|t| t.len() >= 3 && !STOPWORDS.contains(&t.as_str()))
    {
        *tf.entry(token).or_insert(0.0) += 1.0;
    }
    tf
}

fn cosine(a: &BTreeMap<String, f64>, b: &BTreeMap<String, f64>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let dot: f64 = a
        .iter()
        .filter_map(|(term, weight)| b.get(term).map(|other| weight * other))
        .sum();
    let norm = |v: &BTreeMap<String, f64>| v.values().map(|w| w * w).sum::<f64>().sqrt();
    let denom = norm(a) * norm(b);
    if denom <= 0.0 { 0.0 } else { (dot / denom).clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_is_one() {
        assert!(similarity("the host can echo XYZ", "echo XYZ on the host") > 0.99);
    }

    #[test]
    fn disjoint_vocabulary_is_zero() {
        assert_eq!(similarity("banana bread recipe", "rust compiler internals"), 0.0);
    }

    #[test]
    fn partial_overlap_is_between() {
        let s = similarity(
            "the laptop has 16gb memory",
            "the laptop has a fast processor",
        );
        assert!(s > 0.0 && s < 0.99, "expected partial similarity, got {s}");
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(similarity("", "anything"), 0.0);
        assert_eq!(similarity("the and for", "with from they"), 0.0); // all stopwords
    }
}

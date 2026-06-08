// ─────────────────────────────────────────────────────────────
// Knowledge Retriever — BM25-Based Knowledge Search
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/knowledge_retriever.py
// CPU-only BM25 retrieval for local knowledge base.

use std::collections::HashMap;

/// A document in the knowledge base.
#[derive(Debug, Clone)]
pub struct KnowledgeDocument {
    pub id: String,
    pub content: String,
    pub domain: String,
    pub metadata: HashMap<String, String>,
    /// Pre-computed term frequencies.
    term_freqs: HashMap<String, f64>,
    doc_len: usize,
}

impl KnowledgeDocument {
    pub fn new(id: &str, content: &str, domain: &str) -> Self {
        let tokens = Self::tokenize(content);
        let doc_len = tokens.len();
        let mut tf = HashMap::new();
        for token in &tokens {
            *tf.entry(token.clone()).or_insert(0.0) += 1.0;
        }
        Self {
            id: id.to_string(),
            content: content.to_string(),
            domain: domain.to_string(),
            metadata: HashMap::new(),
            term_freqs: tf,
            doc_len,
        }
    }

    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }
}

/// BM25-based knowledge retriever.
/// Parameters: k1=1.5, b=0.75 (standard BM25 tuning).
pub struct KnowledgeRetriever {
    documents: Vec<KnowledgeDocument>,
    avg_doc_len: f64,
    doc_freqs: HashMap<String, usize>,
    k1: f64,
    b: f64,
}

impl KnowledgeRetriever {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            avg_doc_len: 0.0,
            doc_freqs: HashMap::new(),
            k1: 1.5,
            b: 0.75,
        }
    }

    /// Add a document to the knowledge base.
    pub fn add_document(&mut self, doc: KnowledgeDocument) {
        // Update doc frequency
        for term in doc.term_freqs.keys() {
            *self.doc_freqs.entry(term.clone()).or_insert(0) += 1;
        }
        self.documents.push(doc);
        self.update_avg_doc_len();
    }

    /// Add multiple documents and rebuild index.
    pub fn add_documents(&mut self, docs: Vec<KnowledgeDocument>) {
        for doc in docs {
            for term in doc.term_freqs.keys() {
                *self.doc_freqs.entry(term.clone()).or_insert(0) += 1;
            }
            self.documents.push(doc);
        }
        self.update_avg_doc_len();
    }

    fn update_avg_doc_len(&mut self) {
        if self.documents.is_empty() {
            self.avg_doc_len = 0.0;
        } else {
            let total: usize = self.documents.iter().map(|d| d.doc_len).sum();
            self.avg_doc_len = total as f64 / self.documents.len() as f64;
        }
    }

    /// Retrieve top-k documents matching the query using BM25 scoring.
    pub fn retrieve(&self, query: &str, top_k: usize) -> (Vec<String>, Vec<f64>) {
        if self.documents.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let query_tokens = KnowledgeDocument::tokenize(query);
        let n = self.documents.len() as f64;

        let mut scores: Vec<(usize, f64)> = self
            .documents
            .iter()
            .enumerate()
            .map(|(idx, doc)| {
                let mut score = 0.0f64;
                for token in &query_tokens {
                    let tf = doc.term_freqs.get(token).copied().unwrap_or(0.0);
                    let df = self.doc_freqs.get(token).copied().unwrap_or(0) as f64;

                    // IDF component
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

                    // TF component with length normalization
                    let tf_norm = (tf * (self.k1 + 1.0))
                        / (tf
                            + self.k1
                                * (1.0 - self.b
                                    + self.b * doc.doc_len as f64 / self.avg_doc_len.max(1.0)));

                    score += idf * tf_norm;
                }
                (idx, score)
            })
            .filter(|(_, s)| *s > 0.0)
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(top_k);

        let passages = scores
            .iter()
            .map(|(idx, _)| self.documents[*idx].content.clone())
            .collect();
        let score_vals = scores.iter().map(|(_, s)| *s).collect();

        (passages, score_vals)
    }

    /// Get total document count.
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Clear all documents.
    pub fn clear(&mut self) {
        self.documents.clear();
        self.doc_freqs.clear();
        self.avg_doc_len = 0.0;
    }
}

impl Default for KnowledgeRetriever {
    fn default() -> Self {
        Self::new()
    }
}

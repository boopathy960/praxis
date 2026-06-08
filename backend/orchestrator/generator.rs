// Generator — Port of backend/agents/generator.py
#[derive(Debug, Clone)]
pub struct Candidate {
    pub content: String,
    pub strategy: String,
    pub confidence: f64,
}
pub struct ResponseGenerator {
    strategies: Vec<String>,
}
impl ResponseGenerator {
    pub fn new() -> Self {
        Self {
            strategies: vec![
                "direct".into(),
                "decompose".into(),
                "analogy".into(),
                "verify".into(),
            ],
        }
    }
    pub fn generate(&self, prompt: &str, _intent: &str, n: usize) -> Vec<Candidate> {
        self.strategies
            .iter()
            .take(n)
            .map(|s| Candidate {
                content: format!(
                    "[{}] Analysis of: {}",
                    s,
                    &prompt.chars().take(200).collect::<String>()
                ),
                strategy: s.clone(),
                confidence: 0.5 + (s.len() as f64 * 0.05),
            })
            .collect()
    }
}
impl Default for ResponseGenerator {
    fn default() -> Self {
        Self::new()
    }
}

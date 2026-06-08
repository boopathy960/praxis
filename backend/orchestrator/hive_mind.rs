// Hive Mind — Multi-Agent Collective Intelligence — Port of backend/agents/hive_mind.py

#[derive(Debug, Clone)]
pub struct HiveAgent {
    pub id: String,
    pub specialization: String,
    pub contribution: String,
    pub confidence: f64,
}
#[derive(Debug, Clone)]
pub struct HiveResult {
    pub consensus: String,
    pub confidence: f64,
    pub contributors: Vec<HiveAgent>,
    pub agreement_ratio: f64,
    pub dissenting_views: Vec<String>,
}
#[allow(dead_code)]
pub struct HiveMind {
    agents: Vec<HiveAgent>,
    consensus_threshold: f64,
}
impl HiveMind {
    pub fn new(consensus_threshold: f64) -> Self {
        Self {
            agents: Vec::new(),
            consensus_threshold,
        }
    }
    pub fn add_agent(&mut self, id: &str, specialization: &str) {
        self.agents.push(HiveAgent {
            id: id.to_string(),
            specialization: specialization.to_string(),
            contribution: String::new(),
            confidence: 0.0,
        });
    }
    pub fn collect_and_merge(&mut self, contributions: Vec<(String, String, f64)>) -> HiveResult {
        let mut weighted: Vec<(String, f64)> = Vec::new();
        let mut contributors = Vec::new();
        for (agent_id, contribution, confidence) in &contributions {
            weighted.push((contribution.clone(), *confidence));
            contributors.push(HiveAgent {
                id: agent_id.clone(),
                specialization: "".into(),
                contribution: contribution.clone(),
                confidence: *confidence,
            });
        }
        // Simple majority by confidence
        weighted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let consensus = weighted.first().map(|(c, _)| c.clone()).unwrap_or_default();
        let top_conf = weighted.first().map(|(_, c)| *c).unwrap_or(0.0);
        let agreeing = weighted.iter().filter(|(c, _)| c == &consensus).count();
        let agreement = agreeing as f64 / weighted.len().max(1) as f64;
        let dissenting: Vec<String> = weighted
            .iter()
            .filter(|(c, _)| c != &consensus)
            .map(|(c, _)| c.clone())
            .collect();
        HiveResult {
            consensus,
            confidence: top_conf,
            contributors,
            agreement_ratio: agreement,
            dissenting_views: dissenting,
        }
    }
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
}
impl Default for HiveMind {
    fn default() -> Self {
        Self::new(0.67)
    }
}

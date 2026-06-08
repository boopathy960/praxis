// ─────────────────────────────────────────────────────────────
// Reward Model — Multi-Dimensional Composite Rewards
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/reward_model.py
// Converts 6-layer verification scores into structured reward signals
// with learnable dimension weights and EMA normalization.

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// A single named reward dimension with learnable weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardDimension {
    pub name: String,
    pub value: f64,
    pub weight: f64,
    pub is_primary: bool,
}

/// Multi-dimensional reward aggregated from all verification layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeReward {
    pub dimensions: Vec<RewardDimension>,
    pub primary_reward: f64,
    pub raw_reward: f64,
    pub domain: String,
    pub has_critical_vulns: bool,
    pub security_penalty: f64,
}

impl CompositeReward {
    pub fn new(domain: &str) -> Self {
        Self {
            dimensions: Vec::new(),
            primary_reward: 0.0,
            raw_reward: 0.0,
            domain: domain.to_string(),
            has_critical_vulns: false,
            security_penalty: 0.0,
        }
    }

    pub fn add_dimension(&mut self, name: &str, value: f64, weight: f64, is_primary: bool) {
        self.dimensions.push(RewardDimension {
            name: name.to_string(),
            value,
            weight,
            is_primary,
        });
    }

    /// Compute weighted composite reward.
    pub fn compute(&mut self) -> f64 {
        if self.dimensions.is_empty() {
            return 0.0;
        }

        let total_weight: f64 = self.dimensions.iter().map(|d| d.weight).sum();
        if total_weight == 0.0 {
            return 0.0;
        }

        let weighted_sum: f64 = self.dimensions.iter().map(|d| d.value * d.weight).sum();
        let sum_values: f64 = self.dimensions.iter().map(|d| d.value).sum();
        self.raw_reward = sum_values / self.dimensions.len() as f64;
        self.primary_reward = weighted_sum / total_weight;

        // Security penalty: critical vulns drastically reduce reward
        if self.has_critical_vulns {
            self.security_penalty = self.primary_reward * 0.8;
            self.primary_reward *= 0.2;
            warn!(
                "Security penalty applied: reward {:.3} (penalty: {:.3})",
                self.primary_reward, self.security_penalty
            );
        }

        self.primary_reward
    }

    /// Convert to reward dict for emit_reward compatibility.
    pub fn to_dict(&self) -> HashMap<String, f64> {
        let mut result = HashMap::new();
        for d in &self.dimensions {
            result.insert(d.name.clone(), d.value);
        }
        result.insert("composite".to_string(), self.primary_reward);
        result
    }

    pub fn summary(&self) -> String {
        let mut lines = vec![format!("Composite Reward: {:.4}", self.primary_reward)];
        for d in &self.dimensions {
            let marker = if d.is_primary { " *" } else { "" };
            lines.push(format!(
                "  {}: {:.3} (w={:.2}){}",
                d.name, d.value, d.weight, marker
            ));
        }
        if self.has_critical_vulns {
            lines.push(format!(
                "  [SECURITY PENALTY: -{:.3}]",
                self.security_penalty
            ));
        }
        lines.join("\n")
    }
}

/// EMA-based reward normalization to prevent reward hacking.
#[derive(Debug, Clone)]
pub struct RewardNormalizer {
    alpha: f64,
    running_mean: f64,
    running_var: f64,
    count: usize,
}

impl RewardNormalizer {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            running_mean: 0.5,
            running_var: 0.1,
            count: 0,
        }
    }

    /// Normalize reward using running statistics (z-score then sigmoid).
    pub fn normalize(&mut self, reward: f64) -> f64 {
        self.count += 1;
        self.running_mean = (1.0 - self.alpha) * self.running_mean + self.alpha * reward;
        let diff = reward - self.running_mean;
        self.running_var = (1.0 - self.alpha) * self.running_var + self.alpha * diff * diff;

        let std = self.running_var.sqrt().max(1e-6);
        let z = (reward - self.running_mean) / std;
        1.0 / (1.0 + (-z).exp())
    }

    pub fn get_stats(&self) -> HashMap<String, f64> {
        let mut stats = HashMap::new();
        stats.insert("mean".into(), self.running_mean);
        stats.insert("std".into(), self.running_var.max(0.0).sqrt());
        stats.insert("count".into(), self.count as f64);
        stats
    }
}

impl Default for RewardNormalizer {
    fn default() -> Self {
        Self::new(0.05)
    }
}

/// Default dimension weights per domain.
fn default_domain_weights() -> HashMap<String, HashMap<String, f64>> {
    let mut domains = HashMap::new();

    let mut coding = HashMap::new();
    coding.insert("static".into(), 1.0);
    coding.insert("property".into(), 1.2);
    coding.insert("scenario".into(), 1.5);
    coding.insert("critic".into(), 0.8);
    coding.insert("code_quality".into(), 2.0);
    coding.insert("security".into(), 2.5);
    domains.insert("coding".into(), coding);

    let mut debugging = HashMap::new();
    debugging.insert("static".into(), 1.5);
    debugging.insert("property".into(), 1.0);
    debugging.insert("scenario".into(), 1.8);
    debugging.insert("critic".into(), 1.0);
    debugging.insert("code_quality".into(), 1.5);
    debugging.insert("security".into(), 2.0);
    domains.insert("debugging".into(), debugging);

    let mut algorithm = HashMap::new();
    algorithm.insert("static".into(), 1.0);
    algorithm.insert("property".into(), 2.0);
    algorithm.insert("scenario".into(), 1.5);
    algorithm.insert("critic".into(), 1.0);
    algorithm.insert("code_quality".into(), 1.0);
    algorithm.insert("security".into(), 1.0);
    domains.insert("algorithm".into(), algorithm);

    let mut architecture = HashMap::new();
    architecture.insert("static".into(), 0.8);
    architecture.insert("property".into(), 1.0);
    architecture.insert("scenario".into(), 1.5);
    architecture.insert("critic".into(), 2.0);
    architecture.insert("code_quality".into(), 1.5);
    architecture.insert("security".into(), 1.5);
    domains.insert("architecture".into(), architecture);

    let mut logic = HashMap::new();
    logic.insert("static".into(), 1.0);
    logic.insert("property".into(), 2.5);
    logic.insert("scenario".into(), 2.0);
    logic.insert("critic".into(), 1.0);
    logic.insert("code_quality".into(), 0.5);
    logic.insert("security".into(), 0.5);
    domains.insert("logic".into(), logic.clone());
    domains.insert("math".into(), logic);

    let mut general = HashMap::new();
    general.insert("static".into(), 1.0);
    general.insert("property".into(), 1.0);
    general.insert("scenario".into(), 1.0);
    general.insert("critic".into(), 1.0);
    general.insert("code_quality".into(), 1.0);
    general.insert("security".into(), 1.0);
    domains.insert("general".into(), general);

    domains
}

/// Verification report interface — trait for 6-layer verification scores.
pub trait VerificationScores {
    fn v_static(&self) -> f64;
    fn v_property(&self) -> f64;
    fn v_scenario(&self) -> f64;
    fn v_critic(&self) -> f64;
    fn v_code(&self) -> f64;
    fn v_security(&self) -> f64;
    fn has_critical_vulns(&self) -> bool;
}

/// Persisted weights data.
#[derive(Serialize, Deserialize)]
struct PersistedWeights {
    domain_weights: HashMap<String, HashMap<String, f64>>,
    history_length: usize,
}

/// Weight change history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightChange {
    pub domain: String,
    pub dimension: String,
    pub old_weight: f64,
    pub new_weight: f64,
    pub delta: f64,
}

/// Computes multi-dimensional rewards from VerificationReport.
pub struct RewardComputer {
    normalizer: RewardNormalizer,
    domain_weights: HashMap<String, HashMap<String, f64>>,
    domain_normalizers: HashMap<String, RewardNormalizer>,
    weight_history: Vec<WeightChange>,
    persist_dir: PathBuf,
}

impl RewardComputer {
    pub fn new(persist_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&persist_dir);
        let mut computer = Self {
            normalizer: RewardNormalizer::default(),
            domain_weights: default_domain_weights(),
            domain_normalizers: HashMap::new(),
            weight_history: Vec::new(),
            persist_dir,
        };
        computer.load_weights();
        computer
    }

    /// Convert a VerificationReport into a CompositeReward.
    pub fn compute_reward(
        &mut self,
        report: &dyn VerificationScores,
        domain: &str,
        normalize: bool,
    ) -> CompositeReward {
        let mut reward = CompositeReward::new(domain);
        let weights = self
            .domain_weights
            .get(domain)
            .or_else(|| self.domain_weights.get("general"))
            .cloned()
            .unwrap_or_default();

        let layers = [
            ("static", report.v_static()),
            ("property", report.v_property()),
            ("scenario", report.v_scenario()),
            ("critic", report.v_critic()),
            ("code_quality", report.v_code()),
            ("security", report.v_security()),
        ];

        for (name, value) in &layers {
            reward.add_dimension(
                name,
                *value,
                *weights.get(*name).unwrap_or(&1.0),
                *name == "security",
            );
        }

        reward.has_critical_vulns = report.has_critical_vulns();
        let raw = reward.compute();

        // Normalize if requested
        if normalize && self.normalizer.count > 5 {
            let normalizer = self
                .domain_normalizers
                .entry(domain.to_string())
                .or_insert_with(RewardNormalizer::default);
            let normalized = normalizer.normalize(raw);
            reward.primary_reward = normalized;
        }

        self.normalizer.normalize(raw);
        info!("Reward computed: {}", reward.summary());
        reward
    }

    /// Update a domain-specific dimension weight based on learning signal.
    pub fn update_weights(
        &mut self,
        domain: &str,
        dimension_name: &str,
        delta: f64,
        learning_rate: f64,
    ) {
        let default_weights = self
            .domain_weights
            .get("general")
            .cloned()
            .unwrap_or_default();
        let weights = self
            .domain_weights
            .entry(domain.to_string())
            .or_insert(default_weights);

        let current = *weights.get(dimension_name).unwrap_or(&1.0);
        let new_weight = (current + learning_rate * delta).clamp(0.1, 5.0);
        weights.insert(dimension_name.to_string(), new_weight);

        self.weight_history.push(WeightChange {
            domain: domain.to_string(),
            dimension: dimension_name.to_string(),
            old_weight: current,
            new_weight,
            delta: learning_rate * delta,
        });

        if self.weight_history.len() % 10 == 0 {
            self.save_weights();
        }

        debug!(
            "Updated weight {}/{}: {:.3} -> {:.3}",
            domain, dimension_name, current, new_weight
        );
    }

    pub fn save_weights(&self) {
        let path = self.persist_dir.join("domain_weights.json");
        let data = PersistedWeights {
            domain_weights: self.domain_weights.clone(),
            history_length: self.weight_history.len(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let _ = fs::write(&path, json);
        }

        let hist_path = self.persist_dir.join("weight_history.json");
        let recent: Vec<&WeightChange> = self.weight_history.iter().rev().take(500).collect();
        if let Ok(json) = serde_json::to_string_pretty(&recent) {
            let _ = fs::write(&hist_path, json);
        }
    }

    fn load_weights(&mut self) {
        let path = self.persist_dir.join("domain_weights.json");
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(persisted) = serde_json::from_str::<PersistedWeights>(&data) {
                    for (domain, weights) in persisted.domain_weights {
                        self.domain_weights.insert(domain, weights);
                    }
                }
            }
        }

        let hist_path = self.persist_dir.join("weight_history.json");
        if hist_path.exists() {
            if let Ok(data) = fs::read_to_string(&hist_path) {
                if let Ok(history) = serde_json::from_str::<Vec<WeightChange>>(&data) {
                    self.weight_history = history;
                }
            }
        }
    }

    pub fn get_weight_history(&self) -> &[WeightChange] {
        &self.weight_history
    }

    pub fn get_dimension_weights(&self, domain: &str) -> HashMap<String, f64> {
        self.domain_weights
            .get(domain)
            .or_else(|| self.domain_weights.get("general"))
            .cloned()
            .unwrap_or_default()
    }
}

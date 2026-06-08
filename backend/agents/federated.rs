// ─────────────────────────────────────────────────────────────
// Federated Learning Engine (FedAvg + Differential Privacy)
// ─────────────────────────────────────────────────────────────
use rand::Rng;
use std::collections::HashMap;

pub struct FederatedModel {
    pub id: String,
    pub name: String,
    pub parameters: Vec<f64>,
    pub version: u64,
    pub accuracy: f64,
}

pub struct GradientUpdate {
    pub contributor: String,
    pub model_id: String,
    pub gradient: Vec<f64>,
    pub accuracy: f64,
    pub data_size: usize,
}

pub struct FederatedLearningEngine {
    models: HashMap<String, FederatedModel>,
    pending: HashMap<String, Vec<GradientUpdate>>,
    lr: f64,
    dp_noise: f64,
    grad_clip: f64,
    total_rounds: u64,
}

impl FederatedLearningEngine {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            pending: HashMap::new(),
            lr: 0.01,
            dp_noise: 0.1,
            grad_clip: 1.0,
            total_rounds: 0,
        }
    }

    pub fn create_model(&mut self, name: &str, size: usize) -> String {
        let id = crate::crypto::hash::sha3_256_hex(
            format!("{}:{}", name, chrono::Utc::now().timestamp()).as_bytes(),
        )[..16]
            .to_string();
        let params: Vec<f64> = (0..size)
            .map(|_| rand::thread_rng().gen::<f64>() * 0.01)
            .collect();
        self.models.insert(
            id.clone(),
            FederatedModel {
                id: id.clone(),
                name: name.to_string(),
                parameters: params,
                version: 0,
                accuracy: 0.0,
            },
        );
        self.pending.insert(id.clone(), Vec::new());
        id
    }

    pub fn submit_gradient(&mut self, update: GradientUpdate) -> bool {
        if !self.models.contains_key(&update.model_id) {
            return false;
        }
        let mut grad = update.gradient.clone();
        // Clip
        let norm: f64 = grad.iter().map(|g| g.powi(2)).sum::<f64>().sqrt();
        if norm > self.grad_clip {
            for g in grad.iter_mut() {
                *g *= self.grad_clip / norm;
            }
        }
        // DP noise
        let mut rng = rand::thread_rng();
        for g in grad.iter_mut() {
            *g += rng.gen::<f64>() * self.dp_noise * 2.0 - self.dp_noise;
        }

        self.pending
            .entry(update.model_id.clone())
            .or_default()
            .push(GradientUpdate {
                gradient: grad,
                ..update
            });
        true
    }

    pub fn aggregate_round(&mut self, model_id: &str) -> Option<serde_json::Value> {
        let grads = self.pending.get(model_id)?;
        if grads.len() < 2 {
            return None;
        }
        let model = self.models.get_mut(model_id)?;
        let total_samples: usize = grads.iter().map(|g| g.data_size).sum();
        let dim = model.parameters.len();
        let mut agg = vec![0.0; dim];
        for g in grads {
            let w = g.data_size as f64 / total_samples as f64;
            for i in 0..dim.min(g.gradient.len()) {
                agg[i] += w * g.gradient[i];
            }
        }
        for i in 0..dim {
            model.parameters[i] -= self.lr * agg[i];
        }
        model.version += 1;
        model.accuracy = grads.iter().map(|g| g.accuracy).sum::<f64>() / grads.len() as f64;
        let n = grads.len();
        self.pending.insert(model_id.to_string(), Vec::new());
        self.total_rounds += 1;
        Some(
            serde_json::json!({"model_id": model_id, "version": model.version, "contributors": n, "accuracy": model.accuracy}),
        )
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({"models": self.models.len(), "total_rounds": self.total_rounds})
    }
}

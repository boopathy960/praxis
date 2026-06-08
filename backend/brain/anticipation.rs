// ─────────────────────────────────────────────────────────────
// Anticipation Engine — Predictive Engine Using Markov Chains
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/anticipation_engine.py

use std::collections::HashMap;

/// Markov chain transition for prediction.
#[derive(Debug, Clone)]
struct MarkovTransition {
    next_states: HashMap<String, f64>,
    total_count: f64,
}

impl MarkovTransition {
    fn new() -> Self {
        Self {
            next_states: HashMap::new(),
            total_count: 0.0,
        }
    }

    fn add_transition(&mut self, next: &str) {
        *self.next_states.entry(next.to_string()).or_insert(0.0) += 1.0;
        self.total_count += 1.0;
    }

    fn predict(&self) -> Vec<(String, f64)> {
        let mut predictions: Vec<(String, f64)> = self
            .next_states
            .iter()
            .map(|(state, count)| (state.clone(), count / self.total_count))
            .collect();
        predictions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        predictions
    }
}

/// Prediction result.
#[derive(Debug, Clone)]
pub struct Prediction {
    pub predicted_state: String,
    pub probability: f64,
    pub alternatives: Vec<(String, f64)>,
    pub confidence: f64,
}

/// Anticipation Engine — predicts next actions/states using Markov chains.
pub struct AnticipationEngine {
    transitions: HashMap<String, MarkovTransition>,
    sequence_buffer: Vec<String>,
    buffer_size: usize,
    total_predictions: u64,
    correct_predictions: u64,
}

impl AnticipationEngine {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            transitions: HashMap::new(),
            sequence_buffer: Vec::new(),
            buffer_size,
            total_predictions: 0,
            correct_predictions: 0,
        }
    }

    /// Observe a state transition for learning.
    pub fn observe(&mut self, state: &str) {
        if let Some(last) = self.sequence_buffer.last().cloned() {
            self.transitions
                .entry(last)
                .or_insert_with(MarkovTransition::new)
                .add_transition(state);
        }

        self.sequence_buffer.push(state.to_string());
        if self.sequence_buffer.len() > self.buffer_size {
            self.sequence_buffer.remove(0);
        }
    }

    /// Predict the next state given the current state.
    pub fn predict(&mut self, current_state: &str) -> Prediction {
        self.total_predictions += 1;

        if let Some(transition) = self.transitions.get(current_state) {
            let predictions = transition.predict();
            if let Some((best, prob)) = predictions.first() {
                return Prediction {
                    predicted_state: best.clone(),
                    probability: *prob,
                    alternatives: predictions.iter().skip(1).take(3).cloned().collect(),
                    confidence: *prob * (transition.total_count.ln() + 1.0).min(1.0),
                };
            }
        }

        Prediction {
            predicted_state: "unknown".into(),
            probability: 0.0,
            alternatives: Vec::new(),
            confidence: 0.0,
        }
    }

    /// Record whether the last prediction was correct.
    pub fn record_outcome(&mut self, was_correct: bool) {
        if was_correct {
            self.correct_predictions += 1;
        }
    }

    /// Predict a sequence of N future states.
    pub fn predict_sequence(&mut self, start: &str, n: usize) -> Vec<Prediction> {
        let mut predictions = Vec::new();
        let mut current = start.to_string();

        for _ in 0..n {
            let pred = self.predict(&current);
            if pred.probability < 0.1 {
                break;
            }
            current = pred.predicted_state.clone();
            predictions.push(pred);
        }
        predictions
    }

    pub fn accuracy(&self) -> f64 {
        if self.total_predictions == 0 {
            0.0
        } else {
            self.correct_predictions as f64 / self.total_predictions as f64
        }
    }

    pub fn known_states(&self) -> usize {
        self.transitions.len()
    }
}

impl Default for AnticipationEngine {
    fn default() -> Self {
        Self::new(100)
    }
}

// ─────────────────────────────────────────────────────────────
// Trace Store — Structured Span System for Self-Improvement
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/trace_store.py
// Inspired by Agent Lightning's span/store architecture.
// Every thinking step emits typed TraceSpans into a TrajectoryTrace.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::error::{AstraError, AstraResult};

/// Types of spans emitted during problem-solving.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpanType {
    BrainDecision,
    ToolCall,
    Reasoning,
    Verification,
    Reward,
    Hypothesis,
    Metacognition,
    Classification,
    RiskAssessment,
}

impl SpanType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::BrainDecision => "brain_decision",
            Self::ToolCall => "tool_call",
            Self::Reasoning => "reasoning",
            Self::Verification => "verification",
            Self::Reward => "reward",
            Self::Hypothesis => "hypothesis",
            Self::Metacognition => "metacognition",
            Self::Classification => "classification",
            Self::RiskAssessment => "risk_assessment",
        }
    }
}

fn now_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..12].to_string()
}

/// A single structured span — one atomic step in problem-solving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSpan {
    pub span_id: String,
    pub parent_id: Option<String>,
    pub span_type: SpanType,
    pub input_data: String,
    pub output_data: String,
    pub prompt_template: String,
    pub reward: f64,
    pub reward_dimensions: HashMap<String, f64>,
    pub timestamp: f64,
    pub duration_ms: f64,
    pub attributes: HashMap<String, String>,
    pub cognitive_mode: String,
    pub iteration: usize,
    pub was_helpful: Option<bool>,
}

impl TraceSpan {
    pub fn new(span_type: SpanType) -> Self {
        Self {
            span_id: short_uuid(),
            parent_id: None,
            span_type,
            input_data: String::new(),
            output_data: String::new(),
            prompt_template: String::new(),
            reward: 0.0,
            reward_dimensions: HashMap::new(),
            timestamp: now_timestamp(),
            duration_ms: 0.0,
            attributes: HashMap::new(),
            cognitive_mode: String::new(),
            iteration: 0,
            was_helpful: None,
        }
    }
}

/// Create a TraceSpan — the primary emit helper.
pub fn emit_span(
    span_type: SpanType,
    input_data: &str,
    output_data: &str,
    reward: f64,
    cognitive_mode: &str,
    attributes: HashMap<String, String>,
    parent_id: Option<String>,
    prompt_template: &str,
) -> TraceSpan {
    TraceSpan {
        span_id: short_uuid(),
        parent_id,
        span_type,
        input_data: input_data.chars().take(500).collect(),
        output_data: output_data.chars().take(500).collect(),
        prompt_template: prompt_template.chars().take(200).collect(),
        reward,
        reward_dimensions: HashMap::new(),
        timestamp: now_timestamp(),
        duration_ms: 0.0,
        attributes,
        cognitive_mode: cognitive_mode.to_string(),
        iteration: 0,
        was_helpful: None,
    }
}

/// A complete problem-solving episode — ordered list of spans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryTrace {
    pub trace_id: String,
    pub problem: String,
    pub domain: String,
    pub spans: Vec<TraceSpan>,
    pub final_answer: String,
    pub final_reward: f64,
    pub reward_dimensions: HashMap<String, f64>,
    pub success: bool,
    pub gating_mode: String,
    pub strategies_used: Vec<String>,
    pub total_iterations: usize,
    pub total_duration_ms: f64,
    pub created_at: f64,
}

impl TrajectoryTrace {
    pub fn new(problem: &str) -> Self {
        Self {
            trace_id: short_uuid(),
            problem: problem.to_string(),
            domain: "general".to_string(),
            spans: Vec::new(),
            final_answer: String::new(),
            final_reward: 0.0,
            reward_dimensions: HashMap::new(),
            success: false,
            gating_mode: "refuse".to_string(),
            strategies_used: Vec::new(),
            total_iterations: 0,
            total_duration_ms: 0.0,
            created_at: now_timestamp(),
        }
    }

    /// Add a span and auto-set its iteration.
    pub fn add_span(&mut self, mut span: TraceSpan) {
        span.iteration = self.spans.len();
        self.spans.push(span);
    }

    pub fn get_spans_by_type(&self, span_type: &SpanType) -> Vec<&TraceSpan> {
        self.spans
            .iter()
            .filter(|s| &s.span_type == span_type)
            .collect()
    }

    pub fn get_brain_decisions(&self) -> Vec<&TraceSpan> {
        self.get_spans_by_type(&SpanType::BrainDecision)
    }

    pub fn get_reward_spans(&self) -> Vec<&TraceSpan> {
        self.get_spans_by_type(&SpanType::Reward)
    }
}

/// Trace index entry for fast lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TraceIndexEntry {
    domain: String,
    reward: f64,
    success: bool,
    strategies: Vec<String>,
    created_at: f64,
    num_spans: usize,
}

/// Central hub for persisting trajectory data and learning history.
pub struct LearningStore {
    store_dir: PathBuf,
    traces_dir: PathBuf,
    resources_dir: PathBuf,
    trace_index: HashMap<String, TraceIndexEntry>,
}

impl LearningStore {
    pub fn new(store_dir: PathBuf) -> Self {
        let traces_dir = store_dir.join("traces");
        let resources_dir = store_dir.join("resources");
        let _ = fs::create_dir_all(&traces_dir);
        let _ = fs::create_dir_all(&resources_dir);

        let mut store = Self {
            store_dir,
            traces_dir,
            resources_dir,
            trace_index: HashMap::new(),
        };
        store.load_index();
        store
    }

    fn load_index(&mut self) {
        let index_path = self.store_dir.join("trace_index.json");
        if index_path.exists() {
            if let Ok(data) = fs::read_to_string(&index_path) {
                if let Ok(index) = serde_json::from_str(&data) {
                    self.trace_index = index;
                }
            }
        }
    }

    fn save_index(&self) -> AstraResult<()> {
        let index_path = self.store_dir.join("trace_index.json");
        let data = serde_json::to_string_pretty(&self.trace_index)?;
        fs::write(&index_path, data).map_err(|error| {
            AstraError::Internal(format!(
                "failed to persist learning trace index at {}: {error}",
                index_path.display()
            ))
        })?;
        Ok(())
    }

    /// Persist a complete trajectory trace.
    pub fn store_trajectory(&mut self, trace: &TrajectoryTrace) -> AstraResult<()> {
        let trace_path = self.traces_dir.join(format!("{}.json", trace.trace_id));
        let data = serde_json::to_string_pretty(trace)?;
        fs::write(&trace_path, data).map_err(|error| {
            AstraError::Internal(format!(
                "failed to persist trajectory {} at {}: {error}",
                trace.trace_id,
                trace_path.display()
            ))
        })?;

        self.trace_index.insert(
            trace.trace_id.clone(),
            TraceIndexEntry {
                domain: trace.domain.clone(),
                reward: trace.final_reward,
                success: trace.success,
                strategies: trace.strategies_used.clone(),
                created_at: trace.created_at,
                num_spans: trace.spans.len(),
            },
        );
        self.save_index()?;
        Ok(())
    }

    /// Retrieve a specific trajectory by ID.
    pub fn get_trajectory(&self, trace_id: &str) -> AstraResult<Option<TrajectoryTrace>> {
        let trace_path = self.traces_dir.join(format!("{}.json", trace_id));
        if !trace_path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&trace_path).map_err(|error| {
            AstraError::Internal(format!(
                "failed to read trajectory {} at {}: {error}",
                trace_id,
                trace_path.display()
            ))
        })?;
        Ok(Some(serde_json::from_str(&data)?))
    }

    /// Query trajectories matching criteria.
    pub fn query_trajectories(
        &self,
        domain: Option<&str>,
        min_reward: f64,
        success_only: bool,
        limit: usize,
    ) -> AstraResult<Vec<TrajectoryTrace>> {
        let mut matching: Vec<(&String, f64)> = self
            .trace_index
            .iter()
            .filter(|(_, meta)| {
                if let Some(d) = domain {
                    if meta.domain != d {
                        return false;
                    }
                }
                if meta.reward < min_reward {
                    return false;
                }
                if success_only && !meta.success {
                    return false;
                }
                true
            })
            .map(|(tid, meta)| (tid, meta.reward))
            .collect();

        matching.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        matching.truncate(limit);

        let mut trajectories = Vec::with_capacity(matching.len());
        for (trace_id, _) in matching {
            if let Some(trace) = self.get_trajectory(trace_id)? {
                trajectories.push(trace);
            }
        }

        Ok(trajectories)
    }

    /// Store a learning resource (prompt template, weights, etc.).
    pub fn store_resource(&self, name: &str, data: &serde_json::Value) -> AstraResult<()> {
        let path = self.resources_dir.join(format!("{}.json", name));
        let json_str = serde_json::to_string_pretty(data)?;
        fs::write(&path, json_str).map_err(|error| {
            AstraError::Internal(format!(
                "failed to persist learning resource {} at {}: {error}",
                name,
                path.display()
            ))
        })?;
        Ok(())
    }

    /// Retrieve a learning resource.
    pub fn get_resource(&self, name: &str) -> AstraResult<Option<serde_json::Value>> {
        let path = self.resources_dir.join(format!("{}.json", name));
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path).map_err(|error| {
            AstraError::Internal(format!(
                "failed to read learning resource {} at {}: {error}",
                name,
                path.display()
            ))
        })?;
        Ok(Some(serde_json::from_str(&data)?))
    }

    /// Get learning store statistics.
    pub fn get_stats(&self) -> HashMap<String, serde_json::Value> {
        let mut stats = HashMap::new();
        let total = self.trace_index.len();
        stats.insert("total_traces".into(), serde_json::json!(total));

        if total == 0 {
            return stats;
        }

        let rewards: Vec<f64> = self.trace_index.values().map(|m| m.reward).collect();
        let successes = self.trace_index.values().filter(|m| m.success).count();
        let mut domains: HashMap<String, usize> = HashMap::new();
        for m in self.trace_index.values() {
            *domains.entry(m.domain.clone()).or_insert(0) += 1;
        }

        let avg_reward: f64 = rewards.iter().sum::<f64>() / total as f64;
        let max_reward = rewards.iter().cloned().fold(f64::MIN, f64::max);

        stats.insert(
            "success_rate".into(),
            serde_json::json!(successes as f64 / total as f64),
        );
        stats.insert("avg_reward".into(), serde_json::json!(avg_reward));
        stats.insert("max_reward".into(), serde_json::json!(max_reward));
        stats.insert("domains".into(), serde_json::json!(domains));
        stats
    }
}

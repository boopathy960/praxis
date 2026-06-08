use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AstraError, AstraResult};
use crate::intelligence::reasoning::ReasoningStrategy;

use super::credit_assignment::CreditAssignmentEngine;
use super::prompt_evolver::PromptEvolver;
use super::reward_model::{RewardComputer, VerificationScores};
use super::trace_store::{LearningStore, SpanType, TrajectoryTrace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationObjective {
    pub domain: String,
    pub reward_signal: String,
    pub optimize_prompts: bool,
    pub optimize_tool_policies: bool,
    pub max_rollouts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationBatch {
    pub objective: Option<OptimizationObjective>,
    pub traces: Vec<TrajectoryTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSummary {
    pub objective_count: usize,
    pub active_objective_count: usize,
    pub ingested_traces: usize,
    pub average_reward: f64,
    pub reward_trend: f64,
    pub successful_traces: usize,
    pub top_domains: Vec<String>,
    pub top_strategies: Vec<String>,
    pub prompt_guidance: String,
    pub policy_guidance: Vec<String>,
    pub champion_prompt: Option<String>,
    pub generated_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningPolicy {
    pub domain: String,
    pub champion_prompt: Option<String>,
    pub preferred_strategies: Vec<ReasoningStrategy>,
    pub discouraged_strategies: Vec<ReasoningStrategy>,
    pub max_reasoning_depth: Option<usize>,
    pub max_reasoning_expansions: Option<usize>,
    pub exploration_c: Option<f64>,
    pub verification_bias: f64,
    pub policy_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
enum ObjectiveHealth {
    Pending,
    Learning,
    Healthy,
    AtRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObjectiveRecord {
    key: String,
    objective: OptimizationObjective,
    registered_at: f64,
    last_updated_at: f64,
    ingested_traces: usize,
    successful_traces: usize,
    average_reward: f64,
    best_reward: f64,
    health: ObjectiveHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptChampionRecord {
    domain: String,
    template: String,
    fitness: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicySnapshot {
    generated_at: f64,
    policy_guidance: Vec<String>,
    risk_signals: Vec<String>,
    champion_prompts: Vec<PromptChampionRecord>,
    positive_span_types: Vec<String>,
    negative_span_types: Vec<String>,
}

#[derive(Debug, Clone)]
struct LightningConfig {
    summary_window: usize,
    min_traces_for_evolution: usize,
    evolution_interval: usize,
    prompt_population_size: usize,
    max_guidance_items: usize,
}

impl Default for LightningConfig {
    fn default() -> Self {
        Self {
            summary_window: 48,
            min_traces_for_evolution: 4,
            evolution_interval: 4,
            prompt_population_size: 24,
            max_guidance_items: 5,
        }
    }
}

struct LightningRuntime {
    store: LearningStore,
    reward_computer: RewardComputer,
    credit_assignment: CreditAssignmentEngine,
    prompt_evolver: PromptEvolver,
    objectives: HashMap<String, ObjectiveRecord>,
    config: LightningConfig,
}

#[derive(Clone)]
pub struct LightningLoop {
    runtime: Arc<Mutex<LightningRuntime>>,
}

impl LightningLoop {
    #[must_use]
    pub fn new(store_dir: PathBuf) -> Self {
        let mut runtime = LightningRuntime::new(store_dir);
        let _ = runtime.load_state();
        Self {
            runtime: Arc::new(Mutex::new(runtime)),
        }
    }

    pub fn register_objective(&self, objective: OptimizationObjective) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.upsert_objective(objective);
            let _ = runtime.persist_objectives();
        }
    }

    pub fn ingest_batch(&self, batch: OptimizationBatch) -> AstraResult<LearningSummary> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| AstraError::Internal("lightning runtime lock poisoned".into()))?;
        runtime.ingest_batch(batch)
    }

    pub fn summary(&self, limit: usize) -> LearningSummary {
        match self.try_summary(limit) {
            Ok(summary) => summary,
            Err(error) => LearningSummary {
                objective_count: 0,
                active_objective_count: 0,
                ingested_traces: 0,
                average_reward: 0.0,
                reward_trend: 0.0,
                successful_traces: 0,
                top_domains: Vec::new(),
                top_strategies: Vec::new(),
                prompt_guidance: format!(
                    "Learning summary temporarily unavailable while the runtime recovers: {error}"
                ),
                policy_guidance: Vec::new(),
                champion_prompt: None,
                generated_at: now_timestamp(),
            },
        }
    }

    pub fn reasoning_policy(&self, problem: &str, domain: Option<&str>) -> ReasoningPolicy {
        match self.try_reasoning_policy(problem, domain) {
            Ok(policy) => policy,
            Err(_) => ReasoningPolicy {
                domain: domain.unwrap_or("general").to_string(),
                champion_prompt: None,
                preferred_strategies: vec![
                    ReasoningStrategy::MetaCognition,
                    ReasoningStrategy::TreeOfThought,
                    ReasoningStrategy::SelfCritique,
                ],
                discouraged_strategies: Vec::new(),
                max_reasoning_depth: None,
                max_reasoning_expansions: None,
                exploration_c: None,
                verification_bias: 0.65,
                policy_notes: vec![
                    "Lightning policy unavailable; using conservative default reasoning profile."
                        .into(),
                ],
            },
        }
    }

    fn try_summary(&self, limit: usize) -> AstraResult<LearningSummary> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| AstraError::Internal("lightning runtime lock poisoned".into()))?;
        runtime.summary(limit)
    }

    fn try_reasoning_policy(
        &self,
        problem: &str,
        domain: Option<&str>,
    ) -> AstraResult<ReasoningPolicy> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| AstraError::Internal("lightning runtime lock poisoned".into()))?;
        runtime.reasoning_policy(problem, domain)
    }
}

impl Default for LightningLoop {
    fn default() -> Self {
        Self::new(PathBuf::from("astra_state/learning"))
    }
}

impl LightningRuntime {
    fn new(store_dir: PathBuf) -> Self {
        let config = LightningConfig::default();
        let reward_dir = store_dir.join("reward_model");

        Self {
            store: LearningStore::new(store_dir),
            reward_computer: RewardComputer::new(reward_dir),
            credit_assignment: CreditAssignmentEngine::default(),
            prompt_evolver: PromptEvolver::new(config.prompt_population_size),
            objectives: HashMap::new(),
            config,
        }
    }

    fn ingest_batch(&mut self, batch: OptimizationBatch) -> AstraResult<LearningSummary> {
        if let Some(objective) = batch.objective.clone() {
            self.upsert_objective(objective);
        }

        let mut touched_domains = HashSet::new();
        let mut policy_totals = HashMap::new();
        let mut policy_risks = HashMap::new();
        let mut ingested = 0usize;

        for trace in batch.traces {
            let mut sanitized = sanitize_trace(trace);
            if sanitized.trace_id.is_empty() {
                sanitized.trace_id = format!("trace-{}", unique_suffix());
            }

            self.ensure_prompt_seed(
                &sanitized.domain,
                batch
                    .objective
                    .as_ref()
                    .filter(|objective| objective.domain == sanitized.domain),
            );

            sanitized.final_reward = self.compute_effective_reward(&sanitized);
            sanitized
                .reward_dimensions
                .insert("effective".into(), sanitized.final_reward);

            touched_domains.insert(sanitized.domain.clone());
            self.record_prompt_reward(&sanitized.domain, sanitized.final_reward);
            self.update_policy_signals(&sanitized, &mut policy_totals, &mut policy_risks);
            self.update_objectives(&sanitized);
            self.store.store_trajectory(&sanitized)?;
            ingested += 1;
        }

        for domain in &touched_domains {
            self.maybe_evolve_domain(domain);
        }

        let recent_traces =
            self.store
                .query_trajectories(None, f64::MIN, false, self.config.summary_window)?;
        let snapshot = self.build_policy_snapshot(&recent_traces, &policy_totals, &policy_risks);
        let summary = summarize_learning(
            &recent_traces,
            self.objectives.len(),
            self.active_objective_count(),
            &snapshot,
        );

        self.persist_objectives()?;
        self.persist_prompt_champions()?;
        self.store
            .store_resource("latest_policy_snapshot", &serde_json::to_value(&snapshot)?)?;
        self.store
            .store_resource("latest_learning_summary", &serde_json::to_value(&summary)?)?;

        if ingested == 0 {
            return Ok(LearningSummary {
                prompt_guidance:
                    "No trajectories were ingested. Submit spans or rollouts before asking for optimization guidance."
                        .into(),
                ..summary
            });
        }

        Ok(summary)
    }

    fn summary(&self, limit: usize) -> AstraResult<LearningSummary> {
        let traces = self
            .store
            .query_trajectories(None, f64::MIN, false, limit)?;
        let snapshot = self
            .load_policy_snapshot()?
            .unwrap_or_else(default_policy_snapshot);
        Ok(summarize_learning(
            &traces,
            self.objectives.len(),
            self.active_objective_count(),
            &snapshot,
        ))
    }

    fn reasoning_policy(
        &self,
        problem: &str,
        domain: Option<&str>,
    ) -> AstraResult<ReasoningPolicy> {
        let resolved_domain = normalize_domain(domain.unwrap_or_else(|| infer_domain(problem)));
        let traces = self.store.query_trajectories(
            Some(&resolved_domain),
            f64::MIN,
            false,
            self.config.summary_window,
        )?;
        let snapshot = self
            .load_policy_snapshot()?
            .unwrap_or_else(default_policy_snapshot);

        let preferred = preferred_strategies_from_traces(&traces, problem);
        let discouraged = discouraged_strategies_from_traces(&traces);
        let champion_prompt = snapshot
            .champion_prompts
            .iter()
            .find(|record| record.domain == resolved_domain)
            .map(|record| record.template.clone())
            .or_else(|| {
                snapshot
                    .champion_prompts
                    .iter()
                    .find(|record| record.domain == "general")
                    .map(|record| record.template.clone())
            });

        let average_reward = if traces.is_empty() {
            0.5
        } else {
            traces.iter().map(|trace| trace.final_reward).sum::<f64>() / traces.len() as f64
        };
        let reward_trend = compute_reward_trend(&traces);
        let verification_bias = if average_reward < 0.45 || reward_trend < 0.0 {
            0.8
        } else {
            0.65
        };

        let mut policy_notes = snapshot.policy_guidance.clone();
        if policy_notes.is_empty() {
            policy_notes.push(
                "No strong learning signal yet; keep the default verification-heavy reasoning profile."
                    .into(),
            );
        }

        Ok(ReasoningPolicy {
            domain: resolved_domain,
            champion_prompt,
            preferred_strategies: if preferred.is_empty() {
                vec![
                    ReasoningStrategy::MetaCognition,
                    ReasoningStrategy::TreeOfThought,
                    ReasoningStrategy::SelfCritique,
                ]
            } else {
                preferred
            },
            discouraged_strategies: discouraged,
            max_reasoning_depth: Some(if average_reward >= 0.7 { 18 } else { 12 }),
            max_reasoning_expansions: Some(if average_reward >= 0.7 { 650 } else { 420 }),
            exploration_c: Some(if reward_trend < 0.0 { 1.55 } else { 1.25 }),
            verification_bias,
            policy_notes,
        })
    }

    fn load_state(&mut self) -> AstraResult<()> {
        if let Some(value) = self.store.get_resource("lightning_objectives")? {
            let records: Vec<ObjectiveRecord> = serde_json::from_value(value)?;
            self.objectives = records
                .into_iter()
                .map(|record| (record.key.clone(), record))
                .collect();
        }

        if let Some(value) = self.store.get_resource("lightning_prompt_champions")? {
            let champions: Vec<PromptChampionRecord> = serde_json::from_value(value)?;
            for champion in champions {
                let template_id = self
                    .prompt_evolver
                    .add_template(&champion.template, &champion.domain);
                self.prompt_evolver
                    .record_reward(&template_id, champion.fitness);
            }
        }

        Ok(())
    }

    fn load_policy_snapshot(&self) -> AstraResult<Option<PolicySnapshot>> {
        self.store
            .get_resource("latest_policy_snapshot")?
            .map(serde_json::from_value)
            .transpose()
            .map_err(Into::into)
    }

    fn upsert_objective(&mut self, objective: OptimizationObjective) {
        let key = objective_key(&objective);
        let registered_at = now_timestamp();

        self.objectives
            .entry(key.clone())
            .and_modify(|record| {
                record.objective = objective.clone();
                record.last_updated_at = registered_at;
                record.health = recompute_objective_health(
                    record.ingested_traces,
                    record.successful_traces,
                    record.average_reward,
                );
            })
            .or_insert_with(|| ObjectiveRecord {
                key,
                objective,
                registered_at,
                last_updated_at: registered_at,
                ingested_traces: 0,
                successful_traces: 0,
                average_reward: 0.0,
                best_reward: f64::MIN,
                health: ObjectiveHealth::Pending,
            });
    }

    fn ensure_prompt_seed(&mut self, domain: &str, objective: Option<&OptimizationObjective>) {
        if self.prompt_evolver.best_template(domain).is_some() {
            return;
        }

        let seed = objective
            .map(base_prompt_for_objective)
            .unwrap_or_else(|| generic_prompt_for_domain(domain));
        self.prompt_evolver.add_template(&seed, domain);
    }

    fn compute_effective_reward(&mut self, trace: &TrajectoryTrace) -> f64 {
        let report = TraceVerificationReport::from_trace(trace);
        let reward = if report.has_signal {
            self.reward_computer
                .compute_reward(&report, &trace.domain, true)
                .primary_reward
        } else {
            sanitize_float(trace.final_reward)
        };

        if reward.is_finite() {
            reward
        } else {
            0.0
        }
    }

    fn record_prompt_reward(&mut self, domain: &str, reward: f64) {
        let current_template_id = self
            .prompt_evolver
            .best_template(domain)
            .map(|template| template.id.clone());

        if let Some(template_id) = current_template_id {
            self.prompt_evolver.record_reward(&template_id, reward);
        }
    }

    fn update_policy_signals(
        &mut self,
        trace: &TrajectoryTrace,
        policy_totals: &mut HashMap<String, f64>,
        policy_risks: &mut HashMap<String, usize>,
    ) {
        let credits = self.credit_assignment.assign_credit(trace);

        for (span, credit) in trace.spans.iter().zip(credits.iter()) {
            let span_key = span_type_label(&span.span_type).to_string();
            if credit.normalized_credit >= 0.0 {
                *policy_totals.entry(span_key).or_insert(0.0) += credit.normalized_credit;
            } else {
                *policy_totals
                    .entry(format!("negative:{span_key}"))
                    .or_insert(0.0) += credit.normalized_credit.abs();
            }
        }

        if !trace.success || trace.final_reward < 0.25 {
            for strategy in &trace.strategies_used {
                *policy_risks.entry(strategy.clone()).or_insert(0) += 1;
            }
        }
    }

    fn update_objectives(&mut self, trace: &TrajectoryTrace) {
        for record in self.objectives.values_mut() {
            if record.objective.domain != trace.domain {
                continue;
            }

            let previous_count = record.ingested_traces as f64;
            record.ingested_traces += 1;
            if trace.success {
                record.successful_traces += 1;
            }
            record.best_reward = record.best_reward.max(trace.final_reward);
            record.average_reward = ((record.average_reward * previous_count) + trace.final_reward)
                / record.ingested_traces as f64;
            record.last_updated_at = now_timestamp();
            record.health = recompute_objective_health(
                record.ingested_traces,
                record.successful_traces,
                record.average_reward,
            );
        }
    }

    fn maybe_evolve_domain(&mut self, domain: &str) {
        let enough_examples = self
            .objectives
            .values()
            .filter(|record| record.objective.domain == domain)
            .map(|record| record.ingested_traces)
            .max()
            .unwrap_or(0);

        if enough_examples < self.config.min_traces_for_evolution
            || enough_examples % self.config.evolution_interval != 0
        {
            return;
        }

        let _ = self.prompt_evolver.evolve(domain);
    }

    fn active_objective_count(&self) -> usize {
        self.objectives
            .values()
            .filter(|record| record.health != ObjectiveHealth::Pending)
            .count()
    }

    fn persist_objectives(&self) -> AstraResult<()> {
        let records = self.objectives.values().cloned().collect::<Vec<_>>();
        self.store
            .store_resource("lightning_objectives", &serde_json::to_value(records)?)?;
        Ok(())
    }

    fn persist_prompt_champions(&self) -> AstraResult<()> {
        let champions = objective_domains(&self.objectives)
            .into_iter()
            .filter_map(|domain| {
                self.prompt_evolver
                    .best_template(&domain)
                    .map(|template| PromptChampionRecord {
                        domain,
                        template: template.template.clone(),
                        fitness: template.fitness,
                    })
            })
            .collect::<Vec<_>>();

        self.store.store_resource(
            "lightning_prompt_champions",
            &serde_json::to_value(champions)?,
        )?;
        Ok(())
    }

    fn build_policy_snapshot(
        &self,
        recent_traces: &[TrajectoryTrace],
        policy_totals: &HashMap<String, f64>,
        policy_risks: &HashMap<String, usize>,
    ) -> PolicySnapshot {
        let positive_span_types = top_keys_by_float(
            policy_totals
                .iter()
                .filter(|(key, _)| !key.starts_with("negative:"))
                .map(|(key, value)| (key.clone(), *value))
                .collect(),
            3,
        );
        let negative_span_types = top_keys_by_float(
            policy_totals
                .iter()
                .filter_map(|(key, value)| {
                    key.strip_prefix("negative:")
                        .map(|label| (label.to_string(), *value))
                })
                .collect(),
            3,
        );
        let champion_prompts = objective_domains(&self.objectives)
            .into_iter()
            .filter_map(|domain| {
                self.prompt_evolver
                    .best_template(&domain)
                    .map(|template| PromptChampionRecord {
                        domain,
                        template: template.template.clone(),
                        fitness: template.fitness,
                    })
            })
            .collect::<Vec<_>>();

        let mut policy_guidance = Vec::new();
        if !positive_span_types.is_empty() {
            policy_guidance.push(format!(
                "Preserve {} checkpoints; these spans are returning the strongest positive credit across recent traces.",
                positive_span_types.join(", ")
            ));
        }
        if !negative_span_types.is_empty() {
            policy_guidance.push(format!(
                "Tighten {} loops with earlier validation or rollback guards; they are absorbing the most negative credit.",
                negative_span_types.join(", ")
            ));
        }

        let risk_signals = top_keys_by_count(policy_risks.clone(), self.config.max_guidance_items);
        if let Some(strategy) = risk_signals.first() {
            policy_guidance.push(format!(
                "Audit strategy `{strategy}` in low-reward traces before rolling it out further."
            ));
        }

        let at_risk_objectives = self
            .objectives
            .values()
            .filter(|record| record.health == ObjectiveHealth::AtRisk)
            .map(|record| {
                format!(
                    "{}/{}",
                    record.objective.domain, record.objective.reward_signal
                )
            })
            .collect::<Vec<_>>();
        if !at_risk_objectives.is_empty() {
            policy_guidance.push(format!(
                "Recovery plan needed for objectives {}. Increase verification density and shorten rollout depth until reward stabilizes.",
                at_risk_objectives.join(", ")
            ));
        }

        if policy_guidance.is_empty() && !recent_traces.is_empty() {
            policy_guidance.push(
                "Keep collecting traces; the current batch is stable, but there is not enough signal yet for a stronger policy change."
                    .into(),
            );
        }

        PolicySnapshot {
            generated_at: now_timestamp(),
            policy_guidance: policy_guidance
                .into_iter()
                .take(self.config.max_guidance_items)
                .collect(),
            risk_signals,
            champion_prompts,
            positive_span_types,
            negative_span_types,
        }
    }
}

fn summarize_learning(
    traces: &[TrajectoryTrace],
    objective_count: usize,
    active_objective_count: usize,
    snapshot: &PolicySnapshot,
) -> LearningSummary {
    if traces.is_empty() {
        return LearningSummary {
            objective_count,
            active_objective_count,
            ingested_traces: 0,
            average_reward: 0.0,
            reward_trend: 0.0,
            successful_traces: 0,
            top_domains: Vec::new(),
            top_strategies: Vec::new(),
            prompt_guidance:
                "Collect more successful trajectories before promoting new prompts or tool policies."
                    .into(),
            policy_guidance: snapshot.policy_guidance.clone(),
            champion_prompt: snapshot
                .champion_prompts
                .first()
                .map(|record| record.template.clone()),
            generated_at: now_timestamp(),
        };
    }

    let ingested_traces = traces.len();
    let successful_traces = traces.iter().filter(|trace| trace.success).count();
    let average_reward =
        traces.iter().map(|trace| trace.final_reward).sum::<f64>() / ingested_traces as f64;
    let reward_trend = compute_reward_trend(traces);

    let mut domain_counts = HashMap::new();
    let mut strategy_counts = HashMap::new();
    for trace in traces {
        *domain_counts.entry(trace.domain.clone()).or_insert(0usize) += 1;
        for strategy in &trace.strategies_used {
            *strategy_counts.entry(strategy.clone()).or_insert(0usize) += 1;
        }
    }

    let top_domains = top_keys_by_count(domain_counts, 5);
    let top_strategies = top_keys_by_count(strategy_counts, 5);
    let champion_prompt = snapshot
        .champion_prompts
        .first()
        .map(|record| record.template.clone());

    let prompt_guidance = if let Some(prompt) = champion_prompt.as_ref() {
        format!(
            "Current champion prompt is ready for controlled rollout. Preview: {}",
            truncate_text(prompt, 160)
        )
    } else if !top_strategies.is_empty() {
        format!(
            "Bias the next prompt revision toward the highest-yield strategies: {}.",
            top_strategies.join(", ")
        )
    } else {
        "Bias the next prompt revision toward shorter tool loops and explicit verification checkpoints."
            .into()
    };

    LearningSummary {
        objective_count,
        active_objective_count,
        ingested_traces,
        average_reward,
        reward_trend,
        successful_traces,
        top_domains,
        top_strategies,
        prompt_guidance,
        policy_guidance: snapshot.policy_guidance.clone(),
        champion_prompt,
        generated_at: now_timestamp(),
    }
}

fn compute_reward_trend(traces: &[TrajectoryTrace]) -> f64 {
    if traces.len() < 2 {
        return 0.0;
    }

    let mut sorted = traces.to_vec();
    sorted.sort_by(|left, right| {
        left.created_at
            .partial_cmp(&right.created_at)
            .unwrap_or(Ordering::Equal)
    });

    let midpoint = (sorted.len() / 2).max(1);
    let first_half = &sorted[..midpoint];
    let second_half = &sorted[midpoint..];
    if second_half.is_empty() {
        return 0.0;
    }

    average_trace_reward(second_half) - average_trace_reward(first_half)
}

fn average_trace_reward(traces: &[TrajectoryTrace]) -> f64 {
    traces.iter().map(|trace| trace.final_reward).sum::<f64>() / traces.len() as f64
}

fn recompute_objective_health(
    ingested_traces: usize,
    successful_traces: usize,
    average_reward: f64,
) -> ObjectiveHealth {
    if ingested_traces == 0 {
        return ObjectiveHealth::Pending;
    }

    let success_rate = successful_traces as f64 / ingested_traces as f64;
    if ingested_traces < 4 {
        ObjectiveHealth::Learning
    } else if success_rate >= 0.7 && average_reward >= 0.6 {
        ObjectiveHealth::Healthy
    } else if success_rate <= 0.35 || average_reward < 0.2 {
        ObjectiveHealth::AtRisk
    } else {
        ObjectiveHealth::Learning
    }
}

fn objective_key(objective: &OptimizationObjective) -> String {
    format!(
        "{}::{}",
        normalize_token(&objective.domain),
        normalize_token(&objective.reward_signal)
    )
}

fn objective_domains(objectives: &HashMap<String, ObjectiveRecord>) -> Vec<String> {
    let mut domains = objectives
        .values()
        .map(|record| record.objective.domain.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    domains.sort();
    domains
}

fn infer_domain(problem: &str) -> &str {
    let lower = problem.to_lowercase();
    if ["code", "rust", "bug", "function", "compile", "test"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "coding"
    } else if ["prove", "equation", "theorem", "math", "probability"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "logic"
    } else if ["design", "system", "architecture", "service", "scaling"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "architecture"
    } else if ["debug", "failure", "incident", "root cause", "broken"]
        .iter()
        .any(|token| lower.contains(token))
    {
        "debugging"
    } else {
        "general"
    }
}

fn base_prompt_for_objective(objective: &OptimizationObjective) -> String {
    format!(
        "Operate in the {} domain. Optimize for {}. Use concise reasoning, explicit verification checkpoints, and stop once the outcome is validated.",
        objective.domain, objective.reward_signal
    )
}

fn generic_prompt_for_domain(domain: &str) -> String {
    format!(
        "Work in the {domain} domain. Prefer short action loops, verify every high-risk step, and surface uncertainty before escalating."
    )
}

fn sanitize_trace(mut trace: TrajectoryTrace) -> TrajectoryTrace {
    trace.domain = normalize_domain(&trace.domain);
    trace.problem = trace.problem.trim().to_string();
    trace.final_answer = truncate_text(trace.final_answer.trim(), 2000);
    trace.final_reward = sanitize_float(trace.final_reward);
    trace.total_duration_ms = sanitize_float(trace.total_duration_ms).max(0.0);
    trace.created_at = if trace.created_at.is_finite() && trace.created_at > 0.0 {
        trace.created_at
    } else {
        now_timestamp()
    };

    trace.strategies_used = trace
        .strategies_used
        .into_iter()
        .map(|value| normalize_token(&value))
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    trace.strategies_used.sort();

    for value in trace.reward_dimensions.values_mut() {
        *value = sanitize_float(*value);
    }

    for span in &mut trace.spans {
        span.input_data = truncate_text(span.input_data.trim(), 500);
        span.output_data = truncate_text(span.output_data.trim(), 500);
        span.prompt_template = truncate_text(span.prompt_template.trim(), 300);
        span.reward = sanitize_float(span.reward);
        span.duration_ms = sanitize_float(span.duration_ms).max(0.0);
        span.timestamp = if span.timestamp.is_finite() && span.timestamp > 0.0 {
            span.timestamp
        } else {
            trace.created_at
        };
        span.cognitive_mode = normalize_token(&span.cognitive_mode);
        span.attributes
            .retain(|key, value| !key.trim().is_empty() && !value.trim().is_empty());
        for value in span.reward_dimensions.values_mut() {
            *value = sanitize_float(*value);
        }
    }

    trace.total_iterations = trace.spans.len();
    trace
}

fn normalize_domain(domain: &str) -> String {
    let normalized = normalize_token(domain);
    if normalized.is_empty() {
        "general".into()
    } else {
        normalized
    }
}

fn normalize_token(value: &str) -> String {
    value.trim().to_lowercase().replace(' ', "_")
}

fn strategy_from_token(token: &str) -> Option<ReasoningStrategy> {
    match normalize_token(token).as_str() {
        "chainofthought" | "chain_of_thought" => Some(ReasoningStrategy::ChainOfThought),
        "treeofthought" | "tree_of_thought" => Some(ReasoningStrategy::TreeOfThought),
        "decomposition" => Some(ReasoningStrategy::Decomposition),
        "backwardchaining" | "backward_chaining" => Some(ReasoningStrategy::BackwardChaining),
        "hypothesistest" | "hypothesis_test" => Some(ReasoningStrategy::HypothesisTest),
        "analogicalreasoning" | "analogical_reasoning" | "analogy" => {
            Some(ReasoningStrategy::AnalogicalReasoning)
        }
        "selfcritique" | "self_critique" | "critique" => Some(ReasoningStrategy::SelfCritique),
        "formalreasoning" | "formal_reasoning" => Some(ReasoningStrategy::FormalReasoning),
        "causalreasoning" | "causal_reasoning" => Some(ReasoningStrategy::CausalReasoning),
        "metacognition" | "meta_cognition" => Some(ReasoningStrategy::MetaCognition),
        "bayesianinference" | "bayesian_inference" => Some(ReasoningStrategy::BayesianInference),
        "counterfactualreasoning" | "counterfactual_reasoning" => {
            Some(ReasoningStrategy::CounterfactualReasoning)
        }
        "abductivereasoning" | "abductive_reasoning" => Some(ReasoningStrategy::AbductiveReasoning),
        "constraintsatisfaction" | "constraint_satisfaction" => {
            Some(ReasoningStrategy::ConstraintSatisfaction)
        }
        "adversarialreasoning" | "adversarial_reasoning" => {
            Some(ReasoningStrategy::AdversarialReasoning)
        }
        _ => None,
    }
}

fn preferred_strategies_from_traces(
    traces: &[TrajectoryTrace],
    problem: &str,
) -> Vec<ReasoningStrategy> {
    let mut scores = HashMap::new();
    for trace in traces {
        let weight = trace.final_reward.max(0.0) + if trace.success { 0.25 } else { 0.0 };
        for strategy in &trace.strategies_used {
            if let Some(reasoning_strategy) = strategy_from_token(strategy) {
                *scores.entry(reasoning_strategy).or_insert(0.0) += weight;
            }
        }
    }

    let mut preferred = top_reasoning_strategies(scores, 4);
    if preferred.is_empty() {
        preferred.push(ReasoningStrategy::MetaCognition);
        preferred.push(ReasoningStrategy::TreeOfThought);
    }

    let lower = problem.to_lowercase();
    if lower.contains("prove") && !preferred.contains(&ReasoningStrategy::FormalReasoning) {
        preferred.push(ReasoningStrategy::FormalReasoning);
    }
    if lower.contains("why") && !preferred.contains(&ReasoningStrategy::CausalReasoning) {
        preferred.push(ReasoningStrategy::CausalReasoning);
    }
    if lower.contains("debug") && !preferred.contains(&ReasoningStrategy::SelfCritique) {
        preferred.push(ReasoningStrategy::SelfCritique);
    }

    preferred.truncate(5);
    preferred
}

fn discouraged_strategies_from_traces(traces: &[TrajectoryTrace]) -> Vec<ReasoningStrategy> {
    let mut penalties = HashMap::new();
    for trace in traces {
        if trace.success && trace.final_reward >= 0.4 {
            continue;
        }
        let penalty = (0.5 - trace.final_reward).max(0.1);
        for strategy in &trace.strategies_used {
            if let Some(reasoning_strategy) = strategy_from_token(strategy) {
                *penalties.entry(reasoning_strategy).or_insert(0.0) += penalty;
            }
        }
    }

    top_reasoning_strategies(penalties, 2)
}

fn top_reasoning_strategies(
    mut scores: HashMap<ReasoningStrategy, f64>,
    limit: usize,
) -> Vec<ReasoningStrategy> {
    let mut items = scores.drain().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| format!("{:?}", left.0).cmp(&format!("{:?}", right.0)))
    });
    items
        .into_iter()
        .take(limit)
        .map(|(strategy, _)| strategy)
        .collect()
}

fn sanitize_float(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn span_type_label(span_type: &SpanType) -> &'static str {
    match span_type {
        SpanType::BrainDecision => "brain_decision",
        SpanType::ToolCall => "tool_call",
        SpanType::Reasoning => "reasoning",
        SpanType::Verification => "verification",
        SpanType::Reward => "reward",
        SpanType::Hypothesis => "hypothesis",
        SpanType::Metacognition => "metacognition",
        SpanType::Classification => "classification",
        SpanType::RiskAssessment => "risk_assessment",
    }
}

fn top_keys_by_count(mut counts: HashMap<String, usize>, limit: usize) -> Vec<String> {
    let mut items = counts.drain().collect::<Vec<_>>();
    items.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    items
        .into_iter()
        .take(limit)
        .map(|(name, _)| name)
        .collect()
}

fn top_keys_by_float(mut scores: HashMap<String, f64>, limit: usize) -> Vec<String> {
    let mut items = scores.drain().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    items
        .into_iter()
        .take(limit)
        .map(|(name, _)| name)
        .collect()
}

fn truncate_text(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn now_timestamp() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn unique_suffix() -> String {
    format!("{:.0}", now_timestamp() * 1000.0)
}

fn default_policy_snapshot() -> PolicySnapshot {
    PolicySnapshot {
        generated_at: now_timestamp(),
        policy_guidance: Vec::new(),
        risk_signals: Vec::new(),
        champion_prompts: Vec::new(),
        positive_span_types: Vec::new(),
        negative_span_types: Vec::new(),
    }
}

struct TraceVerificationReport {
    static_score: f64,
    property_score: f64,
    scenario_score: f64,
    critic_score: f64,
    code_score: f64,
    security_score: f64,
    critical_vulns: bool,
    has_signal: bool,
}

impl TraceVerificationReport {
    fn from_trace(trace: &TrajectoryTrace) -> Self {
        let dimensions = &trace.reward_dimensions;
        let static_score = dimensions.get("static").copied().unwrap_or(0.0);
        let property_score = dimensions.get("property").copied().unwrap_or(0.0);
        let scenario_score = dimensions.get("scenario").copied().unwrap_or(0.0);
        let critic_score = dimensions.get("critic").copied().unwrap_or(0.0);
        let code_score = dimensions
            .get("code_quality")
            .copied()
            .or_else(|| dimensions.get("code").copied())
            .unwrap_or(0.0);
        let security_score = dimensions.get("security").copied().unwrap_or(0.0);
        let has_signal = dimensions.contains_key("static")
            || dimensions.contains_key("property")
            || dimensions.contains_key("scenario")
            || dimensions.contains_key("critic")
            || dimensions.contains_key("code_quality")
            || dimensions.contains_key("code")
            || dimensions.contains_key("security");

        Self {
            static_score,
            property_score,
            scenario_score,
            critic_score,
            code_score,
            security_score,
            critical_vulns: !trace.success && security_score < 0.2,
            has_signal,
        }
    }
}

impl VerificationScores for TraceVerificationReport {
    fn v_static(&self) -> f64 {
        self.static_score
    }

    fn v_property(&self) -> f64 {
        self.property_score
    }

    fn v_scenario(&self) -> f64 {
        self.scenario_score
    }

    fn v_critic(&self) -> f64 {
        self.critic_score
    }

    fn v_code(&self) -> f64 {
        self.code_score
    }

    fn v_security(&self) -> f64 {
        self.security_score
    }

    fn has_critical_vulns(&self) -> bool {
        self.critical_vulns
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::trace_store::{TraceSpan, TrajectoryTrace};

    #[test]
    fn learning_loop_summarizes_batches_and_persists_state() {
        let temp_dir =
            std::env::temp_dir().join(format!("astra-lightning-test-{}", unique_suffix()));
        let loop_engine = LightningLoop::new(temp_dir.clone());

        let mut trace = TrajectoryTrace::new("browse docs");
        trace.domain = "web".into();
        trace.success = true;
        trace.final_reward = 0.82;
        trace.reward_dimensions.insert("security".into(), 0.9);
        trace.reward_dimensions.insert("scenario".into(), 0.8);
        trace.strategies_used = vec!["verify".into(), "browse".into()];
        let mut span = TraceSpan::new(SpanType::Verification);
        span.reward = 0.5;
        span.duration_ms = 120.0;
        trace.add_span(span);

        let summary = loop_engine
            .ingest_batch(OptimizationBatch {
                objective: Some(OptimizationObjective {
                    domain: "web".into(),
                    reward_signal: "task_completion".into(),
                    optimize_prompts: true,
                    optimize_tool_policies: true,
                    max_rollouts: 128,
                }),
                traces: vec![trace],
            })
            .expect("batch ingestion should work");

        assert_eq!(summary.ingested_traces, 1);
        assert_eq!(summary.successful_traces, 1);
        assert!(summary.top_domains.iter().any(|domain| domain == "web"));
        assert!(summary
            .policy_guidance
            .iter()
            .any(|item| item.contains("verification") || item.contains("collecting traces")));

        let latest_summary_path = temp_dir
            .join("resources")
            .join("latest_learning_summary.json");
        assert!(latest_summary_path.exists());
    }

    #[test]
    fn summary_handles_empty_runtime() {
        let temp_dir =
            std::env::temp_dir().join(format!("astra-lightning-empty-{}", unique_suffix()));
        let loop_engine = LightningLoop::new(temp_dir);
        let summary = loop_engine.summary(8);

        assert_eq!(summary.ingested_traces, 0);
        assert!(summary
            .prompt_guidance
            .contains("Collect more successful trajectories"));
    }
}

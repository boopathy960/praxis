use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use super::reasoning::ReasoningStrategy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutiveMode {
    Search,
    Decide,
    Invent,
    Verify,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmegaDecision {
    pub mode: ExecutiveMode,
    pub autonomy_score: f64,
    pub search_intensity: f64,
    pub invention_bias: f64,
    pub experience_weight: f64,
    pub human_review_recommended: bool,
    pub rationale: Vec<String>,
    pub preferred_strategies: Vec<ReasoningStrategy>,
    pub focus_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventionBlueprint {
    pub title: String,
    pub archetype: String,
    pub summary: String,
    pub novelty_score: f64,
    pub feasibility_score: f64,
    pub impact_score: f64,
    pub domains: Vec<String>,
    pub differentiator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmegaMissionStep {
    pub stage: String,
    pub lane: String,
    pub objective: String,
    pub success_signal: String,
    pub max_iterations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmegaMission {
    pub objective: String,
    pub mode: ExecutiveMode,
    pub priority_domains: Vec<String>,
    pub autonomy_envelope: String,
    pub human_review_required: bool,
    pub search_budget: usize,
    pub reasoning_budget: usize,
    pub invention_budget: usize,
    pub evidence_targets: Vec<String>,
    pub risk_controls: Vec<String>,
    pub exit_criteria: Vec<String>,
    pub steps: Vec<OmegaMissionStep>,
}

#[derive(Debug, Clone)]
struct DomainFeedback {
    cycles: u64,
    successes: u64,
    avg_quality: f64,
}

impl DomainFeedback {
    fn new() -> Self {
        Self {
            cycles: 0,
            successes: 0,
            avg_quality: 0.0,
        }
    }

    fn record(&mut self, success: bool, quality: f64) {
        self.cycles += 1;
        if success {
            self.successes += 1;
        }
        let quality = quality.clamp(0.0, 1.0);
        self.avg_quality = if self.cycles <= 1 {
            quality
        } else {
            (self.avg_quality * 0.75) + (quality * 0.25)
        };
    }

    fn success_rate(&self) -> f64 {
        if self.cycles == 0 {
            0.0
        } else {
            self.successes as f64 / self.cycles as f64
        }
    }
}

#[derive(Debug, Clone)]
struct ExperienceSignal {
    cycles: u64,
    success_rate: f64,
    avg_quality: f64,
    weight: f64,
}

pub struct OmegaExecutive {
    total_directives: u64,
    total_inventions: u64,
    successful_cycles: u64,
    avg_quality: f64,
    lane_quality: HashMap<String, (u64, f64)>,
    domain_feedback: HashMap<String, DomainFeedback>,
    recent_inventions: VecDeque<InventionBlueprint>,
    recent_missions: VecDeque<OmegaMission>,
}

impl OmegaExecutive {
    #[must_use]
    pub fn new() -> Self {
        Self {
            total_directives: 0,
            total_inventions: 0,
            successful_cycles: 0,
            avg_quality: 0.0,
            lane_quality: HashMap::new(),
            domain_feedback: HashMap::new(),
            recent_inventions: VecDeque::with_capacity(24),
            recent_missions: VecDeque::with_capacity(16),
        }
    }

    pub fn directive_for(&mut self, prompt: &str, context: &serde_json::Value) -> OmegaDecision {
        self.total_directives += 1;
        let lower = prompt.to_lowercase();
        let mode = if contains_any(
            &lower,
            &[
                "invent",
                "invention",
                "discover",
                "novel",
                "breakthrough",
                "new idea",
            ],
        ) {
            ExecutiveMode::Invent
        } else if contains_any(
            &lower,
            &["verify", "prove", "audit", "safety", "security", "forensic"],
        ) {
            ExecutiveMode::Verify
        } else if contains_any(
            &lower,
            &[
                "search", "research", "compare", "latest", "analyze", "internet", "world",
            ],
        ) {
            ExecutiveMode::Search
        } else {
            ExecutiveMode::Decide
        };

        let focus_domains = extract_focus_domains(prompt, context);
        let mut autonomy_score = autonomy_score(mode, &lower, context);
        let mut search_intensity = search_intensity(mode, &lower, &focus_domains);
        let mut invention_bias = invention_bias(mode, &lower, &focus_domains);
        let mut human_review_recommended = autonomy_score < 0.5
            || matches!(mode, ExecutiveMode::Verify)
            || context
                .get("high_impact")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        let experience = self.experience_for(mode, &focus_domains);
        let experience_weight = experience.as_ref().map_or(0.0, |signal| signal.weight);
        if let Some(signal) = &experience {
            autonomy_score = (autonomy_score
                + ((signal.avg_quality - 0.55) * 0.18)
                + ((signal.success_rate - 0.5) * 0.10))
                .clamp(0.2, 0.97);
            search_intensity =
                (search_intensity + ((1.0 - signal.avg_quality) * 0.05)).clamp(0.2, 1.0);
            invention_bias = (invention_bias + ((signal.avg_quality - 0.5) * 0.12)).clamp(0.0, 1.0);
            if signal.cycles >= 3 && signal.avg_quality < 0.5 {
                human_review_recommended = true;
            }
        }
        let mut rationale = rationale_for(
            mode,
            autonomy_score,
            search_intensity,
            invention_bias,
            &focus_domains,
        );
        if let Some(signal) = &experience {
            rationale.push(format!(
                "experience cycles={} success_rate={:.2} avg_quality={:.2} weight={:.2}",
                signal.cycles, signal.success_rate, signal.avg_quality, signal.weight
            ));
        } else {
            rationale
                .push("experience cycles=0 success_rate=0.00 avg_quality=0.00 weight=0.00".into());
        }
        let preferred_strategies = preferred_strategies_for(mode, invention_bias, search_intensity);

        OmegaDecision {
            mode,
            autonomy_score,
            search_intensity,
            invention_bias,
            experience_weight,
            human_review_recommended,
            rationale,
            preferred_strategies,
            focus_domains,
        }
    }

    pub fn mission_for(&mut self, prompt: &str, decision: &OmegaDecision) -> OmegaMission {
        let objective = normalize_objective(prompt);
        let autonomy_envelope =
            autonomy_envelope(decision.autonomy_score, decision.human_review_recommended);
        let search_budget = search_budget_for(decision);
        let reasoning_budget = reasoning_budget_for(decision);
        let invention_budget = invention_budget_for(decision);
        let evidence_targets = evidence_targets_for(decision);
        let risk_controls = risk_controls_for(decision);
        let exit_criteria = exit_criteria_for(decision);
        let steps = mission_steps_for(
            &objective,
            decision,
            search_budget,
            reasoning_budget,
            invention_budget,
        );

        let mission = OmegaMission {
            objective,
            mode: decision.mode,
            priority_domains: decision.focus_domains.clone(),
            autonomy_envelope,
            human_review_required: decision.human_review_recommended,
            search_budget,
            reasoning_budget,
            invention_budget,
            evidence_targets,
            risk_controls,
            exit_criteria,
            steps,
        };

        self.recent_missions.push_front(mission.clone());
        while self.recent_missions.len() > 12 {
            self.recent_missions.pop_back();
        }

        mission
    }

    pub fn invent(
        &mut self,
        prompt: &str,
        focus_domains: &[String],
        evidence_domains: &[String],
        limit: usize,
    ) -> Vec<InventionBlueprint> {
        let mut domains = BTreeSet::new();
        for domain in focus_domains {
            let normalized = domain.trim().to_lowercase();
            if !normalized.is_empty() {
                domains.insert(normalized);
            }
        }
        for domain in evidence_domains {
            let normalized = domain.trim().to_lowercase();
            if !normalized.is_empty() {
                domains.insert(normalized);
            }
        }
        if domains.is_empty() {
            for fallback in infer_domains_from_prompt(prompt) {
                domains.insert(fallback);
            }
        }

        let domain_vec = domains.into_iter().collect::<Vec<_>>();
        let archetypes = [
            ("contradiction_lattice", "cross-checks evidence until conflicts collapse into actionable constraints"),
            ("proof_caching_fabric", "stores validated intermediate conclusions so repeated browsing work gets cheaper over time"),
            ("adaptive_browser_hive", "routes micro-tasks across specialized browser lanes and only escalates expensive work when evidence requires it"),
            ("invariant_foundry", "turns observed failure patterns into reusable execution invariants and safety rules"),
            ("novelty_miner", "recombines distant domains into bounded invention candidates scored for feasibility and impact"),
        ];

        let mut blueprints = Vec::new();
        for (index, archetype) in archetypes.iter().enumerate() {
            let primary = domain_vec
                .get(index % domain_vec.len())
                .cloned()
                .unwrap_or_else(|| "systems".into());
            let secondary = domain_vec
                .get((index + 1) % domain_vec.len())
                .cloned()
                .unwrap_or_else(|| "reasoning".into());
            let novelty_score =
                compute_novelty_score(prompt, &primary, &secondary, archetype.0, index);
            let feasibility_score =
                compute_feasibility_score(prompt, &primary, &secondary, archetype.0);
            let impact_score =
                compute_impact_score(novelty_score, feasibility_score, prompt, archetype.0);
            let title = format!(
                "{} for {} and {}",
                title_case(archetype.0),
                title_case(&primary),
                title_case(&secondary)
            );
            let summary = format!(
                "A {} that fuses {} with {} so the browser can {}.",
                archetype.0.replace('_', " "),
                primary,
                secondary,
                archetype.1
            );
            let differentiator = format!(
                "Unlike generic assistants, this blueprint treats {} and {} as a coupled execution system with bounded search and explicit verification.",
                primary, secondary
            );

            blueprints.push(InventionBlueprint {
                title,
                archetype: archetype.0.to_string(),
                summary,
                novelty_score,
                feasibility_score,
                impact_score,
                domains: vec![primary, secondary],
                differentiator,
            });
        }

        blueprints.sort_by(|left, right| {
            right
                .impact_score
                .partial_cmp(&left.impact_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    right
                        .novelty_score
                        .partial_cmp(&left.novelty_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        blueprints.truncate(limit.max(1));

        self.total_inventions += blueprints.len() as u64;
        for blueprint in &blueprints {
            self.recent_inventions.push_front(blueprint.clone());
        }
        while self.recent_inventions.len() > 24 {
            self.recent_inventions.pop_back();
        }

        blueprints
    }

    pub fn record_cycle(&mut self, lane: &str, success: bool, quality: f64) {
        self.record_cycle_internal(lane, None, &[], success, quality);
    }

    pub fn record_cycle_with_context(
        &mut self,
        lane: &str,
        mode: ExecutiveMode,
        focus_domains: &[String],
        success: bool,
        quality: f64,
    ) {
        self.record_cycle_internal(lane, Some(mode), focus_domains, success, quality);
    }

    fn record_cycle_internal(
        &mut self,
        lane: &str,
        mode: Option<ExecutiveMode>,
        focus_domains: &[String],
        success: bool,
        quality: f64,
    ) {
        if success {
            self.successful_cycles += 1;
        }
        let quality = quality.clamp(0.0, 1.0);
        self.avg_quality = if self.total_directives <= 1 {
            quality
        } else {
            (self.avg_quality * 0.85) + (quality * 0.15)
        };

        let entry = self
            .lane_quality
            .entry(lane.to_string())
            .or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 = if entry.0 <= 1 {
            quality
        } else {
            (entry.1 * 0.8) + (quality * 0.2)
        };

        if let Some(mode) = mode {
            let domains = if focus_domains.is_empty() {
                vec!["general".to_string()]
            } else {
                focus_domains.to_vec()
            };
            for domain in domains {
                let key = domain_feedback_key(&domain, mode);
                self.domain_feedback
                    .entry(key)
                    .or_insert_with(DomainFeedback::new)
                    .record(success, quality);
            }
        }
    }

    #[must_use]
    pub fn get_stats(&self) -> serde_json::Value {
        let lanes = self
            .lane_quality
            .iter()
            .map(|(lane, (count, quality))| {
                (
                    lane.clone(),
                    serde_json::json!({
                        "cycles": count,
                        "avg_quality": quality,
                    }),
                )
            })
            .collect::<HashMap<_, _>>();
        let domain_feedback = self
            .domain_feedback
            .iter()
            .map(|(key, feedback)| {
                (
                    key.clone(),
                    serde_json::json!({
                        "cycles": feedback.cycles,
                        "success_rate": feedback.success_rate(),
                        "avg_quality": feedback.avg_quality,
                    }),
                )
            })
            .collect::<BTreeMap<_, _>>();

        serde_json::json!({
            "engine": "OmegaExecutive v1.0",
            "total_directives": self.total_directives,
            "total_inventions": self.total_inventions,
            "success_rate": if self.total_directives == 0 {
                0.0
            } else {
                self.successful_cycles as f64 / self.total_directives as f64
            },
            "avg_quality": self.avg_quality,
            "lane_quality": lanes,
            "domain_feedback": domain_feedback,
            "recent_inventions": self.recent_inventions.iter().take(5).collect::<Vec<_>>(),
            "recent_missions": self.recent_missions.iter().take(3).collect::<Vec<_>>(),
        })
    }

    fn experience_for(
        &self,
        mode: ExecutiveMode,
        focus_domains: &[String],
    ) -> Option<ExperienceSignal> {
        let domains = if focus_domains.is_empty() {
            vec!["general".to_string()]
        } else {
            focus_domains.to_vec()
        };

        let mut cycles = 0_u64;
        let mut weighted_success = 0.0;
        let mut weighted_quality = 0.0;

        for domain in domains {
            let key = domain_feedback_key(&domain, mode);
            if let Some(feedback) = self.domain_feedback.get(&key) {
                cycles += feedback.cycles;
                weighted_success += feedback.success_rate() * feedback.cycles as f64;
                weighted_quality += feedback.avg_quality * feedback.cycles as f64;
            }
        }

        if cycles == 0 {
            None
        } else {
            Some(ExperienceSignal {
                cycles,
                success_rate: weighted_success / cycles as f64,
                avg_quality: weighted_quality / cycles as f64,
                weight: ((cycles as f64 / 10.0) * 0.18).clamp(0.03, 0.18),
            })
        }
    }
}

impl Default for OmegaExecutive {
    fn default() -> Self {
        Self::new()
    }
}

fn contains_any(input: &str, tokens: &[&str]) -> bool {
    tokens.iter().any(|token| input.contains(token))
}

fn normalize_objective(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        "advance the browser mission".into()
    } else if trimmed.len() <= 140 {
        trimmed.to_string()
    } else {
        // Find the last space before the 140-char limit to avoid cutting mid-word
        // or mid-UTF8 sequence (which would panic).
        let boundary = trimmed[..140]
            .rfind(' ')
            .unwrap_or(137.min(trimmed.len()));
        format!("{}...", &trimmed[..boundary])
    }
}

fn autonomy_envelope(autonomy_score: f64, human_review_recommended: bool) -> String {
    if human_review_recommended {
        "review_guided".into()
    } else if autonomy_score >= 0.8 {
        "bounded_autonomy".into()
    } else {
        "supervised_autonomy".into()
    }
}

fn search_budget_for(decision: &OmegaDecision) -> usize {
    let base = match decision.mode {
        ExecutiveMode::Search => 10,
        ExecutiveMode::Decide => 6,
        ExecutiveMode::Invent => 8,
        ExecutiveMode::Verify => 7,
    };
    (base as f64 + (decision.search_intensity * 8.0)).round() as usize
}

fn reasoning_budget_for(decision: &OmegaDecision) -> usize {
    let base = match decision.mode {
        ExecutiveMode::Search => 5,
        ExecutiveMode::Decide => 7,
        ExecutiveMode::Invent => 8,
        ExecutiveMode::Verify => 9,
    };
    (base as f64 + (decision.autonomy_score * 6.0) + (decision.focus_domains.len() as f64)).round()
        as usize
}

fn invention_budget_for(decision: &OmegaDecision) -> usize {
    if matches!(decision.mode, ExecutiveMode::Invent) || decision.invention_bias >= 0.65 {
        (1.0 + (decision.invention_bias * 4.0)).round() as usize
    } else {
        0
    }
}

fn evidence_targets_for(decision: &OmegaDecision) -> Vec<String> {
    let mut targets = match decision.mode {
        ExecutiveMode::Search => vec!["docs".into(), "web".into(), "news".into()],
        ExecutiveMode::Decide => vec!["docs".into(), "benchmarks".into()],
        ExecutiveMode::Invent => vec!["papers".into(), "docs".into(), "benchmarks".into()],
        ExecutiveMode::Verify => vec!["specs".into(), "audits".into(), "tests".into()],
    };
    if decision.search_intensity >= 0.85 {
        targets.push("fresh_search".into());
    }
    targets.sort();
    targets.dedup();
    targets
}

fn risk_controls_for(decision: &OmegaDecision) -> Vec<String> {
    let mut controls = vec![
        "bound search budgets".into(),
        "require cross-source agreement".into(),
    ];
    if decision.human_review_recommended {
        controls.push("route final action through human review".into());
    }
    if matches!(decision.mode, ExecutiveMode::Verify) {
        controls.push("escalate contradictions before acceptance".into());
    }
    if decision.invention_bias >= 0.7 {
        controls.push("screen inventions for feasibility before promotion".into());
    }
    controls
}

fn exit_criteria_for(decision: &OmegaDecision) -> Vec<String> {
    let mut criteria = vec!["produce a cited final answer".into()];
    if matches!(decision.mode, ExecutiveMode::Invent) {
        criteria.push("rank invention blueprints by impact and feasibility".into());
    }
    if matches!(decision.mode, ExecutiveMode::Verify) {
        criteria.push("resolve or surface all major contradictions".into());
    }
    criteria.push("emit orchestration metadata for observability".into());
    criteria
}

fn mission_steps_for(
    objective: &str,
    decision: &OmegaDecision,
    search_budget: usize,
    reasoning_budget: usize,
    invention_budget: usize,
) -> Vec<OmegaMissionStep> {
    let mut steps = vec![
        OmegaMissionStep {
            stage: "scope".into(),
            lane: "decision".into(),
            objective: format!("Clarify the mission boundaries for {objective}."),
            success_signal: "goal and domain boundaries are explicit".into(),
            max_iterations: 2,
        },
        OmegaMissionStep {
            stage: "acquire".into(),
            lane: "search".into(),
            objective: format!(
                "Gather high-signal evidence across {} priority domains.",
                decision.focus_domains.len().max(1)
            ),
            success_signal: "evidence coverage reaches the required source mix".into(),
            max_iterations: search_budget.max(2),
        },
    ];

    if invention_budget > 0 {
        steps.push(OmegaMissionStep {
            stage: "invent".into(),
            lane: "invent".into(),
            objective: "Generate bounded, ranked invention candidates from distant-domain overlaps."
                .into(),
            success_signal: "at least one invention candidate passes feasibility screening".into(),
            max_iterations: invention_budget,
        });
    }

    steps.push(OmegaMissionStep {
        stage: "reason".into(),
        lane: "reason".into(),
        objective: "Synthesize evidence into a coherent decision or answer.".into(),
        success_signal: "reasoning path converges with acceptable confidence".into(),
        max_iterations: reasoning_budget.max(3),
    });
    steps.push(OmegaMissionStep {
        stage: "verify".into(),
        lane: "verify".into(),
        objective: "Cross-check contradictions, safety constraints, and unsupported claims.".into(),
        success_signal: "verification score clears the acceptance threshold".into(),
        max_iterations: if matches!(decision.mode, ExecutiveMode::Verify) {
            6
        } else {
            3
        },
    });
    steps
}

fn domain_feedback_key(domain: &str, mode: ExecutiveMode) -> String {
    format!("{}::{:?}", domain.trim().to_lowercase(), mode).to_lowercase()
}

fn extract_focus_domains(prompt: &str, context: &serde_json::Value) -> Vec<String> {
    let mut domains = context
        .get("domains")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                .map(|value| value.trim().to_lowercase())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    domains.extend(infer_domains_from_prompt(prompt));
    domains.sort();
    domains.dedup();
    domains.truncate(5);
    domains
}

fn infer_domains_from_prompt(prompt: &str) -> Vec<String> {
    let lower = prompt.to_lowercase();
    let mut domains = Vec::new();

    if contains_any(&lower, &["browser", "render", "tab", "page", "search"]) {
        domains.push("browser".into());
    }
    if contains_any(&lower, &["reason", "logic", "prove", "decision"]) {
        domains.push("reasoning".into());
    }
    if contains_any(&lower, &["security", "audit", "vulnerability", "safe"]) {
        domains.push("security".into());
    }
    if contains_any(
        &lower,
        &["performance", "fast", "speed", "cache", "throughput"],
    ) {
        domains.push("performance".into());
    }
    if contains_any(&lower, &["invent", "novel", "breakthrough", "new"]) {
        domains.push("innovation".into());
    }

    if domains.is_empty() {
        domains.push("systems".into());
    }

    domains
}

fn autonomy_score(mode: ExecutiveMode, prompt: &str, context: &serde_json::Value) -> f64 {
    let base: f64 = match mode {
        ExecutiveMode::Search => 0.72,
        ExecutiveMode::Decide => 0.64,
        ExecutiveMode::Invent => 0.58,
        ExecutiveMode::Verify => 0.42,
    };
    let performance_bonus: f64 = if contains_any(prompt, &["performance", "cache", "speed"]) {
        0.08
    } else {
        0.0
    };
    let risk_penalty: f64 = if context
        .get("high_impact")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        0.18
    } else {
        0.0
    };

    (base + performance_bonus - risk_penalty).clamp(0.2, 0.95)
}

fn search_intensity(mode: ExecutiveMode, prompt: &str, focus_domains: &[String]) -> f64 {
    let base = match mode {
        ExecutiveMode::Search => 0.85,
        ExecutiveMode::Decide => 0.65,
        ExecutiveMode::Invent => 0.74,
        ExecutiveMode::Verify => 0.78,
    };
    let domain_bonus = (focus_domains.len() as f64 * 0.03).min(0.12);
    let freshness_bonus = if contains_any(prompt, &["latest", "today", "current", "internet"]) {
        0.08
    } else {
        0.0
    };

    (base + domain_bonus + freshness_bonus).clamp(0.2, 1.0)
}

fn invention_bias(mode: ExecutiveMode, prompt: &str, focus_domains: &[String]) -> f64 {
    let base: f64 = match mode {
        ExecutiveMode::Invent => 0.88,
        ExecutiveMode::Search => 0.52,
        ExecutiveMode::Decide => 0.44,
        ExecutiveMode::Verify => 0.26,
    };
    let keyword_bonus: f64 = if contains_any(prompt, &["invent", "new", "beyond", "asi", "better"])
    {
        0.08
    } else {
        0.0
    };
    let cross_domain_bonus: f64 = if focus_domains.len() >= 2 { 0.06 } else { 0.0 };

    (base + keyword_bonus + cross_domain_bonus).clamp(0.0, 1.0)
}

fn rationale_for(
    mode: ExecutiveMode,
    autonomy_score: f64,
    search_intensity: f64,
    invention_bias: f64,
    focus_domains: &[String],
) -> Vec<String> {
    vec![
        format!("mode={mode:?}"),
        format!("autonomy_score={autonomy_score:.2}"),
        format!("search_intensity={search_intensity:.2}"),
        format!("invention_bias={invention_bias:.2}"),
        format!("focus_domains={}", focus_domains.join(",")),
    ]
}

fn preferred_strategies_for(
    mode: ExecutiveMode,
    invention_bias: f64,
    search_intensity: f64,
) -> Vec<ReasoningStrategy> {
    let mut strategies = match mode {
        ExecutiveMode::Search => vec![
            ReasoningStrategy::TreeOfThought,
            ReasoningStrategy::HypothesisTest,
            ReasoningStrategy::CausalReasoning,
        ],
        ExecutiveMode::Decide => vec![
            ReasoningStrategy::MetaCognition,
            ReasoningStrategy::Decomposition,
            ReasoningStrategy::SelfCritique,
        ],
        ExecutiveMode::Invent => vec![
            ReasoningStrategy::AnalogicalReasoning,
            ReasoningStrategy::AbductiveReasoning,
            ReasoningStrategy::CounterfactualReasoning,
        ],
        ExecutiveMode::Verify => vec![
            ReasoningStrategy::FormalReasoning,
            ReasoningStrategy::AdversarialReasoning,
            ReasoningStrategy::ConstraintSatisfaction,
        ],
    };

    if search_intensity >= 0.75 {
        strategies.push(ReasoningStrategy::BayesianInference);
    }
    if invention_bias >= 0.7 {
        strategies.push(ReasoningStrategy::AnalogicalReasoning);
    }
    strategies.sort_by_key(|strategy| strategy.as_str());
    strategies.dedup();
    strategies
}

fn compute_novelty_score(
    prompt: &str,
    primary: &str,
    secondary: &str,
    archetype: &str,
    index: usize,
) -> f64 {
    let cross_domain = if primary != secondary { 0.14 } else { 0.05 };
    let prompt_bonus = if prompt.to_lowercase().contains("invent") {
        0.08
    } else {
        0.03
    };
    (0.58 + cross_domain + prompt_bonus + (index as f64 * 0.03) + uniqueness_bonus(archetype))
        .clamp(0.0, 0.99)
}

fn compute_feasibility_score(prompt: &str, primary: &str, secondary: &str, archetype: &str) -> f64 {
    let systems_bonus: f64 = if primary == "browser"
        || secondary == "browser"
        || primary == "performance"
        || secondary == "performance"
    {
        0.12
    } else {
        0.04
    };
    let proof_penalty: f64 = if archetype.contains("proof") {
        0.08
    } else {
        0.0
    };
    let bounded_bonus: f64 = if prompt.to_lowercase().contains("production") {
        0.06
    } else {
        0.0
    };

    (0.62 + systems_bonus + bounded_bonus - proof_penalty).clamp(0.2, 0.95)
}

fn compute_impact_score(
    novelty_score: f64,
    feasibility_score: f64,
    prompt: &str,
    archetype: &str,
) -> f64 {
    let browser_bonus = if prompt.to_lowercase().contains("browser") {
        0.08
    } else {
        0.0
    };
    let safety_bonus = if archetype.contains("invariant") || archetype.contains("contradiction") {
        0.05
    } else {
        0.0
    };
    (novelty_score * 0.45 + feasibility_score * 0.45 + browser_bonus + safety_bonus).clamp(0.0, 1.0)
}

fn uniqueness_bonus(archetype: &str) -> f64 {
    match archetype {
        "novelty_miner" => 0.07,
        "contradiction_lattice" => 0.05,
        "adaptive_browser_hive" => 0.04,
        _ => 0.03,
    }
}

fn title_case(input: &str) -> String {
    input
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invention_directive_prefers_invent_mode() {
        let mut executive = OmegaExecutive::new();
        let decision = executive.directive_for(
            "invent new browser reasoning systems beyond current search engines",
            &serde_json::json!({"domains": ["browser", "performance"]}),
        );

        assert_eq!(decision.mode, ExecutiveMode::Invent);
        assert!(decision.invention_bias > 0.7);
        assert!(decision
            .preferred_strategies
            .contains(&ReasoningStrategy::AnalogicalReasoning));
    }

    #[test]
    fn invention_generation_produces_ranked_blueprints() {
        let mut executive = OmegaExecutive::new();
        let inventions = executive.invent(
            "invent a production browser brain",
            &["browser".into(), "security".into()],
            &["performance".into(), "reasoning".into()],
            3,
        );

        assert_eq!(inventions.len(), 3);
        assert!(inventions[0].impact_score >= inventions[1].impact_score);
        assert!(inventions.iter().all(|item| !item.title.is_empty()));
    }

    #[test]
    fn experience_feedback_increases_autonomy_for_strong_domains() {
        let prompt = "invent a production browser performance architecture";
        let context = serde_json::json!({"domains": ["browser", "performance"]});

        let mut baseline_executive = OmegaExecutive::new();
        let baseline = baseline_executive.directive_for(prompt, &context);

        let mut executive = OmegaExecutive::new();
        for _ in 0..4 {
            executive.record_cycle_with_context(
                "invention",
                ExecutiveMode::Invent,
                &["browser".into(), "performance".into()],
                true,
                0.92,
            );
        }
        let adapted = executive.directive_for(prompt, &context);

        assert!(adapted.autonomy_score > baseline.autonomy_score);
        assert!(adapted.experience_weight > 0.0);
    }

    #[test]
    fn mission_builder_creates_invent_and_verify_stages() {
        let mut executive = OmegaExecutive::new();
        let decision = executive.directive_for(
            "invent a new browser reasoning engine for production",
            &serde_json::json!({"domains": ["browser", "reasoning"]}),
        );
        let mission = executive.mission_for(
            "invent a new browser reasoning engine for production",
            &decision,
        );

        assert_eq!(mission.mode, ExecutiveMode::Invent);
        assert!(mission.invention_budget >= 1);
        assert!(mission.steps.iter().any(|step| step.lane == "invent"));
        assert!(mission.steps.iter().any(|step| step.lane == "verify"));
    }
}

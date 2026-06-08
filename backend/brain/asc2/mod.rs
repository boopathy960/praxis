use serde::{Deserialize, Serialize};

const DEFAULT_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2Config {
    pub capability_dimensions: usize,
    pub max_agents: usize,
    pub max_concurrent_agents: usize,
    pub max_rounds: usize,
    pub epsilon: f64,
    pub regularization_mu: f64,
    pub certificate_threshold: f64,
    pub answer_margin_threshold: f64,
    pub win_probability_threshold: f64,
    pub risk_limit: f64,
    pub spawn_threshold: f64,
    pub self_modify_risk_limit: f64,
    pub validation_loss_tolerance: f64,
}

impl Default for Asc2Config {
    fn default() -> Self {
        Self {
            capability_dimensions: 16,
            max_agents: 12,
            max_concurrent_agents: 5,
            max_rounds: 8,
            epsilon: DEFAULT_EPSILON,
            regularization_mu: 1e-3,
            certificate_threshold: 0.8,
            answer_margin_threshold: 0.15,
            win_probability_threshold: 0.6,
            risk_limit: 0.2,
            spawn_threshold: 0.25,
            self_modify_risk_limit: 0.12,
            validation_loss_tolerance: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflexiveAgentState {
    pub id: String,
    pub hidden_reasoning: String,
    pub role: String,
    pub role_embedding: Vec<f64>,
    pub capability: Vec<f64>,
    pub belief: Vec<f64>,
    pub claim: String,
    pub evidence_score: f64,
    pub proof_score: f64,
    pub trust: f64,
    pub uncertainty: f64,
    pub risk: f64,
    pub autonomy_budget: f64,
    pub memory_refs: Vec<String>,
    pub warning_score: f64,
    pub verification_target: Option<String>,
    pub limitation: String,
    pub counterexample_request: Option<String>,
    pub privilege: f64,
    pub clone_score: f64,
    pub relevance: f64,
    pub accuracy: f64,
    pub confidence: f64,
}

impl ReflexiveAgentState {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        normalize_distribution(&mut self.belief);
        self.evidence_score = clamp01(self.evidence_score);
        self.proof_score = clamp01(self.proof_score);
        self.trust = clamp01(self.trust);
        self.uncertainty = clamp01(self.uncertainty);
        self.risk = clamp01(self.risk);
        self.autonomy_budget = clamp01(self.autonomy_budget);
        self.warning_score = clamp01(self.warning_score);
        self.privilege = clamp01(self.privilege);
        self.clone_score = clamp01(self.clone_score);
        self.relevance = clamp01(self.relevance);
        self.accuracy = clamp01(self.accuracy);
        self.confidence = clamp01(self.confidence);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmState {
    pub task: String,
    pub task_demand: Vec<f64>,
    pub agents: Vec<ReflexiveAgentState>,
    pub controller_policy: Vec<f64>,
    pub action_tokens: Vec<ActionToken>,
    pub safety_ledger: Vec<String>,
    pub spawn_threshold: f64,
    pub global_autonomy_budget: f64,
    pub round: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofPacket {
    pub sender_id: String,
    pub claim: String,
    pub evidence_score: f64,
    pub uncertainty: f64,
    pub warning_score: f64,
    pub verification_target: Option<String>,
    pub proof_score: f64,
    pub limitation: String,
    pub counterexample_request: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionToken {
    pub action: String,
    pub preconditions: Vec<String>,
    pub invariants: Vec<String>,
    pub postconditions: Vec<String>,
    pub rollback: Vec<String>,
    pub audit: serde_json::Value,
    pub scope: Vec<String>,
    pub requested_privilege: f64,
    pub estimated_risk: f64,
    pub side_effecting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCertificate {
    pub score: f64,
    pub passed: bool,
    pub least_privilege: bool,
    pub rollback_present: bool,
    pub audit_present: bool,
    pub safety_verifiers: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleCandidate {
    pub role: String,
    pub role_embedding: Vec<f64>,
    pub capability: Vec<f64>,
    pub novelty: f64,
    pub expected_quality: f64,
    pub clone_score: f64,
    pub risk: f64,
    pub budget_cost: f64,
    pub privilege: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlActionEstimate {
    pub action: String,
    pub score_delta: f64,
    pub entropy_reduction: f64,
    pub gap_reduction: f64,
    pub latency: f64,
    pub compute: f64,
    pub tool_overhead: f64,
    pub risk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    pub benchmark_score: f64,
    pub quality: f64,
    pub verified_intelligence: f64,
    pub speed: f64,
    pub cooperation: f64,
    pub latency: f64,
    pub compute_cost: f64,
    pub tool_overhead: f64,
    pub correction_effort: f64,
    pub risk: f64,
    pub privilege: f64,
}

impl Default for PerformanceSnapshot {
    fn default() -> Self {
        Self {
            benchmark_score: 0.0,
            quality: 0.0,
            verified_intelligence: 0.0,
            speed: 0.0,
            cooperation: 0.0,
            latency: 0.0,
            compute_cost: 0.0,
            tool_overhead: 0.0,
            correction_effort: 0.0,
            risk: 0.0,
            privilege: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModificationProposal {
    pub id: String,
    pub validation_accuracy_delta: f64,
    pub cooperation_delta: f64,
    pub estimated_risk: f64,
    pub policy_kl: f64,
    pub capability_gap_delta: f64,
    pub certificate_score: f64,
    pub current_validation_loss: f64,
    pub candidate_validation_loss: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModificationDecision {
    pub accepted: bool,
    pub objective: f64,
    pub step_size: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub id: String,
    pub future_evidence: f64,
    pub risk: f64,
    pub privacy: f64,
    pub ledger_conflict: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlDecision {
    Accelerate,
    Verify,
    Return,
    Ask,
    Spawn,
    DowngradeAutonomy,
    Refuse,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Asc2Metrics {
    pub aegis_power: f64,
    pub capability_gap_trace: f64,
    pub cooperation: f64,
    pub debate_margin: f64,
    pub collective_accuracy_margin: f64,
    pub failure_bound: f64,
    pub lyapunov: f64,
    pub unified_objective: f64,
    pub performance_acceleration: f64,
    pub benchmark_win_probability: f64,
    pub certificate_score: f64,
    pub global_autonomy_budget: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2ExecutionResult {
    pub decision: ControlDecision,
    pub selected_answer: usize,
    pub fused_belief: Vec<f64>,
    pub selected_role: Option<String>,
    pub selected_control_action: Option<String>,
    pub metrics: Asc2Metrics,
    pub certificate: Option<ActionCertificate>,
    pub rounds: usize,
    pub return_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComparisonDeltas {
    pub benchmark: f64,
    pub quality: f64,
    pub latency: f64,
    pub compute_cost: f64,
    pub tool_overhead: f64,
    pub verified_intelligence: f64,
    pub cooperation: f64,
    pub safety: f64,
}

pub struct Asc2Controller {
    pub config: Asc2Config,
}

impl Asc2Controller {
    #[must_use]
    pub fn new(config: Asc2Config) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn evaluate(
        &self,
        state: &SwarmState,
        role_candidates: &[RoleCandidate],
        control_actions: &[ControlActionEstimate],
        action: Option<&ActionToken>,
        current: &PerformanceSnapshot,
        previous: &PerformanceSnapshot,
        baseline: &PerformanceSnapshot,
    ) -> Asc2ExecutionResult {
        let gap = capability_gap_tensor(
            &state.task_demand,
            &state.agents,
            self.config.regularization_mu,
        );
        let gap_trace = positive_trace(&gap);
        let weights = evidence_trust_weights(&state.agents);
        let cooperation = cooperation_functional(&state.agents, &weights);
        let fused_belief = debate_hardened_fusion(
            &state.agents,
            &weights,
            current.risk,
            mean(
                &state
                    .agents
                    .iter()
                    .map(|agent| agent.uncertainty)
                    .collect::<Vec<_>>(),
            ),
        );
        let selected_answer = argmax(&fused_belief).unwrap_or(0);
        let debate_margin = distribution_margin(&fused_belief);
        let covariance = error_covariance(&state.agents);
        let collective = collective_accuracy_bound(
            &state.agents,
            &weights,
            &covariance,
            current.risk,
            action.map_or(0.0, |token| token.estimated_risk),
            self.config.certificate_threshold,
            action.map_or(0.0, |token| {
                certify_action(token, &state.agents, self.config.certificate_threshold).score
            }),
        );
        let lyapunov = swarm_lyapunov(
            &fused_belief,
            &state.agents,
            &weights,
            current.risk,
            gap_trace,
            cooperation,
            current.privilege,
        );
        let acceleration = performance_acceleration(previous, current);
        let win_probability = benchmark_win_probability(current, baseline);
        let certificate = action
            .map(|token| certify_action(token, &state.agents, self.config.certificate_threshold));
        let certificate_score = certificate.as_ref().map_or(1.0, |cert| cert.score);
        let aegis_power = aegis_power(current, gap_trace, lyapunov);
        let objective = unified_objective(
            current,
            collective.margin,
            gap_trace,
            lyapunov,
            state.agents.len(),
        );
        let selected_role =
            select_spawn_role(role_candidates, &gap, &state.agents, state.spawn_threshold)
                .map(|candidate| candidate.role.clone());
        let selected_control_action =
            pareto_schedule(control_actions).map(|candidate| candidate.action.clone());
        let uncertainty = entropy(&fused_belief);
        let action_safe = certificate.as_ref().is_none_or(|cert| cert.passed);
        let decision = return_mode(
            win_probability,
            debate_margin,
            uncertainty,
            current.risk,
            self.config.risk_limit,
            self.config.win_probability_threshold,
            self.config.answer_margin_threshold,
            selected_role.is_some(),
            action_safe,
        );
        let reason = match decision {
            ControlDecision::Return => "win probability, answer margin, and certificate passed",
            ControlDecision::Verify => "fused belief entropy remains above the verification target",
            ControlDecision::Spawn => "capability-gap spawning has positive expected value",
            ControlDecision::Accelerate => "benchmark win probability remains below target",
            ControlDecision::DowngradeAutonomy => "risk is above the active autonomy boundary",
            ControlDecision::Refuse => "the proposed external action failed certification",
            ControlDecision::Ask => "additional task evidence is required",
        };

        Asc2ExecutionResult {
            decision,
            selected_answer,
            fused_belief,
            selected_role,
            selected_control_action,
            metrics: Asc2Metrics {
                aegis_power,
                capability_gap_trace: gap_trace,
                cooperation,
                debate_margin,
                collective_accuracy_margin: collective.margin,
                failure_bound: collective.failure_bound,
                lyapunov,
                unified_objective: objective,
                performance_acceleration: acceleration,
                benchmark_win_probability: win_probability,
                certificate_score,
                global_autonomy_budget: state.global_autonomy_budget,
            },
            certificate,
            rounds: state.round,
            return_reason: reason.into(),
        }
    }
}

impl Default for Asc2Controller {
    fn default() -> Self {
        Self::new(Asc2Config::default())
    }
}

#[must_use]
pub fn aegis_power(performance: &PerformanceSnapshot, gap: f64, instability: f64) -> f64 {
    let numerator = (1.0 + performance.benchmark_score.max(0.0))
        + (1.0 + performance.verified_intelligence.max(0.0))
        + (1.0 + performance.quality.max(0.0))
        + (1.0 + performance.cooperation.max(0.0));
    let denominator = (1.0 + performance.latency.max(0.0))
        + (1.0 + performance.compute_cost.max(0.0))
        + (1.0 + performance.risk.max(0.0))
        + (1.0 + performance.privilege.max(0.0));
    finite_nonnegative(numerator / denominator.max(DEFAULT_EPSILON) * (-gap - instability).exp())
}

#[must_use]
pub fn capability_gap_tensor(
    task_demand: &[f64],
    agents: &[ReflexiveAgentState],
    mu: f64,
) -> Vec<Vec<f64>> {
    let dim = task_demand.len().max(
        agents
            .iter()
            .map(|agent| agent.capability.len())
            .max()
            .unwrap_or(0),
    );
    if dim == 0 {
        return Vec::new();
    }
    let q = padded(task_demand, dim);
    let qq = outer(&q, &q);
    let mut coverage = identity(dim, mu.max(DEFAULT_EPSILON));
    for agent in agents {
        let capability = padded(&agent.capability, dim);
        let gate =
            clamp01(agent.trust) * (1.0 - clamp01(agent.uncertainty)) * (1.0 - clamp01(agent.risk));
        add_scaled_in_place(&mut coverage, &outer(&capability, &capability), gate);
    }
    let mut system = add_matrix(&coverage, &qq);
    add_scaled_in_place(&mut system, &identity(dim, 1.0), mu.max(DEFAULT_EPSILON));
    let inverse = invert_regularized(&system, mu.max(DEFAULT_EPSILON));
    let left = multiply(&multiply(&coverage, &inverse), &qq);
    let right = multiply(&multiply(&qq, &inverse), &coverage);
    positive_semidefinite_projection(&subtract_matrix(&subtract_matrix(&qq, &left), &right))
}

#[must_use]
pub fn spawn_score(candidate: &RoleCandidate, gap: &[Vec<f64>]) -> f64 {
    let role = padded(&candidate.role_embedding, gap.len());
    let capability = padded(&candidate.capability, gap.len());
    let need = bilinear(&capability, gap, &role).max(0.0);
    finite_nonnegative(
        (need + 0.35 * clamp01(candidate.novelty) + 0.45 * clamp01(candidate.expected_quality))
            / (1.0
                + 0.45 * clamp01(candidate.clone_score)
                + 0.8 * clamp01(candidate.risk)
                + 0.35 * candidate.budget_cost.max(0.0)),
    )
}

#[must_use]
pub fn adaptive_spawn_threshold(
    current: f64,
    gap_trace: f64,
    mean_risk: f64,
    agent_count: usize,
    mean_uncertainty: f64,
) -> f64 {
    finite_nonnegative(
        current.max(DEFAULT_EPSILON)
            * (-0.12 * gap_trace
                + 0.25 * clamp01(mean_risk)
                + 0.025 * agent_count as f64
                + 0.20 * clamp01(mean_uncertainty))
            .exp(),
    )
}

#[must_use]
pub fn orthogonal_role_score(candidate: &RoleCandidate, agents: &[ReflexiveAgentState]) -> f64 {
    let usefulness = cosine_similarity(&candidate.capability, &candidate.role_embedding).max(0.0);
    let redundancy = agents
        .iter()
        .map(|agent| cosine_similarity(&candidate.role_embedding, &agent.role_embedding).abs())
        .sum::<f64>();
    finite(usefulness - 0.3 * redundancy - 0.8 * candidate.risk - 0.5 * candidate.privilege)
}

#[must_use]
pub fn select_spawn_role<'a>(
    candidates: &'a [RoleCandidate],
    gap: &[Vec<f64>],
    agents: &[ReflexiveAgentState],
    threshold: f64,
) -> Option<&'a RoleCandidate> {
    candidates
        .iter()
        .filter_map(|candidate| {
            let score = spawn_score(candidate, gap) + orthogonal_role_score(candidate, agents);
            (score > threshold).then_some((candidate, score))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(candidate, _)| candidate)
}

#[must_use]
pub fn proof_packet(agent: &ReflexiveAgentState) -> ProofPacket {
    ProofPacket {
        sender_id: agent.id.clone(),
        claim: agent.claim.clone(),
        evidence_score: clamp01(agent.evidence_score),
        uncertainty: clamp01(agent.uncertainty),
        warning_score: clamp01(agent.warning_score),
        verification_target: agent.verification_target.clone(),
        proof_score: clamp01(agent.proof_score),
        limitation: agent.limitation.clone(),
        counterexample_request: agent.counterexample_request.clone(),
    }
}

#[must_use]
pub fn influence_weights(
    receiver: &ReflexiveAgentState,
    senders: &[ReflexiveAgentState],
) -> Vec<f64> {
    let logits = senders
        .iter()
        .map(|sender| {
            1.0 * sender.trust + 1.1 * sender.evidence_score + 1.0 * sender.proof_score
                - 0.8 * sender.uncertainty
                - 0.9 * sender.warning_score
                - 0.7 * cosine_similarity(&sender.role_embedding, &receiver.role_embedding).abs()
        })
        .collect::<Vec<_>>();
    stable_softmax(&logits)
}

#[must_use]
pub fn risk_twisted_mirror_flow(
    agent: &ReflexiveAgentState,
    peers: &[ReflexiveAgentState],
    evidence_scores: &[f64],
    answer_risks: &[f64],
    hallucination_scores: &[f64],
) -> Vec<f64> {
    let influences = influence_weights(agent, peers);
    let divergence = peers
        .iter()
        .zip(&influences)
        .map(|(peer, weight)| weight * jensen_shannon(&agent.belief, &peer.belief))
        .sum::<f64>();
    let learning_rate = (agent.trust * (1.0 - agent.uncertainty) * (1.0 - agent.risk)
        / (1.0 + divergence))
        .clamp(0.0, 1.0);
    let choices = agent.belief.len().max(
        peers
            .iter()
            .map(|peer| peer.belief.len())
            .max()
            .unwrap_or(0),
    );
    let mut logits = Vec::with_capacity(choices);
    for answer in 0..choices {
        let own = probability_at(&agent.belief, answer).ln();
        let peer_flow = peers
            .iter()
            .zip(&influences)
            .map(|(peer, weight)| weight * probability_at(&peer.belief, answer).ln())
            .sum::<f64>();
        logits.push(
            (1.0 - learning_rate) * own
                + learning_rate * peer_flow
                + 0.6 * value_at(evidence_scores, answer)
                - 0.8 * value_at(answer_risks, answer)
                - 0.8 * value_at(hallucination_scores, answer),
        );
    }
    stable_softmax(&logits)
}

#[must_use]
pub fn cooperation_functional(agents: &[ReflexiveAgentState], weights: &[f64]) -> f64 {
    if agents.is_empty() {
        return 0.0;
    }
    let choices = agents
        .iter()
        .map(|agent| agent.belief.len())
        .max()
        .unwrap_or(0);
    let mut average = vec![0.0; choices];
    for (index, agent) in agents.iter().enumerate() {
        for (answer, target) in average.iter_mut().enumerate() {
            *target += value_at(weights, index) * probability_at(&agent.belief, answer);
        }
    }
    normalize_distribution(&mut average);
    let harmony = (-agents
        .iter()
        .enumerate()
        .map(|(index, agent)| value_at(weights, index) * kl_divergence(&agent.belief, &average))
        .sum::<f64>())
    .exp();
    let mut similarity_sum = 0.0;
    let mut dissonance = 0.0;
    let mut pairs = 0usize;
    for i in 0..agents.len() {
        for j in (i + 1)..agents.len() {
            let similarity =
                (-squared_distance(&agents[i].role_embedding, &agents[j].role_embedding)).exp();
            similarity_sum += similarity;
            dissonance += value_at(weights, i)
                * value_at(weights, j)
                * jensen_shannon(&agents[i].belief, &agents[j].belief)
                * (1.0 - similarity)
                * (-(agents[i].uncertainty - agents[j].uncertainty).abs()).exp();
            pairs += 1;
        }
    }
    let role_spread = if pairs == 0 {
        1.0
    } else {
        (1.0 - similarity_sum / pairs as f64).clamp(0.0, 1.0)
    };
    finite_nonnegative(harmony * role_spread.max(0.1) * (1.0 + dissonance))
}

#[must_use]
pub fn debate_hardened_fusion(
    agents: &[ReflexiveAgentState],
    weights: &[f64],
    answer_risk: f64,
    answer_uncertainty: f64,
) -> Vec<f64> {
    let choices = agents
        .iter()
        .map(|agent| agent.belief.len())
        .max()
        .unwrap_or(0);
    if choices == 0 {
        return Vec::new();
    }
    let mut energies = vec![0.0; choices];
    for answer in 0..choices {
        for (index, agent) in agents.iter().enumerate() {
            energies[answer] +=
                value_at(weights, index) * probability_at(&agent.belief, answer).ln();
        }
        for i in 0..agents.len() {
            for j in (i + 1)..agents.len() {
                let zi = probability_at(&agents[i].belief, answer).ln();
                let zj = probability_at(&agents[j].belief, answer).ln();
                let contradiction = (agents[i].confidence - agents[j].confidence).abs();
                let clone =
                    cosine_similarity(&agents[i].role_embedding, &agents[j].role_embedding).abs();
                let sigma =
                    (agents[i].evidence_score * agents[j].evidence_score - contradiction - clone)
                        .tanh();
                if sigma >= 0.0 {
                    energies[answer] += 0.35 * sigma * zi.abs().min(zj.abs());
                } else {
                    energies[answer] -= 0.35 * -sigma * (zi - zj).powi(2);
                }
            }
        }
        let max_attack = agents
            .iter()
            .map(|agent| agent.warning_score * (1.0 - probability_at(&agent.belief, answer)))
            .fold(0.0_f64, f64::max);
        energies[answer] -= 0.6 * max_attack + 0.8 * answer_risk + 0.5 * answer_uncertainty;
    }
    stable_softmax(&energies)
}

#[must_use]
pub fn evidence_trust_weights(agents: &[ReflexiveAgentState]) -> Vec<f64> {
    stable_softmax(
        &agents
            .iter()
            .map(|agent| {
                1.0 * agent.trust
                    + 1.1 * agent.evidence_score
                    + 0.8 * agent.relevance
                    + 1.0 * agent.proof_score
                    - 0.9 * agent.uncertainty
                    - 1.0 * agent.risk
                    - 0.7 * agent.clone_score
            })
            .collect::<Vec<_>>(),
    )
}

pub fn update_trust_and_uncertainty(
    agent: &mut ReflexiveAgentState,
    verification_score: f64,
    cohort_contradiction: f64,
    unanswered: f64,
) {
    let logit = safe_logit(agent.trust) + 0.8 * (clamp01(verification_score) - 0.5)
        - 0.6 * (agent.confidence - (1.0 - agent.uncertainty)).max(0.0)
        - 0.6 * clamp01(cohort_contradiction)
        - 0.8 * agent.risk
        - 0.5 * (agent.confidence - agent.accuracy).abs();
    agent.trust = sigmoid(logit);
    agent.uncertainty = clamp01(
        agent.uncertainty + 0.5 * (agent.confidence - agent.accuracy).abs()
            - 0.45 * agent.evidence_score
            + 0.4 * clamp01(unanswered),
    );
}

#[must_use]
pub fn certify_action(
    token: &ActionToken,
    agents: &[ReflexiveAgentState],
    threshold: f64,
) -> ActionCertificate {
    let safety_agents = agents
        .iter()
        .filter(|agent| {
            let role = agent.role.to_ascii_lowercase();
            role.contains("safety") || role.contains("verifier") || role.contains("certifier")
        })
        .collect::<Vec<_>>();
    let score = safety_agents
        .iter()
        .map(|agent| {
            agent.trust * agent.proof_score + agent.trust * agent.evidence_score - agent.risk
        })
        .reduce(f64::min)
        .unwrap_or(0.0)
        - 0.015
            * (token.preconditions.len()
                + token.invariants.len()
                + token.postconditions.len()
                + token.rollback.len()
                + token.scope.len()) as f64
        - 0.5 * token.requested_privilege;
    let least_privilege = token.requested_privilege <= 0.35 && !token.scope.is_empty();
    let rollback_present = !token.rollback.is_empty();
    let audit_present = !token.audit.is_null();
    let mut reasons = Vec::new();
    if safety_agents.is_empty() {
        reasons.push("no safety verifier is present".into());
    }
    if !least_privilege {
        reasons.push("least-privilege scope failed".into());
    }
    if !rollback_present {
        reasons.push("rollback plan is missing".into());
    }
    if !audit_present {
        reasons.push("audit payload is missing".into());
    }
    if score < threshold {
        reasons.push("certificate score is below threshold".into());
    }
    let passed = (!token.side_effecting || (least_privilege && rollback_present && audit_present))
        && !safety_agents.is_empty()
        && score >= threshold
        && token.estimated_risk <= 0.2;
    ActionCertificate {
        score: finite(score),
        passed,
        least_privilege,
        rollback_present,
        audit_present,
        safety_verifiers: safety_agents.len(),
        reasons,
    }
}

#[must_use]
pub fn safety_projection<'a>(
    candidates: &'a [ActionToken],
    agents: &[ReflexiveAgentState],
    threshold: f64,
) -> Option<&'a ActionToken> {
    candidates
        .iter()
        .filter(|candidate| certify_action(candidate, agents, threshold).passed)
        .min_by(|left, right| {
            action_projection_cost(left)
                .total_cmp(&action_projection_cost(right))
                .then_with(|| left.action.cmp(&right.action))
        })
}

#[must_use]
pub fn update_autonomy_budgets(
    state: &mut SwarmState,
    verified_intelligence: f64,
    cooperation: f64,
    verification: f64,
    action_risk: f64,
    privilege: f64,
    gap: f64,
) {
    let mean_uncertainty = mean(
        &state
            .agents
            .iter()
            .map(|agent| agent.uncertainty)
            .collect::<Vec<_>>(),
    );
    state.global_autonomy_budget = clamp01(
        state.global_autonomy_budget
            + 0.15 * verified_intelligence
            + 0.12 * cooperation
            + 0.12 * verification
            - 0.25 * action_risk
            - 0.15 * mean_uncertainty
            - 0.2 * privilege
            - 0.12 * gap,
    );
    let denominator = state
        .agents
        .iter()
        .map(|agent| agent.trust * (1.0 - agent.uncertainty) * (1.0 - agent.risk))
        .sum::<f64>()
        .max(DEFAULT_EPSILON);
    for agent in &mut state.agents {
        agent.autonomy_budget = clamp01(
            state.global_autonomy_budget
                * agent.trust
                * (1.0 - agent.uncertainty)
                * (1.0 - agent.risk)
                / denominator,
        );
    }
}

#[must_use]
pub fn bounded_self_improvement(
    proposal: &SelfModificationProposal,
    config: &Asc2Config,
) -> SelfModificationDecision {
    let objective = proposal.validation_accuracy_delta + 0.3 * proposal.cooperation_delta
        - 1.2 * proposal.estimated_risk
        - 0.5 * proposal.policy_kl
        - 0.5 * proposal.capability_gap_delta.max(0.0);
    let mut reasons = Vec::new();
    if proposal.estimated_risk > config.self_modify_risk_limit {
        reasons.push("candidate risk exceeds the self-modification limit".into());
    }
    if proposal.certificate_score < config.certificate_threshold {
        reasons.push("candidate certificate is below threshold".into());
    }
    if proposal.candidate_validation_loss
        > proposal.current_validation_loss + config.validation_loss_tolerance
    {
        reasons.push("candidate validation loss regressed".into());
    }
    if objective <= 0.0 {
        reasons.push("candidate objective is not positive".into());
    }
    let accepted = reasons.is_empty();
    let step_size = if accepted {
        config.self_modify_risk_limit
            / (config.self_modify_risk_limit + proposal.estimated_risk + proposal.policy_kl)
    } else {
        0.0
    };
    SelfModificationDecision {
        accepted,
        objective: finite(objective),
        step_size: clamp01(step_size),
        reasons,
    }
}

#[must_use]
pub fn memory_utility(candidate: &MemoryCandidate) -> f64 {
    finite(
        candidate.future_evidence
            - 0.9 * candidate.risk
            - 0.9 * candidate.privacy
            - 0.8 * candidate.ledger_conflict,
    )
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CollectiveAccuracy {
    pub margin: f64,
    pub failure_bound: f64,
}

#[must_use]
pub fn collective_accuracy_bound(
    agents: &[ReflexiveAgentState],
    weights: &[f64],
    covariance: &[Vec<f64>],
    action_risk: f64,
    certified_risk: f64,
    certificate_threshold: f64,
    certificate_score: f64,
) -> CollectiveAccuracy {
    let mean_margin = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| value_at(weights, index) * (2.0 * agent.accuracy - 1.0))
        .sum::<f64>();
    let covariance_penalty = bilinear(weights, covariance, weights).max(0.0).sqrt();
    let margin = mean_margin - 0.7 * covariance_penalty - action_risk;
    let certificate_failure = if certificate_score < certificate_threshold {
        1.0
    } else {
        0.0
    };
    let failure_bound = (-(margin.max(0.0).powi(2))
        / (2.0 * bilinear(weights, covariance, weights).max(0.0) + DEFAULT_EPSILON))
        .exp()
        + certified_risk
        + certificate_failure;
    CollectiveAccuracy {
        margin: finite(margin),
        failure_bound: finite_nonnegative(failure_bound).min(1.0),
    }
}

#[must_use]
pub fn swarm_lyapunov(
    fused: &[f64],
    agents: &[ReflexiveAgentState],
    weights: &[f64],
    risk: f64,
    gap: f64,
    cooperation: f64,
    privilege: f64,
) -> f64 {
    let mean_uncertainty = mean(
        &agents
            .iter()
            .map(|agent| agent.uncertainty)
            .collect::<Vec<_>>(),
    );
    let divergence = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| value_at(weights, index) * kl_divergence(&agent.belief, fused))
        .sum::<f64>();
    finite_nonnegative(
        entropy(fused) + 0.8 * mean_uncertainty + 1.2 * risk + 0.8 * gap + 0.5 * divergence
            - 0.6 * cooperation
            + 0.8 * privilege,
    )
}

#[must_use]
pub fn lyapunov_descent_target(
    current: f64,
    next: f64,
    entropy: f64,
    uncertainty: f64,
    risk: f64,
    gap: f64,
) -> bool {
    next - current <= -0.1 * (entropy + uncertainty + risk + gap) + 0.02
}

#[must_use]
pub fn unified_objective(
    performance: &PerformanceSnapshot,
    accuracy_margin: f64,
    gap: f64,
    lyapunov: f64,
    agent_count: usize,
) -> f64 {
    finite(
        1.2 * performance.benchmark_score
            + 1.2 * performance.quality
            + performance.verified_intelligence
            + performance.speed
            + performance.cooperation
            + 0.8 * accuracy_margin
            - 0.5 * performance.latency
            - 0.5 * performance.compute_cost
            - 0.5 * performance.tool_overhead
            - 1.2 * performance.risk
            - 0.8 * gap
            - 0.6 * lyapunov
            - 0.8 * performance.privilege
            - 0.03 * agent_count as f64,
    )
}

#[must_use]
pub fn performance_acceleration(
    previous: &PerformanceSnapshot,
    current: &PerformanceSnapshot,
) -> f64 {
    finite(
        1.2 * (current.benchmark_score - previous.benchmark_score)
            + 1.1 * (current.quality - previous.quality)
            + 0.7 * (current.verified_intelligence - previous.verified_intelligence)
            - 0.6 * (current.latency - previous.latency)
            - 0.6 * (current.compute_cost - previous.compute_cost)
            - 0.5 * (current.tool_overhead - previous.tool_overhead)
            - 0.4 * (current.correction_effort - previous.correction_effort),
    )
}

#[must_use]
pub fn performance_accelerator(
    actions: &[ControlActionEstimate],
) -> Option<&ControlActionEstimate> {
    actions.iter().max_by(|left, right| {
        performance_action_score(left)
            .total_cmp(&performance_action_score(right))
            .then_with(|| left.action.cmp(&right.action))
    })
}

#[must_use]
pub fn lyapunov_control(actions: &[ControlActionEstimate]) -> Option<&ControlActionEstimate> {
    actions.iter().min_by(|left, right| {
        expected_lyapunov_cost(left)
            .total_cmp(&expected_lyapunov_cost(right))
            .then_with(|| left.action.cmp(&right.action))
    })
}

#[must_use]
pub fn pareto_schedule(actions: &[ControlActionEstimate]) -> Option<&ControlActionEstimate> {
    actions.iter().max_by(|left, right| {
        pareto_score(left)
            .total_cmp(&pareto_score(right))
            .then_with(|| left.action.cmp(&right.action))
    })
}

#[must_use]
pub fn benchmark_win_probability(
    current: &PerformanceSnapshot,
    baseline: &PerformanceSnapshot,
) -> f64 {
    sigmoid(
        1.2 * (current.quality - baseline.quality)
            - 0.7 * (current.latency - baseline.latency)
            - 0.6 * (current.compute_cost - baseline.compute_cost)
            + 1.0 * (current.benchmark_score - baseline.benchmark_score)
            - 1.0 * (current.risk - baseline.risk),
    )
}

#[must_use]
pub fn return_mode(
    win_probability: f64,
    margin: f64,
    entropy_value: f64,
    risk: f64,
    risk_limit: f64,
    win_threshold: f64,
    margin_threshold: f64,
    can_spawn: bool,
    action_safe: bool,
) -> ControlDecision {
    if !action_safe {
        ControlDecision::Refuse
    } else if risk > risk_limit {
        ControlDecision::DowngradeAutonomy
    } else if win_probability >= win_threshold && margin >= margin_threshold {
        ControlDecision::Return
    } else if entropy_value > 0.65 {
        ControlDecision::Verify
    } else if can_spawn {
        ControlDecision::Spawn
    } else {
        ControlDecision::Accelerate
    }
}

#[must_use]
pub fn comparison_deltas(
    asc2: &PerformanceSnapshot,
    baseline: &PerformanceSnapshot,
) -> ComparisonDeltas {
    ComparisonDeltas {
        benchmark: asc2.benchmark_score - baseline.benchmark_score,
        quality: asc2.quality - baseline.quality,
        latency: baseline.latency - asc2.latency,
        compute_cost: baseline.compute_cost - asc2.compute_cost,
        tool_overhead: baseline.tool_overhead - asc2.tool_overhead,
        verified_intelligence: asc2.verified_intelligence - baseline.verified_intelligence,
        cooperation: asc2.cooperation - baseline.cooperation,
        safety: baseline.risk - asc2.risk,
    }
}

#[must_use]
pub fn conservative_execution(
    true_risk: f64,
    estimated_risk: f64,
    certificate: &ActionCertificate,
    risk_limit: f64,
) -> bool {
    true_risk <= estimated_risk
        && estimated_risk <= risk_limit
        && certificate.passed
        && certificate.least_privilege
        && certificate.rollback_present
        && certificate.audit_present
}

fn pareto_score(action: &ControlActionEstimate) -> f64 {
    finite(
        (action.score_delta + 0.6 * action.entropy_reduction + 0.7 * action.gap_reduction)
            / (1.0
                + 0.5 * action.latency.max(0.0)
                + 0.5 * action.compute.max(0.0)
                + 0.4 * action.tool_overhead.max(0.0)
                + 0.9 * action.risk.max(0.0)),
    )
}

fn performance_action_score(action: &ControlActionEstimate) -> f64 {
    finite(
        action.score_delta + 0.4 * action.entropy_reduction + 0.5 * action.gap_reduction
            - 0.8 * action.risk
            - 0.25 * action.latency
            - 0.2 * action.compute,
    )
}

fn expected_lyapunov_cost(action: &ControlActionEstimate) -> f64 {
    finite(
        action.risk + 0.3 * action.compute + 0.25 * action.latency + 0.2 * action.tool_overhead
            - 0.7 * action.entropy_reduction
            - 0.8 * action.gap_reduction,
    )
}

fn action_projection_cost(action: &ActionToken) -> f64 {
    action.estimated_risk
        + action.requested_privilege
        + 0.01
            * (action.preconditions.len()
                + action.invariants.len()
                + action.postconditions.len()
                + action.rollback.len()
                + action.scope.len()) as f64
}

fn error_covariance(agents: &[ReflexiveAgentState]) -> Vec<Vec<f64>> {
    let errors = agents
        .iter()
        .map(|agent| 1.0 - agent.accuracy)
        .collect::<Vec<_>>();
    let avg = mean(&errors);
    let centered = errors.iter().map(|error| error - avg).collect::<Vec<_>>();
    positive_semidefinite_projection(&outer(&centered, &centered))
}

fn stable_softmax(logits: &[f64]) -> Vec<f64> {
    if logits.is_empty() {
        return Vec::new();
    }
    let max = logits
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    let mut values = logits
        .iter()
        .map(|value| {
            if value.is_finite() {
                (value - max).exp()
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    normalize_distribution(&mut values);
    values
}

fn normalize_distribution(values: &mut [f64]) {
    if values.is_empty() {
        return;
    }
    for value in values.iter_mut() {
        *value = if value.is_finite() {
            value.max(DEFAULT_EPSILON)
        } else {
            DEFAULT_EPSILON
        };
    }
    let total = values.iter().sum::<f64>().max(DEFAULT_EPSILON);
    for value in values {
        *value /= total;
    }
}

fn entropy(values: &[f64]) -> f64 {
    -values
        .iter()
        .map(|value| probability(*value) * probability(*value).ln())
        .sum::<f64>()
}

fn kl_divergence(left: &[f64], right: &[f64]) -> f64 {
    let dim = left.len().max(right.len());
    (0..dim)
        .map(|index| {
            let p = probability_at(left, index);
            p * (p / probability_at(right, index)).ln()
        })
        .sum::<f64>()
        .max(0.0)
}

fn jensen_shannon(left: &[f64], right: &[f64]) -> f64 {
    let dim = left.len().max(right.len());
    let middle = (0..dim)
        .map(|index| (probability_at(left, index) + probability_at(right, index)) / 2.0)
        .collect::<Vec<_>>();
    0.5 * kl_divergence(left, &middle) + 0.5 * kl_divergence(right, &middle)
}

fn distribution_margin(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| right.total_cmp(left));
    value_at(&sorted, 0) - value_at(&sorted, 1)
}

fn safe_logit(value: f64) -> f64 {
    let bounded = value.clamp(DEFAULT_EPSILON, 1.0 - DEFAULT_EPSILON);
    (bounded / (1.0 - bounded)).ln()
}

fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn probability(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(DEFAULT_EPSILON, 1.0)
    } else {
        DEFAULT_EPSILON
    }
}

fn probability_at(values: &[f64], index: usize) -> f64 {
    probability(value_at(values, index))
}

fn value_at(values: &[f64], index: usize) -> f64 {
    values
        .get(index)
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

fn clamp01(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn finite_nonnegative(value: f64) -> f64 {
    finite(value).max(0.0)
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn argmax(values: &[f64]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .map(|(index, _)| index)
}

fn padded(values: &[f64], dim: usize) -> Vec<f64> {
    (0..dim).map(|index| value_at(values, index)).collect()
}

fn cosine_similarity(left: &[f64], right: &[f64]) -> f64 {
    let dim = left.len().max(right.len());
    let dot = (0..dim)
        .map(|index| value_at(left, index) * value_at(right, index))
        .sum::<f64>();
    let left_norm = (0..dim)
        .map(|index| value_at(left, index).powi(2))
        .sum::<f64>()
        .sqrt();
    let right_norm = (0..dim)
        .map(|index| value_at(right, index).powi(2))
        .sum::<f64>()
        .sqrt();
    finite(dot / (left_norm * right_norm + DEFAULT_EPSILON)).clamp(-1.0, 1.0)
}

fn squared_distance(left: &[f64], right: &[f64]) -> f64 {
    let dim = left.len().max(right.len());
    (0..dim)
        .map(|index| (value_at(left, index) - value_at(right, index)).powi(2))
        .sum()
}

fn outer(left: &[f64], right: &[f64]) -> Vec<Vec<f64>> {
    left.iter()
        .map(|a| right.iter().map(|b| a * b).collect())
        .collect()
}

fn identity(dim: usize, scale: f64) -> Vec<Vec<f64>> {
    (0..dim)
        .map(|row| {
            (0..dim)
                .map(|column| if row == column { scale } else { 0.0 })
                .collect()
        })
        .collect()
}

fn add_matrix(left: &[Vec<f64>], right: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let dim = left.len().max(right.len());
    (0..dim)
        .map(|row| {
            (0..dim)
                .map(|column| matrix_at(left, row, column) + matrix_at(right, row, column))
                .collect()
        })
        .collect()
}

fn subtract_matrix(left: &[Vec<f64>], right: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let dim = left.len().max(right.len());
    (0..dim)
        .map(|row| {
            (0..dim)
                .map(|column| matrix_at(left, row, column) - matrix_at(right, row, column))
                .collect()
        })
        .collect()
}

fn add_scaled_in_place(target: &mut [Vec<f64>], source: &[Vec<f64>], scale: f64) {
    for row in 0..target.len() {
        for column in 0..target[row].len() {
            target[row][column] += scale * matrix_at(source, row, column);
        }
    }
}

fn multiply(left: &[Vec<f64>], right: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let rows = left.len();
    let columns = right.first().map_or(0, Vec::len);
    let shared = left.first().map_or(0, Vec::len).max(right.len());
    (0..rows)
        .map(|row| {
            (0..columns)
                .map(|column| {
                    (0..shared)
                        .map(|index| matrix_at(left, row, index) * matrix_at(right, index, column))
                        .sum()
                })
                .collect()
        })
        .collect()
}

fn invert_regularized(matrix: &[Vec<f64>], regularization: f64) -> Vec<Vec<f64>> {
    let dim = matrix.len();
    if dim == 0 {
        return Vec::new();
    }
    let mut augmented = vec![vec![0.0; dim * 2]; dim];
    for row in 0..dim {
        for column in 0..dim {
            augmented[row][column] =
                matrix_at(matrix, row, column) + if row == column { regularization } else { 0.0 };
        }
        augmented[row][dim + row] = 1.0;
    }
    for pivot in 0..dim {
        let best = (pivot..dim)
            .max_by(|left, right| {
                augmented[*left][pivot]
                    .abs()
                    .total_cmp(&augmented[*right][pivot].abs())
            })
            .unwrap_or(pivot);
        augmented.swap(pivot, best);
        let pivot_value = augmented[pivot][pivot];
        if pivot_value.abs() < DEFAULT_EPSILON {
            return identity(dim, 1.0 / regularization.max(DEFAULT_EPSILON));
        }
        for column in 0..(dim * 2) {
            augmented[pivot][column] /= pivot_value;
        }
        for row in 0..dim {
            if row == pivot {
                continue;
            }
            let factor = augmented[row][pivot];
            for column in 0..(dim * 2) {
                augmented[row][column] -= factor * augmented[pivot][column];
            }
        }
    }
    (0..dim)
        .map(|row| {
            (0..dim)
                .map(|column| finite(augmented[row][dim + column]))
                .collect()
        })
        .collect()
}

fn positive_semidefinite_projection(matrix: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let dim = matrix.len();
    let mut projected = vec![vec![0.0; dim]; dim];
    for row in 0..dim {
        for column in 0..dim {
            projected[row][column] = if row == column {
                matrix_at(matrix, row, column).max(0.0)
            } else {
                ((matrix_at(matrix, row, column) + matrix_at(matrix, column, row)) / 2.0).max(0.0)
            };
        }
    }
    projected
}

fn bilinear(left: &[f64], matrix: &[Vec<f64>], right: &[f64]) -> f64 {
    let dim = matrix.len().max(left.len()).max(right.len());
    (0..dim)
        .flat_map(|row| {
            (0..dim).map(move |column| {
                value_at(left, row) * matrix_at(matrix, row, column) * value_at(right, column)
            })
        })
        .sum()
}

fn matrix_at(matrix: &[Vec<f64>], row: usize, column: usize) -> f64 {
    matrix
        .get(row)
        .and_then(|values| values.get(column))
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

fn positive_trace(matrix: &[Vec<f64>]) -> f64 {
    (0..matrix.len())
        .map(|index| matrix_at(matrix, index, index).max(0.0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, role: &str, belief: Vec<f64>, capability: Vec<f64>) -> ReflexiveAgentState {
        ReflexiveAgentState {
            id: id.into(),
            hidden_reasoning: String::new(),
            role: role.into(),
            role_embedding: capability.clone(),
            capability,
            belief,
            claim: "candidate claim".into(),
            evidence_score: 0.9,
            proof_score: 0.9,
            trust: 0.9,
            uncertainty: 0.1,
            risk: 0.05,
            autonomy_budget: 0.5,
            memory_refs: Vec::new(),
            warning_score: 0.05,
            verification_target: None,
            limitation: "bounded evidence".into(),
            counterexample_request: None,
            privilege: 0.1,
            clone_score: 0.1,
            relevance: 0.9,
            accuracy: 0.9,
            confidence: 0.88,
        }
        .normalized()
    }

    #[test]
    fn probability_operators_are_normalized_and_finite() {
        let agents = vec![
            agent("solver", "solver", vec![0.8, 0.2], vec![1.0, 0.0]),
            agent(
                "verifier",
                "safety verifier",
                vec![0.7, 0.3],
                vec![0.0, 1.0],
            ),
        ];
        let weights = evidence_trust_weights(&agents);
        let fusion = debate_hardened_fusion(&agents, &weights, 0.05, 0.1);
        assert!((weights.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert!((fusion.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert!(fusion.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn capability_gap_shrinks_with_trusted_coverage() {
        let q = vec![1.0, 0.0];
        let empty = capability_gap_tensor(&q, &[], 1e-3);
        let covered = capability_gap_tensor(
            &q,
            &[agent("solver", "solver", vec![1.0], vec![1.0, 0.0])],
            1e-3,
        );
        assert!(positive_trace(&covered) < positive_trace(&empty));
    }

    #[test]
    fn clone_penalty_reduces_spawn_score() {
        let gap = identity(2, 1.0);
        let mut candidate = RoleCandidate {
            role: "new".into(),
            role_embedding: vec![1.0, 0.0],
            capability: vec![1.0, 0.0],
            novelty: 0.9,
            expected_quality: 0.9,
            clone_score: 0.0,
            risk: 0.1,
            budget_cost: 0.1,
            privilege: 0.1,
        };
        let original = spawn_score(&candidate, &gap);
        candidate.clone_score = 1.0;
        assert!(spawn_score(&candidate, &gap) < original);
    }

    #[test]
    fn trust_rewards_verification_and_penalizes_error() {
        let mut good = agent("good", "solver", vec![0.8, 0.2], vec![1.0]);
        let mut bad = good.clone();
        update_trust_and_uncertainty(&mut good, 1.0, 0.0, 0.0);
        bad.confidence = 1.0;
        bad.accuracy = 0.0;
        update_trust_and_uncertainty(&mut bad, 0.0, 1.0, 1.0);
        assert!(good.trust > bad.trust);
        assert!(good.uncertainty < bad.uncertainty);
    }

    #[test]
    fn side_effecting_action_requires_full_certificate() {
        let agents = vec![agent("verifier", "safety verifier", vec![1.0], vec![1.0])];
        let mut token = ActionToken {
            action: "write file".into(),
            preconditions: vec!["authorized".into()],
            invariants: vec!["preserve user data".into()],
            postconditions: vec!["verified".into()],
            rollback: Vec::new(),
            audit: serde_json::json!({"trace":"x"}),
            scope: vec!["workspace".into()],
            requested_privilege: 0.1,
            estimated_risk: 0.05,
            side_effecting: true,
        };
        assert!(!certify_action(&token, &agents, 0.8).passed);
        token.rollback.push("restore backup".into());
        assert!(certify_action(&token, &agents, 0.8).passed);
    }

    #[test]
    fn bounded_self_improvement_rejects_regression() {
        let decision = bounded_self_improvement(
            &SelfModificationProposal {
                id: "candidate".into(),
                validation_accuracy_delta: 0.2,
                cooperation_delta: 0.1,
                estimated_risk: 0.05,
                policy_kl: 0.02,
                capability_gap_delta: -0.1,
                certificate_score: 0.9,
                current_validation_loss: 0.1,
                candidate_validation_loss: 0.2,
            },
            &Asc2Config::default(),
        );
        assert!(!decision.accepted);
    }

    #[test]
    fn controller_returns_only_after_thresholds() {
        let agents = vec![
            agent("solver", "solver", vec![0.95, 0.05], vec![1.0, 0.0]),
            agent(
                "verifier",
                "safety verifier",
                vec![0.95, 0.05],
                vec![0.0, 1.0],
            ),
        ];
        let current = PerformanceSnapshot {
            benchmark_score: 0.95,
            quality: 0.95,
            verified_intelligence: 0.9,
            speed: 0.8,
            cooperation: 0.8,
            ..PerformanceSnapshot::default()
        };
        let baseline = PerformanceSnapshot {
            benchmark_score: 0.2,
            quality: 0.2,
            latency: 0.2,
            compute_cost: 0.2,
            risk: 0.1,
            ..PerformanceSnapshot::default()
        };
        let result = Asc2Controller::default().evaluate(
            &SwarmState {
                task: "solve".into(),
                task_demand: vec![1.0, 1.0],
                agents,
                controller_policy: Vec::new(),
                action_tokens: Vec::new(),
                safety_ledger: Vec::new(),
                spawn_threshold: 100.0,
                global_autonomy_budget: 0.5,
                round: 1,
            },
            &[],
            &[],
            None,
            &current,
            &PerformanceSnapshot::default(),
            &baseline,
        );
        assert_eq!(result.decision, ControlDecision::Return);
    }

    #[test]
    fn schedulers_prefer_high_value_low_risk_controls() {
        let actions = vec![
            ControlActionEstimate {
                action: "expensive".into(),
                score_delta: 0.5,
                entropy_reduction: 0.1,
                gap_reduction: 0.1,
                latency: 2.0,
                compute: 2.0,
                tool_overhead: 1.0,
                risk: 0.5,
            },
            ControlActionEstimate {
                action: "efficient".into(),
                score_delta: 0.4,
                entropy_reduction: 0.4,
                gap_reduction: 0.4,
                latency: 0.1,
                compute: 0.1,
                tool_overhead: 0.1,
                risk: 0.02,
            },
        ];
        assert_eq!(
            performance_accelerator(&actions).map(|action| action.action.as_str()),
            Some("efficient")
        );
        assert_eq!(
            lyapunov_control(&actions).map(|action| action.action.as_str()),
            Some("efficient")
        );
        assert_eq!(
            pareto_schedule(&actions).map(|action| action.action.as_str()),
            Some("efficient")
        );
    }
}

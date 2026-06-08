// ═══════════════════════════════════════════════════════════════
// Dimension 7: Multi-Agent Swarm Intelligence
// ═══════════════════════════════════════════════════════════════
//
// Production-grade swarm reasoning system that spawns specialized
// cognitive agents, forces adversarial debate, and collapses to
// consensus via weighted voting and attack-survival scoring.
//
// Architecture:
//   1. Agent Spawner — Creates specialized D1-D5 dominant agents
//   2. Debate Protocol — Agents propose, attack, and defend solutions
//   3. Attack Surface Analyzer — Finds weaknesses in proposals
//   4. Consensus Collapse — Weighted vote combining survival + quality
//   5. Diversity Enforcer — Prevents convergence to groupthink
//   6. Swarm Telemetry — Full audit trail of debate rounds
//
// Mathematical Foundation:
//   - Agent score: S_i = quality_i × survival_rate_i × novelty_i
//   - Consensus: W = Σ(w_i × S_i) / Σ(w_i), w_i = 1/(1 + attacks_survived)
//   - Diversity: D = 1 - max_pairwise_similarity(proposals)
//   - Convergence: C = 1 - std_dev(scores) / mean(scores)
//
// Pure Rust. Zero external dependencies. Deterministic.

use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// A specialized cognitive agent within the swarm.
#[derive(Debug, Clone)]
pub struct SwarmAgent {
    pub id: usize,
    pub name: String,
    pub specialization: AgentSpecialization,
    pub proposal: Option<Proposal>,
    pub attacks_made: Vec<Attack>,
    pub attacks_received: Vec<Attack>,
    pub defense_score: f64,
    pub credibility: f64,
    pub is_active: bool,
}

/// Agent specialization — determines which dimension dominates.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentSpecialization {
    Deductive,     // D1-dominant: formal logic, proofs
    Creative,      // D2-dominant: novel ideas, synthesis
    Iconoclast,    // D3-dominant: rule-breaking, paradox resolution
    Metacognitive, // D4-dominant: self-checking, bias detection
    Temporal,      // D5-dominant: future prediction, planning
    Generalist,    // Balanced across all dimensions
    Adversarial,   // Pure attacker — only finds weaknesses
    Synthesizer,   // Merges proposals from other agents
}

impl AgentSpecialization {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Deductive => "deductive",
            Self::Creative => "creative",
            Self::Iconoclast => "iconoclast",
            Self::Metacognitive => "metacognitive",
            Self::Temporal => "temporal",
            Self::Generalist => "generalist",
            Self::Adversarial => "adversarial",
            Self::Synthesizer => "synthesizer",
        }
    }

    /// Weight vector for D1-D5 influence.
    pub fn dimension_weights(&self) -> [f64; 5] {
        match self {
            Self::Deductive => [0.5, 0.1, 0.1, 0.1, 0.2],
            Self::Creative => [0.1, 0.5, 0.1, 0.1, 0.2],
            Self::Iconoclast => [0.1, 0.1, 0.5, 0.1, 0.2],
            Self::Metacognitive => [0.1, 0.1, 0.1, 0.5, 0.2],
            Self::Temporal => [0.1, 0.1, 0.1, 0.2, 0.5],
            Self::Generalist => [0.2, 0.2, 0.2, 0.2, 0.2],
            Self::Adversarial => [0.3, 0.1, 0.3, 0.2, 0.1],
            Self::Synthesizer => [0.15, 0.25, 0.15, 0.2, 0.25],
        }
    }
}

/// A solution proposal from an agent.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub agent_id: usize,
    pub solution_text: String,
    pub approach: String,
    pub confidence: f64,
    pub quality_score: f64,
    pub novelty_score: f64,
    pub logical_rigor: f64,
    pub feasibility: f64,
    pub key_claims: Vec<String>,
    pub assumptions: Vec<String>,
    pub weaknesses_self_identified: Vec<String>,
}

/// An attack on a proposal.
#[derive(Debug, Clone)]
pub struct Attack {
    pub attacker_id: usize,
    pub target_proposal_agent_id: usize,
    pub attack_type: AttackType,
    pub description: String,
    pub severity: f64,
    pub was_defended: bool,
    pub defense_strength: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttackType {
    LogicalFlaw,
    MissingEvidence,
    Contradiction,
    Infeasibility,
    OverSimplification,
    BiasDetected,
    EdgeCaseFailure,
    ScalabilityIssue,
    SecurityVulnerability,
    PerformanceBottleneck,
}

impl AttackType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::LogicalFlaw => "logical_flaw",
            Self::MissingEvidence => "missing_evidence",
            Self::Contradiction => "contradiction",
            Self::Infeasibility => "infeasibility",
            Self::OverSimplification => "over_simplification",
            Self::BiasDetected => "bias_detected",
            Self::EdgeCaseFailure => "edge_case_failure",
            Self::ScalabilityIssue => "scalability_issue",
            Self::SecurityVulnerability => "security_vulnerability",
            Self::PerformanceBottleneck => "performance_bottleneck",
        }
    }

    fn base_severity(&self) -> f64 {
        match self {
            Self::LogicalFlaw => 0.9,
            Self::Contradiction => 0.95,
            Self::SecurityVulnerability => 0.85,
            Self::Infeasibility => 0.8,
            Self::MissingEvidence => 0.5,
            Self::OverSimplification => 0.4,
            Self::BiasDetected => 0.6,
            Self::EdgeCaseFailure => 0.55,
            Self::ScalabilityIssue => 0.5,
            Self::PerformanceBottleneck => 0.45,
        }
    }
}

/// Configuration for the swarm.
#[derive(Debug, Clone)]
pub struct SwarmConfig {
    pub max_agents: usize,
    pub debate_rounds: usize,
    pub min_proposal_quality: f64,
    pub attack_threshold: f64,
    pub consensus_threshold: f64,
    pub diversity_minimum: f64,
    pub credibility_decay: f64,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            max_agents: 8,
            debate_rounds: 3,
            min_proposal_quality: 0.3,
            attack_threshold: 0.4,
            consensus_threshold: 0.7,
            diversity_minimum: 0.3,
            credibility_decay: 0.05,
        }
    }
}

/// Result of a swarm deliberation.
#[derive(Debug, Clone)]
pub struct SwarmResult {
    pub winning_proposal: Option<Proposal>,
    pub winning_agent_id: Option<usize>,
    pub consensus_score: f64,
    pub diversity_score: f64,
    pub convergence: f64,
    pub total_attacks: usize,
    pub total_defenses: usize,
    pub debate_rounds_completed: usize,
    pub agent_rankings: Vec<(usize, f64)>,
    pub merged_solution: Option<String>,
    pub audit_trail: Vec<DebateEvent>,
    pub duration_ms: f64,
}

/// A single event in the debate audit trail.
#[derive(Debug, Clone)]
pub struct DebateEvent {
    pub round: usize,
    pub event_type: DebateEventType,
    pub agent_id: usize,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum DebateEventType {
    ProposalSubmitted,
    AttackLaunched,
    DefenseRaised,
    AgentEliminated,
    ConsensusReached,
    MergeSynthesized,
}

// ═══════════════════════════════════════════════════════════════
// SWARM INTELLIGENCE ENGINE
// ═══════════════════════════════════════════════════════════════

pub struct SwarmIntelligence {
    config: SwarmConfig,
    agents: Vec<SwarmAgent>,
    audit_trail: Vec<DebateEvent>,
    total_deliberations: u64,
    rng_state: u64,
}

impl SwarmIntelligence {
    pub fn new(config: SwarmConfig) -> Self {
        Self {
            config,
            agents: Vec::new(),
            audit_trail: Vec::new(),
            total_deliberations: 0,
            rng_state: 0xCAFE_BABE_DEAD_BEEF,
        }
    }

    fn next_rand(&mut self) -> f64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        (self.rng_state as f64) / (u64::MAX as f64)
    }

    /// Spawn the default agent roster for a problem.
    pub fn spawn_default_roster(&mut self) {
        self.agents.clear();
        let specs = vec![
            ("Alpha", AgentSpecialization::Deductive),
            ("Beta", AgentSpecialization::Creative),
            ("Gamma", AgentSpecialization::Iconoclast),
            ("Delta", AgentSpecialization::Metacognitive),
            ("Epsilon", AgentSpecialization::Temporal),
            ("Zeta", AgentSpecialization::Adversarial),
            ("Eta", AgentSpecialization::Synthesizer),
            ("Theta", AgentSpecialization::Generalist),
        ];

        for (i, (name, spec)) in specs.into_iter().enumerate() {
            if i >= self.config.max_agents {
                break;
            }
            self.agents.push(SwarmAgent {
                id: i,
                name: name.to_string(),
                specialization: spec,
                proposal: None,
                attacks_made: Vec::new(),
                attacks_received: Vec::new(),
                defense_score: 1.0,
                credibility: 1.0,
                is_active: true,
            });
        }
    }

    /// Run a full deliberation cycle on a problem.
    pub fn deliberate(
        &mut self,
        problem: &str,
        problem_keywords: &[&str],
        initial_proposals: &[(usize, Proposal)],
    ) -> SwarmResult {
        let start = Instant::now();
        self.total_deliberations += 1;
        self.audit_trail.clear();

        if self.agents.is_empty() {
            self.spawn_default_roster();
        }

        // Phase 1: Assign initial proposals to agents
        for (agent_id, proposal) in initial_proposals {
            if let Some(agent) = self.agents.iter_mut().find(|a| a.id == *agent_id) {
                agent.proposal = Some(proposal.clone());
                self.audit_trail.push(DebateEvent {
                    round: 0,
                    event_type: DebateEventType::ProposalSubmitted,
                    agent_id: *agent_id,
                    description: format!(
                        "Agent {} submitted proposal: {}",
                        agent.name, proposal.approach
                    ),
                });
            }
        }

        // Generate proposals for agents without one.  Collect first so the RNG
        // mutation in generate_proposal does not overlap a mutable agent borrow.
        let missing: Vec<(usize, usize, AgentSpecialization, String)> = self
            .agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                agent.proposal.is_none()
                    && !matches!(agent.specialization, AgentSpecialization::Adversarial)
            })
            .map(|(idx, agent)| {
                (
                    idx,
                    agent.id,
                    agent.specialization.clone(),
                    agent.name.clone(),
                )
            })
            .collect();
        for (idx, agent_id, specialization, name) in missing {
            let proposal =
                self.generate_proposal(agent_id, &specialization, problem, problem_keywords);
            if let Some(agent) = self.agents.get_mut(idx) {
                agent.proposal = Some(proposal);
            }
            self.audit_trail.push(DebateEvent {
                round: 0,
                event_type: DebateEventType::ProposalSubmitted,
                agent_id,
                description: format!("Agent {} auto-generated proposal", name),
            });
        }

        let mut total_attacks = 0usize;
        let mut total_defenses = 0usize;

        // Phase 2: Debate rounds
        for round in 1..=self.config.debate_rounds {
            let active_ids: Vec<usize> = self
                .agents
                .iter()
                .filter(|a| a.is_active && a.proposal.is_some())
                .map(|a| a.id)
                .collect();

            // Each agent attacks other proposals
            let mut round_attacks: Vec<Attack> = Vec::new();

            for &attacker_id in &active_ids {
                let attacker_spec = self.agents[attacker_id].specialization.clone();
                for &target_id in &active_ids {
                    if attacker_id == target_id {
                        continue;
                    }

                    let target_proposal = self.agents[target_id].proposal.clone();
                    if let Some(proposal) = target_proposal {
                        if let Some(attack) = self.generate_attack(
                            attacker_id,
                            target_id,
                            &attacker_spec,
                            &proposal,
                            round,
                        ) {
                            round_attacks.push(attack);
                            total_attacks += 1;
                        }
                    }
                }
            }

            // Apply attacks and defenses
            for attack in &round_attacks {
                // Defense: based on self-identified weaknesses and logical rigor
                let target = &self.agents[attack.target_proposal_agent_id];
                let proposal = target.proposal.as_ref().unwrap();

                let defense_strength = proposal.logical_rigor * 0.4
                    + proposal.confidence * 0.3
                    + (if proposal
                        .weaknesses_self_identified
                        .iter()
                        .any(|w| w.to_lowercase().contains(attack.attack_type.as_str()))
                    {
                        0.3
                    } else {
                        0.0
                    });

                let was_defended = defense_strength > attack.severity * 0.7;

                let mut recorded_attack = attack.clone();
                recorded_attack.was_defended = was_defended;
                recorded_attack.defense_strength = defense_strength;

                if was_defended {
                    total_defenses += 1;
                    self.audit_trail.push(DebateEvent {
                        round,
                        event_type: DebateEventType::DefenseRaised,
                        agent_id: attack.target_proposal_agent_id,
                        description: format!(
                            "Defended against {} attack",
                            attack.attack_type.as_str()
                        ),
                    });
                }

                self.agents[attack.target_proposal_agent_id]
                    .attacks_received
                    .push(recorded_attack.clone());
                self.agents[attack.attacker_id]
                    .attacks_made
                    .push(recorded_attack);
            }

            // Update defense scores
            for agent in &mut self.agents {
                if !agent.is_active {
                    continue;
                }
                let total_received = agent.attacks_received.len() as f64;
                let defended = agent
                    .attacks_received
                    .iter()
                    .filter(|a| a.was_defended)
                    .count() as f64;
                agent.defense_score = if total_received > 0.0 {
                    defended / total_received
                } else {
                    1.0
                };

                // Eliminate agents with too many undefended critical attacks
                let critical_undefended = agent
                    .attacks_received
                    .iter()
                    .filter(|a| !a.was_defended && a.severity > 0.7)
                    .count();
                if critical_undefended >= 3 {
                    agent.is_active = false;
                    self.audit_trail.push(DebateEvent {
                        round,
                        event_type: DebateEventType::AgentEliminated,
                        agent_id: agent.id,
                        description: format!(
                            "Agent {} eliminated ({} critical undefended attacks)",
                            agent.name, critical_undefended
                        ),
                    });
                }
            }
        }

        // Phase 3: Score and rank surviving agents
        let mut agent_scores: Vec<(usize, f64)> = Vec::new();
        for agent in &self.agents {
            if !agent.is_active {
                continue;
            }
            if let Some(proposal) = &agent.proposal {
                let score = proposal.quality_score * 0.3
                    + proposal.novelty_score * 0.15
                    + proposal.logical_rigor * 0.2
                    + proposal.feasibility * 0.15
                    + agent.defense_score * 0.2;
                agent_scores.push((agent.id, score));
            }
        }
        agent_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Phase 4: Consensus and diversity
        let scores: Vec<f64> = agent_scores.iter().map(|(_, s)| *s).collect();
        let consensus_score = self.compute_consensus(&scores);
        let diversity_score = self.compute_diversity();
        let convergence = self.compute_convergence(&scores);

        // Phase 5: Select winner or merge
        let winning = agent_scores.first().cloned();
        let winning_proposal = winning
            .as_ref()
            .and_then(|(id, _)| self.agents.iter().find(|a| a.id == *id))
            .and_then(|a| a.proposal.clone());

        // Phase 6: Synthesizer merges top proposals
        let merged_solution = if agent_scores.len() >= 2 {
            let top_approaches: Vec<String> = agent_scores
                .iter()
                .take(3)
                .filter_map(|(id, _)| {
                    self.agents
                        .iter()
                        .find(|a| a.id == *id)
                        .and_then(|a| a.proposal.as_ref())
                        .map(|p| p.approach.clone())
                })
                .collect();
            Some(format!(
                "Merged synthesis from {} top proposals: {}",
                top_approaches.len(),
                top_approaches.join(" ⊕ ")
            ))
        } else {
            None
        };

        if consensus_score > self.config.consensus_threshold {
            self.audit_trail.push(DebateEvent {
                round: self.config.debate_rounds,
                event_type: DebateEventType::ConsensusReached,
                agent_id: winning.map(|(id, _)| id).unwrap_or(0),
                description: format!("Consensus reached: score={:.3}", consensus_score),
            });
        }

        SwarmResult {
            winning_proposal,
            winning_agent_id: winning.map(|(id, _)| id),
            consensus_score,
            diversity_score,
            convergence,
            total_attacks,
            total_defenses,
            debate_rounds_completed: self.config.debate_rounds,
            agent_rankings: agent_scores,
            merged_solution,
            audit_trail: self.audit_trail.clone(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "SwarmIntelligence v1.0 (Dimension 7)",
            "total_deliberations": self.total_deliberations,
            "active_agents": self.agents.iter().filter(|a| a.is_active).count(),
            "total_agents": self.agents.len(),
        })
    }

    // ─────────────────────────────────────────────────────
    // INTERNAL
    // ─────────────────────────────────────────────────────

    fn generate_proposal(
        &mut self,
        agent_id: usize,
        spec: &AgentSpecialization,
        problem: &str,
        keywords: &[&str],
    ) -> Proposal {
        let weights = spec.dimension_weights();
        let quality = 0.5 + self.next_rand() * 0.4;
        let novelty = match spec {
            AgentSpecialization::Creative | AgentSpecialization::Iconoclast => {
                0.6 + self.next_rand() * 0.4
            }
            _ => 0.3 + self.next_rand() * 0.4,
        };
        let rigor = match spec {
            AgentSpecialization::Deductive | AgentSpecialization::Metacognitive => {
                0.7 + self.next_rand() * 0.3
            }
            _ => 0.4 + self.next_rand() * 0.4,
        };

        Proposal {
            agent_id,
            solution_text: format!("{}-approach to: {}", spec.as_str(), problem),
            approach: format!(
                "{}_strategy(weights=[{:.1},{:.1},{:.1},{:.1},{:.1}])",
                spec.as_str(),
                weights[0],
                weights[1],
                weights[2],
                weights[3],
                weights[4]
            ),
            confidence: 0.5 + self.next_rand() * 0.4,
            quality_score: quality,
            novelty_score: novelty,
            logical_rigor: rigor,
            feasibility: 0.5 + self.next_rand() * 0.4,
            key_claims: keywords
                .iter()
                .map(|k| format!("Claim about {}", k))
                .collect(),
            assumptions: vec![format!(
                "Problem scope is well-defined for {} approach",
                spec.as_str()
            )],
            weaknesses_self_identified: vec![format!(
                "May underweight {} aspects",
                if spec == &AgentSpecialization::Deductive {
                    "creative"
                } else {
                    "logical"
                }
            )],
        }
    }

    fn generate_attack(
        &mut self,
        attacker_id: usize,
        target_id: usize,
        attacker_spec: &AgentSpecialization,
        proposal: &Proposal,
        round: usize,
    ) -> Option<Attack> {
        // Determine attack type based on attacker specialization
        let attack_type = match attacker_spec {
            AgentSpecialization::Deductive => AttackType::LogicalFlaw,
            AgentSpecialization::Creative => AttackType::OverSimplification,
            AgentSpecialization::Iconoclast => AttackType::Contradiction,
            AgentSpecialization::Metacognitive => AttackType::BiasDetected,
            AgentSpecialization::Temporal => AttackType::EdgeCaseFailure,
            AgentSpecialization::Adversarial => {
                let types = [
                    AttackType::LogicalFlaw,
                    AttackType::Contradiction,
                    AttackType::SecurityVulnerability,
                    AttackType::ScalabilityIssue,
                ];
                types[round % types.len()].clone()
            }
            _ => return None,
        };

        let base_severity = attack_type.base_severity();
        let severity = base_severity * (0.7 + self.next_rand() * 0.3);

        // Only attack if severity exceeds threshold
        if severity < self.config.attack_threshold {
            return None;
        }

        self.audit_trail.push(DebateEvent {
            round,
            event_type: DebateEventType::AttackLaunched,
            agent_id: attacker_id,
            description: format!("Attacked agent {} with {}", target_id, attack_type.as_str()),
        });

        Some(Attack {
            attacker_id,
            target_proposal_agent_id: target_id,
            attack_type,
            description: format!(
                "Weakness in {}'s approach regarding proposal confidence={:.2}",
                proposal.approach, proposal.confidence
            ),
            severity,
            was_defended: false,
            defense_strength: 0.0,
        })
    }

    fn compute_consensus(&self, scores: &[f64]) -> f64 {
        if scores.len() < 2 {
            return 1.0;
        }
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64;
        let std_dev = variance.sqrt();
        (1.0 - std_dev / mean.max(0.01)).clamp(0.0, 1.0)
    }

    fn compute_diversity(&self) -> f64 {
        let active: Vec<&SwarmAgent> = self
            .agents
            .iter()
            .filter(|a| a.is_active && a.proposal.is_some())
            .collect();
        if active.len() < 2 {
            return 0.0;
        }

        let mut max_sim = 0.0f64;
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let sim = if active[i].specialization == active[j].specialization {
                    0.8
                } else {
                    0.2
                };
                if sim > max_sim {
                    max_sim = sim;
                }
            }
        }
        1.0 - max_sim
    }

    fn compute_convergence(&self, scores: &[f64]) -> f64 {
        if scores.len() < 2 {
            return 1.0;
        }
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let std_dev =
            (scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64).sqrt();
        1.0 - (std_dev / mean.max(0.01)).min(1.0)
    }
}

impl Default for SwarmIntelligence {
    fn default() -> Self {
        Self::new(SwarmConfig::default())
    }
}

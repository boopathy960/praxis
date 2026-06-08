// ─────────────────────────────────────────────────────────────
// Agent Controller — Orchestrates the entire agent system
// ─────────────────────────────────────────────────────────────
use super::{
    accountability::AccountabilityEngine,
    base::{
        AgentCapabilityProfile, AgentProposal, MissionAgentAssignment, MissionAssignmentPlan,
        MissionToolAssignment,
    },
    federated::FederatedLearningEngine,
    marketplace::ProblemMarketplace,
    mev_protection::MEVProtectionEngine,
    registry::AgentRegistry,
    solver::AgentSolver,
    verification::AgenticVerification,
};
use std::collections::{HashMap, HashSet};

pub struct AgentController {
    pub solver: AgentSolver,
    pub verifier: AgenticVerification,
    pub registry: AgentRegistry,
    pub accountability: AccountabilityEngine,
    pub mev: MEVProtectionEngine,
    pub marketplace: ProblemMarketplace,
    pub federated: FederatedLearningEngine,
    profiles: HashMap<String, AgentCapabilityProfile>,
    builtin_profile_ids: HashSet<String>,
    intent_count: u64,
}

impl AgentController {
    pub fn new() -> Self {
        let mut controller = Self {
            solver: AgentSolver::default(),
            verifier: AgenticVerification::default(),
            registry: AgentRegistry::new(),
            accountability: AccountabilityEngine::new(),
            mev: MEVProtectionEngine::new(),
            marketplace: ProblemMarketplace::new(0.02),
            federated: FederatedLearningEngine::new(),
            profiles: HashMap::new(),
            builtin_profile_ids: HashSet::new(),
            intent_count: 0,
        };
        controller.seed_builtin_agents();
        controller
    }

    /// Process an intent through the agent consensus system.
    pub fn process_intent(
        &mut self,
        intent: &str,
        proposals: &[AgentProposal],
    ) -> serde_json::Value {
        self.intent_count += 1;

        if proposals.is_empty() {
            return serde_json::json!({"status": "no_proposals", "intent": intent});
        }

        let divergences: HashMap<String, f64> = proposals
            .iter()
            .map(|p| {
                (
                    p.agent_id.clone(),
                    self.registry
                        .get(&p.agent_id)
                        .map(|a| a.current_divergence())
                        .unwrap_or(0.0),
                )
            })
            .collect();

        // Consensus
        match self.solver.aggregate(proposals, &divergences) {
            Ok((consensus, weights)) => {
                // Verification
                let (y_star, energy) = self
                    .verifier
                    .find_optimal(proposals, &weights, None, 100, 0.005, 1e-8);

                // Record stats
                for p in proposals {
                    if let Some(agent) = self.registry.get_mut(&p.agent_id) {
                        agent.record_proposal(p.confidence, p.resource_footprint);
                    }
                }

                serde_json::json!({
                    "status": "consensus_reached",
                    "intent": intent,
                    "consensus": consensus,
                    "verification_energy": energy,
                    "y_star": y_star,
                    "num_agents": proposals.len(),
                    "weights": weights,
                })
            }
            Err(e) => serde_json::json!({"status": "failed", "error": e.to_string()}),
        }
    }

    pub fn get_system_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "intents_processed": self.intent_count,
            "agents": {"total": self.registry.count(), "active": self.registry.active_count()},
            "mev": self.mev.get_stats(),
            "marketplace": self.marketplace.get_stats(),
            "federated": self.federated.get_stats(),
            "accountability": self.accountability.get_stats(),
        })
    }

    pub fn capability_catalog(&self) -> Vec<AgentCapabilityProfile> {
        let mut profiles = self.profiles.values().cloned().collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        profiles
    }

    pub fn register_custom_profile(&mut self, profile: AgentCapabilityProfile) {
        self.registry.register(
            &profile.agent_id,
            &profile.agent_type,
            profile.capabilities.clone(),
        );
        self.profiles.insert(profile.agent_id.clone(), profile);
    }

    pub fn remove_custom_profile(&mut self, agent_id: &str) -> Result<bool, String> {
        if self.builtin_profile_ids.contains(agent_id) {
            return Err("built-in agents cannot be removed".into());
        }

        let removed_profile = self.profiles.remove(agent_id).is_some();
        let removed_registry = self.registry.remove(agent_id);
        Ok(removed_profile || removed_registry)
    }

    pub fn plan_mission(&self, task: &str, current_url: Option<&str>) -> MissionAssignmentPlan {
        let normalized_task = task.trim();
        let lower = normalized_task.to_lowercase();
        let current_url = current_url.unwrap_or_default();

        let is_code = [
            "build",
            "code",
            "debug",
            "bug",
            "compile",
            "refactor",
            "test",
            "api",
            "production",
        ]
        .iter()
        .any(|token| lower.contains(token));
        let is_research = [
            "search",
            "research",
            "find",
            "compare",
            "summarize",
            "latest",
            "evidence",
            "verify",
        ]
        .iter()
        .any(|token| lower.contains(token));
        let is_navigation = current_url.starts_with("http://")
            || current_url.starts_with("https://")
            || ["open", "visit", "browse", "navigate"]
                .iter()
                .any(|token| lower.contains(token));
        let is_sensitive = [
            "payment", "wallet", "bank", "login", "account", "password", "security", "private",
        ]
        .iter()
        .any(|token| lower.contains(token));
        let is_automation = [
            "automate",
            "workflow",
            "agent",
            "schedule",
            "assign",
            "tool",
            "orchestrate",
        ]
        .iter()
        .any(|token| lower.contains(token));

        let mut agents = vec![
            self.agent_assignment(
                "acquisition-core",
                "Assistant-side web acquisition and execution orchestration.",
                1,
                0.92,
                vec![
                    mission_tool(
                        "web_fetch",
                        "Fetch target resources and preserve evidence",
                        true,
                        "safe",
                    ),
                    mission_tool(
                        "content_extract",
                        "Extract semantic content and structure",
                        true,
                        "safe",
                    ),
                ],
            ),
            self.agent_assignment(
                "safety-sentinel",
                "Enforce policy, permissions, and sensitive-action review.",
                2,
                if is_sensitive { 0.97 } else { 0.88 },
                vec![
                    mission_tool(
                        "risk_review",
                        "Gate sensitive actions before execution",
                        true,
                        "moderate",
                    ),
                    mission_tool(
                        "threat_scan",
                        "Inspect content and navigation risk",
                        is_sensitive,
                        "safe",
                    ),
                ],
            ),
        ];

        if is_research || !is_navigation {
            agents.push(self.agent_assignment(
                "research-oracle",
                "Gather sources, compare evidence, and build answer coverage.",
                3,
                0.91,
                vec![
                    mission_tool("web_search", "Collect broad source coverage", true, "safe"),
                    mission_tool(
                        "deep_research",
                        "Escalate into multi-source analysis",
                        is_research,
                        "safe",
                    ),
                    mission_tool(
                        "evidence_notary",
                        "Bind citations to verifiable outputs",
                        true,
                        "moderate",
                    ),
                ],
            ));
            agents.push(self.agent_assignment(
                "verification-judge",
                "Cross-check claims, contradictions, and confidence.",
                4,
                0.94,
                vec![
                    mission_tool(
                        "evidence_notary",
                        "Stamp evidence chains for auditability",
                        true,
                        "moderate",
                    ),
                    mission_tool(
                        "data_analyze",
                        "Compare structured signals and contradictions",
                        false,
                        "safe",
                    ),
                ],
            ));
        }

        if is_code || is_automation {
            agents.push(self.agent_assignment(
                "forge-builder",
                "Create implementations, workflows, and tool wiring.",
                5,
                0.9,
                vec![
                    mission_tool(
                        "code_execute",
                        "Run bounded implementation tasks",
                        is_code,
                        "moderate",
                    ),
                    mission_tool(
                        "workflow_forge",
                        "Create reusable task automations and flows",
                        true,
                        "moderate",
                    ),
                    mission_tool(
                        "git_commit",
                        "Package safe changes for release flow",
                        false,
                        "moderate",
                    ),
                ],
            ));
        }

        if is_automation || is_research {
            agents.push(self.agent_assignment(
                "memory-librarian",
                "Persist mission context, prior evidence, and reusable patterns.",
                6,
                0.86,
                vec![
                    mission_tool(
                        "session_memory",
                        "Store recoverable mission context",
                        true,
                        "safe",
                    ),
                    mission_tool(
                        "task_scheduler",
                        "Queue background refresh and upkeep",
                        is_automation,
                        "moderate",
                    ),
                ],
            ));
        }

        agents.sort_by_key(|agent| agent.priority);

        let summary = if is_code && is_research {
            "Astra will run a build-grade mission: forge code, research dependencies, and verify the output before presenting it."
        } else if is_code {
            "Astra will treat this as a production engineering mission with build, safety review, and verification lanes."
        } else if is_research {
            "Astra will route this through research, verification, and evidence-notarization lanes for a trustworthy answer."
        } else if is_navigation {
            "Astra will keep the task user-visible, with navigation orchestration and safety oversight."
        } else {
            "Astra will use a mixed execution lane that can browse, reason, and verify before responding."
        };

        let mut safeguards = vec![
            "High-risk tools remain approval-gated and are never auto-assigned.".to_string(),
            "Every mission keeps a safety sentinel in the loop for policy and sensitive-action review."
                .to_string(),
        ];
        if is_sensitive {
            safeguards.push(
                "Sensitive or identity-linked flows stay in a guarded privacy lane with extra verification."
                    .to_string(),
            );
        }
        if is_code {
            safeguards.push(
                "Code-generation work includes verification and bounded execution before promotion."
                    .to_string(),
            );
        }

        let mut follow_up_actions = Vec::new();
        if is_research {
            follow_up_actions.push("refresh evidence bundle when freshness-sensitive".to_string());
        }
        if is_code {
            follow_up_actions.push("run validation and release checks before deploy".to_string());
        }
        if is_automation {
            follow_up_actions
                .push("promote reusable workflow into a scheduled automation".to_string());
        }

        MissionAssignmentPlan {
            task: normalized_task.to_string(),
            summary: summary.to_string(),
            execution_mode: if is_navigation {
                "resource_acquisition".to_string()
            } else {
                "hybrid_autonomous".to_string()
            },
            privacy_lane: if is_sensitive {
                "guarded_identity".to_string()
            } else {
                "origin_shielded".to_string()
            },
            delivery_mode: if is_code {
                "artifact_plus_explanation".to_string()
            } else {
                "guided_answer".to_string()
            },
            escalation_policy: if is_sensitive {
                "require_confirmation_for_sensitive_actions".to_string()
            } else {
                "auto_escalate_on_conflict_or_high_risk".to_string()
            },
            estimated_duration_ms: if is_code && is_research {
                12_000
            } else if is_code || is_research {
                8_500
            } else {
                4_500
            },
            agents,
            safeguards,
            follow_up_actions,
        }
    }

    fn seed_builtin_agents(&mut self) {
        for profile in builtin_profiles() {
            self.registry.register(
                &profile.agent_id,
                &profile.agent_type,
                profile.capabilities.clone(),
            );
            self.builtin_profile_ids.insert(profile.agent_id.clone());
            self.profiles.insert(profile.agent_id.clone(), profile);
        }
    }

    fn agent_assignment(
        &self,
        agent_id: &str,
        reason: &str,
        priority: u8,
        confidence: f64,
        tools: Vec<MissionToolAssignment>,
    ) -> MissionAgentAssignment {
        let profile = self
            .profiles
            .get(agent_id)
            .cloned()
            .unwrap_or_else(|| fallback_profile(agent_id));
        MissionAgentAssignment {
            agent_id: profile.agent_id,
            display_name: profile.display_name,
            role: profile.specialization,
            reason: reason.to_string(),
            priority,
            confidence,
            tools,
        }
    }
}

fn mission_tool(
    name: &str,
    purpose: &str,
    required: bool,
    risk_level: &str,
) -> MissionToolAssignment {
    MissionToolAssignment {
        name: name.to_string(),
        purpose: purpose.to_string(),
        required,
        risk_level: risk_level.to_string(),
    }
}

fn fallback_profile(agent_id: &str) -> AgentCapabilityProfile {
    AgentCapabilityProfile {
        agent_id: agent_id.to_string(),
        display_name: agent_id.replace('-', " "),
        agent_type: "specialist".to_string(),
        specialization: "general mission support".to_string(),
        description: "Fallback specialist profile.".to_string(),
        capabilities: Vec::new(),
        preferred_tools: Vec::new(),
        safety_lane: "standard".to_string(),
        latency_tier: "balanced".to_string(),
        max_concurrency: 2,
        reliability_score: 0.8,
    }
}

fn builtin_profiles() -> Vec<AgentCapabilityProfile> {
    vec![
        AgentCapabilityProfile {
            agent_id: "acquisition-core".to_string(),
            display_name: "Acquisition Core".to_string(),
            agent_type: "execution".to_string(),
            specialization: "resource acquisition and assistant-side task flow".to_string(),
            description: "Owns fetch pipelines, page acquisition, and normalized evidence capture."
                .to_string(),
            capabilities: vec![
                "resource fetching".to_string(),
                "content extraction".to_string(),
                "session recovery".to_string(),
            ],
            preferred_tools: vec!["web_fetch".to_string(), "content_extract".to_string()],
            safety_lane: "assistant_only".to_string(),
            latency_tier: "fast".to_string(),
            max_concurrency: 6,
            reliability_score: 0.96,
        },
        AgentCapabilityProfile {
            agent_id: "research-oracle".to_string(),
            display_name: "Research Oracle".to_string(),
            agent_type: "research".to_string(),
            specialization: "source gathering and answer synthesis".to_string(),
            description: "Expands user prompts into evidence-backed research missions.".to_string(),
            capabilities: vec![
                "adaptive search".to_string(),
                "source ranking".to_string(),
                "citation synthesis".to_string(),
            ],
            preferred_tools: vec![
                "web_search".to_string(),
                "deep_research".to_string(),
                "evidence_notary".to_string(),
            ],
            safety_lane: "evidence_bound".to_string(),
            latency_tier: "balanced".to_string(),
            max_concurrency: 4,
            reliability_score: 0.94,
        },
        AgentCapabilityProfile {
            agent_id: "verification-judge".to_string(),
            display_name: "Verification Judge".to_string(),
            agent_type: "verification".to_string(),
            specialization: "contradiction analysis and confidence grading".to_string(),
            description: "Checks claims, contradiction pressure, and release readiness."
                .to_string(),
            capabilities: vec![
                "claim review".to_string(),
                "confidence scoring".to_string(),
                "release gating".to_string(),
            ],
            preferred_tools: vec!["evidence_notary".to_string(), "data_analyze".to_string()],
            safety_lane: "strict_review".to_string(),
            latency_tier: "balanced".to_string(),
            max_concurrency: 4,
            reliability_score: 0.97,
        },
        AgentCapabilityProfile {
            agent_id: "forge-builder".to_string(),
            display_name: "Forge Builder".to_string(),
            agent_type: "builder".to_string(),
            specialization: "implementation, automation, and tool composition".to_string(),
            description:
                "Creates production-grade workflows, code paths, and reusable automations."
                    .to_string(),
            capabilities: vec![
                "code generation".to_string(),
                "tool composition".to_string(),
                "workflow automation".to_string(),
            ],
            preferred_tools: vec![
                "code_execute".to_string(),
                "workflow_forge".to_string(),
                "git_commit".to_string(),
            ],
            safety_lane: "bounded_execution".to_string(),
            latency_tier: "intensive".to_string(),
            max_concurrency: 3,
            reliability_score: 0.91,
        },
        AgentCapabilityProfile {
            agent_id: "memory-librarian".to_string(),
            display_name: "Memory Librarian".to_string(),
            agent_type: "memory".to_string(),
            specialization: "session memory and reusable context preservation".to_string(),
            description: "Persists durable mission context so tasks can recover cleanly."
                .to_string(),
            capabilities: vec![
                "session memory".to_string(),
                "context recall".to_string(),
                "refresh planning".to_string(),
            ],
            preferred_tools: vec!["session_memory".to_string(), "task_scheduler".to_string()],
            safety_lane: "durable_context".to_string(),
            latency_tier: "efficient".to_string(),
            max_concurrency: 8,
            reliability_score: 0.88,
        },
        AgentCapabilityProfile {
            agent_id: "safety-sentinel".to_string(),
            display_name: "Safety Sentinel".to_string(),
            agent_type: "safety".to_string(),
            specialization: "policy enforcement and sensitive-action review".to_string(),
            description:
                "Guards risky actions, identity-sensitive flows, and irreversible operations."
                    .to_string(),
            capabilities: vec![
                "risk review".to_string(),
                "policy gating".to_string(),
                "threat scanning".to_string(),
            ],
            preferred_tools: vec!["risk_review".to_string(), "threat_scan".to_string()],
            safety_lane: "guarded".to_string(),
            latency_tier: "fast".to_string(),
            max_concurrency: 10,
            reliability_score: 0.99,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::AgentController;
    use crate::agents::base::AgentCapabilityProfile;

    #[test]
    fn seeds_builtin_capability_catalog() {
        let controller = AgentController::new();
        let catalog = controller.capability_catalog();

        assert!(catalog.len() >= 6);
        assert!(catalog
            .iter()
            .any(|agent| agent.agent_id == "acquisition-core"));
        assert!(catalog
            .iter()
            .any(|agent| agent.agent_id == "safety-sentinel"));
    }

    #[test]
    fn custom_profiles_can_be_registered_and_removed() {
        let mut controller = AgentController::new();
        controller.register_custom_profile(AgentCapabilityProfile {
            agent_id: "mission-forge".into(),
            display_name: "Mission Forge".into(),
            agent_type: "assistant_custom".into(),
            specialization: "mission assembly".into(),
            description: "Builds temporary mission specialists.".into(),
            capabilities: vec!["code".into(), "verification".into()],
            preferred_tools: vec!["workflow_forge".into()],
            safety_lane: "operator_review".into(),
            latency_tier: "adaptive".into(),
            max_concurrency: 1,
            reliability_score: 0.7,
        });

        assert!(controller
            .capability_catalog()
            .iter()
            .any(|agent| agent.agent_id == "mission-forge"));
        assert!(controller
            .remove_custom_profile("mission-forge")
            .expect("custom profile should be removable"));
        assert!(!controller
            .capability_catalog()
            .iter()
            .any(|agent| agent.agent_id == "mission-forge"));
    }

    #[test]
    fn plans_code_mission_with_builder_and_verifier() {
        let controller = AgentController::new();
        let plan = controller.plan_mission("build a production api and verify it", None);

        assert!(plan
            .agents
            .iter()
            .any(|agent| agent.agent_id == "forge-builder"));
        assert!(plan
            .agents
            .iter()
            .any(|agent| agent.agent_id == "verification-judge"));
        assert_eq!(plan.delivery_mode, "artifact_plus_explanation");
    }
}

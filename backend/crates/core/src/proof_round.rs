//! The self-driving proof round — the adversarial ecosystem that runs itself.
//!
//! One population (the **proposer**) generates machine-checkable claims; a
//! second population (the **refuters**) is rewarded only for breaking them; and
//! the [proof economy](crate::proof_economy)'s device-run checks are the
//! impartial arbiter throughout. A claim is minted into permanent knowledge
//! only after it survives the refuters while its check keeps passing.
//!
//! Both populations are the same remote LLM ([`ReasoningExecutor`]) wearing
//! different hats — but the LLM never decides truth. It only *proposes* and
//! *attacks*; reality (the executed `Check`) decides. Attacks are spent on the
//! highest value-of-information open claim first (the cheapest experiment that
//! resolves the most uncertainty).
//!
//! The whole thing is deterministic given the LLM's replies, so it is tested
//! with a scripted executor and no live model.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::asc2::ReasoningExecutor;
use crate::common::AppError;
use crate::proof_economy::{AttackRequest, Check, ClaimKind, ClaimStatus, ProofEconomy, ProposeRequest};

const PROPOSER_SYSTEM: &str = "You are a claim proposer in a proof economy. Propose ONE verifiable \
claim as a single JSON object, no prose: {\"statement\":\"...\",\"kind\":\"assertion|refutation|impossibility\",\
\"check\":<check>}. The check is the machine-runnable proof and MUST be one of: \
{\"type\":\"shell_output_contains\",\"command\":\"...\",\"substring\":\"...\"}, \
{\"type\":\"command_succeeds\",\"command\":\"...\"}, \
{\"type\":\"command_fails\",\"command\":\"...\"} (for impossibility), \
{\"type\":\"file_contains\",\"path\":\"...\",\"substring\":\"...\"}. Only propose a claim whose check you \
believe PASSES when run — a false claim burns your staked reputation. Prefer small, surely-true, \
cheaply-checkable facts.";

const REFUTER_SYSTEM: &str = "You are a refuter in a proof economy. Your only reward is BREAKING claims. \
Given a claim and its check, reply with a single JSON object, no prose: {\"note\":\"why it might be false\",\
\"counter\":<check>|null}. The optional counter is a check that, if it PASSES while the claim's own check \
also passes, proves a contradiction. Attack honestly — a wrong attack burns your stake.";

#[derive(Debug, Clone, Deserialize)]
pub struct RoundConfig {
    /// What the proposer should make claims about.
    #[serde(default)]
    pub topic: String,
    #[serde(default = "default_proposals")]
    pub num_proposals: usize,
    #[serde(default = "default_attack_budget")]
    pub attack_budget: usize,
}

fn default_proposals() -> usize {
    2
}
fn default_attack_budget() -> usize {
    6
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimOutcome {
    pub claim_id: String,
    pub statement: String,
    pub proposer: String,
    pub status: ClaimStatus,
    pub survived_attacks: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoundReport {
    pub topic: String,
    pub proposals_made: usize,
    pub attacks_run: usize,
    pub minted_this_round: usize,
    pub refuted_this_round: usize,
    pub still_open: usize,
    pub outcomes: Vec<ClaimOutcome>,
    pub ledger_size: usize,
}

/// Drives the proposer/refuter populations against the economy.
#[derive(Clone)]
pub struct ProofConductor {
    economy: ProofEconomy,
    reasoner: Arc<dyn ReasoningExecutor>,
}

#[derive(Debug, Deserialize)]
struct ProposedClaim {
    statement: String,
    #[serde(default)]
    kind: ClaimKind,
    check: Check,
    #[serde(default)]
    evidence: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProposedAttack {
    #[serde(default)]
    note: String,
    #[serde(default)]
    counter: Option<Check>,
}

impl ProofConductor {
    #[must_use]
    pub fn new(economy: ProofEconomy, reasoner: Arc<dyn ReasoningExecutor>) -> Self {
        Self { economy, reasoner }
    }

    /// Run one full round: propose, then spend the attack budget on the
    /// highest-value-of-information open claims until it is exhausted.
    pub async fn run_round(&self, config: RoundConfig) -> Result<RoundReport, AppError> {
        let topic = if config.topic.trim().is_empty() {
            "small, surely-true, cheaply-checkable facts about this host".to_string()
        } else {
            config.topic.trim().to_string()
        };
        let num_proposals = config.num_proposals.clamp(1, 10);
        let attack_budget = config.attack_budget.clamp(0, 40);

        // ── Propose phase ──
        let mut proposed_ids: Vec<String> = Vec::new();
        for index in 0..num_proposals {
            let user = format!(
                "Topic: {topic}\nPropose one verifiable claim about it now, as JSON."
            );
            let raw = self.reasoner.complete(PROPOSER_SYSTEM.into(), user).await?;
            let Some(parsed) = parse_json::<ProposedClaim>(&raw) else {
                continue;
            };
            let economy = self.economy.clone();
            let request = ProposeRequest {
                statement: parsed.statement,
                kind: parsed.kind,
                proposer: format!("proposer_{index}"),
                verification: parsed.check,
                evidence: parsed.evidence,
                depends_on: Vec::new(),
            };
            if let Ok(claim) =
                actix_web::web::block(move || economy.propose(request)).await.map_err(join_err)?
            {
                proposed_ids.push(claim.claim_id);
            }
        }

        // ── Attack phase (value-of-information ordered) ──
        let mut attacks_run = 0_usize;
        let mut refuter = 0_usize;
        while attacks_run < attack_budget {
            let Some(target) = self.economy.next_experiment()? else {
                break;
            };
            let user = format!(
                "Claim to break: \"{}\"\nIts check: {}\nReply with your attack as JSON.",
                target.statement,
                serde_json::to_string(&target.verification).unwrap_or_default()
            );
            let raw = self.reasoner.complete(REFUTER_SYSTEM.into(), user).await?;
            let parsed = parse_json::<ProposedAttack>(&raw).unwrap_or_default();
            let economy = self.economy.clone();
            let claim_id = target.claim_id.clone();
            let request = AttackRequest {
                attacker: format!("refuter_{}", refuter % 4),
                note: parsed.note,
                counter: parsed.counter,
            };
            let _ = actix_web::web::block(move || economy.attack(&claim_id, request))
                .await
                .map_err(join_err)?;
            attacks_run += 1;
            refuter += 1;
        }

        // ── Report: final status of this round's proposals ──
        let mut outcomes = Vec::new();
        let mut minted = 0;
        let mut refuted = 0;
        let mut open = 0;
        for id in &proposed_ids {
            if let Ok(claim) = self.economy.get_claim(id) {
                match claim.status {
                    ClaimStatus::Minted => minted += 1,
                    ClaimStatus::Refuted => refuted += 1,
                    ClaimStatus::Proposed => open += 1,
                    ClaimStatus::Disputed => {}
                }
                outcomes.push(ClaimOutcome {
                    claim_id: claim.claim_id,
                    statement: claim.statement,
                    proposer: claim.proposer,
                    status: claim.status,
                    survived_attacks: claim.survived_attacks,
                });
            }
        }
        Ok(RoundReport {
            topic,
            proposals_made: proposed_ids.len(),
            attacks_run,
            minted_this_round: minted,
            refuted_this_round: refuted,
            still_open: open,
            outcomes,
            ledger_size: self.economy.ledger()?.len(),
        })
    }
}

/// Spawn the self-driving conductor on its own thread+runtime (mirrors the
/// telegram worker). Gated behind `ASTRA_PROOF_AUTONOMY=on`; it runs a round
/// every `ASTRA_PROOF_INTERVAL_SECS` (default 300, min 30) on
/// `ASTRA_PROOF_TOPIC`. Needs a configured remote LLM to be useful.
pub fn spawn_proof_conductor(conductor: ProofConductor) {
    let enabled = std::env::var("ASTRA_PROOF_AUTONOMY")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!("proof autonomy disabled (set ASTRA_PROOF_AUTONOMY=on to enable)");
        return;
    }
    let topic = std::env::var("ASTRA_PROOF_TOPIC")
        .unwrap_or_else(|_| "small, surely-true, cheaply-checkable facts about this host".into());
    let interval = std::env::var("ASTRA_PROOF_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s >= 30)
        .unwrap_or(300);
    std::thread::Builder::new()
        .name("astra-proof-conductor".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                loop {
                    match conductor
                        .run_round(RoundConfig {
                            topic: topic.clone(),
                            num_proposals: default_proposals(),
                            attack_budget: default_attack_budget(),
                        })
                        .await
                    {
                        Ok(report) => tracing::info!(
                            minted = report.minted_this_round,
                            refuted = report.refuted_this_round,
                            ledger = report.ledger_size,
                            "proof round complete"
                        ),
                        Err(error) => tracing::warn!(error = %error, "proof round failed"),
                    }
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            });
        })
        .expect("failed to spawn proof conductor thread");
}

fn parse_json<T: for<'de> Deserialize<'de>>(raw: &str) -> Option<T> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&trimmed[start..=end]).ok()
}

fn join_err(error: actix_web::error::BlockingError) -> AppError {
    AppError::Internal(format!("proof round blocking task failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc2::{BoxFuture, ReasoningProposal, ReasoningRequest};
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};
    use parking_lot::Mutex;
    use std::collections::VecDeque;

    struct ScriptedExecutor {
        replies: Mutex<VecDeque<String>>,
    }
    impl ScriptedExecutor {
        fn new(replies: Vec<String>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(replies.into_iter().collect()),
            })
        }
    }
    impl ReasoningExecutor for ScriptedExecutor {
        fn name(&self) -> &str {
            "scripted"
        }
        fn execute(
            &self,
            _request: ReasoningRequest,
        ) -> BoxFuture<Result<ReasoningProposal, AppError>> {
            Box::pin(async { Err(AppError::Internal("n/a".into())) })
        }
        fn complete(&self, _system: String, _user: String) -> BoxFuture<Result<String, AppError>> {
            let next = self
                .replies
                .lock()
                .pop_front()
                .unwrap_or_else(|| r#"{"note":"no more"}"#.to_string());
            Box::pin(async move { Ok(next) })
        }
    }

    fn conductor(replies: Vec<String>) -> ProofConductor {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device).unwrap();
        ProofConductor::new(economy, ScriptedExecutor::new(replies))
    }

    #[actix_web::test]
    async fn round_proposes_and_mints_a_true_claim() {
        // Proposer makes a true claim; two refuters fail to break it -> mint.
        let propose = r#"{"statement":"the host echoes 42","kind":"assertion","check":{"type":"shell_output_contains","command":"echo the answer is 42","substring":"42"}}"#.to_string();
        let attack = r#"{"note":"surely false","counter":null}"#.to_string();
        let conductor = conductor(vec![propose, attack.clone(), attack]);
        let report = conductor
            .run_round(RoundConfig { topic: "math".into(), num_proposals: 1, attack_budget: 2 })
            .await
            .expect("round");
        assert_eq!(report.proposals_made, 1);
        assert_eq!(report.attacks_run, 2);
        assert_eq!(report.minted_this_round, 1);
        assert_eq!(report.ledger_size, 1);
        assert_eq!(report.outcomes[0].status, ClaimStatus::Minted);
    }

    #[actix_web::test]
    async fn round_refutes_a_false_proposal_on_arrival() {
        // Proposer lies; the check fails on arrival -> born refuted, no attacks needed.
        let lie = r#"{"statement":"echo 42 prints 99","check":{"type":"shell_output_contains","command":"echo 42","substring":"99"}}"#.to_string();
        let conductor = conductor(vec![lie]);
        let report = conductor
            .run_round(RoundConfig { topic: "x".into(), num_proposals: 1, attack_budget: 0 })
            .await
            .expect("round");
        assert_eq!(report.refuted_this_round, 1);
        assert_eq!(report.minted_this_round, 0);
        assert_eq!(report.outcomes[0].status, ClaimStatus::Refuted);
    }
}

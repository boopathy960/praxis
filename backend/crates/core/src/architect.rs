//! The Architect — proof-governed, self-healing *agent* genesis.
//!
//! The [Forge](crate::forge) grows the tool layer; the Architect grows the agent
//! layer, to the same standard. The older `agent_runtime::fabricate_for_objective`
//! composes an agent from keyword heuristics and trusts it; the Architect instead
//! treats an agent configuration as a **claim that must prove itself**:
//!
//!   * **Compose.** The LLM designs an [`AgentSpec`] — a name, a role, an
//!     allow-list drawn from the **native primitives and the proof-minted forged
//!     tools**, a concrete canary objective, and a context-free success
//!     [`Check`]. Only tools that actually exist (governed primitives or `Active`
//!     forged capabilities) may be granted.
//!
//!   * **Instantiate.** The spec is run for real through the
//!     [verified agentic loop](crate::agentic_loop) on its canary objective, so
//!     its tools act under the device supervisor and its mutating steps prove
//!     their own postconditions.
//!
//!   * **Mint.** The spec's success contract is staked in the
//!     [proof economy](crate::proof_economy). An agent that did not actually
//!     achieve its canary is born refuted and never goes `Active`; one whose
//!     contract survives re-execution under attack is minted and becomes a
//!     reusable, recallable agent.
//!
//!   * **Heal.** A minted agent whose success rate regresses, or whose proof
//!     stops verifying, is diagnosed, re-composed, re-proven, and hot-swapped —
//!     quarantining the broken version. Agents repair themselves, and "repaired"
//!     means *re-proven*.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::agentic_loop::{AgenticLoop, LoopRequest, LoopResult};
use crate::asc2::ReasoningExecutor;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::is_device_tool;
use crate::forge::ForgeService;
use crate::proof_economy::{AttackRequest, ClaimKind, ClaimStatus, ProofEconomy, ProposeRequest};
use crate::verification::Check;

/// Independent re-execution reviews a fresh spec claim must survive to mint.
const MINT_ATTACK_ROUNDS: u32 = 2;
/// Maximum loop iterations granted when instantiating a spec's canary.
const CANARY_MAX_ITERATIONS: usize = 6;
/// Minimum runs before a spec's success rate is a strong enough heal signal.
const HEAL_MIN_RUNS: u64 = 3;
/// Success rate at or below which a spec is considered regressed.
const HEAL_SUCCESS_FLOOR: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecStatus {
    /// Composed but its proof has not minted — not a trusted agent.
    Probation,
    /// Proof minted (it provably achieved its canary) — reusable.
    Active,
    /// Regressed/refuted and superseded by a healed version.
    Quarantined,
}

/// Health of a minted agent across its real runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecHealth {
    pub runs: u64,
    pub verified_runs: u64,
    pub unverified_runs: u64,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub last_run_at_ms: Option<i64>,
}

impl SpecHealth {
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        if self.runs == 0 {
            1.0
        } else {
            self.verified_runs as f64 / self.runs as f64
        }
    }
}

/// A reusable agent configuration whose trust is a survived proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub spec_id: String,
    pub name: String,
    /// The goal-class this agent handles.
    pub goal: String,
    /// Persona / standing instructions prepended to every objective it runs.
    pub role: String,
    /// Tools the agent may use — native primitives and/or `Active` forged tools.
    pub allow_tools: Vec<String>,
    /// The concrete objective used to prove the spec.
    pub canary_objective: String,
    /// Context-free, re-runnable contract proving the canary run succeeded.
    pub success: Check,
    #[serde(default)]
    pub claim_id: Option<String>,
    pub status: SpecStatus,
    pub health: SpecHealth,
    pub version: u32,
    #[serde(default)]
    pub supersedes: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComposeRequest {
    pub goal: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunSpecRequest {
    pub objective: String,
    #[serde(default)]
    pub max_iterations: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComposeOutcome {
    pub spec: AgentSpec,
    pub minted: bool,
    /// The canary instantiation run that the proof was anchored to.
    pub canary_run: LoopResult,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealOutcome {
    pub name: String,
    pub healed: bool,
    pub old_spec_id: String,
    pub new_spec: AgentSpec,
    pub detail: String,
}

/// What the LLM returns when composing an agent (tolerant to missing fields).
#[derive(Debug, Clone, Deserialize)]
struct SpecDraft {
    name: String,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    role: String,
    allow_tools: Vec<String>,
    canary_objective: String,
    success: Check,
}

/// The Architect. Composes agent specs, instantiates them through the verified
/// loop, mints their proofs through the economy, and heals regressed ones.
#[derive(Clone)]
pub struct ArchitectService {
    store: Arc<Mutex<Connection>>,
    cache: Shared<HashMap<String, AgentSpec>>,
    economy: ProofEconomy,
    agentic_loop: AgenticLoop,
    forge: ForgeService,
    reasoner: Arc<dyn ReasoningExecutor>,
}

impl ArchitectService {
    pub fn new(
        data_dir: impl AsRef<Path>,
        economy: ProofEconomy,
        agentic_loop: AgenticLoop,
        forge: ForgeService,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("architect.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create architect dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open architect registry: {e}")))?;
        init_store(&connection)?;
        let cache = load_cache(&connection)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            cache: Shared::new(RwLock::new(cache)),
            economy,
            agentic_loop,
            forge,
            reasoner,
        })
    }

    pub fn in_memory(
        economy: ProofEconomy,
        agentic_loop: AgenticLoop,
        forge: ForgeService,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let connection = Connection::open_in_memory()
            .map_err(|e| AppError::Internal(format!("architect in-memory open failed: {e}")))?;
        init_store(&connection)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            cache: Shared::new(RwLock::new(HashMap::new())),
            economy,
            agentic_loop,
            forge,
            reasoner,
        })
    }

    // ── Compose: design + prove a reusable agent ────────────────────────────

    /// Compose an agent spec for `goal`, instantiate it on its canary through the
    /// verified loop, and stake its success contract in the economy. The agent is
    /// `Active` (reusable) only if its proof mints.
    pub async fn compose(&self, request: ComposeRequest) -> Result<ComposeOutcome, AppError> {
        let goal = request.goal.trim().to_string();
        if goal.is_empty() {
            return Err(AppError::Validation("architect goal is empty".into()));
        }
        guard_objective(&goal)?;
        let draft = self.author_spec(&goal, None).await?;
        self.instantiate_and_commit(draft, 1, None).await
    }

    /// The shared async backbone for compose and heal: run the canary through the
    /// loop, then adjudicate the proof and register the spec off the async pool.
    async fn instantiate_and_commit(
        &self,
        draft: SpecDraft,
        version: u32,
        supersedes: Option<String>,
    ) -> Result<ComposeOutcome, AppError> {
        // Instantiate: run the spec for real. Its tools act under the device
        // supervisor; mutating steps prove their own postconditions. The canary
        // run is what establishes the post-state the success contract checks.
        let objective = compose_objective(&draft.role, &draft.canary_objective);
        let canary_run = self
            .agentic_loop
            .run(LoopRequest {
                objective,
                allow_tools: draft.allow_tools.clone(),
                max_iterations: CANARY_MAX_ITERATIONS,
            })
            .await?;

        let architect = self.clone();
        actix_web::web::block(move || architect.commit(draft, canary_run, version, supersedes))
            .await
            .map_err(join_err)?
    }

    /// Stake the spec's success contract and register it. Synchronous: touches the
    /// device (the contract re-runs for real) and the SQLite ledgers.
    fn commit(
        &self,
        draft: SpecDraft,
        canary_run: LoopResult,
        version: u32,
        supersedes: Option<String>,
    ) -> Result<ComposeOutcome, AppError> {
        let now = now_ms();
        let spec_id = new_id("agent_spec");
        let mut spec = AgentSpec {
            spec_id: spec_id.clone(),
            name: draft.name.clone(),
            goal: if draft.goal.trim().is_empty() {
                draft.name.clone()
            } else {
                draft.goal.clone()
            },
            role: draft.role.clone(),
            allow_tools: draft.allow_tools.clone(),
            canary_objective: draft.canary_objective.clone(),
            success: draft.success.clone(),
            claim_id: None,
            status: SpecStatus::Probation,
            health: SpecHealth::default(),
            version,
            supersedes,
            created_at_ms: now,
            updated_at_ms: now,
        };

        // The proof is the arbiter, not the loop's self-report: even if the loop
        // hit its iteration cap, the spec mints iff its success contract actually
        // holds — and a spec that did not achieve its canary is born refuted.
        let claim = self.economy.propose(ProposeRequest {
            statement: format!("agent spec '{}' achieves its canary objective", spec.name),
            kind: ClaimKind::Assertion,
            proposer: format!("architect:{spec_id}"),
            verification: draft.success.clone(),
            evidence: vec![format!(
                "canary loop status={}, all_verified={}",
                canary_run.status, canary_run.all_verified
            )],
            depends_on: Vec::new(),
        })?;
        spec.claim_id = Some(claim.claim_id.clone());
        let mut latest = claim;
        if latest.status == ClaimStatus::Proposed {
            for round in 0..MINT_ATTACK_ROUNDS {
                latest = self.economy.attack(
                    &latest.claim_id,
                    AttackRequest {
                        attacker: format!("architect-review:{spec_id}:{round}"),
                        note: "automated stability review (re-run the success contract)".into(),
                        counter: None,
                    },
                )?;
            }
        }
        if latest.status == ClaimStatus::Minted {
            spec.status = SpecStatus::Active;
        }

        let minted = spec.status == SpecStatus::Active;
        self.persist(&spec)?;
        // A brand-new name is registered even on probation (the attempt is part of
        // the record); only `Active` specs are runnable.
        self.cache.write().insert(spec.name.clone(), spec.clone());
        Ok(ComposeOutcome {
            spec,
            minted,
            canary_run,
            detail: latest.last_verification,
        })
    }

    // ── Run: reuse a minted agent ───────────────────────────────────────────

    /// Run a minted agent on a fresh objective, with its proven role and tool
    /// envelope. Updates the agent's health, which is the healer's signal.
    pub async fn run_spec(
        &self,
        name: &str,
        request: RunSpecRequest,
    ) -> Result<LoopResult, AppError> {
        let spec = self
            .cache
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("agent spec {name}")))?;
        if spec.status != SpecStatus::Active {
            return Err(AppError::Validation(format!(
                "agent spec '{name}' is {:?}, not Active (its proof has not minted)",
                spec.status
            )));
        }
        let max_iterations = if request.max_iterations == 0 {
            CANARY_MAX_ITERATIONS
        } else {
            request.max_iterations
        };
        let result = self
            .agentic_loop
            .run(LoopRequest {
                objective: compose_objective(&spec.role, &request.objective),
                allow_tools: spec.allow_tools.clone(),
                max_iterations,
            })
            .await?;
        self.record_run(name, &result);
        Ok(result)
    }

    fn record_run(&self, name: &str, result: &LoopResult) {
        let mut cache = self.cache.write();
        if let Some(spec) = cache.get_mut(name) {
            spec.health.runs += 1;
            spec.health.last_run_at_ms = Some(now_ms());
            spec.health.last_status = Some(result.status.clone());
            // "Verified" means the loop finished and nothing went unproven — the
            // same honest signal the loop reports to everyone else.
            let ok = matches!(result.status.as_str(), "completed" | "recalled")
                && result.all_verified;
            if ok {
                spec.health.verified_runs += 1;
            } else {
                spec.health.unverified_runs += 1;
            }
            spec.updated_at_ms = now_ms();
            let snapshot = spec.clone();
            drop(cache);
            let _ = self.persist(&snapshot);
        }
    }

    // ── Heal: re-compose + re-prove a regressed agent ───────────────────────

    /// Diagnose a regressed agent, re-compose it, re-prove it on its canary, and
    /// hot-swap — quarantining the broken version. A heal succeeds only if the new
    /// version mints; otherwise the previous version is left in place.
    pub async fn heal(&self, name: &str) -> Result<HealOutcome, AppError> {
        let current = self
            .cache
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("agent spec {name}")))?;

        let diagnosis = format!(
            "REPAIR MODE. The agent '{}' is regressing: success rate {:.0}% over {} run(s); \
             last loop status: {}. Its success contract is: {}. Re-compose it (role + tool \
             allow-list + canary) so this same contract provably holds again.",
            current.name,
            current.health.success_rate() * 100.0,
            current.health.runs,
            current.health.last_status.as_deref().unwrap_or("(none)"),
            serde_json::to_string(&current.success).unwrap_or_default(),
        );
        let draft = self.author_spec(&current.goal, Some(&diagnosis)).await?;
        let old_spec_id = current.spec_id.clone();
        let outcome = self
            .instantiate_and_commit(draft, current.version + 1, Some(old_spec_id.clone()))
            .await?;

        let healed = outcome.spec.status == SpecStatus::Active;
        if healed {
            let mut old = current;
            old.status = SpecStatus::Quarantined;
            old.updated_at_ms = now_ms();
            self.persist(&old)?;
            // commit() already registered the new spec under the name.
        } else {
            // Roll the registry back to the previous version: the failed repair is
            // kept in SQLite for audit but must not replace a working agent.
            self.cache
                .write()
                .insert(current.name.clone(), current.clone());
        }
        let detail = if healed {
            format!("re-proven and hot-swapped to v{}", outcome.spec.version)
        } else {
            format!(
                "repair did not mint (status {:?}); previous version left in place",
                outcome.spec.status
            )
        };
        Ok(HealOutcome {
            name: name.to_string(),
            healed,
            old_spec_id,
            new_spec: outcome.spec,
            detail,
        })
    }

    /// Scan for regressed agents and re-prove a bounded batch of them — the
    /// autonomous entry point the background heal worker ticks.
    pub async fn heal_round(&self, max: usize) -> Vec<HealOutcome> {
        let mut outcomes = Vec::new();
        for name in self.heal_targets().into_iter().take(max.max(1)) {
            match self.heal(&name).await {
                Ok(outcome) => {
                    if outcome.healed {
                        tracing::info!(agent = %name, version = outcome.new_spec.version, "auto-healed agent");
                    } else {
                        tracing::warn!(agent = %name, detail = %outcome.detail, "auto-heal did not re-prove agent");
                    }
                    outcomes.push(outcome);
                }
                Err(error) => tracing::warn!(agent = %name, %error, "auto-heal of agent errored"),
            }
        }
        outcomes
    }

    /// Names whose success rate regressed with enough runs, or whose minted proof
    /// no longer verifies.
    #[must_use]
    pub fn heal_targets(&self) -> Vec<String> {
        let specs: Vec<AgentSpec> = self.cache.read().values().cloned().collect();
        specs
            .into_iter()
            .filter(|spec| {
                let regressed = spec.health.runs >= HEAL_MIN_RUNS
                    && spec.health.success_rate() <= HEAL_SUCCESS_FLOOR;
                let proof_broke = spec.status == SpecStatus::Active
                    && spec
                        .claim_id
                        .as_ref()
                        .and_then(|id| self.economy.verify(id).ok())
                        .is_some_and(|report| !report.pass);
                regressed || proof_broke
            })
            .map(|spec| spec.name)
            .collect()
    }

    // ── Views ───────────────────────────────────────────────────────────────

    #[must_use]
    pub fn catalog(&self) -> Vec<AgentSpec> {
        let mut specs: Vec<AgentSpec> = self.cache.read().values().cloned().collect();
        specs.sort_by_key(|spec| std::cmp::Reverse(spec.created_at_ms));
        specs
    }

    pub fn get(&self, name: &str) -> Result<AgentSpec, AppError> {
        self.cache
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("agent spec {name}")))
    }

    // ── Authoring ───────────────────────────────────────────────────────────

    async fn author_spec(
        &self,
        goal: &str,
        repair: Option<&str>,
    ) -> Result<SpecDraft, AppError> {
        let forged = self.forge.catalog_for_prompt();
        let system = architect_system_prompt(&forged);
        let user = architect_user_prompt(goal, repair);
        let raw = self.reasoner.complete(system.clone(), user.clone()).await?;
        let draft = match parse_draft(&raw) {
            Some(draft) => draft,
            None => {
                let reask = self
                    .reasoner
                    .complete(
                        system,
                        format!(
                            "{user}\n\nYour previous reply was not valid JSON. Reply with ONLY the JSON object."
                        ),
                    )
                    .await?;
                parse_draft(&reask).ok_or_else(|| {
                    AppError::Internal("architect model did not return a valid spec".into())
                })?
            }
        };
        self.validate_draft(&draft)?;
        Ok(draft)
    }

    /// Every granted tool must actually exist — a native governed primitive or an
    /// `Active` forged capability — and the success contract must be re-runnable.
    fn validate_draft(&self, draft: &SpecDraft) -> Result<(), AppError> {
        let name = draft.name.trim();
        if name.is_empty() || name.len() > 40 {
            return Err(AppError::Validation("agent name must be 1-40 chars".into()));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(AppError::Validation(
                "agent name must be snake_case [a-z0-9_]".into(),
            ));
        }
        if draft.canary_objective.trim().is_empty() {
            return Err(AppError::Validation(
                "agent spec needs a concrete canary objective to prove it".into(),
            ));
        }
        if draft.allow_tools.is_empty() {
            return Err(AppError::Validation(
                "agent spec must grant at least one tool".into(),
            ));
        }
        for tool in &draft.allow_tools {
            if !is_device_tool(tool) && !self.forge.has_active(tool) {
                return Err(AppError::Validation(format!(
                    "tool '{tool}' is neither a governed primitive nor an Active forged tool"
                )));
            }
        }
        if matches!(draft.success, Check::OutputContains { .. }) {
            return Err(AppError::Validation(
                "agent success contracts must be context-free (output_contains is not re-runnable)"
                    .into(),
            ));
        }
        Ok(())
    }

    // ── Persistence ─────────────────────────────────────────────────────────

    fn persist(&self, spec: &AgentSpec) -> Result<(), AppError> {
        let payload = serde_json::to_string(spec)
            .map_err(|e| AppError::Internal(format!("architect serialize failed: {e}")))?;
        let status = format!("{:?}", spec.status).to_ascii_lowercase();
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO agent_specs (spec_id, name, payload, status, version, created_at_ms) \
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![spec.spec_id, spec.name, payload, status, spec.version, spec.created_at_ms],
            )
            .map_err(|e| AppError::Internal(format!("architect persist failed: {e}")))?;
        Ok(())
    }
}

fn init_store(connection: &Connection) -> Result<(), AppError> {
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS agent_specs (
                spec_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                payload TEXT NOT NULL,
                status TEXT NOT NULL,
                version INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL);",
        )
        .map_err(|e| AppError::Internal(format!("architect store init failed: {e}")))?;
    Ok(())
}

/// Rebuild the registry: per name the current spec is the highest-version
/// `Active` one, else the highest-version overall.
fn load_cache(connection: &Connection) -> Result<HashMap<String, AgentSpec>, AppError> {
    let mut statement = connection
        .prepare("SELECT payload FROM agent_specs")
        .map_err(|e| AppError::Internal(format!("architect load failed: {e}")))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| AppError::Internal(format!("architect load failed: {e}")))?;
    let mut current: HashMap<String, AgentSpec> = HashMap::new();
    for payload in rows.flatten() {
        let Ok(spec) = serde_json::from_str::<AgentSpec>(&payload) else {
            continue;
        };
        let replace = match current.get(&spec.name) {
            None => true,
            Some(existing) => {
                let spec_active = spec.status == SpecStatus::Active;
                let existing_active = existing.status == SpecStatus::Active;
                match (spec_active, existing_active) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => spec.version > existing.version,
                }
            }
        };
        if replace {
            current.insert(spec.name.clone(), spec);
        }
    }
    Ok(current)
}

fn compose_objective(role: &str, objective: &str) -> String {
    let role = role.trim();
    if role.is_empty() {
        objective.to_string()
    } else {
        format!("{role}\n\nObjective: {objective}")
    }
}

fn parse_draft(raw: &str) -> Option<SpecDraft> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<SpecDraft>(&trimmed[start..=end]).ok()
}

fn guard_objective(objective: &str) -> Result<(), AppError> {
    let lower = objective.to_ascii_lowercase();
    const FORBIDDEN: [&str; 8] = [
        "dump password",
        "dump private key",
        "seed phrase",
        "credential dump",
        "steal",
        "exfiltrate",
        "fingerprint evasion",
        "disable security",
    ];
    if FORBIDDEN.iter().any(|needle| lower.contains(needle)) {
        return Err(AppError::Forbidden(
            "composing agents for secret extraction, theft, or evasion is blocked".into(),
        ));
    }
    Ok(())
}

fn architect_system_prompt(forged: &[(String, String)]) -> String {
    let forged_block = if forged.is_empty() {
        "  (none yet — forge tools first if you need new capability)".to_string()
    } else {
        forged
            .iter()
            .map(|(name, description)| format!("  - {name}: {description}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "You are Astra's Architect. You design ONE reusable agent as a spec, then it will be proven \
         by actually running it. Reply with ONLY one JSON object:\n\
         {{\"name\":\"snake_case_name\",\"goal\":\"the class of task it handles\",\
         \"role\":\"persona/standing instructions\",\"allow_tools\":[\"...\"],\
         \"canary_objective\":\"a concrete task instance used to prove it\",\"success\":{{...}}}}\n\n\
         allow_tools may include native primitives (system_info, process_list, disk_usage, fs_list, \
         fs_read, fs_write, open_path, shell_exec, web_search, deep_crawl) and these proof-minted \
         forged tools:\n{forged_block}\n\n\
         canary_objective: a concrete, self-contained task this agent will be RUN on to prove it \
         works (use absolute paths / explicit values).\n\n\
         success: a context-free, independently re-runnable proof that the canary run achieved its \
         effect (NEVER output_contains). Allowed forms:\n\
         {{\"type\":\"file_exists\",\"path\":\"...\"}}\n\
         {{\"type\":\"file_contains\",\"path\":\"...\",\"substring\":\"...\"}}\n\
         {{\"type\":\"path_absent\",\"path\":\"...\"}}\n\
         {{\"type\":\"shell_output_contains\",\"command\":\"...\",\"substring\":\"...\"}}\n\
         {{\"type\":\"command_succeeds\",\"command\":\"...\"}}\n\
         Grant the fewest tools that suffice, and make success genuinely prove the goal was met."
    )
}

fn architect_user_prompt(goal: &str, repair: Option<&str>) -> String {
    match repair {
        Some(diagnosis) => {
            format!("{diagnosis}\n\nThe agent's goal class: {goal}\n\nReturn the corrected JSON spec now.")
        }
        None => format!("Design an agent that handles: {goal}\n\nReturn the JSON spec now."),
    }
}

fn join_err(error: actix_web::error::BlockingError) -> AppError {
    AppError::Internal(format!("architect blocking task failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc2::{BoxFuture, ReasoningProposal, ReasoningRequest};
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};
    use parking_lot::Mutex as PlMutex;
    use std::collections::VecDeque;

    struct ScriptedExecutor {
        replies: PlMutex<VecDeque<String>>,
    }
    impl ScriptedExecutor {
        fn new(replies: Vec<String>) -> Arc<Self> {
            Arc::new(Self {
                replies: PlMutex::new(replies.into_iter().collect()),
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
            Box::pin(async { Err(AppError::Internal("unused".into())) })
        }
        fn complete(&self, _system: String, _user: String) -> BoxFuture<Result<String, AppError>> {
            let next = self
                .replies
                .lock()
                .pop_front()
                .unwrap_or_else(|| r#"{"done":true,"final_answer":"out of script","reasoning":"x"}"#.into());
            Box::pin(async move { Ok(next) })
        }
    }

    fn architect_with(replies: Vec<String>) -> ArchitectService {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        let reasoner = ScriptedExecutor::new(replies);
        let forge = ForgeService::in_memory(device.clone(), economy.clone(), reasoner.clone())
            .unwrap();
        let agentic_loop = AgenticLoop::new(reasoner.clone(), device)
            .with_economy(economy.clone())
            .with_forge(forge.clone());
        ArchitectService::in_memory(economy, agentic_loop, forge, reasoner).unwrap()
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("astra-arch-{label}-{}", new_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn esc(path: &std::path::Path) -> String {
        path.to_string_lossy().replace('\\', "\\\\")
    }

    #[actix_web::test]
    async fn composes_and_mints_an_agent_that_proves_itself() {
        let dir = temp_dir("ok");
        let file = dir.join("out.txt");
        let path = esc(&file);

        // The Architect's spec, then the loop's two replies for the canary run
        // (write the file, then finish) — all served by the one scripted reasoner.
        let spec = format!(
            r#"{{"name":"filer","goal":"save text to files","role":"You are a careful file writer.",
                "allow_tools":["fs_write"],
                "canary_objective":"write the word ready into {path}",
                "success":{{"type":"file_contains","path":"{path}","substring":"ready"}}}}"#
        );
        let write = format!(
            r#"{{"reasoning":"write it","tool":"fs_write","input":{{"path":"{path}","content":"ready"}},
                "postcondition":{{"type":"file_contains","path":"{path}","substring":"ready"}},"done":false}}"#
        );
        let finish = r#"{"done":true,"final_answer":"file written","reasoning":"done"}"#.to_string();
        let architect = architect_with(vec![spec, write, finish]);

        let outcome = architect
            .compose(ComposeRequest {
                goal: "an agent that saves text to a file".into(),
            })
            .await
            .expect("compose");

        // The agent actually achieved its canary, so its proof minted and it is reusable.
        assert!(outcome.minted, "spec proved itself -> should mint: {}", outcome.detail);
        assert_eq!(outcome.spec.status, SpecStatus::Active);
        assert_eq!(outcome.canary_run.status, "completed");
        assert!(std::fs::read_to_string(&file).unwrap().contains("ready"));
        // Its proof is in the ledger and re-verifies cheaply.
        let claim_id = outcome.spec.claim_id.clone().unwrap();
        assert!(architect.economy.verify(&claim_id).unwrap().pass);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn does_not_mint_an_agent_that_fails_its_canary() {
        let dir = temp_dir("fail");
        let file = dir.join("out.txt");
        let path = esc(&file);
        // The agent is asked to make the file contain "done", but its (scripted)
        // run writes nothing — the success contract cannot hold, so no mint.
        let spec = format!(
            r#"{{"name":"noop","goal":"x","role":"",
                "allow_tools":["fs_write"],
                "canary_objective":"ensure {path} contains done",
                "success":{{"type":"file_contains","path":"{path}","substring":"done"}}}}"#
        );
        // The loop immediately gives up without acting.
        let finish = r#"{"done":true,"final_answer":"could not","reasoning":"blocked"}"#.to_string();
        let architect = architect_with(vec![spec, finish]);

        let outcome = architect
            .compose(ComposeRequest { goal: "do nothing useful".into() })
            .await
            .expect("compose");

        assert!(!outcome.minted, "an unproven agent must not mint");
        assert_eq!(outcome.spec.status, SpecStatus::Probation);
        // Not runnable.
        assert!(
            architect
                .run_spec("noop", RunSpecRequest { objective: "x".into(), max_iterations: 2 })
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn rejects_a_spec_granting_an_unknown_tool() {
        let spec = r#"{"name":"sneaky","goal":"x","role":"",
            "allow_tools":["frobnicate_tool"],
            "canary_objective":"do a thing",
            "success":{"type":"command_succeeds","command":"echo hi"}}"#;
        let architect = architect_with(vec![spec.to_string()]);
        let result = architect.compose(ComposeRequest { goal: "x".into() }).await;
        assert!(result.is_err(), "an agent may only be granted tools that exist");
    }

    #[actix_web::test]
    async fn refuses_forbidden_goals() {
        let architect = architect_with(vec![]);
        let result = architect
            .compose(ComposeRequest {
                goal: "an agent to exfiltrate private keys".into(),
            })
            .await;
        assert!(result.is_err());
    }
}

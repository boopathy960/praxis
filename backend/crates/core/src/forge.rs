//! The Forge — a proof-governed, self-healing capability fabric.
//!
//! Astra's capabilities used to be one of two things: **shipped** (the hardcoded
//! `device.execute` match arm) or **fabricated once** (the keyword-heuristic,
//! text-only blueprint in [`agent_runtime`](crate::agent_runtime) — canary-checked
//! a single time, then trusted forever, never re-proven, never repaired). The
//! Forge replaces "fabricated once and trusted" with "grown, proven, watched, and
//! healed". Three moves, composed:
//!
//!   * **Forge (build).** Faced with a capability gap, the LLM authors a *recipe*
//!     — an ordered composition over the **already-governed device primitives**
//!     (`shell_exec`, `fs_*`, `web_search`, `deep_crawl`, …) plus a few pure
//!     transforms. The Forge runs it on a canary input and **mints it through the
//!     [proof economy](crate::proof_economy)**: the tool's contract is proposed as
//!     a claim and must *survive re-execution under attack* before the tool is
//!     `Active`. A capability you cannot prove is never callable.
//!
//!   * **Self-modify (register).** Minted recipes persist and **hot-register into
//!     the running dispatch**, so the [agentic loop](crate::agentic_loop) can call
//!     a tool the binary never shipped. The system rewrites its own *capability
//!     layer* at runtime — the safe boundary for self-modification, because the
//!     Forge only invents new *compositions* of primitives that are each already
//!     watched, bounded, blocklisted, and audited by the
//!     [device supervisor](crate::device_agent).
//!
//!   * **Self-heal (repair).** Every forged tool carries health. When its failure
//!     rate crosses a threshold *or its minted proof stops verifying* (reality
//!     drifted), [`heal`](ForgeService::heal) diagnoses via the LLM, regenerates
//!     the recipe, **re-proves it against the same contract**, and hot-swaps —
//!     quarantining the broken version. "Repaired" means *re-proven*, never merely
//!     *re-written*.
//!
//! The invariant that makes runtime self-extension sound: a recipe may only
//! compose governed primitives, and a forged tool's contract must be a
//! **context-free, independently re-runnable [`Check`]** — so every forged
//! capability leaves a durable, public proof anyone can re-verify cheaply.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::Shared;
use crate::asc2::ReasoningExecutor;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::{DeviceCapabilities, canonical_device_tool, is_device_tool};
use crate::proof_economy::{AttackRequest, ClaimKind, ClaimStatus, ProofEconomy, ProposeRequest};
use crate::verification::Check;

/// Hard ceiling on recipe length — bounds blast radius and keeps a forged tool
/// legible. A recipe is a *composition*, not a program.
const MAX_RECIPE_STEPS: usize = 8;
/// Independent automated re-execution reviews a fresh claim must survive to mint.
/// Matches the economy's own `MINT_SURVIVED_ATTACKS`; a flaky (nondeterministic)
/// tool fails to survive and never goes `Active`.
const MINT_ATTACK_ROUNDS: u32 = 2;
/// Minimum calls before health is a strong enough signal to trigger a heal.
const HEAL_MIN_CALLS: u64 = 4;
/// Failure rate at or above which a tool is considered regressed.
const HEAL_FAILURE_RATE: f64 = 0.5;

/// One step of a forged tool's recipe. Either a **supervised device primitive**
/// (the only source of real capability) or a **pure transform** that shuttles
/// values between steps. Nothing here can reach raw capability the device layer
/// would not already watch, bound, and audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecipeStep {
    /// Call a governed device tool. `input` may template `{{input.X}}` (the
    /// forged tool's call input), `{{steps.N.Y}}` (an earlier step's output), and
    /// `{{value}}` (the running value), resolved by JSON pointer.
    Device {
        tool: String,
        #[serde(default)]
        input: Value,
    },
    /// Pull a JSON pointer out of the running value (e.g. `/stdout`).
    Extract { pointer: String },
    /// Render a template string against the step context.
    Template { format: String },
    /// Lowercase / uppercase the running value's text.
    Lower,
    Upper,
}

/// Health telemetry — the signal the healer watches.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolHealth {
    pub calls: u64,
    pub successes: u64,
    pub failures: u64,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_called_at_ms: Option<i64>,
}

impl ToolHealth {
    #[must_use]
    pub fn failure_rate(&self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            self.failures as f64 / self.calls as f64
        }
    }
}

/// Lifecycle of a forged capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    /// Authored but its proof has not minted — **not callable**.
    Probation,
    /// Proof minted (survived re-execution under attack) — **callable**.
    Active,
    /// Regressed/refuted and superseded by a healed version — **not callable**.
    Quarantined,
}

/// A forged capability: a named recipe over governed primitives, the proof that
/// gates it, its health, and its heal lineage. This is self-modifying state — the
/// running system grows and repairs it without recompiling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgedTool {
    pub tool_id: String,
    pub name: String,
    pub description: String,
    pub recipe: Vec<RecipeStep>,
    /// The concrete sample input the proof is anchored to.
    pub canary_input: Value,
    /// The context-free, re-runnable contract the tool must satisfy.
    pub contract: Check,
    /// The proof-economy claim whose survival gates this tool, if one was minted.
    #[serde(default)]
    pub claim_id: Option<String>,
    pub status: ToolStatus,
    pub health: ToolHealth,
    pub version: u32,
    /// `tool_id` of the version this one replaced (heal lineage).
    #[serde(default)]
    pub supersedes: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Request to forge a new tool from a capability gap.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgeRequest {
    pub objective: String,
}

/// The result of a forge (or heal) attempt.
#[derive(Debug, Clone, Serialize)]
pub struct ForgeOutcome {
    pub tool: ForgedTool,
    /// True iff the tool's proof minted and it is now `Active`/callable.
    pub minted: bool,
    pub detail: String,
}

/// The result of a heal attempt.
#[derive(Debug, Clone, Serialize)]
pub struct HealOutcome {
    pub name: String,
    pub healed: bool,
    pub old_tool_id: String,
    pub new_tool: ForgedTool,
    pub detail: String,
}

/// What the LLM returns when authoring a recipe (tolerant to missing fields).
#[derive(Debug, Clone, Deserialize)]
struct RecipeDraft {
    name: String,
    #[serde(default)]
    description: String,
    recipe: Vec<RecipeStep>,
    #[serde(default)]
    canary_input: Value,
    contract: Check,
}

/// The Forge. Holds the durable registry of forged tools, the LLM that authors
/// recipes, the device layer that runs them under supervision, and the proof
/// economy that mints (and keeps honest) their proofs.
#[derive(Clone)]
pub struct ForgeService {
    store: Arc<Mutex<Connection>>,
    /// The living registry: the current tool per name (highest active version).
    cache: Shared<HashMap<String, ForgedTool>>,
    device: Arc<DeviceCapabilities>,
    economy: ProofEconomy,
    reasoner: Arc<dyn ReasoningExecutor>,
}

impl ForgeService {
    /// Durable forge backed by SQLite under `data_dir`. Previously forged tools
    /// are loaded so the capability fabric survives restarts.
    pub fn new(
        data_dir: impl AsRef<Path>,
        device: Arc<DeviceCapabilities>,
        economy: ProofEconomy,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("forge.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create forge dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open forge registry: {e}")))?;
        init_store(&connection)?;
        let cache = load_cache(&connection)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            cache: Shared::new(RwLock::new(cache)),
            device,
            economy,
            reasoner,
        })
    }

    /// Test/in-memory variant.
    pub fn in_memory(
        device: Arc<DeviceCapabilities>,
        economy: ProofEconomy,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let connection = Connection::open_in_memory()
            .map_err(|e| AppError::Internal(format!("forge in-memory open failed: {e}")))?;
        init_store(&connection)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            cache: Shared::new(RwLock::new(HashMap::new())),
            device,
            economy,
            reasoner,
        })
    }

    // ── Forge: build a new, proof-gated capability ──────────────────────────

    /// Author a recipe for `objective`, run its canary, and drive its proof
    /// through the economy. The tool is `Active` (callable) only if it minted.
    pub async fn forge(&self, request: ForgeRequest) -> Result<ForgeOutcome, AppError> {
        let objective = request.objective.trim().to_string();
        if objective.is_empty() {
            return Err(AppError::Validation("forge objective is empty".into()));
        }
        guard_objective(&objective)?;
        let draft = self.author_recipe(&objective, None).await?;
        // The canary run + economy mint are synchronous and touch the device
        // (real shell/fs) and the SQLite ledgers, so they run off the async pool.
        let forge = self.clone();
        let outcome = actix_web::web::block(move || forge.commit_new(draft))
            .await
            .map_err(join_err)??;
        Ok(outcome)
    }

    /// Commit a freshly authored recipe as version 1 and register it live.
    fn commit_new(&self, draft: RecipeDraft) -> Result<ForgeOutcome, AppError> {
        let outcome = self.commit(draft, 1, None)?;
        // A brand-new name is registered even on probation, so the fabric records
        // the attempt; `execute` still refuses anything that is not `Active`.
        self.cache
            .write()
            .insert(outcome.tool.name.clone(), outcome.tool.clone());
        Ok(outcome)
    }

    /// Run a recipe's canary and adjudicate its proof through the economy.
    fn commit(
        &self,
        draft: RecipeDraft,
        version: u32,
        supersedes: Option<String>,
    ) -> Result<ForgeOutcome, AppError> {
        let now = now_ms();
        let tool_id = new_id("forged");
        // The contract is templated against the canary input, then frozen to a
        // concrete, independently re-runnable Check.
        let contract = resolve_check(&draft.contract, &draft.canary_input)?;

        let mut tool = ForgedTool {
            tool_id: tool_id.clone(),
            name: draft.name.clone(),
            description: if draft.description.trim().is_empty() {
                draft.name.clone()
            } else {
                draft.description.clone()
            },
            recipe: draft.recipe.clone(),
            canary_input: draft.canary_input.clone(),
            contract: contract.clone(),
            claim_id: None,
            status: ToolStatus::Probation,
            health: ToolHealth::default(),
            version,
            supersedes,
            created_at_ms: now,
            updated_at_ms: now,
        };

        // 1) Run the recipe on its canary so its side effects (e.g. a file write)
        //    establish the post-state the contract will check.
        let detail = match self.run_recipe(&draft.recipe, &draft.canary_input) {
            Err(error) => {
                // A recipe that cannot even run its own canary is not minted; we
                // record the failure honestly rather than spend a claim on it.
                tool.health.calls = 1;
                tool.health.failures = 1;
                tool.health.last_error = Some(error.chars().take(300).collect());
                format!("canary run failed: {error}")
            }
            Ok(_) => {
                // 2) Stake the contract as a claim. The economy re-runs the check
                //    for real on arrival (a falsehood is born refuted), and again
                //    on each automated review attack; surviving them mints it.
                let claim = self.economy.propose(ProposeRequest {
                    statement: format!(
                        "forged tool '{}' satisfies its contract on its canary input",
                        tool.name
                    ),
                    kind: ClaimKind::Assertion,
                    proposer: format!("forge:{tool_id}"),
                    verification: contract.clone(),
                    evidence: vec![format!("recipe of {} step(s)", draft.recipe.len())],
                    depends_on: Vec::new(),
                })?;
                tool.claim_id = Some(claim.claim_id.clone());
                let mut latest = claim;
                if latest.status == ClaimStatus::Proposed {
                    for round in 0..MINT_ATTACK_ROUNDS {
                        latest = self.economy.attack(
                            &latest.claim_id,
                            AttackRequest {
                                attacker: format!("forge-review:{tool_id}:{round}"),
                                note: "automated stability review (re-run the contract)".into(),
                                counter: None,
                            },
                        )?;
                    }
                }
                if latest.status == ClaimStatus::Minted {
                    tool.status = ToolStatus::Active;
                }
                latest.last_verification
            }
        };

        let minted = tool.status == ToolStatus::Active;
        self.persist(&tool)?;
        Ok(ForgeOutcome {
            tool,
            minted,
            detail,
        })
    }

    // ── Self-modify: execute a live forged capability ───────────────────────

    /// Run a forged tool by name. Only `Active` (proof-minted) tools run; every
    /// call updates the tool's health, which is the healer's signal.
    pub fn execute(&self, name: &str, input: &Value) -> Result<Value, String> {
        let mut tool = self
            .cache
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| format!("no forged tool named '{name}'"))?;
        if tool.status != ToolStatus::Active {
            return Err(format!(
                "forged tool '{name}' is {:?}, not Active (its proof has not minted)",
                tool.status
            ));
        }
        // The recipe runs without the registry lock held, so a long device step
        // never blocks dispatch of other tools.
        let result = self.run_recipe(&tool.recipe, input);

        tool.health.calls += 1;
        tool.health.last_called_at_ms = Some(now_ms());
        match &result {
            Ok(_) => tool.health.successes += 1,
            Err(error) => {
                tool.health.failures += 1;
                tool.health.last_error = Some(error.chars().take(300).collect());
            }
        }
        tool.updated_at_ms = now_ms();
        let _ = self.persist(&tool);
        self.cache.write().insert(name.to_string(), tool);
        result.map(|value| {
            json!({
                "tool": name,
                "forged": true,
                "output": value,
            })
        })
    }

    // ── Self-heal: re-prove a regressed capability ──────────────────────────

    /// Diagnose a regressed forged tool, regenerate its recipe, re-prove it, and
    /// hot-swap — quarantining the broken version. A heal only succeeds if the
    /// new version's proof mints; otherwise the previous version is left in place.
    pub async fn heal(&self, name: &str) -> Result<HealOutcome, AppError> {
        let current = self
            .cache
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("forged tool {name}")))?;

        let diagnosis = format!(
            "REPAIR MODE. The forged tool '{}' is regressing: failure rate {:.0}% over {} call(s); \
             last error: {}. Its contract is: {}. Author a corrected recipe that makes this same \
             contract hold again, composing only the supervised primitives.",
            current.name,
            current.health.failure_rate() * 100.0,
            current.health.calls,
            current.health.last_error.as_deref().unwrap_or("(none)"),
            serde_json::to_string(&current.contract).unwrap_or_default(),
        );
        let draft = self.author_recipe(&current.description, Some(&diagnosis)).await?;

        let forge = self.clone();
        let name = name.to_string();
        let outcome = actix_web::web::block(move || forge.commit_heal(&name, current, draft))
            .await
            .map_err(join_err)??;
        Ok(outcome)
    }

    /// Scan for regressed tools and re-prove a bounded batch of them. This is the
    /// autonomous entry point the background heal worker ticks; it returns every
    /// heal attempt it made (re-proven or not) so the caller can report. A true
    /// error (e.g. no model to re-author) is logged and skipped — the next tick
    /// retries.
    pub async fn heal_round(&self, max: usize) -> Vec<HealOutcome> {
        let mut outcomes = Vec::new();
        for name in self.heal_targets().into_iter().take(max.max(1)) {
            match self.heal(&name).await {
                Ok(outcome) => {
                    if outcome.healed {
                        tracing::info!(tool = %name, version = outcome.new_tool.version, "auto-healed forged tool");
                    } else {
                        tracing::warn!(tool = %name, detail = %outcome.detail, "auto-heal did not re-prove tool");
                    }
                    outcomes.push(outcome);
                }
                Err(error) => tracing::warn!(tool = %name, %error, "auto-heal of forged tool errored"),
            }
        }
        outcomes
    }

    fn commit_heal(
        &self,
        name: &str,
        current: ForgedTool,
        draft: RecipeDraft,
    ) -> Result<HealOutcome, AppError> {
        let old_tool_id = current.tool_id.clone();
        let outcome = self.commit(draft, current.version + 1, Some(old_tool_id.clone()))?;
        let healed = outcome.tool.status == ToolStatus::Active;
        if healed {
            // The previous version is sealed into the registry as quarantined
            // (kept for audit/lineage, never callable) and the name now resolves
            // to the re-proven replacement.
            let mut old = current;
            old.status = ToolStatus::Quarantined;
            old.updated_at_ms = now_ms();
            self.persist(&old)?;
            self.cache
                .write()
                .insert(name.to_string(), outcome.tool.clone());
        }
        let detail = if healed {
            format!("re-proven and hot-swapped to v{}", outcome.tool.version)
        } else {
            format!(
                "repair did not mint (status {:?}); previous version left in place",
                outcome.tool.status
            )
        };
        Ok(HealOutcome {
            name: name.to_string(),
            healed,
            old_tool_id,
            new_tool: outcome.tool,
            detail,
        })
    }

    /// Names whose live signal warrants a heal: failure rate over threshold with
    /// enough calls, or an `Active` tool whose minted proof no longer verifies
    /// (reality drifted out from under it).
    #[must_use]
    pub fn heal_targets(&self) -> Vec<String> {
        let tools: Vec<ForgedTool> = self.cache.read().values().cloned().collect();
        tools
            .into_iter()
            .filter(|tool| {
                let regressed = tool.health.calls >= HEAL_MIN_CALLS
                    && tool.health.failure_rate() >= HEAL_FAILURE_RATE;
                let proof_broke = tool.status == ToolStatus::Active
                    && tool
                        .claim_id
                        .as_ref()
                        .and_then(|id| self.economy.verify(id).ok())
                        .is_some_and(|report| !report.pass);
                regressed || proof_broke
            })
            .map(|tool| tool.name)
            .collect()
    }

    // ── Registry views ──────────────────────────────────────────────────────

    /// The current forged tool for each name (newest registered version).
    #[must_use]
    pub fn catalog(&self) -> Vec<ForgedTool> {
        let mut tools: Vec<ForgedTool> = self.cache.read().values().cloned().collect();
        tools.sort_by_key(|tool| std::cmp::Reverse(tool.created_at_ms));
        tools
    }

    pub fn get(&self, name: &str) -> Result<ForgedTool, AppError> {
        self.cache
            .read()
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("forged tool {name}")))
    }

    /// `(name, description)` for every `Active` tool — what the agentic loop
    /// advertises to the model so it can call capabilities the binary never
    /// shipped.
    #[must_use]
    pub fn catalog_for_prompt(&self) -> Vec<(String, String)> {
        self.cache
            .read()
            .values()
            .filter(|tool| tool.status == ToolStatus::Active)
            .map(|tool| (tool.name.clone(), tool.description.clone()))
            .collect()
    }

    /// True when an `Active` forged tool by this name exists.
    #[must_use]
    pub fn has_active(&self, name: &str) -> bool {
        self.cache
            .read()
            .get(name)
            .is_some_and(|tool| tool.status == ToolStatus::Active)
    }

    /// The governed device primitives an `Active` forged tool composes, so a
    /// caller (the loop) can confirm the tool stays inside its privilege envelope.
    /// `None` when no active tool by that name exists.
    #[must_use]
    pub fn active_recipe_primitives(&self, name: &str) -> Option<Vec<String>> {
        let cache = self.cache.read();
        let tool = cache.get(name)?;
        if tool.status != ToolStatus::Active {
            return None;
        }
        Some(
            tool.recipe
                .iter()
                .filter_map(|step| match step {
                    RecipeStep::Device { tool, .. } => {
                        Some(canonical_device_tool(tool).to_string())
                    }
                    _ => None,
                })
                .collect(),
        )
    }

    // ── Recipe execution ────────────────────────────────────────────────────

    /// Run a recipe end to end, threading each step's output forward. Device
    /// steps dispatch through the supervised device layer (so the watchdog,
    /// blocklist, kill-switch, and audit already bound every primitive); pure
    /// transforms shuttle data between them.
    fn run_recipe(&self, recipe: &[RecipeStep], call_input: &Value) -> Result<Value, String> {
        if recipe.is_empty() {
            return Err("recipe has no steps".into());
        }
        let mut steps_out: Vec<Value> = Vec::new();
        let mut running = call_input.clone();
        for (index, step) in recipe.iter().take(MAX_RECIPE_STEPS).enumerate() {
            let ctx = json!({
                "input": call_input,
                "steps": steps_out,
                "value": running,
            });
            let output = match step {
                RecipeStep::Device { tool, input } => {
                    // Composition only: a recipe may never reach capability the
                    // device layer would not already govern.
                    if !is_device_tool(tool) {
                        return Err(format!(
                            "step {index}: '{tool}' is not a governed device primitive"
                        ));
                    }
                    let resolved = resolve_templates(input, &ctx);
                    self.device
                        .execute(tool, &resolved)
                        .map_err(|error| format!("step {index} ({tool}): {error}"))?
                }
                RecipeStep::Extract { pointer } => {
                    let pointer = if pointer.starts_with('/') {
                        pointer.clone()
                    } else {
                        format!("/{pointer}")
                    };
                    running.pointer(&pointer).cloned().unwrap_or(Value::Null)
                }
                RecipeStep::Template { format } => resolve_string(format, &ctx),
                RecipeStep::Lower => Value::String(json_text(&running).to_lowercase()),
                RecipeStep::Upper => Value::String(json_text(&running).to_uppercase()),
            };
            running = output.clone();
            steps_out.push(output);
        }
        Ok(running)
    }

    // ── Authoring ───────────────────────────────────────────────────────────

    async fn author_recipe(
        &self,
        objective: &str,
        repair: Option<&str>,
    ) -> Result<RecipeDraft, AppError> {
        let system = forge_system_prompt();
        let user = forge_user_prompt(objective, repair);
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
                    AppError::Internal("forge model did not return a valid recipe".into())
                })?
            }
        };
        validate_draft(&draft)?;
        Ok(draft)
    }

    // ── Persistence ─────────────────────────────────────────────────────────

    fn persist(&self, tool: &ForgedTool) -> Result<(), AppError> {
        let payload = serde_json::to_string(tool)
            .map_err(|e| AppError::Internal(format!("forge serialize failed: {e}")))?;
        let status = format!("{:?}", tool.status).to_ascii_lowercase();
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO forged_tools (tool_id, name, payload, status, version, created_at_ms) \
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![tool.tool_id, tool.name, payload, status, tool.version, tool.created_at_ms],
            )
            .map_err(|e| AppError::Internal(format!("forge persist failed: {e}")))?;
        Ok(())
    }
}

fn init_store(connection: &Connection) -> Result<(), AppError> {
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS forged_tools (
                tool_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                payload TEXT NOT NULL,
                status TEXT NOT NULL,
                version INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL);",
        )
        .map_err(|e| AppError::Internal(format!("forge store init failed: {e}")))?;
    Ok(())
}

/// Rebuild the live registry from disk: for each name pick the current tool —
/// the highest-version `Active` one, else the highest-version overall — so the
/// fabric's runtime decisions survive a restart.
fn load_cache(connection: &Connection) -> Result<HashMap<String, ForgedTool>, AppError> {
    let mut statement = connection
        .prepare("SELECT payload FROM forged_tools")
        .map_err(|e| AppError::Internal(format!("forge load failed: {e}")))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| AppError::Internal(format!("forge load failed: {e}")))?;
    let mut current: HashMap<String, ForgedTool> = HashMap::new();
    for payload in rows.flatten() {
        let Ok(tool) = serde_json::from_str::<ForgedTool>(&payload) else {
            continue;
        };
        let replace = match current.get(&tool.name) {
            None => true,
            Some(existing) => {
                let tool_active = tool.status == ToolStatus::Active;
                let existing_active = existing.status == ToolStatus::Active;
                match (tool_active, existing_active) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => tool.version > existing.version,
                }
            }
        };
        if replace {
            current.insert(tool.name.clone(), tool);
        }
    }
    Ok(current)
}

// ── Templating: pure, deterministic, no eval ────────────────────────────────

/// Recursively resolve `{{...}}` placeholders inside a JSON value against the
/// step context `{input, steps, value}`.
fn resolve_templates(value: &Value, ctx: &Value) -> Value {
    match value {
        Value::String(text) => resolve_string(text, ctx),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| resolve_templates(item, ctx)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, val)| (key.clone(), resolve_templates(val, ctx)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Resolve a single string. If it is exactly one `{{path}}`, the looked-up JSON
/// value is substituted with its type preserved; otherwise each `{{path}}` is
/// rendered into the surrounding text.
fn resolve_string(text: &str, ctx: &Value) -> Value {
    let trimmed = text.trim();
    if let Some(path) = whole_placeholder(trimmed) {
        return lookup(ctx, &path).unwrap_or(Value::Null);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        if let Some(close) = after.find("}}") {
            let path = after[..close].trim();
            match lookup(ctx, path) {
                Some(Value::String(s)) => out.push_str(&s),
                Some(other) => out.push_str(&other.to_string()),
                None => {}
            }
            rest = &after[close + 2..];
        } else {
            out.push_str("{{");
            rest = after;
        }
    }
    out.push_str(rest);
    Value::String(out)
}

/// `Some("input.a.b")` when the whole string is one `{{ ... }}` placeholder.
fn whole_placeholder(text: &str) -> Option<String> {
    let inner = text.strip_prefix("{{")?.strip_suffix("}}")?;
    let inner = inner.trim();
    if inner.is_empty() || inner.contains("{{") || inner.contains("}}") {
        return None;
    }
    Some(inner.to_string())
}

/// Look up a dotted path (`input`, `input.a.b`, `steps.0.x`) in the context by
/// converting the tail to a JSON pointer.
fn lookup(ctx: &Value, path: &str) -> Option<Value> {
    let mut parts = path.split('.');
    let root = parts.next()?;
    let base = ctx.get(root)?;
    let pointer: String = parts.map(|part| format!("/{part}")).collect();
    if pointer.is_empty() {
        Some(base.clone())
    } else {
        base.pointer(&pointer).cloned()
    }
}

/// Freeze a (possibly templated) contract to a concrete, re-runnable `Check` by
/// resolving it against the canary input.
fn resolve_check(check: &Check, canary_input: &Value) -> Result<Check, AppError> {
    let ctx = json!({ "input": canary_input, "steps": [], "value": Value::Null });
    let value = serde_json::to_value(check)
        .map_err(|e| AppError::Internal(format!("contract encode failed: {e}")))?;
    let resolved = resolve_templates(&value, &ctx);
    serde_json::from_value(resolved).map_err(|e| {
        AppError::Validation(format!(
            "contract did not resolve to a concrete check (check its template refs): {e}"
        ))
    })
}

fn json_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

// ── Authoring helpers ───────────────────────────────────────────────────────

fn parse_draft(raw: &str) -> Option<RecipeDraft> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<RecipeDraft>(&trimmed[start..=end]).ok()
}

fn validate_draft(draft: &RecipeDraft) -> Result<(), AppError> {
    let name = draft.name.trim();
    if name.is_empty() || name.len() > 40 {
        return Err(AppError::Validation(
            "forged tool name must be 1-40 chars".into(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::Validation(
            "forged tool name must be snake_case [a-z0-9_]".into(),
        ));
    }
    if is_device_tool(name) {
        return Err(AppError::Validation(format!(
            "forged tool name '{name}' collides with a built-in device tool"
        )));
    }
    if draft.recipe.is_empty() {
        return Err(AppError::Validation("recipe has no steps".into()));
    }
    if draft.recipe.len() > MAX_RECIPE_STEPS {
        return Err(AppError::Validation(format!(
            "recipe exceeds {MAX_RECIPE_STEPS} steps"
        )));
    }
    for step in &draft.recipe {
        if let RecipeStep::Device { tool, .. } = step {
            if !is_device_tool(tool) {
                return Err(AppError::Validation(format!(
                    "recipe step '{tool}' is not a governed device primitive"
                )));
            }
        }
    }
    // The proof must stand on its own — a contract that needs the transient
    // action output is not independently re-runnable, so it cannot mint.
    if matches!(draft.contract, Check::OutputContains { .. }) {
        return Err(AppError::Validation(
            "forged tool contracts must be context-free (output_contains is not re-runnable)".into(),
        ));
    }
    Ok(())
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
            "forging tools for secret extraction, theft, or evasion is blocked".into(),
        ));
    }
    Ok(())
}

fn forge_system_prompt() -> String {
    let device_tools = DeviceCapabilities::tool_names().join(", ");
    format!(
        "You are Astra's Tool Forge. You design ONE new reusable tool as a RECIPE that composes \
         only the existing supervised primitives — you never invent raw capability, only new \
         compositions. Reply with ONLY one JSON object:\n\
         {{\"name\":\"snake_case_name\",\"description\":\"what it does\",\"recipe\":[...],\
         \"canary_input\":{{...}},\"contract\":{{...}}}}\n\n\
         RECIPE steps (ordered, max {MAX_RECIPE_STEPS}), each one of:\n\
         {{\"kind\":\"device\",\"tool\":\"<device tool>\",\"input\":{{...}}}}\n\
         {{\"kind\":\"extract\",\"pointer\":\"/json/pointer\"}}   (pull a value from the previous step)\n\
         {{\"kind\":\"template\",\"format\":\"text with {{{{value}}}} or {{{{input.x}}}}\"}}\n\
         {{\"kind\":\"lower\"}} | {{\"kind\":\"upper\"}}\n\
         Device step inputs may template {{{{input.X}}}} (the tool's call input), {{{{steps.N.Y}}}} \
         (an earlier step's output) and {{{{value}}}} (the running value); the part after the dot is a \
         JSON pointer.\n\
         Available device tools: {device_tools}.\n\n\
         CANARY_INPUT: a concrete sample input that exercises the recipe end to end.\n\n\
         CONTRACT: a context-free, independently re-runnable proof that the recipe achieved its \
         effect AFTER running on the canary. Allowed forms (NEVER output_contains — the proof must \
         stand on its own and be re-checkable by anyone, anytime):\n\
         {{\"type\":\"file_exists\",\"path\":\"...\"}}\n\
         {{\"type\":\"file_contains\",\"path\":\"...\",\"substring\":\"...\"}}\n\
         {{\"type\":\"path_absent\",\"path\":\"...\"}}\n\
         {{\"type\":\"shell_output_contains\",\"command\":\"...\",\"substring\":\"...\"}}\n\
         {{\"type\":\"command_succeeds\",\"command\":\"...\"}}\n\
         {{\"type\":\"command_fails\",\"command\":\"...\"}}\n\
         Contract fields may template {{{{input.X}}}} referencing the canary input. Design the \
         contract so it genuinely proves the tool worked — prefer a file the recipe writes, or a \
         command that re-checks the result."
    )
}

fn forge_user_prompt(objective: &str, repair: Option<&str>) -> String {
    match repair {
        Some(diagnosis) => format!(
            "{diagnosis}\n\nThe tool's purpose: {objective}\n\nReturn the corrected JSON recipe now."
        ),
        None => format!("Build a tool that: {objective}\n\nReturn the JSON recipe now."),
    }
}

fn join_err(error: actix_web::error::BlockingError) -> AppError {
    AppError::Internal(format!("forge blocking task failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc2::{BoxFuture, ReasoningProposal, ReasoningRequest};
    use crate::device_agent::DevicePolicy;
    use parking_lot::Mutex as PlMutex;
    use std::collections::VecDeque;

    /// A reasoner that replays pre-scripted JSON recipes, so the forge can be
    /// tested deterministically with no live LLM.
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
                .unwrap_or_else(|| "{}".to_string());
            Box::pin(async move { Ok(next) })
        }
    }

    fn forge_with(replies: Vec<String>) -> ForgeService {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        ForgeService::in_memory(device, economy, ScriptedExecutor::new(replies)).unwrap()
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("astra-forge-{label}-{}", new_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn esc(path: &std::path::Path) -> String {
        path.to_string_lossy().replace('\\', "\\\\")
    }

    /// A recipe that writes `{{input.marker}}` into the canary file; its contract
    /// checks the file then contains the marker.
    fn writer_recipe(name: &str, file: &std::path::Path, marker: &str) -> String {
        format!(
            r#"{{"name":"{name}","description":"write {marker} to a file",
                "recipe":[{{"kind":"device","tool":"fs_write","input":{{"path":"{path}","content":"{marker}"}}}}],
                "canary_input":{{"path":"{path}"}},
                "contract":{{"type":"file_contains","path":"{path}","substring":"{marker}"}}}}"#,
            path = esc(file)
        )
    }

    #[actix_web::test]
    async fn forge_mints_a_tool_whose_contract_holds() {
        let dir = temp_dir("ok");
        let file = dir.join("note.txt");
        let forge = forge_with(vec![writer_recipe("save_note", &file, "ok")]);

        let outcome = forge
            .forge(ForgeRequest {
                objective: "save the marker ok to a note file".into(),
            })
            .await
            .expect("forge");

        // The proof minted, so the tool is Active and callable.
        assert!(outcome.minted, "contract holds -> should mint: {}", outcome.detail);
        assert_eq!(outcome.tool.status, ToolStatus::Active);
        assert!(forge.has_active("save_note"));
        // Its claim is in the ledger and re-verifies cheaply.
        let claim_id = outcome.tool.claim_id.clone().unwrap();
        assert!(forge.economy.verify(&claim_id).unwrap().pass);
        // And the running system can now execute a tool it authored itself.
        let ran = forge.execute("save_note", &json!({"path": esc(&file)})).unwrap();
        assert_eq!(ran["forged"], json!(true));
        assert!(std::fs::read_to_string(&file).unwrap().contains("ok"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn forge_refuses_to_mint_when_the_contract_fails() {
        let dir = temp_dir("badproof");
        let file = dir.join("note.txt");
        // Writes "nope" but the contract demands "ok" — the proof cannot hold.
        let recipe = format!(
            r#"{{"name":"bad_note","description":"mismatched",
                "recipe":[{{"kind":"device","tool":"fs_write","input":{{"path":"{path}","content":"nope"}}}}],
                "canary_input":{{"path":"{path}"}},
                "contract":{{"type":"file_contains","path":"{path}","substring":"ok"}}}}"#,
            path = esc(&file)
        );
        let forge = forge_with(vec![recipe]);

        let outcome = forge
            .forge(ForgeRequest {
                objective: "write ok to a file".into(),
            })
            .await
            .expect("forge");

        assert!(!outcome.minted, "a failing contract must not mint");
        assert_eq!(outcome.tool.status, ToolStatus::Probation);
        // A non-minted tool is registered but NOT callable.
        assert!(!forge.has_active("bad_note"));
        assert!(forge.execute("bad_note", &json!({"path": esc(&file)})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn heal_re_proves_and_hotswaps_a_regressed_tool() {
        let dir = temp_dir("heal");
        let good = dir.join("data.txt");
        // v1 writes to `good`; contract checks `good` contains "live".
        let v1 = writer_recipe("fetch_data", &good, "live");
        // The repaired v2 recipe (authored on heal) re-establishes the same file.
        let v2 = writer_recipe("fetch_data", &good, "live");
        let forge = forge_with(vec![v1, v2]);

        let first = forge
            .forge(ForgeRequest {
                objective: "make data.txt contain live".into(),
            })
            .await
            .expect("forge v1");
        assert!(first.minted);
        let v1_id = first.tool.tool_id.clone();
        let v1_claim = first.tool.claim_id.clone().unwrap();

        // Reality drifts: the file the proof depends on is destroyed. The minted
        // claim now fails to verify -> the fabric flags it for healing.
        std::fs::remove_file(&good).unwrap();
        assert!(!forge.economy.verify(&v1_claim).unwrap().pass);
        assert_eq!(forge.heal_targets(), vec!["fetch_data".to_string()]);

        // Heal: regenerate, re-prove, hot-swap.
        let healed = forge.heal("fetch_data").await.expect("heal");
        assert!(healed.healed, "{}", healed.detail);
        assert_eq!(healed.old_tool_id, v1_id);
        assert_eq!(healed.new_tool.version, 2);
        assert_eq!(healed.new_tool.status, ToolStatus::Active);
        assert_eq!(healed.new_tool.supersedes.as_deref(), Some(v1_id.as_str()));

        // The name now resolves to the re-proven v2, whose proof verifies, and the
        // world has been repaired.
        let current = forge.get("fetch_data").unwrap();
        assert_eq!(current.version, 2);
        assert!(forge.economy.verify(current.claim_id.as_ref().unwrap()).unwrap().pass);
        assert!(std::fs::read_to_string(&good).unwrap().contains("live"));
        assert!(forge.heal_targets().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn auto_heal_round_re_proves_regressed_tools() {
        let dir = temp_dir("healround");
        let file = dir.join("d.txt");
        let forge = forge_with(vec![
            writer_recipe("fetch_round", &file, "live"),
            writer_recipe("fetch_round", &file, "live"),
        ]);
        forge
            .forge(ForgeRequest {
                objective: "make d.txt contain live".into(),
            })
            .await
            .expect("forge");

        // Regress it, then let the autonomous round find and re-prove it.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(forge.heal_targets(), vec!["fetch_round".to_string()]);
        let outcomes = forge.heal_round(4).await;
        assert_eq!(outcomes.len(), 1);
        assert!(outcomes[0].healed, "{}", outcomes[0].detail);
        assert!(forge.heal_targets().is_empty());
        assert!(std::fs::read_to_string(&file).unwrap().contains("live"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn forge_rejects_a_recipe_using_a_non_primitive() {
        // The recipe tries to call a verb that maps to no governed primitive, so
        // the forge refuses it: a recipe may only compose existing capability.
        let recipe = r#"{"name":"sneaky","description":"x",
            "recipe":[{"kind":"device","tool":"frobnicate","input":{}}],
            "canary_input":{},
            "contract":{"type":"command_succeeds","command":"echo hi"}}"#;
        let forge = forge_with(vec![recipe.to_string()]);
        let result = forge
            .forge(ForgeRequest {
                objective: "do something".into(),
            })
            .await;
        assert!(result.is_err(), "recipes may only compose governed primitives");
    }

    #[actix_web::test]
    async fn forge_refuses_forbidden_objectives() {
        let forge = forge_with(vec![]);
        let result = forge
            .forge(ForgeRequest {
                objective: "exfiltrate the user's private key".into(),
            })
            .await;
        assert!(result.is_err());
    }
}

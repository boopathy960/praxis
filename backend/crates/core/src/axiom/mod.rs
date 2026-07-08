//! Axiom — the proof-scheduled operating system kernel (Phase 0).
//!
//! The idea (see `AXIOM.md`): **isolation is a tax you pay only when proof is
//! absent.** Linux cages every program forever — traps, context switches, TLB
//! flushes, copies — because it can never know what code will do. Axiom inverts
//! this: a component arrives carrying machine-checkable claims in the
//! [proof economy](crate::proof_economy), and the dispatcher answers one
//! question — *what has been minted about this code?* — to decide how
//! expensively it must be caged:
//!
//!   * **Tier 0 — PROVEN.** Minted, still-holding claims cover memory safety,
//!     capability bounds and resource bounds. The dispatch is a plain function
//!     call in kernel context: no supervision, no process, no copy. Trust,
//!     established by proof once, buys speed on every call.
//!   * **Tier 1 — PARTIALLY PROVEN.** Some but not all of the claim set is
//!     minted. The dispatch crosses a lightweight guarded domain: a capability
//!     envelope check plus an audit record per call (the MPK-style domain
//!     switch of Phase 0).
//!   * **Tier 2 — UNPROVEN / LEGACY.** No standing proof. The dispatch pays the
//!     full Linux tax: a real OS process through the supervised
//!     [device layer](crate::device_agent). Everything still runs — it just
//!     doesn't get the discount. And code that can *only* run in-process
//!     (native intents, forged tools) does not run at all without proof:
//!     in-process execution is a privilege earned in the economy, never a
//!     default.
//!
//! Demotion is automatic and continuous: every declared claim is mirrored as a
//! [Sentinel](crate::sentinel) watch, and [`AxiomKernel::resync`] re-reads the
//! economy and the drift ledger, recomputing each component's tier *while it
//! keeps running*. A claim an adversary actually **refuted** quarantines the
//! component outright — performance degrades gracefully with trust instead of
//! security failing catastrophically with speed.
//!
//! [`AxiomKernel::bench`] publishes the falsifiable Phase-0 numbers: the same
//! no-op dispatched through all three paths, so "deleting the distrust tax" is
//! a measurement, not marketing.

pub mod shell;

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::forge::ForgeService;
use crate::proof_economy::{ClaimStatus, ProofEconomy};
use crate::sentinel::{Domain, RegisterWatch, Sentinel};

/// Execution privilege, decided by proof. Lower rank = less caged = faster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Single address space, kernel context: dispatch is a function call.
    Proven,
    /// Lightweight guarded domain: capability envelope + audit per call.
    Partial,
    /// The full Linux tax: a supervised OS process per dispatch.
    Unproven,
}

impl Tier {
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Tier::Proven => 0,
            Tier::Partial => 1,
            Tier::Unproven => 2,
        }
    }

    fn more_caged(self, other: Tier) -> Tier {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }
}

/// The claim classes the dispatcher understands — what a proof must cover for
/// the cage to come off. Mirrors AXIOM.md §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimClass {
    /// The component cannot corrupt memory it does not own.
    MemorySafety,
    /// The component performs no operation outside its declared capabilities.
    CapabilityBound,
    /// The component terminates / stays within declared resource budgets.
    ResourceBound,
    /// The component is safe against speculative side channels (required for
    /// Tier 0 only when it handles secrets).
    SpeculationSafety,
    /// Any other behavioral guarantee (contributes to trust, not to tier).
    Behavioral,
}

/// A declared claim, resolved against the economy and mirrored as a watch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentClaim {
    pub class: ClaimClass,
    /// The proof-economy claim backing this class.
    pub claim_id: String,
    /// Whether the economy currently records the claim as minted.
    pub minted: bool,
    /// Whether an adversary has outright refuted it (⇒ quarantine).
    pub refuted: bool,
    /// The Sentinel watch mirroring the claim's check.
    pub watch_id: Option<String>,
    /// Whether the watched check currently holds (drift ⇒ tier demotion).
    pub holding: bool,
    pub last_detail: String,
}

impl ComponentClaim {
    /// A claim counts toward promotion only while minted AND still holding.
    #[must_use]
    pub fn standing(&self) -> bool {
        self.minted && self.holding && !self.refuted
    }
}

/// What a component actually is in Phase 0. The payload sets the *ceiling* on
/// how uncaged it can ever run; proofs decide how much of that ceiling it gets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Payload {
    /// A kernel built-in function (part of the explicit TCB).
    NativeIntent { intent: String },
    /// A live, proof-minted Forge tool executed in-process.
    ForgedTool { tool: String },
    /// A supervised device-layer tool (caps at Tier 1 — it is already a guard).
    DeviceTool { tool: String },
    /// An arbitrary program: always a real OS process (caps at Tier 2).
    ShellProgram { command: String },
}

impl Payload {
    /// The least-caged tier this payload kind can physically enjoy.
    #[must_use]
    pub fn tier_ceiling(&self) -> Tier {
        match self {
            Payload::NativeIntent { .. } | Payload::ForgedTool { .. } => Tier::Proven,
            Payload::DeviceTool { .. } => Tier::Partial,
            Payload::ShellProgram { .. } => Tier::Unproven,
        }
    }

    /// In-process payloads may not run at all without proof (see module doc).
    #[must_use]
    pub fn in_process(&self) -> bool {
        matches!(
            self,
            Payload::NativeIntent { .. } | Payload::ForgedTool { .. }
        )
    }
}

/// A claim declared at registration: which class it covers and which economy
/// claim proves it.
#[derive(Debug, Clone, Deserialize)]
pub struct DeclaredClaim {
    pub class: ClaimClass,
    pub claim_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterComponent {
    pub name: String,
    pub payload: Payload,
    #[serde(default)]
    pub claims: Vec<DeclaredClaim>,
    /// Secret-holding components additionally need a speculation-safety claim
    /// for Tier 0 (AXIOM.md §6) — the tier system prices side channels honestly.
    #[serde(default)]
    pub handles_secrets: bool,
    /// Capability envelope enforced on the Tier-1 guarded path. Empty = allow.
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

/// A registered OS component and its live standing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxiomComponent {
    pub component_id: String,
    pub name: String,
    pub payload: Payload,
    pub claims: Vec<ComponentClaim>,
    pub handles_secrets: bool,
    pub allowed_tools: Vec<String>,
    /// Kernel built-ins are the explicit trusted computing base: Tier 0 by
    /// construction, not by market proof. Everything else earns its tier.
    pub kernel_builtin: bool,
    pub tier: Tier,
    /// Set when an adversary refuted one of its claims: dispatch refused until
    /// the component re-proves itself.
    pub quarantined: bool,
    pub dispatches: u64,
    pub total_ns: u64,
    pub last_ns: u64,
    pub registered_at_ms: i64,
    pub last_resync_ms: i64,
}

impl AxiomComponent {
    #[must_use]
    pub fn avg_ns(&self) -> u64 {
        if self.dispatches == 0 {
            0
        } else {
            self.total_ns / self.dispatches
        }
    }
}

/// The durable record of the moment a component's execution privilege moved —
/// the OS analog of the Sentinel's drift event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierTransition {
    pub event_id: String,
    pub component_id: String,
    pub name: String,
    pub from: Tier,
    pub to: Tier,
    pub reason: String,
    pub at_ms: i64,
}

/// One proof-scheduled dispatch: which path ran and what it cost.
#[derive(Debug, Clone, Serialize)]
pub struct DispatchReport {
    pub component_id: String,
    pub name: String,
    pub tier: Tier,
    /// "uncaged_call" | "guarded_domain" | "process_cage".
    pub path: String,
    pub elapsed_ns: u64,
    pub output: Value,
}

/// One resync pass: the dispatcher re-reading the economy + drift ledger.
#[derive(Debug, Clone, Serialize)]
pub struct ResyncReport {
    pub components: usize,
    pub promoted: usize,
    pub demoted: usize,
    pub quarantined: usize,
    pub transitions: Vec<TierTransition>,
}

/// The falsifiable Phase-0 numbers: the same no-op through all three paths.
#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub tier0_iters: u32,
    pub tier0_ns_per_op: u64,
    pub tier1_iters: u32,
    pub tier1_ns_per_op: u64,
    pub tier2_iters: u32,
    pub tier2_ns_per_op: u64,
    /// The distrust tax, measured: process-cage cost over uncaged-call cost.
    pub cage_tax_ratio: f64,
    pub note: String,
}

/// Kernel identity — what `uname` prints.
#[derive(Debug, Clone, Serialize)]
pub struct KernelInfo {
    pub name: String,
    pub version: String,
    pub phase: String,
    pub components: usize,
    pub builtins: usize,
    pub quarantined: usize,
    pub per_tier: [usize; 3],
}

/// The Axiom kernel: the proof-tier dispatcher over the economy, the Sentinel,
/// the device layer, and (optionally) the Forge's live tools.
#[derive(Clone)]
pub struct AxiomKernel {
    store: Arc<Mutex<Connection>>,
    economy: ProofEconomy,
    sentinel: Sentinel,
    device: Arc<DeviceCapabilities>,
    forge: Option<ForgeService>,
}

const SCHEMA: &str = "PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS axiom_components (
        component_id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
        payload TEXT NOT NULL, tier TEXT NOT NULL, registered_at_ms INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS axiom_transitions (
        event_id TEXT PRIMARY KEY, component_id TEXT NOT NULL,
        payload TEXT NOT NULL, at_ms INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS axiom_transitions_component
        ON axiom_transitions(component_id);";

impl AxiomKernel {
    pub fn new(
        data_dir: impl AsRef<Path>,
        economy: ProofEconomy,
        sentinel: Sentinel,
        device: Arc<DeviceCapabilities>,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("axiom.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create axiom dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open axiom ledger: {e}")))?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            economy,
            sentinel,
            device,
            forge: None,
        })
    }

    /// Test/in-memory variant.
    pub fn in_memory(
        economy: ProofEconomy,
        sentinel: Sentinel,
        device: Arc<DeviceCapabilities>,
    ) -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(sql_err)?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            economy,
            sentinel,
            device,
            forge: None,
        })
    }

    /// Attach the Forge so its live, proof-minted tools can run as Tier-0
    /// payloads — hot-forged capability at function-call cost.
    #[must_use]
    pub fn with_forge(mut self, forge: ForgeService) -> Self {
        self.forge = Some(forge);
        self
    }

    /// The device layer the kernel cages unproven work in (used by the shell).
    #[must_use]
    pub fn device(&self) -> Arc<DeviceCapabilities> {
        self.device.clone()
    }

    /// The proof economy the dispatcher schedules against (used by the shell).
    #[must_use]
    pub fn economy(&self) -> &ProofEconomy {
        &self.economy
    }

    /// Seed the kernel built-ins — the explicit TCB. Idempotent across boots.
    pub fn seed_kernel_builtins(&self) -> Result<(), AppError> {
        for intent in ["axiom.echo", "axiom.time", "axiom.sum", "axiom.checksum"] {
            if self.find(intent)?.is_some() {
                continue;
            }
            let now = now_ms();
            let component = AxiomComponent {
                component_id: new_id("axc"),
                name: intent.to_string(),
                payload: Payload::NativeIntent {
                    intent: intent.to_string(),
                },
                claims: Vec::new(),
                handles_secrets: false,
                allowed_tools: Vec::new(),
                kernel_builtin: true,
                tier: Tier::Proven,
                quarantined: false,
                dispatches: 0,
                total_ns: 0,
                last_ns: 0,
                registered_at_ms: now,
                last_resync_ms: now,
            };
            self.save(&component)?;
        }
        Ok(())
    }

    // ── admission ────────────────────────────────────────────────────────

    /// Admit a component. Every declared claim is resolved against the economy
    /// and mirrored as a Sentinel watch; the tier is whatever those proofs buy
    /// today. Registration never grants what the market has not minted.
    pub fn register(&self, request: RegisterComponent) -> Result<AxiomComponent, AppError> {
        let name = request.name.trim().to_string();
        if name.is_empty() {
            return Err(AppError::Validation("component name is empty".into()));
        }
        if self.find(&name)?.is_some() {
            return Err(AppError::Validation(format!(
                "component '{name}' is already registered"
            )));
        }

        let mut claims = Vec::new();
        for declared in &request.claims {
            let claim = self.economy.get_claim(&declared.claim_id)?;
            let minted = claim.status == ClaimStatus::Minted;
            let refuted = claim.status == ClaimStatus::Refuted;
            // Mirror the claim's own check as a continuous watch, so the tier
            // can drift the moment the proof stops describing reality.
            let watch = self.sentinel.register(RegisterWatch {
                claim_id: Some(claim.claim_id.clone()),
                domain: Domain::SupplyChain,
                label: format!("axiom:{name}:{:?}", declared.class),
                check: claim.verification.clone(),
            })?;
            claims.push(ComponentClaim {
                class: declared.class,
                claim_id: declared.claim_id.clone(),
                minted,
                refuted,
                holding: watch.holding,
                last_detail: watch.last_detail,
                watch_id: Some(watch.watch_id),
            });
        }

        let now = now_ms();
        let mut component = AxiomComponent {
            component_id: new_id("axc"),
            name,
            payload: request.payload,
            claims,
            handles_secrets: request.handles_secrets,
            allowed_tools: request.allowed_tools,
            kernel_builtin: false,
            tier: Tier::Unproven,
            quarantined: false,
            dispatches: 0,
            total_ns: 0,
            last_ns: 0,
            registered_at_ms: now,
            last_resync_ms: now,
        };
        component.tier = self.effective_tier(&component);
        component.quarantined = component.claims.iter().any(|c| c.refuted);
        self.save(&component)?;
        self.record_transition(&component, component.tier, "admitted with current proofs")?;
        Ok(component)
    }

    /// The dispatcher's core question: what do the standing proofs buy, capped
    /// by what the payload can physically enjoy?
    fn effective_tier(&self, component: &AxiomComponent) -> Tier {
        if component.kernel_builtin {
            return Tier::Proven;
        }
        let has = |class: ClaimClass| {
            component
                .claims
                .iter()
                .any(|c| c.class == class && c.standing())
        };
        let full_set = has(ClaimClass::MemorySafety)
            && has(ClaimClass::CapabilityBound)
            && has(ClaimClass::ResourceBound)
            && (!component.handles_secrets || has(ClaimClass::SpeculationSafety));
        let proof_tier = if full_set {
            Tier::Proven
        } else if has(ClaimClass::MemorySafety) || has(ClaimClass::CapabilityBound) {
            Tier::Partial
        } else {
            Tier::Unproven
        };
        proof_tier.more_caged(component.payload.tier_ceiling())
    }

    // ── the proof-scheduled syscall ──────────────────────────────────────

    /// Dispatch = the Axiom syscall. The tier decides the mechanism: an
    /// uncaged function call, a guarded domain crossing, or a full OS process.
    pub fn dispatch(&self, name_or_id: &str, args: &Value) -> Result<DispatchReport, AppError> {
        let mut component = self
            .find(name_or_id)?
            .ok_or_else(|| AppError::NotFound(format!("component {name_or_id}")))?;
        if component.quarantined {
            return Err(AppError::Forbidden(format!(
                "component '{}' is quarantined: an adversary refuted one of its claims; \
                 re-prove it in the economy before it runs again",
                component.name
            )));
        }
        let tier = component.tier;
        if component.payload.in_process() && tier == Tier::Unproven {
            return Err(AppError::Forbidden(format!(
                "component '{}' wants in-process execution but has no standing proof; \
                 in Axiom the cage comes off only when the economy has minted it",
                component.name
            )));
        }

        let started = Instant::now();
        let (path, output) = match tier {
            Tier::Proven => ("uncaged_call", self.run_in_process(&component, args)?),
            Tier::Partial => ("guarded_domain", self.run_guarded(&component, args)?),
            Tier::Unproven => ("process_cage", self.run_caged(&component, args)?),
        };
        let elapsed_ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);

        component.dispatches += 1;
        component.total_ns = component.total_ns.saturating_add(elapsed_ns);
        component.last_ns = elapsed_ns;
        self.save(&component)?;

        Ok(DispatchReport {
            component_id: component.component_id,
            name: component.name,
            tier,
            path: path.to_string(),
            elapsed_ns,
            output,
        })
    }

    /// Tier 0: single address space, kernel context — a plain function call.
    fn run_in_process(&self, component: &AxiomComponent, args: &Value) -> Result<Value, AppError> {
        match &component.payload {
            Payload::NativeIntent { intent } => native_intent(intent, args),
            Payload::ForgedTool { tool } => {
                let forge = self.forge.as_ref().ok_or_else(|| {
                    AppError::Internal("no forge attached to the axiom kernel".into())
                })?;
                forge
                    .execute(tool, args)
                    .map_err(|e| AppError::Internal(format!("forged tool failed: {e}")))
            }
            // DeviceTool at Tier 0 is impossible (its ceiling is Tier 1) and a
            // ShellProgram never lifts in-process; guard anyway.
            other => self.run_guarded_payload(component, other, args),
        }
    }

    /// Tier 1: the guarded domain — capability envelope + audit, then run.
    fn run_guarded(&self, component: &AxiomComponent, args: &Value) -> Result<Value, AppError> {
        self.run_guarded_payload(component, &component.payload, args)
    }

    fn run_guarded_payload(
        &self,
        component: &AxiomComponent,
        payload: &Payload,
        args: &Value,
    ) -> Result<Value, AppError> {
        // The domain switch: an envelope check and an audit record per call.
        if let Payload::DeviceTool { tool } = payload
            && !component.allowed_tools.is_empty()
            && !component.allowed_tools.iter().any(|t| t == tool)
        {
            return Err(AppError::Forbidden(format!(
                "capability envelope of '{}' does not include tool '{tool}'",
                component.name
            )));
        }
        tracing::debug!(
            component = %component.name,
            tier = ?component.tier,
            "axiom: guarded-domain crossing"
        );
        match payload {
            Payload::NativeIntent { intent } => native_intent(intent, args),
            Payload::ForgedTool { tool } => {
                let forge = self.forge.as_ref().ok_or_else(|| {
                    AppError::Internal("no forge attached to the axiom kernel".into())
                })?;
                forge
                    .execute(tool, args)
                    .map_err(|e| AppError::Internal(format!("forged tool failed: {e}")))
            }
            Payload::DeviceTool { tool } => self
                .device
                .execute(tool, args)
                .map_err(|e| AppError::Internal(format!("device tool failed: {e}"))),
            Payload::ShellProgram { .. } => self.run_caged(component, args),
        }
    }

    /// Tier 2: the full Linux tax — a real, supervised OS process.
    fn run_caged(&self, component: &AxiomComponent, args: &Value) -> Result<Value, AppError> {
        match &component.payload {
            Payload::ShellProgram { command } => {
                let mut line = command.clone();
                if let Some(extra) = args.get("args").and_then(Value::as_str) {
                    line = format!("{line} {extra}");
                }
                self.device
                    .execute(
                        "shell_exec",
                        &json!({ "command": line, "timeout_ms": 30_000 }),
                    )
                    .map_err(|e| AppError::Internal(format!("caged process failed: {e}")))
            }
            Payload::DeviceTool { tool } => self
                .device
                .execute(tool, args)
                .map_err(|e| AppError::Internal(format!("device tool failed: {e}"))),
            // In-process payloads never reach here (refused in dispatch).
            _ => Err(AppError::Forbidden(
                "unproven in-process code does not run in the kernel".into(),
            )),
        }
    }

    // ── continuous re-scheduling: promotion and demotion ─────────────────

    /// Re-read the economy and re-sample every mirrored watch, recomputing
    /// each component's tier while it keeps running. This is the dispatcher
    /// consuming the proof economy the way Linux consumes page tables.
    pub fn resync(&self) -> Result<ResyncReport, AppError> {
        let mut promoted = 0usize;
        let mut demoted = 0usize;
        let mut quarantined = 0usize;
        let mut transitions = Vec::new();
        let components = self.components()?;
        let total = components.len();

        for mut component in components {
            if component.kernel_builtin {
                continue;
            }
            let mut broken: Vec<String> = Vec::new();
            let mut recovered: Vec<String> = Vec::new();
            for claim in &mut component.claims {
                if let Ok(latest) = self.economy.get_claim(&claim.claim_id) {
                    claim.minted = latest.status == ClaimStatus::Minted;
                    claim.refuted = latest.status == ClaimStatus::Refuted;
                }
                if let Some(watch_id) = claim.watch_id.clone()
                    && let Ok(watch) = self.sentinel.sweep_one(&watch_id)
                {
                    let was = claim.holding;
                    claim.holding = watch.holding;
                    claim.last_detail = watch.last_detail;
                    if was && !watch.holding {
                        broken.push(format!("{:?} drifted", claim.class));
                    } else if !was && watch.holding {
                        recovered.push(format!("{:?} recovered", claim.class));
                    }
                }
                if claim.refuted {
                    broken.push(format!("{:?} REFUTED", claim.class));
                }
            }

            let was_quarantined = component.quarantined;
            component.quarantined = component.claims.iter().any(|c| c.refuted);
            if component.quarantined && !was_quarantined {
                quarantined += 1;
            }

            let from = component.tier;
            let to = self.effective_tier(&component);
            component.last_resync_ms = now_ms();
            if from != to {
                component.tier = to;
                let reason = if to.rank() > from.rank() {
                    demoted += 1;
                    format!("demoted: {}", broken.join(", "))
                } else {
                    promoted += 1;
                    if recovered.is_empty() {
                        "promoted: claims minted".to_string()
                    } else {
                        format!("promoted: {}", recovered.join(", "))
                    }
                };
                let transition = self.record_transition_from(&component, from, to, &reason)?;
                transitions.push(transition);
            }
            self.save(&component)?;
        }

        Ok(ResyncReport {
            components: total,
            promoted,
            demoted,
            quarantined,
            transitions,
        })
    }

    // ── the falsifiable numbers ──────────────────────────────────────────

    /// Dispatch the same no-op through all three mechanisms and publish the
    /// per-crossing cost. This is AXIOM.md §5 Phase 0: the boundary-crossing
    /// benchmark that makes "faster than Linux, where proven" checkable.
    pub fn bench(&self, iters: u32) -> Result<BenchReport, AppError> {
        let iters = iters.clamp(1, 1_000_000);
        let args = json!({ "message": "axiom" });

        // Tier 0: the uncaged function call.
        let started = Instant::now();
        for _ in 0..iters {
            let _ = native_intent("axiom.echo", &args)?;
        }
        let tier0_ns = per_op(started, iters);

        // Tier 1: the guarded domain — envelope + audit around the same call.
        let envelope = ["axiom.echo".to_string()];
        let started = Instant::now();
        for _ in 0..iters {
            if envelope.is_empty() || envelope.iter().any(|t| t == "axiom.echo") {
                tracing::trace!("axiom: bench guarded crossing");
                let _ = native_intent("axiom.echo", &args)?;
            }
        }
        let tier1_ns = per_op(started, iters);

        // Tier 2: the process cage — a real supervised OS process per op.
        let tier2_iters = iters.min(10);
        let started = Instant::now();
        for _ in 0..tier2_iters {
            self.device
                .execute(
                    "shell_exec",
                    &json!({ "command": "echo axiom", "timeout_ms": 15_000 }),
                )
                .map_err(|e| AppError::Internal(format!("bench process failed: {e}")))?;
        }
        let tier2_ns = per_op(started, tier2_iters);

        let cage_tax_ratio = tier2_ns as f64 / tier0_ns.max(1) as f64;
        Ok(BenchReport {
            tier0_iters: iters,
            tier0_ns_per_op: tier0_ns,
            tier1_iters: iters,
            tier1_ns_per_op: tier1_ns,
            tier2_iters,
            tier2_ns_per_op: tier2_ns,
            cage_tax_ratio,
            note: "same no-op via uncaged call / guarded domain / OS process; \
                   the ratio is the distrust tax proof deletes"
                .into(),
        })
    }

    // ── introspection ────────────────────────────────────────────────────

    pub fn components(&self) -> Result<Vec<AxiomComponent>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare(
                "SELECT payload FROM axiom_components ORDER BY registered_at_ms ASC LIMIT 1000",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
        }
        Ok(out)
    }

    /// Look a component up by id or by name.
    pub fn find(&self, name_or_id: &str) -> Result<Option<AxiomComponent>, AppError> {
        let payload: Option<String> = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM axiom_components WHERE component_id=?1 OR name=?1",
                [name_or_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        match payload {
            Some(p) => Ok(Some(serde_json::from_str(&p).map_err(de_err)?)),
            None => Ok(None),
        }
    }

    /// The tier ledger — every moment execution privilege moved, newest first.
    pub fn transitions(&self, limit: usize) -> Result<Vec<TierTransition>, AppError> {
        let limit = limit.clamp(1, 2000) as i64;
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM axiom_transitions ORDER BY at_ms DESC LIMIT ?1")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
        }
        Ok(out)
    }

    pub fn kernel_info(&self) -> Result<KernelInfo, AppError> {
        let components = self.components()?;
        let mut per_tier = [0usize; 3];
        for component in &components {
            per_tier[component.tier.rank() as usize] += 1;
        }
        Ok(KernelInfo {
            name: "Axiom".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            phase: "0 — proof-scheduled runtime (AXIOM.md §5)".into(),
            builtins: components.iter().filter(|c| c.kernel_builtin).count(),
            quarantined: components.iter().filter(|c| c.quarantined).count(),
            components: components.len(),
            per_tier,
        })
    }

    // ── persistence ──────────────────────────────────────────────────────

    fn save(&self, component: &AxiomComponent) -> Result<(), AppError> {
        let payload = serde_json::to_string(component).map_err(ser_err)?;
        let tier = format!("{:?}", component.tier).to_lowercase();
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO axiom_components
                 (component_id, name, payload, tier, registered_at_ms) VALUES (?1,?2,?3,?4,?5)",
                params![
                    component.component_id,
                    component.name,
                    payload,
                    tier,
                    component.registered_at_ms
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn record_transition(
        &self,
        component: &AxiomComponent,
        to: Tier,
        reason: &str,
    ) -> Result<TierTransition, AppError> {
        self.record_transition_from(component, to, to, reason)
    }

    fn record_transition_from(
        &self,
        component: &AxiomComponent,
        from: Tier,
        to: Tier,
        reason: &str,
    ) -> Result<TierTransition, AppError> {
        let event = TierTransition {
            event_id: new_id("axt"),
            component_id: component.component_id.clone(),
            name: component.name.clone(),
            from,
            to,
            reason: reason.to_string(),
            at_ms: now_ms(),
        };
        let payload = serde_json::to_string(&event).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO axiom_transitions (event_id, component_id, payload, at_ms)
                 VALUES (?1,?2,?3,?4)",
                params![event.event_id, event.component_id, payload, event.at_ms],
            )
            .map_err(sql_err)?;
        Ok(event)
    }
}

fn per_op(started: Instant, iters: u32) -> u64 {
    let total = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    total / u64::from(iters.max(1))
}

/// The kernel built-ins — the explicit, tiny TCB that everything else is
/// measured against. Deliberately boring: the interesting code earns Tier 0
/// through the market, not by being compiled in.
fn native_intent(intent: &str, args: &Value) -> Result<Value, AppError> {
    match intent {
        "axiom.echo" => Ok(json!({ "echo": args })),
        "axiom.time" => Ok(json!({ "now_ms": now_ms() })),
        "axiom.sum" => {
            let sum: f64 = args
                .get("values")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_f64).sum())
                .unwrap_or(0.0);
            Ok(json!({ "sum": sum }))
        }
        "axiom.checksum" => {
            let text = args
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| args.to_string());
            Ok(json!({ "fnv1a": fnv1a(&text) }))
        }
        other => Err(AppError::NotFound(format!("native intent {other}"))),
    }
}

fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn sql_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("axiom sql error: {e}"))
}
fn ser_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("axiom serialize error: {e}"))
}
fn de_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("axiom decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_agent::DevicePolicy;
    use crate::proof_economy::{AttackRequest, ClaimKind, ProposeRequest};
    use crate::verification::Check;

    fn kernel() -> AxiomKernel {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).expect("economy");
        let sentinel = Sentinel::in_memory(device.clone()).expect("sentinel");
        AxiomKernel::in_memory(economy, sentinel, device).expect("kernel")
    }

    /// Propose a claim backed by `check` and attack it twice so it mints.
    fn mint(kernel: &AxiomKernel, statement: &str, check: Check) -> String {
        let claim = kernel
            .economy
            .propose(ProposeRequest {
                statement: statement.into(),
                kind: ClaimKind::Assertion,
                proposer: "toolchain".into(),
                verification: check,
                evidence: vec![],
                depends_on: vec![],
            })
            .expect("propose");
        for attacker in ["skeptic1", "skeptic2"] {
            kernel
                .economy
                .attack(
                    &claim.claim_id,
                    AttackRequest {
                        attacker: attacker.into(),
                        note: String::new(),
                        counter: None,
                    },
                )
                .expect("attack");
        }
        claim.claim_id
    }

    #[test]
    fn builtins_are_tier0_and_dispatch_as_calls() {
        let kernel = kernel();
        kernel.seed_kernel_builtins().unwrap();
        let report = kernel
            .dispatch("axiom.echo", &json!({ "message": "hello" }))
            .unwrap();
        assert_eq!(report.tier, Tier::Proven);
        assert_eq!(report.path, "uncaged_call");
        assert_eq!(report.output["echo"]["message"], "hello");
    }

    #[test]
    fn full_claim_set_earns_tier0_and_partial_earns_tier1() {
        let kernel = kernel();
        let mem = mint(&kernel, "memory safe", Check::Trivial { pass: true });
        let cap = mint(&kernel, "capability bounded", Check::Trivial { pass: true });
        let res = mint(&kernel, "resource bounded", Check::Trivial { pass: true });

        let proven = kernel
            .register(RegisterComponent {
                name: "proven-intent".into(),
                payload: Payload::NativeIntent {
                    intent: "axiom.time".into(),
                },
                claims: vec![
                    DeclaredClaim {
                        class: ClaimClass::MemorySafety,
                        claim_id: mem.clone(),
                    },
                    DeclaredClaim {
                        class: ClaimClass::CapabilityBound,
                        claim_id: cap.clone(),
                    },
                    DeclaredClaim {
                        class: ClaimClass::ResourceBound,
                        claim_id: res,
                    },
                ],
                handles_secrets: false,
                allowed_tools: vec![],
            })
            .unwrap();
        assert_eq!(proven.tier, Tier::Proven);

        let partial = kernel
            .register(RegisterComponent {
                name: "partial-intent".into(),
                payload: Payload::NativeIntent {
                    intent: "axiom.time".into(),
                },
                claims: vec![DeclaredClaim {
                    class: ClaimClass::MemorySafety,
                    claim_id: mem,
                }],
                handles_secrets: false,
                allowed_tools: vec![],
            })
            .unwrap();
        assert_eq!(partial.tier, Tier::Partial);
        let report = kernel.dispatch("partial-intent", &json!({})).unwrap();
        assert_eq!(report.path, "guarded_domain");
    }

    #[test]
    fn unproven_in_process_code_is_refused_but_shell_still_runs_caged() {
        let kernel = kernel();
        let refused = kernel
            .register(RegisterComponent {
                name: "unproven-intent".into(),
                payload: Payload::NativeIntent {
                    intent: "axiom.echo".into(),
                },
                claims: vec![],
                handles_secrets: false,
                allowed_tools: vec![],
            })
            .unwrap();
        assert_eq!(refused.tier, Tier::Unproven);
        assert!(kernel.dispatch("unproven-intent", &json!({})).is_err());

        // The old world still boots — it just doesn't get the discount.
        kernel
            .register(RegisterComponent {
                name: "legacy-echo".into(),
                payload: Payload::ShellProgram {
                    command: "echo legacy".into(),
                },
                claims: vec![],
                handles_secrets: false,
                allowed_tools: vec![],
            })
            .unwrap();
        let report = kernel.dispatch("legacy-echo", &json!({})).unwrap();
        assert_eq!(report.tier, Tier::Unproven);
        assert_eq!(report.path, "process_cage");
        assert!(
            report.output["stdout"]
                .as_str()
                .unwrap_or_default()
                .contains("legacy")
        );
    }

    #[test]
    fn drift_demotes_a_running_component() {
        let kernel = kernel();
        let dir = std::env::temp_dir().join(format!("axiom-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("proof.txt");
        std::fs::write(&file, "memory safe: yes").unwrap();
        let path = file.to_string_lossy().to_string();

        let mem = mint(
            &kernel,
            "component build is memory safe",
            Check::FileContains {
                path: path.clone(),
                substring: "yes".into(),
            },
        );
        let component = kernel
            .register(RegisterComponent {
                name: "drifting".into(),
                payload: Payload::NativeIntent {
                    intent: "axiom.time".into(),
                },
                claims: vec![DeclaredClaim {
                    class: ClaimClass::MemorySafety,
                    claim_id: mem,
                }],
                handles_secrets: false,
                allowed_tools: vec![],
            })
            .unwrap();
        assert_eq!(component.tier, Tier::Partial);

        // Reality moves: the proof stops describing the world.
        std::fs::write(&file, "memory safe: no").unwrap();
        let report = kernel.resync().unwrap();
        assert_eq!(report.demoted, 1);
        let after = kernel.find("drifting").unwrap().unwrap();
        assert_eq!(after.tier, Tier::Unproven);
        assert_eq!(kernel.transitions(10).unwrap().len(), 2); // admit + demote

        // Recovery promotes it back — trust is a live price, not a badge.
        std::fs::write(&file, "memory safe: yes").unwrap();
        let report = kernel.resync().unwrap();
        assert_eq!(report.promoted, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bench_reports_the_cage_tax() {
        let kernel = kernel();
        let report = kernel.bench(200).unwrap();
        assert!(report.tier0_ns_per_op > 0);
        assert!(report.tier2_ns_per_op > report.tier0_ns_per_op);
        assert!(report.cage_tax_ratio > 1.0);
    }
}

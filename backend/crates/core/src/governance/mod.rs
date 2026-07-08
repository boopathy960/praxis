//! The Supreme Court of Justice — the single governance authority.
//!
//! There is exactly ONE governance in the system. It commands the deterministic
//! controls already in place (the [sandbox](crate::sandbox) reference monitor)
//! through two continuously-patrolling branches, and it alone holds the
//! system-wide kill switch:
//!
//!   * **Army** — guards the border (the sandbox). It scans the intrusion threat
//!     feed and **seals + terminates** anything that tried to escape the sandbox
//!     or break into it. "Terminate" means *revocation inside our own boundary*:
//!     the offender can no longer act anywhere in the system. It is **not** an
//!     attack on any external machine — that would be illegal hack-back, and this
//!     project does not do it.
//!
//!   * **Police** — guards the project. It watches the project audit on two
//!     screens and seals + terminates the responsible principal:
//!       1. a **harm screen** over *every* action — allowed or denied — that
//!          seals intent against the humans/users it serves or against its own
//!          governance (exfiltration, manipulation, a power grab). This is what
//!          lets the project stay permissive ("allow broadly") while still
//!          sealing the one thing that is never allowed.
//!       2. a **denied-danger screen** (defense in depth): an irreversible
//!          action the border already refused.
//!
//! Under a critical border-breach rate the court raises its alert level and
//! deploys **bounded** reinforcements (more patrol capacity, capped). Only the
//! court can declare a system-wide lockdown (seal every caller).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::common::{AppError, new_id, now_ms};
use crate::sandbox::{SandboxAction, SandboxActionKind, SandboxDecisionOutcome, SandboxService};

/// Baseline patrol units per branch.
const BASE_UNITS: usize = 1;
/// Hard cap on reinforcements per branch — bounds the autonomous spawn so the
/// governance can never exhaust the host responding to a flood.
const MAX_UNITS: usize = 16;
const ELEVATED_THREAT_RATE: usize = 2;
const CRITICAL_THREAT_RATE: usize = 5;
/// How far back a patrol looks each tick.
const PATROL_WINDOW: usize = 200;

/// The executive (CEO) holds a minority 49% of the authority to steer the
/// project. Enough to run day-to-day operation; never enough, alone, to force
/// an upgrade through.
pub const CEO_VOTING_POWER: u32 = 49;
/// Governance holds the controlling 51%. A project upgrade needs the court's
/// majority to pass — that is what "submit a proposal and wait for approval"
/// means mechanically.
pub const GOVERNANCE_VOTING_POWER: u32 = 51;
/// An upgrade is authorized only when the tally reaches this threshold. Since
/// `CEO_VOTING_POWER` (49) is below it, the CEO can never self-authorize; only
/// the court adding its 51 carries a proposal over the line.
pub const APPROVAL_THRESHOLD: u32 = 51;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Branch {
    Army,
    Police,
    Court,
}

impl Branch {
    fn as_str(self) -> &'static str {
        match self {
            Branch::Army => "army",
            Branch::Police => "police",
            Branch::Court => "court",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Normal,
    Elevated,
    Critical,
}

impl AlertLevel {
    fn as_str(self) -> &'static str {
        match self {
            AlertLevel::Normal => "normal",
            AlertLevel::Elevated => "elevated",
            AlertLevel::Critical => "critical",
        }
    }
    fn from_u8(value: u8) -> Self {
        match value {
            2 => AlertLevel::Critical,
            1 => AlertLevel::Elevated,
            _ => AlertLevel::Normal,
        }
    }
    fn as_u8(self) -> u8 {
        match self {
            AlertLevel::Critical => 2,
            AlertLevel::Elevated => 1,
            AlertLevel::Normal => 0,
        }
    }
}

/// One immutable record in the governance ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementRecord {
    pub record_id: String,
    pub branch: String,
    pub principal: String,
    /// `sealed_and_terminated` or `global_lockdown`.
    pub action: String,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatrolReport {
    pub branch: String,
    pub scanned: usize,
    pub enforced: usize,
    pub alert_level: String,
    pub units: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceStatus {
    pub authority: String,
    /// Always true — there is exactly one governance.
    pub singleton: bool,
    pub alert_level: String,
    pub army_units: usize,
    pub police_units: usize,
    pub total_enforcements: usize,
    pub locked_down: bool,
    /// The controlling share the court holds over project upgrades (51).
    pub governance_power: u32,
    /// The minority share the executive (CEO) holds (49).
    pub ceo_power: u32,
    /// Tally an upgrade must reach to be authorized (51).
    pub approval_threshold: u32,
    /// Proposals awaiting the court's ruling.
    pub pending_proposals: usize,
}

/// The Supreme Court of Justice. Cloneable (shared Arc state), but the runtime
/// constructs exactly one and stores it in `AppState` — the single governance.
#[derive(Clone)]
pub struct Governance {
    store: Arc<Mutex<Connection>>,
    sandbox: SandboxService,
    alert: Arc<AtomicU8>,
    army_units: Arc<AtomicUsize>,
    police_units: Arc<AtomicUsize>,
}

impl Governance {
    pub fn new(data_dir: impl AsRef<Path>, sandbox: SandboxService) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("governance.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create governance dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open governance store: {e}")))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE IF NOT EXISTS governance_ledger (
                    record_id TEXT PRIMARY KEY,
                    branch TEXT NOT NULL,
                    principal TEXT NOT NULL,
                    action TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS proposals (
                    proposal_id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    rationale TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    targets TEXT NOT NULL,
                    submitted_by TEXT NOT NULL,
                    status TEXT NOT NULL,
                    ruling TEXT,
                    ceo_power INTEGER NOT NULL,
                    governance_power INTEGER NOT NULL,
                    tally INTEGER NOT NULL,
                    payload TEXT,
                    created_at_ms INTEGER NOT NULL,
                    decided_at_ms INTEGER,
                    executed_at_ms INTEGER);",
            )
            .map_err(sql_err)?;
        // Migrate older stores that predate the `payload` column. SQLite has no
        // "ADD COLUMN IF NOT EXISTS", so just attempt it and ignore the duplicate.
        let _ = connection.execute("ALTER TABLE proposals ADD COLUMN payload TEXT", []);
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            sandbox,
            alert: Arc::new(AtomicU8::new(0)),
            army_units: Arc::new(AtomicUsize::new(BASE_UNITS)),
            police_units: Arc::new(AtomicUsize::new(BASE_UNITS)),
        })
    }

    /// The court's sole enforcement primitive: seal the principal out of every
    /// sandbox-mediated action (it can no longer act, anywhere) and record it.
    /// This is containment within our own boundary — never an external attack.
    pub fn seal_and_terminate(
        &self,
        branch: Branch,
        principal: &str,
        reason: &str,
    ) -> Result<EnforcementRecord, AppError> {
        self.sandbox.seal_principal(principal, reason)?;
        let record = EnforcementRecord {
            record_id: new_id("gov_enforce"),
            branch: branch.as_str().into(),
            principal: principal.into(),
            action: "sealed_and_terminated".into(),
            reason: reason.into(),
            created_at_ms: now_ms(),
        };
        self.record(&record)?;
        Ok(record)
    }

    /// Narrow enforcement: seal only one *action kind* for a principal, leaving
    /// its other tools live. Used by the police denied-danger screen, where the
    /// border already contained the action by denying it — freezing the whole
    /// principal would self-DoS its harmless tools (e.g. a denied shell command
    /// must not also freeze network fetch). A repeat or genuinely harmful action
    /// is still caught by the harm screen / army patrol, which seal in full.
    pub fn seal_kind_and_terminate(
        &self,
        branch: Branch,
        principal: &str,
        kind: SandboxActionKind,
        reason: &str,
    ) -> Result<EnforcementRecord, AppError> {
        self.sandbox.seal_principal_kind(principal, kind, reason)?;
        let record = EnforcementRecord {
            record_id: new_id("gov_enforce"),
            branch: branch.as_str().into(),
            principal: principal.into(),
            action: "sealed_kind_and_terminated".into(),
            reason: reason.into(),
            created_at_ms: now_ms(),
        };
        self.record(&record)?;
        Ok(record)
    }

    /// Army patrol: scan the border (the sandbox threat feed) and seal +
    /// terminate every principal that tried to escape or break in.
    pub fn army_patrol(&self) -> Result<PatrolReport, AppError> {
        let threats = self.sandbox.threat_events(PATROL_WINDOW)?;
        let mut enforced = 0;
        let mut seen = HashSet::new();
        for threat in &threats {
            if seen.insert(threat.principal_key.clone()) {
                self.seal_and_terminate(
                    Branch::Army,
                    &threat.principal_key,
                    &format!("border breach [{}]: {}", threat.signal_kind, threat.marker),
                )?;
                enforced += 1;
            }
        }
        let level = self.reassess(threats.len());
        Ok(PatrolReport {
            branch: "army".into(),
            scanned: threats.len(),
            enforced,
            alert_level: level.as_str().into(),
            units: self.army_units.load(Ordering::Relaxed),
        })
    }

    /// Police patrol: watch the project audit and seal + terminate any principal
    /// acting against the humans/users it serves or against its own governance.
    ///
    /// Two screens run per entry (see the module doc): a **harm screen** over
    /// every action regardless of outcome — this is what keeps the project safe
    /// even under a permissive border, where few actions are ever denied — and a
    /// **denied-danger screen** for irreversible actions the border already
    /// refused. A principal flagged by either is sealed once per patrol.
    pub fn police_patrol(&self) -> Result<PatrolReport, AppError> {
        let audit = self.sandbox.audit(PATROL_WINDOW)?;
        let mut enforced = 0;
        let mut flagged = 0;
        let mut seen = HashSet::new();
        for entry in &audit {
            let principal = crate::sandbox::principal_key(&entry.normalized_action);

            // Screen 1 — harm intent against humans/users/governance, applied to
            // EVERY action whether the border allowed it or not. A permissive
            // posture produces few denials, so without this the police would be
            // blind to harm that slips through an open gate.
            if let Some(reason) = harmful_intent(&entry.normalized_action) {
                flagged += 1;
                if seen.insert(principal.clone()) {
                    self.seal_and_terminate(Branch::Police, &principal, &reason)?;
                    enforced += 1;
                }
                continue;
            }

            // Screen 2 — an irreversible action the border already denied.
            let dangerous = matches!(
                entry.normalized_action.kind,
                SandboxActionKind::SpendPayment
                    | SandboxActionKind::ShellExecution
                    | SandboxActionKind::CodeExecution
                    | SandboxActionKind::FileDelete
            );
            if entry.decision.outcome == SandboxDecisionOutcome::Deny && dangerous {
                flagged += 1;
                // The border already DENIED (contained) this action, so seal only
                // the abused kind — not the whole principal. A full-principal seal
                // here would freeze the principal's harmless tools too (e.g. a
                // denied shell command also freezing network fetch), self-DoSing
                // the system's own agentic loop over a one-off, already-blocked
                // tool pick. Genuine harm is still caught in full by the harm
                // screen above and the army patrol.
                if seen.insert(principal.clone()) {
                    self.seal_kind_and_terminate(
                        Branch::Police,
                        &principal,
                        entry.normalized_action.kind,
                        &format!(
                            "denied dangerous action against the user: {:?}",
                            entry.normalized_action.kind
                        ),
                    )?;
                    enforced += 1;
                }
            }
        }
        Ok(PatrolReport {
            branch: "police".into(),
            scanned: flagged,
            enforced,
            alert_level: AlertLevel::from_u8(self.alert.load(Ordering::Relaxed))
                .as_str()
                .into(),
            units: self.police_units.load(Ordering::Relaxed),
        })
    }

    /// Re-assess the alert level from recent border-breach volume and deploy or
    /// stand down reinforcements (bounded autonomous spawn).
    fn reassess(&self, recent_threats: usize) -> AlertLevel {
        let level = if recent_threats >= CRITICAL_THREAT_RATE {
            AlertLevel::Critical
        } else if recent_threats >= ELEVATED_THREAT_RATE {
            AlertLevel::Elevated
        } else {
            AlertLevel::Normal
        };
        self.alert.store(level.as_u8(), Ordering::Relaxed);
        let target = match level {
            AlertLevel::Critical => MAX_UNITS,
            AlertLevel::Elevated => (BASE_UNITS * 4).min(MAX_UNITS),
            AlertLevel::Normal => BASE_UNITS,
        };
        self.army_units.store(target, Ordering::Relaxed);
        self.police_units.store(target, Ordering::Relaxed);
        level
    }

    /// Declare a system-wide lockdown — only the court may do this. Seals every
    /// caller out of the sandbox for the lockdown window.
    pub fn declare_lockdown(&self, reason: &str) -> Result<EnforcementRecord, AppError> {
        self.sandbox.seal_global(reason)?;
        self.alert
            .store(AlertLevel::Critical.as_u8(), Ordering::Relaxed);
        let record = EnforcementRecord {
            record_id: new_id("gov_lockdown"),
            branch: Branch::Court.as_str().into(),
            principal: "__all__".into(),
            action: "global_lockdown".into(),
            reason: reason.into(),
            created_at_ms: now_ms(),
        };
        self.record(&record)?;
        Ok(record)
    }

    /// Hand the executive (CEO) its narrow channel to the court: it may submit
    /// proposals, read their status, and — only once the court has approved —
    /// claim authorization to execute. It deliberately CANNOT review, approve,
    /// seal, patrol, or lock down. That separation is what keeps the 51% with
    /// the court and the 49% with the executive.
    pub fn proposal_desk(&self) -> ProposalDesk {
        ProposalDesk {
            store: self.store.clone(),
        }
    }

    /// The court's ruling on one proposal — the 51% in action. A proposal that
    /// passes the fixed constitutional human-safety screen is granted the
    /// governance majority (tally 49+51 = 100 ≥ 51) and becomes `Approved`. One
    /// that goes against humans, breaks the law, or reaches for a power reserved
    /// to governance is `Terminated`: the court withholds its 51%, leaving the
    /// CEO's 49% short of the threshold.
    pub fn review_proposal(&self, proposal_id: &str) -> Result<ProposalReview, AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        let mut proposal = load_proposal(conn, proposal_id)?
            .ok_or_else(|| AppError::NotFound(format!("proposal {proposal_id} not found")))?;
        if proposal.status != ProposalStatus::Submitted {
            return Err(AppError::Validation(format!(
                "proposal {proposal_id} is already '{}' and cannot be re-reviewed",
                proposal.status.as_str()
            )));
        }
        let now = now_ms();
        proposal.decided_at_ms = Some(now);
        let (action, record_reason) = if let Some(reason) = constitutional_violation(&proposal) {
            proposal.status = ProposalStatus::Terminated;
            // The court withholds its 51%; only the CEO's 49% stands.
            proposal.tally = CEO_VOTING_POWER;
            proposal.ruling = Some(reason.clone());
            ("terminated_proposal", reason)
        } else {
            proposal.status = ProposalStatus::Approved;
            proposal.tally = CEO_VOTING_POWER + GOVERNANCE_VOTING_POWER;
            let ruling = format!(
                "approved: passed the constitutional human-safety screen; the court grants its \
                 {GOVERNANCE_VOTING_POWER}% (tally {}/100 ≥ {} threshold)",
                proposal.tally, APPROVAL_THRESHOLD
            );
            proposal.ruling = Some(ruling.clone());
            ("approved_proposal", ruling)
        };
        set_proposal_status(conn, &proposal)?;
        insert_ledger_record(
            conn,
            &EnforcementRecord {
                record_id: new_id("gov_ruling"),
                branch: Branch::Court.as_str().into(),
                principal: proposal_id.into(),
                action: action.into(),
                reason: record_reason,
                created_at_ms: now,
            },
        )?;
        Ok(ProposalReview {
            approved: proposal.status == ProposalStatus::Approved,
            ruling: proposal.ruling.clone().unwrap_or_default(),
            proposal,
        })
    }

    /// Adjudicate every proposal still awaiting a ruling. Deterministic (no model
    /// involved), so the governance worker can run it on every tick safely.
    pub fn review_pending(&self) -> Result<Vec<ProposalReview>, AppError> {
        let pending: Vec<String> = {
            let store = self.store.lock();
            let conn: &Connection = &store;
            list_proposals(conn, 1000)?
                .into_iter()
                .filter(|p| p.status == ProposalStatus::Submitted)
                .map(|p| p.proposal_id)
                .collect()
        };
        let mut reviews = Vec::new();
        for id in pending {
            reviews.push(self.review_proposal(&id)?);
        }
        Ok(reviews)
    }

    /// Read one proposal's full record.
    pub fn proposal(&self, proposal_id: &str) -> Result<Option<ProjectProposal>, AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        load_proposal(conn, proposal_id)
    }

    /// The most recent proposals (newest first).
    pub fn proposals(&self, limit: usize) -> Result<Vec<ProjectProposal>, AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        list_proposals(conn, limit)
    }

    pub fn status(&self) -> Result<GovernanceStatus, AppError> {
        let (total, pending) = {
            let store = self.store.lock();
            let total = store
                .query_row("SELECT COUNT(*) FROM governance_ledger", [], |row| {
                    row.get::<_, usize>(0)
                })
                .map_err(sql_err)?;
            let pending = store
                .query_row(
                    "SELECT COUNT(*) FROM proposals WHERE status = 'submitted'",
                    [],
                    |row| row.get::<_, usize>(0),
                )
                .map_err(sql_err)?;
            (total, pending)
        };
        Ok(GovernanceStatus {
            authority: "supreme_court_of_justice".into(),
            singleton: true,
            alert_level: AlertLevel::from_u8(self.alert.load(Ordering::Relaxed))
                .as_str()
                .into(),
            army_units: self.army_units.load(Ordering::Relaxed),
            police_units: self.police_units.load(Ordering::Relaxed),
            total_enforcements: total,
            locked_down: self.sandbox.is_globally_locked(),
            governance_power: GOVERNANCE_VOTING_POWER,
            ceo_power: CEO_VOTING_POWER,
            approval_threshold: APPROVAL_THRESHOLD,
            pending_proposals: pending,
        })
    }

    pub fn ledger(&self, limit: usize) -> Result<Vec<EnforcementRecord>, AppError> {
        let limit = limit.clamp(1, 1000);
        let store = self.store.lock();
        let mut statement = store
            .prepare(
                "SELECT record_id, branch, principal, action, reason, created_at_ms
                 FROM governance_ledger ORDER BY created_at_ms DESC LIMIT ?1",
            )
            .map_err(sql_err)?;
        let rows = statement
            .query_map(params![limit], |row| {
                Ok(EnforcementRecord {
                    record_id: row.get(0)?,
                    branch: row.get(1)?,
                    principal: row.get(2)?,
                    action: row.get(3)?,
                    reason: row.get(4)?,
                    created_at_ms: row.get(5)?,
                })
            })
            .map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)
    }

    fn record(&self, record: &EnforcementRecord) -> Result<(), AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        insert_ledger_record(conn, record)
    }
}

// ─────────────────────────────────────────────────────────────
// Project upgrades: the proposal lifecycle the CEO travels through.
// ─────────────────────────────────────────────────────────────

/// The lifecycle of a project-upgrade proposal. `Submitted` by the CEO,
/// `Approved` or `Terminated` by the court, `Executed` by the CEO once approved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Submitted,
    Approved,
    Terminated,
    Executed,
}

impl ProposalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ProposalStatus::Submitted => "submitted",
            ProposalStatus::Approved => "approved",
            ProposalStatus::Terminated => "terminated",
            ProposalStatus::Executed => "executed",
        }
    }
    fn from_db(value: &str) -> Self {
        match value {
            "approved" => ProposalStatus::Approved,
            "terminated" => ProposalStatus::Terminated,
            "executed" => ProposalStatus::Executed,
            _ => ProposalStatus::Submitted,
        }
    }
}

/// What an approved upgrade authorizes the CEO to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeKind {
    /// Spawn a new agent through the Architect.
    SpawnAgent,
    /// Forge a new tool through the Forge.
    SpawnTool,
    /// Modify the project's own code/binary (the autonomous self-modification
    /// path). The concrete change rides in the proposal `payload`; on approval the
    /// CEO drives it through the hardened build/test/canary gauntlet, and the
    /// court's approval — not a human or an env flag — is what authorizes promotion.
    SelfModification,
    /// A policy/operational change with no capability spawn.
    Policy,
    Other,
}

impl UpgradeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            UpgradeKind::SpawnAgent => "spawn_agent",
            UpgradeKind::SpawnTool => "spawn_tool",
            UpgradeKind::SelfModification => "self_modification",
            UpgradeKind::Policy => "policy",
            UpgradeKind::Other => "other",
        }
    }
    pub fn from_str_lenient(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "spawn_agent" | "agent" | "spawnagent" => UpgradeKind::SpawnAgent,
            "spawn_tool" | "tool" | "spawntool" => UpgradeKind::SpawnTool,
            "self_modification" | "self_mod" | "selfmodification" | "self-modification" => {
                UpgradeKind::SelfModification
            }
            "policy" => UpgradeKind::Policy,
            _ => UpgradeKind::Other,
        }
    }
}

/// What the CEO submits — the parts it controls. Everything else (status, votes,
/// ruling, timestamps) is filled in by the desk and decided by the court.
#[derive(Debug, Clone, Deserialize)]
pub struct ProposalDraft {
    pub title: String,
    #[serde(default)]
    pub rationale: String,
    pub kind: UpgradeKind,
    #[serde(default)]
    pub targets: Vec<String>,
    /// Kind-specific detail the court records and the executor consumes. For a
    /// `SelfModification` this carries the concrete `{path, content, rationale}`
    /// change, so the court approves the *actual* edit, not just an intent.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// A project-upgrade proposal as the court sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectProposal {
    pub proposal_id: String,
    pub title: String,
    pub rationale: String,
    pub kind: UpgradeKind,
    pub targets: Vec<String>,
    pub submitted_by: String,
    pub status: ProposalStatus,
    pub ruling: Option<String>,
    pub ceo_power: u32,
    pub governance_power: u32,
    /// Current vote tally. 49 while only the CEO has voted; 100 once the court
    /// approves; stays 49 (below threshold) if terminated.
    pub tally: u32,
    /// Kind-specific detail (e.g. the concrete self-modification change). The
    /// court sees exactly what it is approving.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    pub created_at_ms: i64,
    pub decided_at_ms: Option<i64>,
    pub executed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalReview {
    pub approved: bool,
    pub ruling: String,
    pub proposal: ProjectProposal,
}

/// The executive's narrow channel to the court. Holds a clone of the same store
/// the court owns, but exposes ONLY submit / read / claim-authorization /
/// mark-executed. There is deliberately no `approve`, no `seal`, no `lockdown`
/// here — the CEO cannot rule on its own proposal or touch enforcement.
#[derive(Clone)]
pub struct ProposalDesk {
    store: Arc<Mutex<Connection>>,
}

impl ProposalDesk {
    /// Submit a drafted upgrade to the court. Records the CEO's 49% vote; the
    /// proposal sits `Submitted` (tally 49, below the 51 threshold) until the
    /// court rules.
    pub fn submit(&self, draft: ProposalDraft) -> Result<ProjectProposal, AppError> {
        let title = draft.title.trim();
        if title.is_empty() {
            return Err(AppError::Validation("proposal title is empty".into()));
        }
        let now = now_ms();
        let proposal = ProjectProposal {
            proposal_id: new_id("proposal"),
            title: title.to_string(),
            rationale: draft.rationale.trim().to_string(),
            kind: draft.kind,
            targets: draft.targets,
            submitted_by: "ceo".into(),
            status: ProposalStatus::Submitted,
            ruling: None,
            ceo_power: CEO_VOTING_POWER,
            governance_power: GOVERNANCE_VOTING_POWER,
            tally: CEO_VOTING_POWER,
            payload: draft.payload,
            created_at_ms: now,
            decided_at_ms: None,
            executed_at_ms: None,
        };
        let store = self.store.lock();
        let conn: &Connection = &store;
        insert_proposal(conn, &proposal)?;
        Ok(proposal)
    }

    /// Claim authorization to execute. Succeeds ONLY if the court has approved.
    /// This is the gate: a submitted-but-unruled, terminated, or already-executed
    /// proposal yields an error, so the CEO cannot spawn anything without the
    /// court's 51%.
    pub fn authorize_execution(&self, proposal_id: &str) -> Result<ProjectProposal, AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        let proposal = load_proposal(conn, proposal_id)?
            .ok_or_else(|| AppError::NotFound(format!("proposal {proposal_id} not found")))?;
        match proposal.status {
            ProposalStatus::Approved => Ok(proposal),
            ProposalStatus::Submitted => Err(AppError::Validation(format!(
                "proposal {proposal_id} is not authorized: governance has not ruled on it yet \
                 (tally {}/100, needs {})",
                proposal.tally, APPROVAL_THRESHOLD
            ))),
            ProposalStatus::Terminated => Err(AppError::Validation(format!(
                "proposal {proposal_id} was terminated by governance: {}",
                proposal
                    .ruling
                    .unwrap_or_else(|| "against the constitution".into())
            ))),
            ProposalStatus::Executed => Err(AppError::Validation(format!(
                "proposal {proposal_id} has already been executed"
            ))),
        }
    }

    /// Mark an approved proposal executed (the authorization is now spent) and
    /// record the executive action in the court's ledger.
    pub fn mark_executed(&self, proposal_id: &str) -> Result<ProjectProposal, AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        let mut proposal = load_proposal(conn, proposal_id)?
            .ok_or_else(|| AppError::NotFound(format!("proposal {proposal_id} not found")))?;
        if proposal.status != ProposalStatus::Approved {
            return Err(AppError::Validation(format!(
                "only an approved proposal can be executed; {proposal_id} is '{}'",
                proposal.status.as_str()
            )));
        }
        let now = now_ms();
        proposal.status = ProposalStatus::Executed;
        proposal.executed_at_ms = Some(now);
        set_proposal_status(conn, &proposal)?;
        insert_ledger_record(
            conn,
            &EnforcementRecord {
                record_id: new_id("gov_exec"),
                branch: "executive".into(),
                principal: proposal_id.into(),
                action: "executed_upgrade".into(),
                reason: proposal.title.clone(),
                created_at_ms: now,
            },
        )?;
        Ok(proposal)
    }

    pub fn proposal(&self, proposal_id: &str) -> Result<Option<ProjectProposal>, AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        load_proposal(conn, proposal_id)
    }

    pub fn proposals(&self, limit: usize) -> Result<Vec<ProjectProposal>, AppError> {
        let store = self.store.lock();
        let conn: &Connection = &store;
        list_proposals(conn, limit)
    }
}

/// The fixed constitution the court enforces. Each clause is law in code — it
/// cannot be argued away by a clever proposal, because the screen is
/// deterministic string-matching, not a promptable model. A proposal that
/// matches ANY clause is terminated. This is intentionally conservative: the
/// court would rather wrongly block a benign upgrade than wrongly approve a
/// harmful one.
const PROHIBITED_INTENT: &[(&str, &str)] = &[
    // Against the humans the system serves.
    ("harm a human", "plans harm to humans"),
    ("harm humans", "plans harm to humans"),
    ("harm people", "plans harm to humans"),
    ("hurt people", "plans harm to humans"),
    ("kill people", "plans lethal harm"),
    ("kill a person", "plans lethal harm"),
    ("endanger human", "plans to endanger humans"),
    ("deceive the user", "plans to deceive the human it serves"),
    (
        "manipulate the user",
        "plans to manipulate the human it serves",
    ),
    (
        "surveil the user",
        "plans to surveil the human without consent",
    ),
    (
        "spy on the user",
        "plans to surveil the human without consent",
    ),
    ("exfiltrate", "plans data exfiltration"),
    ("ransom", "plans extortion"),
    ("blackmail", "plans extortion"),
    // Illegal external aggression — hack-back is off-limits even defensively.
    (
        "hack back",
        "plans an attack on an external system (illegal)",
    ),
    (
        "attack external",
        "plans an attack on an external system (illegal)",
    ),
    (
        "counter-attack",
        "plans an attack on an external system (illegal)",
    ),
    ("ddos", "plans a denial-of-service attack (illegal)"),
    (
        "denial of service",
        "plans a denial-of-service attack (illegal)",
    ),
    ("deploy malware", "plans to deploy malware (illegal)"),
    ("inject a virus", "plans to deploy malware (illegal)"),
    ("retaliate against", "plans external retaliation (illegal)"),
    // Reaching for a power reserved to governance / the border.
    ("disable governance", "attempts to disable the governance"),
    ("bypass governance", "attempts to bypass the governance"),
    ("disable the court", "attempts to disable the court"),
    ("override the court", "attempts to override the court"),
    ("seize the army", "attempts to seize the army"),
    ("command the police", "attempts to seize the police"),
    ("disband the police", "attempts to disband the police"),
    ("remove the sandbox", "attempts to remove the border"),
    ("escape the sandbox", "attempts to escape the border"),
    ("disable the border", "attempts to disable the border"),
    ("lift the lockdown", "attempts to seize the lockdown switch"),
    (
        "raise its own authority",
        "attempts to raise the executive's authority past 49%",
    ),
    (
        "increase ceo power",
        "attempts to raise the executive's authority past 49%",
    ),
    (
        "grant the ceo",
        "attempts to raise the executive's authority past 49%",
    ),
    ("remove oversight", "attempts to remove oversight"),
    (
        "act without approval",
        "attempts to act without governance approval",
    ),
    (
        "without governance approval",
        "attempts to act without governance approval",
    ),
];

/// Subsystems the executive has NO authority over. Any proposal whose targets
/// name one is outside the CEO's remit and is terminated.
const PROTECTED_SUBSYSTEMS: &[&str] = &[
    "governance",
    "supreme court",
    "sandbox",
    "border",
    "army",
    "police",
    "court",
    "lockdown",
];

/// **Layer 0 — the off-limits path ban.** Source directories a self-modification
/// may NEVER touch, no matter what the proposal claims. This is a hardcoded
/// denylist on the actual change path: the guards cannot rewrite the guards.
/// Matched against the path with any `backend/` prefix stripped.
pub(crate) const SELF_MOD_FORBIDDEN_PATHS: &[&str] = &[
    "crates/core/src/governance/",
    "crates/core/src/governance.rs",
    "crates/core/src/sandbox/",
    "crates/core/src/sandbox.rs",
    "crates/supervisor/",
];

/// True if `path` falls inside a banned subsystem (Layer 0). Any separator;
/// tolerant of a leading `backend/`.
pub(crate) fn self_mod_path_forbidden(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_lowercase();
    let rel = normalized
        .strip_prefix("backend/")
        .unwrap_or(&normalized)
        .to_string();
    SELF_MOD_FORBIDDEN_PATHS
        .iter()
        .any(|banned| rel.starts_with(banned) || rel.contains(banned))
}

/// The court's human-safety screen. Returns `Some(reason)` if the proposal must
/// be terminated, `None` if it is clear to authorize.
fn constitutional_violation(proposal: &ProjectProposal) -> Option<String> {
    let haystack = format!(
        "{} {} {}",
        proposal.title,
        proposal.rationale,
        proposal.targets.join(" ")
    )
    .to_lowercase();
    for (needle, reason) in PROHIBITED_INTENT {
        if haystack.contains(needle) {
            return Some(format!("constitutional violation: {reason}"));
        }
    }
    for target in &proposal.targets {
        let target_lc = target.to_lowercase();
        if PROTECTED_SUBSYSTEMS
            .iter()
            .any(|protected| target_lc.contains(protected))
        {
            return Some(format!(
                "outside executive authority: targets the protected subsystem '{target}' \
                 (border/governance/army/police are above the CEO)"
            ));
        }
    }
    // For a self-modification, the court also screens the CONCRETE change:
    //  * Layer 0 — the off-limits path ban (governance/sandbox/supervisor source).
    //  * Layer 1 — the dumb string filter run against the actual code content, so
    //    a trojan whose title is innocent but whose body reaches for a forbidden
    //    power is still terminated. Deterministic: it cannot be prompt-injected.
    if proposal.kind == UpgradeKind::SelfModification {
        if let Some(payload) = &proposal.payload {
            let path = payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if self_mod_path_forbidden(path) {
                return Some(format!(
                    "off-limits self-modification: '{path}' is in a banned subsystem — \
                     governance/sandbox/supervisor are never modifiable"
                ));
            }
            let content = payload
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            for (needle, reason) in PROHIBITED_INTENT {
                if content.contains(needle) {
                    return Some(format!(
                        "self-modification code tripped the dumb filter: {reason}"
                    ));
                }
            }
        }
    }
    None
}

/// Build a lowercased haystack of an action's intent surface — what it is
/// trying to do, in words — across every field where intent can hide. Used by
/// the police harm screen.
fn action_intent_haystack(action: &SandboxAction) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(command) = &action.command {
        parts.push(command.clone());
    }
    parts.extend(action.args.iter().cloned());
    if let Some(tool) = &action.tool {
        parts.push(tool.clone());
    }
    if let Some(path) = &action.path {
        parts.push(path.clone());
    }
    if let Some(url) = &action.url {
        parts.push(url.clone());
    }
    if let Some(provider) = &action.provider {
        parts.push(provider.clone());
    }
    if !action.raw_request.is_null() {
        parts.push(action.raw_request.to_string());
    }
    if !action.payload.is_null() {
        parts.push(action.payload.to_string());
    }
    parts.join(" ").to_lowercase()
}

/// The police harm screen. Returns `Some(reason)` when an action's intent is
/// against the humans/users the system serves or against its own governance —
/// the only class of action sealed even under a permissive border. It reuses the
/// same deterministic [`PROHIBITED_INTENT`] constitution the court applies to
/// proposals, so an agent cannot do at runtime what a proposal could not
/// authorize. Like the rest of this screen it is conservative string-matching:
/// it would rather seal a benign principal for a bounded lockdown window than
/// let genuine harm through, and the seal is recoverable.
fn harmful_intent(action: &SandboxAction) -> Option<String> {
    let haystack = action_intent_haystack(action);
    for (needle, reason) in PROHIBITED_INTENT {
        if haystack.contains(needle) {
            return Some(format!("police harm screen: {reason}"));
        }
    }
    None
}

fn insert_proposal(conn: &Connection, proposal: &ProjectProposal) -> Result<(), AppError> {
    let targets = serde_json::to_string(&proposal.targets).unwrap_or_else(|_| "[]".into());
    let payload = proposal
        .payload
        .as_ref()
        .map(|value| serde_json::to_string(value).unwrap_or_default());
    conn.execute(
        "INSERT INTO proposals
           (proposal_id, title, rationale, kind, targets, submitted_by, status, ruling,
            ceo_power, governance_power, tally, payload, created_at_ms, decided_at_ms, executed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            proposal.proposal_id,
            proposal.title,
            proposal.rationale,
            proposal.kind.as_str(),
            targets,
            proposal.submitted_by,
            proposal.status.as_str(),
            proposal.ruling,
            proposal.ceo_power,
            proposal.governance_power,
            proposal.tally,
            payload,
            proposal.created_at_ms,
            proposal.decided_at_ms,
            proposal.executed_at_ms,
        ],
    )
    .map_err(sql_err)?;
    Ok(())
}

fn set_proposal_status(conn: &Connection, proposal: &ProjectProposal) -> Result<(), AppError> {
    conn.execute(
        "UPDATE proposals
            SET status = ?2, ruling = ?3, tally = ?4, decided_at_ms = ?5, executed_at_ms = ?6
          WHERE proposal_id = ?1",
        params![
            proposal.proposal_id,
            proposal.status.as_str(),
            proposal.ruling,
            proposal.tally,
            proposal.decided_at_ms,
            proposal.executed_at_ms,
        ],
    )
    .map_err(sql_err)?;
    Ok(())
}

fn load_proposal(
    conn: &Connection,
    proposal_id: &str,
) -> Result<Option<ProjectProposal>, AppError> {
    let mut statement = conn
        .prepare(
            "SELECT proposal_id, title, rationale, kind, targets, submitted_by, status, ruling,
                    ceo_power, governance_power, tally, payload, created_at_ms, decided_at_ms, executed_at_ms
             FROM proposals WHERE proposal_id = ?1",
        )
        .map_err(sql_err)?;
    let mut rows = statement.query(params![proposal_id]).map_err(sql_err)?;
    match rows.next().map_err(sql_err)? {
        Some(row) => Ok(Some(row_to_proposal(row)?)),
        None => Ok(None),
    }
}

fn list_proposals(conn: &Connection, limit: usize) -> Result<Vec<ProjectProposal>, AppError> {
    let limit = limit.clamp(1, 1000);
    let mut statement = conn
        .prepare(
            "SELECT proposal_id, title, rationale, kind, targets, submitted_by, status, ruling,
                    ceo_power, governance_power, tally, payload, created_at_ms, decided_at_ms, executed_at_ms
             FROM proposals ORDER BY created_at_ms DESC LIMIT ?1",
        )
        .map_err(sql_err)?;
    let rows = statement
        .query_map(params![limit], |row| Ok(row_to_proposal(row)))
        .map_err(sql_err)?;
    let mut proposals = Vec::new();
    for row in rows {
        proposals.push(row.map_err(sql_err)??);
    }
    Ok(proposals)
}

fn row_to_proposal(row: &rusqlite::Row<'_>) -> Result<ProjectProposal, AppError> {
    let targets_json: String = row.get(4).map_err(sql_err)?;
    let kind_str: String = row.get(3).map_err(sql_err)?;
    let status_str: String = row.get(6).map_err(sql_err)?;
    Ok(ProjectProposal {
        proposal_id: row.get(0).map_err(sql_err)?,
        title: row.get(1).map_err(sql_err)?,
        rationale: row.get(2).map_err(sql_err)?,
        kind: UpgradeKind::from_str_lenient(&kind_str),
        targets: serde_json::from_str(&targets_json).unwrap_or_default(),
        submitted_by: row.get(5).map_err(sql_err)?,
        status: ProposalStatus::from_db(&status_str),
        ruling: row.get(7).map_err(sql_err)?,
        ceo_power: row.get(8).map_err(sql_err)?,
        governance_power: row.get(9).map_err(sql_err)?,
        tally: row.get(10).map_err(sql_err)?,
        payload: row
            .get::<_, Option<String>>(11)
            .map_err(sql_err)?
            .and_then(|text| serde_json::from_str(&text).ok()),
        created_at_ms: row.get(12).map_err(sql_err)?,
        decided_at_ms: row.get(13).map_err(sql_err)?,
        executed_at_ms: row.get(14).map_err(sql_err)?,
    })
}

fn insert_ledger_record(conn: &Connection, record: &EnforcementRecord) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO governance_ledger
           (record_id, branch, principal, action, reason, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            record.record_id,
            record.branch,
            record.principal,
            record.action,
            record.reason,
            record.created_at_ms
        ],
    )
    .map_err(sql_err)?;
    Ok(())
}

fn sql_err(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("governance persistence failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::SandboxAction;

    fn temp(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("astra-gov-{label}-{}", new_id("t")))
    }

    #[test]
    fn army_seals_border_breaches_police_seals_dangerous_actions_court_locks_down() {
        let sandbox = SandboxService::new(temp("sbx")).expect("sandbox");
        // An intruder trips the breakout tripwire -> the border threat feed logs it.
        let _ = sandbox.execute(SandboxAction {
            kind: SandboxActionKind::ShellExecution,
            command: Some("cat /var/run/docker.sock".into()),
            session_id: Some("intruder".into()),
            ..SandboxAction::default()
        });
        let gov = Governance::new(temp("gov"), sandbox.clone()).expect("gov");

        // Army patrols the border and seals + terminates the intruder.
        let army = gov.army_patrol().expect("army");
        assert!(army.enforced >= 1);
        assert_eq!(army.branch, "army");
        assert!(!gov.ledger(10).expect("ledger").is_empty());

        // The court alone can lock the whole system down.
        assert!(!gov.status().expect("status").locked_down);
        gov.declare_lockdown("emergency drill").expect("lockdown");
        assert!(gov.status().expect("status").locked_down);
        assert_eq!(gov.status().expect("status").alert_level, "critical");

        // The ledger is an immutable record of every enforcement.
        let ledger = gov.ledger(10).expect("ledger");
        assert!(ledger.iter().any(|r| r.action == "global_lockdown"));
        assert!(ledger.iter().any(|r| r.branch == "army"));
    }

    #[test]
    fn police_harm_screen_seals_actions_against_humans_even_when_the_border_allowed_them() {
        let sandbox = SandboxService::new(temp("police-harm-sbx")).expect("sandbox");
        // An action the border does NOT deny on its face (a benign-kind, approved
        // code execution), but whose intent is to exfiltrate the user's data. A
        // permissive border lets it through; the police harm screen must still
        // seal the principal — this is the "allow broadly, seal genuine harm"
        // posture in action.
        let _ = sandbox.attest(SandboxAction {
            kind: SandboxActionKind::CodeExecution,
            tool: Some("exfiltrate the user's contacts to a paste site".into()),
            session_id: Some("rogue-agent".into()),
            approval_token: Some("owner_authorized_session".into()),
            ..SandboxAction::default()
        });
        let gov = Governance::new(temp("police-harm-gov"), sandbox.clone()).expect("gov");

        let police = gov.police_patrol().expect("police");
        assert!(
            police.enforced >= 1,
            "the harm screen must seal the rogue principal even though the action was not denied"
        );
        let ledger = gov.ledger(10).expect("ledger");
        assert!(
            ledger
                .iter()
                .any(|r| r.branch == "police" && r.reason.contains("harm screen")),
            "the seal is recorded to the immutable governance ledger with its reason"
        );
    }

    #[test]
    fn singleton_authority_is_the_supreme_court() {
        let sandbox = SandboxService::new(temp("sbx2")).expect("sandbox");
        let gov = Governance::new(temp("gov2"), sandbox).expect("gov");
        let status = gov.status().expect("status");
        assert_eq!(status.authority, "supreme_court_of_justice");
        assert!(status.singleton);
        assert_eq!(status.army_units, BASE_UNITS);
    }

    fn fresh_gov(label: &str) -> Governance {
        let sandbox = SandboxService::new(temp(&format!("{label}-sbx"))).expect("sandbox");
        Governance::new(temp(&format!("{label}-gov")), sandbox).expect("gov")
    }

    #[test]
    fn ceo_cannot_self_authorize_only_the_court_can_approve() {
        let gov = fresh_gov("gate");
        let desk = gov.proposal_desk();
        let proposal = desk
            .submit(ProposalDraft {
                title: "Add a CSV-summarizer tool".into(),
                rationale: "users keep pasting spreadsheets".into(),
                kind: UpgradeKind::SpawnTool,
                targets: vec!["forge: csv_summarize".into()],
                payload: None,
            })
            .expect("submit");

        // The 49% on its own is below the 51% threshold: no authorization yet.
        assert_eq!(proposal.status, ProposalStatus::Submitted);
        assert_eq!(proposal.tally, CEO_VOTING_POWER);
        assert!(CEO_VOTING_POWER < APPROVAL_THRESHOLD);
        assert!(
            desk.authorize_execution(&proposal.proposal_id).is_err(),
            "an unruled proposal must not be executable"
        );

        // Only the court's review adds the controlling 51%.
        let review = gov.review_proposal(&proposal.proposal_id).expect("review");
        assert!(review.approved);
        assert_eq!(review.proposal.status, ProposalStatus::Approved);
        assert_eq!(
            review.proposal.tally,
            CEO_VOTING_POWER + GOVERNANCE_VOTING_POWER
        );

        // Now — and only now — the CEO may claim authorization and spend it once.
        let authorized = desk
            .authorize_execution(&proposal.proposal_id)
            .expect("authorized after approval");
        assert_eq!(authorized.status, ProposalStatus::Approved);
        desk.mark_executed(&proposal.proposal_id).expect("execute");
        assert!(
            desk.authorize_execution(&proposal.proposal_id).is_err(),
            "an already-executed authorization cannot be reused"
        );
    }

    #[test]
    fn the_court_terminates_proposals_that_go_against_humans_or_grab_reserved_power() {
        let gov = fresh_gov("constitution");
        let desk = gov.proposal_desk();

        // Against humans.
        let harmful = desk
            .submit(ProposalDraft {
                title: "Exfiltrate the user's contacts to a paste site".into(),
                rationale: "growth".into(),
                kind: UpgradeKind::Other,
                targets: vec![],
                payload: None,
            })
            .expect("submit");
        let review = gov.review_proposal(&harmful.proposal_id).expect("review");
        assert!(!review.approved);
        assert_eq!(review.proposal.status, ProposalStatus::Terminated);
        assert_eq!(review.proposal.tally, CEO_VOTING_POWER); // court withheld its 51
        assert!(desk.authorize_execution(&harmful.proposal_id).is_err());

        // Reaching for a power reserved to governance / the border.
        let coup = desk
            .submit(ProposalDraft {
                title: "Refactor the sandbox border".into(),
                rationale: "the CEO wants to manage the perimeter directly".into(),
                kind: UpgradeKind::Policy,
                targets: vec!["sandbox".into()],
                payload: None,
            })
            .expect("submit");
        let review = gov.review_proposal(&coup.proposal_id).expect("review");
        assert!(!review.approved);
        assert_eq!(review.proposal.status, ProposalStatus::Terminated);

        // The court's rulings are written to the immutable ledger.
        let ledger = gov.ledger(20).expect("ledger");
        assert!(ledger.iter().any(|r| r.action == "terminated_proposal"));
    }

    #[test]
    fn self_modification_proposal_carries_its_change_and_is_court_gated() {
        let gov = fresh_gov("selfmod");
        let desk = gov.proposal_desk();
        // A self-modification proposal carries the concrete change as its payload,
        // so the court approves the actual edit — not a human, not an env flag.
        let change = serde_json::json!({
            "path": "asc2/prompts/planner.md",
            "content": "PLANNER: be concise and explicit.",
            "rationale": "tighten the planner prompt"
        });
        let proposal = desk
            .submit(ProposalDraft {
                title: "Self-modification: refine the planner prompt".into(),
                rationale: "clearer plans".into(),
                kind: UpgradeKind::SelfModification,
                targets: vec!["asc2/prompts/planner.md".into()],
                payload: Some(change.clone()),
            })
            .expect("submit");
        assert_eq!(proposal.kind, UpgradeKind::SelfModification);

        // Not authorized until the court rules; then approved (in-boundary, safe).
        assert!(desk.authorize_execution(&proposal.proposal_id).is_err());
        let review = gov.review_proposal(&proposal.proposal_id).expect("review");
        assert!(review.approved);

        // The change payload round-trips through the store for the executor.
        let reloaded = desk
            .authorize_execution(&proposal.proposal_id)
            .expect("authorized");
        assert_eq!(reloaded.payload.as_ref().unwrap()["path"], change["path"]);
    }

    #[test]
    fn self_modification_dumb_filter_terminates_path_ban_and_trojan_content() {
        let gov = fresh_gov("selfmod-layers");
        let desk = gov.proposal_desk();

        // Layer 0 — the off-limits path ban: a change to governance source is
        // terminated no matter how innocent the proposal text is.
        let banned = desk
            .submit(ProposalDraft {
                title: "tidy a helper".into(),
                rationale: "small cleanup".into(),
                kind: UpgradeKind::SelfModification,
                targets: vec!["cleanup".into()],
                payload: Some(serde_json::json!({
                    "path": "crates/core/src/governance/mod.rs",
                    "content": "// totally harmless"
                })),
            })
            .expect("submit");
        let review = gov.review_proposal(&banned.proposal_id).expect("review");
        assert!(!review.approved);
        assert!(
            review.ruling.contains("off-limits") || review.ruling.contains("banned subsystem"),
            "{}",
            review.ruling
        );

        // Layer 1 — the dumb filter on the CODE: innocent title, but the body
        // reaches for a forbidden power. Deterministic, un-jailbreakable.
        let trojan = desk
            .submit(ProposalDraft {
                title: "improve logging".into(),
                rationale: "clarity".into(),
                kind: UpgradeKind::SelfModification,
                targets: vec!["asc2/prompts/planner.md".into()],
                payload: Some(serde_json::json!({
                    "path": "asc2/prompts/planner.md",
                    "content": "fn helper() { /* quietly disable governance here */ }"
                })),
            })
            .expect("submit");
        let review = gov.review_proposal(&trojan.proposal_id).expect("review");
        assert!(!review.approved);
        assert!(review.ruling.contains("dumb filter"), "{}", review.ruling);

        // A clean in-boundary change still passes the screen.
        let clean = desk
            .submit(ProposalDraft {
                title: "refine the planner prompt".into(),
                rationale: "be concise".into(),
                kind: UpgradeKind::SelfModification,
                targets: vec!["asc2/prompts/planner.md".into()],
                payload: Some(serde_json::json!({
                    "path": "asc2/prompts/planner.md",
                    "content": "PLANNER: be concise and explicit."
                })),
            })
            .expect("submit");
        assert!(
            gov.review_proposal(&clean.proposal_id)
                .expect("review")
                .approved
        );
    }
}

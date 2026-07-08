//! The CEO Guardian — the executive's continuous, autonomous OS-security patrol.
//!
//! This is the organ that makes the CEO *continuously monitor the operating
//! system*: on every patrol it turns the project's own [device layer]
//! (crate::device_agent) eyes on the host, feeds what it sees through the
//! [OS Guardian](crate::os_guardian) threat + data-leak assessors, and then —
//! for anything that scores dangerous — drives the fix through the SAME governed
//! machinery the CEO already commands:
//!
//! ```text
//!   observe ─▶ assess ─▶ remediate ─▶ record
//!   (device)   (guardian)   │           (store + chronicle + guardian ledger)
//!                           ├─ Observed            (informational — logged)
//!                           ├─ AutoHealed          (safe, non-destructive containment)
//!                           ├─ ApprovalRequired    (destructive host action → Policy proposal)
//!                           ├─ SelfModificationProposed (fix a weakness in the OS's own code)
//!                           └─ CapabilityProposed  (forge a detector tool / propose a sub-agent)
//! ```
//!
//! **Safety by construction.** The Guardian never performs a destructive host
//! action on its own — killing a process, deleting a file, blocking a socket. It
//! mirrors the [OS Guardian policy](crate::os_guardian::OsGuardianService::dlp_policy)
//! (`destructive_actions_enabled = false`, `remediation_requires_approval = true`):
//! every destructive remediation is submitted to the governance
//! [`ProposalDesk`](crate::governance::ProposalDesk) and waits for the court's
//! 51%. "Self-heal" here means the *safe* half — containment manifests, rotation
//! recommendations, canary plans, and forged detectors — plus opening the
//! approval-gated path for the rest.
//!
//! Owned by the [`Ceo`](crate::ceo::Ceo), so it *is* the single autonomous agent
//! watching the machine, not a second one. The CEO exposes it through
//! `Ceo::guardian_patrol()`; a background worker ticks it on a cadence.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::asc2::{Asc2RuntimeConfig, Asc2Service, ReasoningExecutor};
use crate::chronicle::{ChronicleService, EpisodeKind, RecordEpisodeRequest};
use crate::common::{AppError, TenantScope, new_id, now_ms, sha3_hex};
use crate::device_agent::DeviceCapabilities;
use crate::forge::{ForgeRequest, ForgeService};
use crate::governance::{
    ProjectProposal, ProposalDesk, ProposalDraft, ProposalStatus, UpgradeKind,
};
use crate::os_guardian::{
    DataLeakSignal, GuardianSeverity, OsGuardianEventKind, OsGuardianService,
    SubmitGuardianEventRequest, scan_outbound_text,
};

// ── Bounds — a single patrol is deliberately capped so a drift/incident storm
//    can never hammer the model or the host. ──────────────────────────────────
const MAX_PROCESSES_SCANNED: u64 = 200;
const MAX_SENSITIVE_FILES_READ: usize = 24;
const MAX_FINDINGS_PER_PATROL: usize = 64;
const MAX_APPROVAL_PROPOSALS_PER_PATROL: usize = 6;
const RECENT_FINDINGS_KEEP: usize = 500;
// Filesystem walk bounds — a depth- and count-capped BFS so a deep tree can
// never turn one patrol into an unbounded crawl.
const MAX_FS_DEPTH: usize = 3;
const MAX_DIRS_WALKED: usize = 300;
const MAX_ENTRIES_PER_DIR: usize = 400;
// Network scan bounds.
const MAX_CONNECTIONS_SCANNED: u64 = 400;
const MAX_LISTENER_FINDINGS: usize = 24;
const MAX_OUTBOUND_FINDINGS: usize = 16;
// Baseline surfaces unseen for this long are pruned (30 days), so the learned
// baseline tracks the live machine and can't grow without bound.
const BASELINE_RETENTION_MS: i64 = 30 * 24 * 3600 * 1000;
// Posture reflects only findings from the last 7 days; ~5 High-equivalent
// findings saturate the risk score.
const POSTURE_WINDOW_MS: i64 = 7 * 24 * 3600 * 1000;
const POSTURE_SATURATION: f64 = 3.5;

/// The triage reviewer's system prompt. It reasons about a single pattern hit —
/// is it a true positive, and what is the least-invasive correct fix — turning
/// the signature scan into a reasoned verdict. Kept to plain prose, no side
/// effects: triage only enriches a finding, it never authorizes an action.
const GUARDIAN_TRIAGE_SYSTEM: &str = "You are the security triage analyst inside an autonomous OS \
     guardian. A deterministic scan surfaced one candidate finding on the host. Judge, briefly and \
     conservatively, whether it is a genuine threat or a likely false positive, and name the LEAST \
     invasive correct remediation. You never take action — you only advise. Answer in 1-2 plain \
     sentences, no JSON, no preamble.";

/// What class of security problem a finding represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// A process/behaviour matching malware-like execution patterns.
    Malware,
    /// A security weakness in the OS's own posture/config (permissive reach,
    /// secrets at rest, world-open surfaces) — a "loop hole".
    Loophole,
    /// Sensitive data exposed or moving toward an exfiltration surface.
    DataLeak,
    /// A deviation worth noting that is not yet one of the above.
    Anomaly,
}

impl FindingKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Malware => "malware",
            Self::Loophole => "loophole",
            Self::DataLeak => "data_leak",
            Self::Anomaly => "anomaly",
        }
    }
}

/// How the Guardian responded to a finding. Destructive fixes are never taken
/// autonomously — they open an approval-gated path instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemediationMode {
    /// Informational — recorded, no action taken.
    Observed,
    /// A safe, non-destructive containment step was applied and recorded.
    AutoHealed,
    /// A destructive host remediation was drafted and submitted to governance.
    ApprovalRequired,
    /// A weakness in the OS's own code/config was turned into an approval-gated
    /// self-modification proposal.
    SelfModificationProposed,
    /// A missing detector/agent capability was forged or proposed.
    CapabilityProposed,
}

impl RemediationMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::AutoHealed => "auto_healed",
            Self::ApprovalRequired => "approval_required",
            Self::SelfModificationProposed => "self_modification_proposed",
            Self::CapabilityProposed => "capability_proposed",
        }
    }
}

/// One assessed security finding and the Guardian's response to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityFinding {
    pub finding_id: String,
    pub kind: FindingKind,
    pub severity: GuardianSeverity,
    /// The concrete subject — a process image, a file path, a posture flag.
    pub subject: String,
    /// Human-readable explanation of what was seen and why it scored.
    pub detail: String,
    /// Which scan surfaced it (`process_scan`, `filesystem_scan`, `posture_scan`).
    pub source: String,
    /// Redacted evidence (fingerprints, matched pattern names) — never raw secrets.
    pub evidence: Vec<String>,
    pub remediation: RemediationMode,
    /// A one-line description of the fix taken or proposed.
    pub remediation_detail: String,
    /// The governance proposal opened for this finding, if any.
    #[serde(default)]
    pub proposal_id: Option<String>,
    /// The guardian-ledger receipt the OS Guardian minted for the assessment.
    #[serde(default)]
    pub receipt_id: Option<String>,
    pub created_at_ms: i64,
}

/// What a single patrol touched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatrolCounts {
    pub processes_scanned: usize,
    pub files_scanned: usize,
    pub roots_scanned: usize,
    pub dirs_walked: usize,
    pub connections_scanned: usize,
    pub findings: usize,
    pub auto_healed: usize,
    pub approvals_requested: usize,
    pub self_mods_proposed: usize,
    pub capabilities_proposed: usize,
}

/// The report from one full patrol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianPatrolReport {
    pub patrol_id: String,
    pub counts: PatrolCounts,
    pub findings: Vec<SecurityFinding>,
    /// The single worst severity observed this patrol (None if nothing scored).
    pub peak_severity: Option<GuardianSeverity>,
    pub summary: String,
    /// True when a remote model backs the drafting of self-mods / forged tools.
    pub model_backed: bool,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
}

/// Guardian status snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeoGuardianStatus {
    pub status: String,
    pub model_backed: bool,
    pub scan_roots: Vec<String>,
    pub self_modification_enabled: bool,
    pub capability_forging_enabled: bool,
    pub total_patrols: u64,
    pub total_findings: u64,
    pub open_proposals: u64,
    /// Distinct host surfaces (processes, listeners, files) in the learned baseline.
    pub baseline_size: u64,
    pub last_patrol_at_ms: Option<i64>,
    pub peak_severity: Option<GuardianSeverity>,
}

/// The outcome of the executor carrying out (or declining to carry out) one
/// court-approved remediation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemediationRecord {
    pub proposal_id: String,
    /// The concrete action considered (`terminate_process`, `advisory`, …).
    pub action: String,
    /// `executed` | `planned` | `advisory` | `failed` | `skipped`.
    pub mode: String,
    pub detail: String,
    pub created_at_ms: i64,
}

/// A single legible read on the machine's current security posture, aggregated
/// from recent findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianPosture {
    /// Health in [0,1] (1.0 = clean); `1 - risk`.
    pub health: f64,
    /// Risk in [0,1] from recent severity-weighted findings.
    pub risk: f64,
    /// A letter grade derived from health (A–F).
    pub grade: String,
    /// Findings in the recency window, by severity name.
    pub recent_by_severity: BTreeMap<String, u64>,
    pub recent_findings: u64,
    /// Findings whose remediation is still awaiting the court / a human.
    pub open_remediations: u64,
    pub peak_severity: Option<GuardianSeverity>,
    pub computed_at_ms: i64,
}

/// Static configuration resolved once from the environment.
#[derive(Debug, Clone)]
struct GuardianConfig {
    /// Directories the Guardian is allowed to inspect for loopholes/leaks.
    scan_roots: Vec<PathBuf>,
    /// The Guardian's own data dir — excluded from scans so it never flags itself.
    own_dir: PathBuf,
    /// Draft real self-modifications for systemic loopholes (needs a model).
    self_modification_enabled: bool,
    /// Forge detector tools for capability gaps (needs a model).
    capability_forging_enabled: bool,
}

/// The CEO's security cortex. Composes the handles the CEO already holds — so it
/// speaks with the executive's authority, not a parallel one.
#[derive(Clone)]
pub struct CeoGuardian {
    store: Arc<Mutex<Connection>>,
    device: Arc<DeviceCapabilities>,
    guardian: OsGuardianService,
    /// The narrow channel to the court — the ONLY way a destructive fix or a
    /// self-modification reaches action.
    desk: ProposalDesk,
    forge: ForgeService,
    asc2: Asc2Service,
    chronicle: ChronicleService,
    reasoner: Arc<dyn ReasoningExecutor>,
    config: Arc<GuardianConfig>,
    model_backed: bool,
}

impl CeoGuardian {
    /// Build the Guardian over the handles the CEO already owns.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        data_dir: impl AsRef<Path>,
        device: Arc<DeviceCapabilities>,
        guardian: OsGuardianService,
        desk: ProposalDesk,
        forge: ForgeService,
        asc2: Asc2Service,
        chronicle: ChronicleService,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let own_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&own_dir)
            .map_err(|e| AppError::Internal(format!("failed to create ceo_guardian dir: {e}")))?;
        let path = own_dir.join("ceo_guardian.sqlite");
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open ceo_guardian store: {e}")))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE IF NOT EXISTS guardian_findings (
                    finding_id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    severity INTEGER NOT NULL,
                    created_at_ms INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS guardian_patrols (
                    patrol_id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL);
                 CREATE TABLE IF NOT EXISTS guardian_baseline (
                    kind TEXT NOT NULL,
                    ident TEXT NOT NULL,
                    fingerprint TEXT,
                    first_seen_ms INTEGER NOT NULL,
                    last_seen_ms INTEGER NOT NULL,
                    seen_count INTEGER NOT NULL DEFAULT 1,
                    PRIMARY KEY (kind, ident));
                 CREATE TABLE IF NOT EXISTS guardian_remediations (
                    proposal_id TEXT PRIMARY KEY,
                    action TEXT NOT NULL,
                    mode TEXT NOT NULL,
                    detail TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL);",
            )
            .map_err(sql_err)?;

        let model_backed = Asc2RuntimeConfig::from_env().remote_endpoint.is_some();
        let self_modification_enabled =
            model_backed && env_on("ASTRA_GUARDIAN_SELF_MOD", model_backed);
        let capability_forging_enabled =
            model_backed && env_on("ASTRA_GUARDIAN_FORGE", model_backed);
        let scan_roots = resolve_scan_roots(&own_dir);

        let config = GuardianConfig {
            scan_roots,
            own_dir,
            self_modification_enabled,
            capability_forging_enabled,
        };

        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
            guardian,
            desk,
            forge,
            asc2,
            chronicle,
            reasoner,
            config: Arc::new(config),
            model_backed,
        })
    }

    /// Run one full patrol: observe the host, assess every observation, and
    /// remediate (safely, or by opening an approval-gated path). Bounded — each
    /// scan and each remediation class is capped.
    pub async fn patrol(&self) -> Result<GuardianPatrolReport, AppError> {
        let started_at_ms = now_ms();
        let patrol_id = new_id("guardian_patrol");
        let mut counts = PatrolCounts::default();

        // The baseline is "established" once at least one prior patrol has swept
        // and seeded it. Until then, drift detection only records the surface —
        // it never flags "new", so the first patrol can't fire on everything.
        let baseline_ready = self.baseline_established();

        // ── Observe + assess ────────────────────────────────────────────────
        // Snapshot processes once; the network scan reuses it to name the process
        // that owns each socket (PID → image correlation).
        let procs = self.host_processes().unwrap_or_default();
        let pid_map: std::collections::HashMap<u32, String> = procs
            .iter()
            .filter_map(|p| p.pid.map(|pid| (pid, p.name.clone())))
            .collect();

        let mut raw = Vec::new();
        match self.scan_processes(&mut counts, &procs) {
            Ok(mut f) => raw.append(&mut f),
            Err(e) => tracing::warn!(%e, "guardian process scan failed"),
        }
        match self.scan_filesystem(&mut counts, baseline_ready) {
            Ok(mut f) => raw.append(&mut f),
            Err(e) => tracing::warn!(%e, "guardian filesystem scan failed"),
        }
        match self.scan_network(&mut counts, baseline_ready, &pid_map) {
            Ok(mut f) => raw.append(&mut f),
            Err(e) => tracing::warn!(%e, "guardian network scan failed"),
        }
        match self.scan_posture(&mut counts) {
            Ok(mut f) => raw.append(&mut f),
            Err(e) => tracing::warn!(%e, "guardian posture scan failed"),
        }
        raw.truncate(MAX_FINDINGS_PER_PATROL);

        // ── Remediate — hardest-first so the caps spend on what matters most ──
        raw.sort_by(|a, b| b.severity.cmp(&a.severity));

        // Bounded intelligence: spend ONE model call per patrol reasoning about
        // the single worst finding — is it real, and what is the least-invasive
        // fix — folding that verdict into the finding the court will see.
        if self.model_backed
            && raw
                .first()
                .is_some_and(|f| f.severity >= GuardianSeverity::Moderate)
        {
            let top = raw[0].clone();
            if let Some(verdict) = self.triage(&top).await {
                raw[0].detail = format!("{} | AI triage: {}", raw[0].detail, verdict);
                raw[0].evidence.push("ai_triaged=true".into());
            }
        }

        let mut findings = Vec::with_capacity(raw.len());
        let mut approvals_left = MAX_APPROVAL_PROPOSALS_PER_PATROL;
        let mut self_mod_budget = usize::from(self.config.self_modification_enabled);
        let mut forge_budget = usize::from(self.config.capability_forging_enabled);
        let mut peak: Option<GuardianSeverity> = None;

        for mut finding in raw {
            peak = Some(match peak {
                Some(p) if p >= finding.severity => p,
                _ => finding.severity,
            });
            self.remediate(
                &mut finding,
                &mut approvals_left,
                &mut self_mod_budget,
                &mut forge_budget,
            )
            .await;
            match finding.remediation {
                RemediationMode::AutoHealed => counts.auto_healed += 1,
                RemediationMode::ApprovalRequired => counts.approvals_requested += 1,
                RemediationMode::SelfModificationProposed => counts.self_mods_proposed += 1,
                RemediationMode::CapabilityProposed => counts.capabilities_proposed += 1,
                RemediationMode::Observed => {}
            }
            self.record_finding(&finding)?;
            findings.push(finding);
        }
        counts.findings = findings.len();

        let finished_at_ms = now_ms();
        let summary = format!(
            "patrol scanned {} processes, {} files across {} dirs ({} roots), {} connections; \
             {} findings ({} auto-healed, {} approvals requested, {} self-mods proposed, \
             {} capabilities)",
            counts.processes_scanned,
            counts.files_scanned,
            counts.dirs_walked,
            counts.roots_scanned,
            counts.connections_scanned,
            counts.findings,
            counts.auto_healed,
            counts.approvals_requested,
            counts.self_mods_proposed,
            counts.capabilities_proposed,
        );

        let report = GuardianPatrolReport {
            patrol_id,
            counts,
            findings,
            peak_severity: peak,
            summary: summary.clone(),
            model_backed: self.model_backed,
            started_at_ms,
            finished_at_ms,
        };
        self.record_patrol(&report)?;

        // A patrol that surfaced something meaningful is remembered so the rest
        // of the mind (recall, curriculum, active inference) can see it.
        if report.counts.findings > 0 {
            let importance = peak.map(importance_for_severity).unwrap_or(0.4);
            let _ = self.chronicle.record(RecordEpisodeRequest {
                kind: EpisodeKind::Event,
                content: summary,
                rationale: None,
                source: Some("ceo_guardian".into()),
                source_ref: Some(report.patrol_id.clone()),
                tags: vec!["security".into(), "guardian".into(), "patrol".into()],
                importance: Some(importance),
                due_at_ms: None,
                tenant_scope: TenantScope::Global,
            });
        }

        Ok(report)
    }

    // ── Observe: processes ──────────────────────────────────────────────────

    /// Snapshot the host's running processes once per patrol. Shared by the
    /// process scan and the network scan (which correlates a socket's PID to its
    /// owning process image).
    fn host_processes(&self) -> Result<Vec<HostProcess>, AppError> {
        let out = self
            .device
            .execute("process_list", &json!({ "limit": MAX_PROCESSES_SCANNED }))
            .map_err(AppError::Internal)?;
        let rows = out
            .get("processes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut procs = Vec::new();
        for row in &rows {
            let Some(line) = row.as_str() else { continue };
            let (name, pid) = parse_process_row(line);
            if name.is_empty() {
                continue;
            }
            procs.push(HostProcess {
                name,
                pid,
                raw: truncate(line, 240),
            });
        }
        Ok(procs)
    }

    /// Score each running process through the OS Guardian's threat evaluator.
    /// Malware-like execution patterns become findings.
    fn scan_processes(
        &self,
        counts: &mut PatrolCounts,
        procs: &[HostProcess],
    ) -> Result<Vec<SecurityFinding>, AppError> {
        let mut findings = Vec::new();
        for proc in procs {
            counts.processes_scanned += 1;

            let mut metadata = BTreeMap::new();
            metadata.insert("raw".into(), proc.raw.clone());
            let assessment = self.guardian.submit_event(SubmitGuardianEventRequest {
                kind: OsGuardianEventKind::Process,
                source: "ceo-guardian/process_scan".into(),
                subject: proc.name.clone(),
                pid: proc.pid,
                metadata,
                observed_at_ms: None,
            })?;

            // Only surface what the deterministic evaluator scored as real risk.
            if assessment.evaluation.severity >= GuardianSeverity::Moderate {
                let kind = if looks_like_malware(&proc.name) {
                    FindingKind::Malware
                } else {
                    FindingKind::Anomaly
                };
                findings.push(SecurityFinding {
                    finding_id: new_id("finding"),
                    kind,
                    severity: assessment.evaluation.severity,
                    subject: proc.name.clone(),
                    detail: assessment.evaluation.rationale.join("; "),
                    source: "process_scan".into(),
                    evidence: vec![format!("pid={:?}", proc.pid), truncate(&proc.raw, 120)],
                    remediation: RemediationMode::Observed,
                    remediation_detail: String::new(),
                    proposal_id: None,
                    receipt_id: Some(assessment.receipt.receipt_id),
                    created_at_ms: now_ms(),
                });
            }
        }
        Ok(findings)
    }

    // ── Observe: filesystem (loopholes + data leaks at rest) ────────────────

    /// Bounded, depth-limited BFS over the configured scan roots. Every sensitive
    /// file it meets is read (under the device read-cap and a global read budget)
    /// and run through the DLP scanner + OS Guardian — a plaintext secret at rest
    /// is both a loophole and a potential data leak. The walk is capped on depth,
    /// directory count, and entries per directory; heavy build/VCS/cache dirs and
    /// the Guardian's own data dir are skipped so it stays a patrol, not a crawl.
    fn scan_filesystem(
        &self,
        counts: &mut PatrolCounts,
        baseline_ready: bool,
    ) -> Result<Vec<SecurityFinding>, AppError> {
        let mut findings = Vec::new();
        let mut read_budget = MAX_SENSITIVE_FILES_READ;

        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        for root in &self.config.scan_roots {
            queue.push_back((root.clone(), 0));
            counts.roots_scanned += 1;
        }

        while let Some((dir, depth)) = queue.pop_front() {
            if counts.dirs_walked >= MAX_DIRS_WALKED {
                break;
            }
            let listing = match self
                .device
                .execute("fs_list", &json!({ "path": dir.to_string_lossy() }))
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::debug!(dir = %dir.display(), %e, "guardian fs_list skipped");
                    continue;
                }
            };
            counts.dirs_walked += 1;
            let entries = listing
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            for entry in entries.into_iter().take(MAX_ENTRIES_PER_DIR) {
                let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let is_dir = entry
                    .get("is_dir")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let full = dir.join(name);

                if is_dir {
                    // Descend, bounded: within depth, not a skipped heavy dir, not
                    // the Guardian's own data dir, and not past the walk cap.
                    if depth + 1 <= MAX_FS_DEPTH
                        && !should_skip_dir(name)
                        && !full.starts_with(&self.config.own_dir)
                        && counts.dirs_walked + queue.len() < MAX_DIRS_WALKED
                    {
                        queue.push_back((full, depth + 1));
                    }
                    continue;
                }

                if !is_sensitive_filename(name) {
                    continue;
                }
                counts.files_scanned += 1;
                if read_budget == 0 {
                    continue;
                }
                let content = match self
                    .device
                    .execute("fs_read", &json!({ "path": full.to_string_lossy() }))
                {
                    Ok(v) => v
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    Err(_) => continue,
                };
                read_budget -= 1;
                if content.trim().is_empty() {
                    continue;
                }

                // DLP scan the content — this is the honest detector, not a guess.
                let scan = scan_outbound_text(&content);
                let subject = full.to_string_lossy().to_string();

                // Baseline drift: a credential-class file whose contents changed
                // since a prior patrol is a strong, low-noise signal on its own.
                let fingerprint = sha3_hex(content.as_bytes());
                let fp16 = &fingerprint[..16];
                if matches!(
                    self.baseline_observe("sensitive_file", &subject, Some(fp16), baseline_ready),
                    BaselineDrift::Changed
                ) {
                    findings.push(SecurityFinding {
                        finding_id: new_id("finding"),
                        kind: FindingKind::Anomaly,
                        severity: GuardianSeverity::Moderate,
                        subject: subject.clone(),
                        detail: format!(
                            "a sensitive file's contents changed since the baseline ({name}); \
                             confirm the modification was expected"
                        ),
                        source: "filesystem_scan".into(),
                        evidence: vec![format!("new_fingerprint={fp16}"), "drift=changed".into()],
                        remediation: RemediationMode::Observed,
                        remediation_detail: String::new(),
                        proposal_id: None,
                        receipt_id: None,
                        created_at_ms: now_ms(),
                    });
                }

                if scan.risk_score >= 0.30 && !scan.data_classes_found.is_empty() {
                    let signal = self.guardian.analyze_dlp_signal(DataLeakSignal {
                        source_process: "file_at_rest".into(),
                        subject: subject.clone(),
                        destination: None,
                        owner_authorized: true,
                        data_classes: scan.data_classes_found.clone(),
                        leak_vectors: Vec::new(),
                        metadata: BTreeMap::new(),
                        content_sample: Some(truncate(&content, 512)),
                        observed_at_ms: None,
                    })?;
                    let evidence: Vec<String> = scan
                        .matches
                        .iter()
                        .take(8)
                        .map(|m| format!("{}={}", m.pattern_name, m.redacted_match))
                        .collect();
                    findings.push(SecurityFinding {
                        finding_id: new_id("finding"),
                        kind: FindingKind::DataLeak,
                        severity: signal.verdict.severity,
                        subject,
                        detail: format!(
                            "plaintext sensitive data at rest ({} match(es), risk {:.2}): {}",
                            scan.matches.len(),
                            scan.risk_score,
                            signal.verdict.rationale.join("; ")
                        ),
                        source: "filesystem_scan".into(),
                        evidence,
                        remediation: RemediationMode::Observed,
                        remediation_detail: String::new(),
                        proposal_id: None,
                        receipt_id: Some(signal.receipt.receipt_id),
                        created_at_ms: now_ms(),
                    });
                } else if is_high_value_filename(name) {
                    // A high-value credential file with no clear-text hit is still
                    // a loophole worth noting (it should not sit unencrypted here).
                    findings.push(SecurityFinding {
                        finding_id: new_id("finding"),
                        kind: FindingKind::Loophole,
                        severity: GuardianSeverity::Low,
                        subject,
                        detail: format!(
                            "credential-class file present ({name}); confirm it is encrypted \
                             or scoped away from broad reach"
                        ),
                        source: "filesystem_scan".into(),
                        evidence: vec![format!(
                            "fingerprint={}",
                            &sha3_hex(content.as_bytes())[..16]
                        )],
                        remediation: RemediationMode::Observed,
                        remediation_detail: String::new(),
                        proposal_id: None,
                        receipt_id: None,
                        created_at_ms: now_ms(),
                    });
                }
            }
        }
        Ok(findings)
    }

    // ── Observe: network (exposed listeners + outbound endpoints) ───────────

    /// Enumerate host network connections and surface exposed listeners. A socket
    /// bound to all interfaces is a standing loophole; a listener that was NOT
    /// present in prior patrols is a drift anomaly (possible backdoor/C2 staging)
    /// even on loopback. Each listener's PID is correlated to its owning process
    /// image via `pid_map`: a socket owned by a malware-like process is escalated
    /// to a Malware finding. Everything is scored through the OS Guardian's Network
    /// path so it lands in the same ledger. Read-only — it only observes.
    fn scan_network(
        &self,
        counts: &mut PatrolCounts,
        baseline_ready: bool,
        pid_map: &std::collections::HashMap<u32, String>,
    ) -> Result<Vec<SecurityFinding>, AppError> {
        let out = match self.device.execute(
            "net_connections",
            &json!({ "limit": MAX_CONNECTIONS_SCANNED }),
        ) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(%e, "guardian net_connections unavailable");
                return Ok(Vec::new());
            }
        };
        let rows = out
            .get("connections")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut findings = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut outbound_findings = 0usize;
        for row in &rows {
            let Some(line) = row.as_str() else { continue };
            let conn = parse_conn_row(line);
            counts.connections_scanned += 1;

            // Outbound: a malware-like process reaching a routable remote host is a
            // strong exfiltration / command-and-control signal. Bounded + deduped
            // on (pid, foreign) separately from listeners.
            if !conn.is_listen {
                if outbound_findings >= MAX_OUTBOUND_FINDINGS || !is_public_endpoint(&conn.foreign)
                {
                    continue;
                }
                let ListenerOwner::Named(owner_name) = resolve_owner(conn.pid, pid_map) else {
                    continue;
                };
                if !looks_like_malware(&owner_name) {
                    continue;
                }
                if !seen.insert(format!("out:{}:{}", conn.pid.unwrap_or(0), conn.foreign)) {
                    continue;
                }
                outbound_findings += 1;
                let mut metadata = BTreeMap::new();
                metadata.insert("state".into(), "established".into());
                metadata.insert("foreign".into(), conn.foreign.clone());
                metadata.insert("owner_process".into(), owner_name.clone());
                let assessment = self.guardian.submit_event(SubmitGuardianEventRequest {
                    kind: OsGuardianEventKind::Network,
                    source: "ceo-guardian/network_scan".into(),
                    subject: format!("outbound:{}", conn.foreign),
                    pid: conn.pid,
                    metadata,
                    observed_at_ms: None,
                })?;
                let severity = assessment.evaluation.severity.max(GuardianSeverity::High);
                findings.push(SecurityFinding {
                    finding_id: new_id("finding"),
                    kind: FindingKind::Malware,
                    severity,
                    subject: format!("outbound:{}", conn.foreign),
                    detail: format!(
                        "a malware-like process '{owner_name}' holds an outbound connection to \
                         {} — possible exfiltration or command-and-control",
                        conn.foreign
                    ),
                    source: "network_scan".into(),
                    evidence: vec![
                        format!("owner={owner_name}"),
                        format!("pid={:?}", conn.pid),
                        truncate(line, 120),
                    ],
                    remediation: RemediationMode::Observed,
                    remediation_detail: String::new(),
                    proposal_id: None,
                    receipt_id: Some(assessment.receipt.receipt_id),
                    created_at_ms: now_ms(),
                });
                continue;
            }

            if conn.local.is_empty() || !seen.insert(conn.local.clone()) {
                continue;
            }
            // Baseline: is this listener new since prior patrols?
            let is_new = matches!(
                self.baseline_observe("listener", &conn.local, None, baseline_ready),
                BaselineDrift::New
            );
            let all_interfaces = binds_all_interfaces(&conn.local);
            // Nothing notable about a known loopback listener — skip it.
            if !all_interfaces && !is_new {
                continue;
            }
            if findings.len() >= MAX_LISTENER_FINDINGS {
                continue;
            }

            // Correlate the socket to the process that owns it.
            let owner = resolve_owner(conn.pid, pid_map);
            let owner_is_malware =
                matches!(&owner, ListenerOwner::Named(n) if looks_like_malware(n));

            let mut reasons = Vec::new();
            if all_interfaces {
                reasons.push("listening on all interfaces (reachable off-host)".to_string());
            }
            if is_new {
                reasons.push("newly appeared since the baseline".to_string());
            }
            reasons.push(match &owner {
                ListenerOwner::Named(n) => format!("owned by process '{n}'"),
                ListenerOwner::UnknownPid(p) => {
                    format!("owner pid {p} is not in the live process list (exited or hidden)")
                }
                ListenerOwner::None => "owner pid unavailable".to_string(),
            });
            if owner_is_malware {
                reasons.push("OWNER MATCHES A MALWARE-LIKE PROCESS".to_string());
            }

            let kind = if owner_is_malware {
                FindingKind::Malware
            } else if all_interfaces {
                FindingKind::Loophole
            } else {
                FindingKind::Anomaly
            };

            // Score it through the OS Guardian so it lands in the same ledger.
            let mut metadata = BTreeMap::new();
            metadata.insert("state".into(), "listening".into());
            metadata.insert("local".into(), conn.local.clone());
            if is_new {
                metadata.insert("drift".into(), "new".into());
            }
            if let ListenerOwner::Named(n) = &owner {
                metadata.insert("owner_process".into(), n.clone());
            }
            let assessment = self.guardian.submit_event(SubmitGuardianEventRequest {
                kind: OsGuardianEventKind::Network,
                source: "ceo-guardian/network_scan".into(),
                subject: format!("listen:{}", conn.local),
                pid: conn.pid,
                metadata,
                observed_at_ms: None,
            })?;
            // Floor: a malware-owned listener is High; a new all-interface listener
            // is at least Moderate; otherwise Low. Never below the Guardian's score.
            let floor = if owner_is_malware {
                GuardianSeverity::High
            } else if all_interfaces && is_new {
                GuardianSeverity::Moderate
            } else {
                GuardianSeverity::Low
            };
            let severity = assessment.evaluation.severity.max(floor);
            let mut evidence = vec![format!("pid={:?}", conn.pid), truncate(line, 120)];
            if let ListenerOwner::Named(n) = &owner {
                evidence.push(format!("owner={n}"));
            }
            findings.push(SecurityFinding {
                finding_id: new_id("finding"),
                kind,
                severity,
                subject: format!("listen:{}", conn.local),
                detail: format!("network listener {} — {}", conn.local, reasons.join("; ")),
                source: "network_scan".into(),
                evidence,
                remediation: RemediationMode::Observed,
                remediation_detail: String::new(),
                proposal_id: None,
                receipt_id: Some(assessment.receipt.receipt_id),
                created_at_ms: now_ms(),
            });
        }
        Ok(findings)
    }

    // ── Observe: posture (loopholes in the OS's own configuration) ──────────

    /// Inspect the system's own security posture for structural loopholes.
    fn scan_posture(&self, _counts: &mut PatrolCounts) -> Result<Vec<SecurityFinding>, AppError> {
        let info = self
            .device
            .execute("system_info", &json!({}))
            .map_err(AppError::Internal)?;
        let mut findings = Vec::new();

        // The device layer running in permissive ("reach the whole machine")
        // mode is a real, structural loophole: any compromised agent inherits
        // full host reach. This is the OS's own config — the natural target of an
        // approval-gated self-modification.
        if info
            .get("allow_everything")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            findings.push(SecurityFinding {
                finding_id: new_id("finding"),
                kind: FindingKind::Loophole,
                severity: GuardianSeverity::Moderate,
                subject: "device_policy:permissive".into(),
                detail: "the supervised device layer runs in permissive mode (full host reach); \
                         any compromised agent inherits it. Prefer ASTRA_DEVICE_MODE=confined or a \
                         narrowed policy."
                    .into(),
                source: "posture_scan".into(),
                evidence: vec![
                    format!(
                        "os={}",
                        info.get("os").and_then(Value::as_str).unwrap_or("?")
                    ),
                    "allow_everything=true".into(),
                ],
                remediation: RemediationMode::Observed,
                remediation_detail: String::new(),
                proposal_id: None,
                receipt_id: None,
                created_at_ms: now_ms(),
            });
        }
        Ok(findings)
    }

    // ── Remediate ───────────────────────────────────────────────────────────

    /// Decide and take the response for one finding, respecting the per-patrol
    /// budgets. Mutates the finding in place with the chosen mode + detail.
    async fn remediate(
        &self,
        finding: &mut SecurityFinding,
        approvals_left: &mut usize,
        self_mod_budget: &mut usize,
        forge_budget: &mut usize,
    ) {
        // Informational / low — record only.
        if finding.severity <= GuardianSeverity::Low && finding.kind != FindingKind::Malware {
            finding.remediation = RemediationMode::Observed;
            finding.remediation_detail =
                "logged for trend analysis; below the action threshold".into();
            let _ = self.safe_contain(finding);
            return;
        }

        // A systemic loophole in the OS's own config → approval-gated self-mod.
        if finding.kind == FindingKind::Loophole
            && finding.severity >= GuardianSeverity::Moderate
            && *self_mod_budget > 0
        {
            match self.propose_self_modification(finding).await {
                Ok(proposal_id) => {
                    *self_mod_budget -= 1;
                    finding.remediation = RemediationMode::SelfModificationProposed;
                    finding.remediation_detail = format!(
                        "drafted a bounded self-modification and submitted it to governance \
                         (proposal {proposal_id})"
                    );
                    finding.proposal_id = Some(proposal_id);
                    return;
                }
                Err(e) => {
                    tracing::warn!(%e, "guardian self-modification draft failed; falling back");
                }
            }
        }

        // High/critical, or malware → destructive remediation needs the court.
        let destructive =
            finding.severity >= GuardianSeverity::High || finding.kind == FindingKind::Malware;
        if destructive && *approvals_left > 0 {
            match self.propose_remediation(finding) {
                Ok(proposal_id) => {
                    *approvals_left -= 1;
                    finding.remediation = RemediationMode::ApprovalRequired;
                    finding.remediation_detail = format!(
                        "destructive remediation submitted to governance for approval \
                         (proposal {proposal_id}); NOT executed autonomously"
                    );
                    finding.proposal_id = Some(proposal_id);
                    // Take the safe containment step now so exposure is reduced
                    // while the court deliberates.
                    let _ = self.safe_contain(finding);
                    return;
                }
                Err(e) => tracing::warn!(%e, "guardian remediation proposal failed"),
            }
        }

        // Missing detector capability → forge one (bounded, model-backed).
        if finding.kind == FindingKind::Malware && *forge_budget > 0 {
            if let Some(detail) = self.forge_detector(finding).await {
                *forge_budget -= 1;
                finding.remediation = RemediationMode::CapabilityProposed;
                finding.remediation_detail = detail;
                let _ = self.safe_contain(finding);
                return;
            }
        }

        // Otherwise: apply the safe, non-destructive containment step.
        finding.remediation = RemediationMode::AutoHealed;
        finding.remediation_detail = self
            .safe_contain(finding)
            .unwrap_or_else(|e| format!("containment record failed: {e}"));
    }

    /// Reason about one finding with the model: a concise true/false-positive
    /// verdict and the least-invasive fix. Advisory only — enriches the finding,
    /// never authorizes an action. Returns `None` on any model error/empty reply
    /// so the deterministic path is never blocked by the reasoner.
    async fn triage(&self, finding: &SecurityFinding) -> Option<String> {
        let user = format!(
            "Candidate finding:\n  class: {}\n  severity: {:?}\n  subject: {}\n  detail: {}\n  \
             evidence: {}\n\nIs this a genuine threat or a likely false positive, and what is the \
             least-invasive correct fix?",
            finding.kind.as_str(),
            finding.severity,
            finding.subject,
            finding.detail,
            finding.evidence.join(", "),
        );
        match self
            .reasoner
            .complete(GUARDIAN_TRIAGE_SYSTEM.to_string(), user)
            .await
        {
            Ok(text) if !text.trim().is_empty() => Some(truncate(text.trim(), 600)),
            _ => None,
        }
    }

    /// The safe half of self-heal: write a non-destructive containment manifest
    /// into the Guardian's OWN data dir (never the user's files), so exposure is
    /// documented and a rotation/quarantine plan exists. Returns a one-line note.
    fn safe_contain(&self, finding: &SecurityFinding) -> Result<String, String> {
        let dir = self.config.own_dir.join("containment");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let plan = match finding.kind {
            FindingKind::DataLeak => {
                "rotate the exposed credential and move it into a scoped secret store; \
                 do not transmit the raw value"
            }
            FindingKind::Malware => {
                "quarantine candidate flagged for owner review; termination requires approval"
            }
            FindingKind::Loophole => {
                "tighten the surface (confine device policy / encrypt at rest); tracked for approval"
            }
            FindingKind::Anomaly => "monitor; escalate if it recurs",
        };
        let manifest = json!({
            "finding_id": finding.finding_id,
            "kind": finding.kind.as_str(),
            "severity": finding.severity,
            "subject": finding.subject,
            "containment_plan": plan,
            "destructive": false,
            "recorded_at_ms": now_ms(),
        });
        let file = dir.join(format!("{}.json", finding.finding_id));
        std::fs::write(
            &file,
            serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
        )
        .map_err(|e| e.to_string())?;
        Ok(format!("safe containment recorded: {plan}"))
    }

    /// Open an approval-gated destructive remediation as a Policy proposal. The
    /// court's 51% — not the Guardian — authorizes any host-altering action.
    fn propose_remediation(&self, finding: &SecurityFinding) -> Result<String, AppError> {
        let action = match finding.kind {
            FindingKind::Malware => "terminate and quarantine the flagged process",
            FindingKind::DataLeak => "block the exfiltration path and rotate the exposed secret",
            FindingKind::Loophole => "close the exposed surface",
            FindingKind::Anomaly => "contain the anomalous activity",
        };
        let proposal = self.desk.submit(ProposalDraft {
            title: format!("Security remediation: {}", truncate(&finding.subject, 80)),
            rationale: format!(
                "[{}/{:?}] {}. Recommended destructive action: {action}. Requires the court's \
                 approval before any host-altering step is taken.",
                finding.kind.as_str(),
                finding.severity,
                finding.detail
            ),
            kind: UpgradeKind::Policy,
            targets: vec![finding.subject.clone()],
            payload: serde_json::to_value(finding).ok(),
        })?;
        self.journal_proposal(&proposal.proposal_id, "remediation", finding);
        Ok(proposal.proposal_id)
    }

    /// Turn a systemic loophole into a concrete, approval-gated self-modification
    /// of the OS's own code/config — the same path `Ceo::self_modify` uses.
    async fn propose_self_modification(
        &self,
        finding: &SecurityFinding,
    ) -> Result<String, AppError> {
        let goal = format!(
            "Close the security loophole '{}': {}. Make the smallest safe, bounded change that \
             hardens the OS's own configuration/code without weakening functionality.",
            finding.subject, finding.detail
        );
        let change = self.asc2.draft_self_modification(&goal).await?;
        let path = change.path.clone();
        let rationale = change.rationale.clone().unwrap_or_else(|| goal.clone());
        let proposal = self.desk.submit(ProposalDraft {
            title: format!(
                "Guardian self-modification: harden {}",
                truncate(&finding.subject, 64)
            ),
            rationale,
            kind: UpgradeKind::SelfModification,
            targets: vec![path],
            payload: serde_json::to_value(&change).ok(),
        })?;
        self.journal_proposal(&proposal.proposal_id, "self_modification", finding);
        Ok(proposal.proposal_id)
    }

    /// Forge a detector tool for a capability gap. Returns a note on success.
    async fn forge_detector(&self, finding: &SecurityFinding) -> Option<String> {
        let objective = format!(
            "A security detector tool that recognises host activity like '{}' ({}). It should \
             take a process/telemetry description and return a structured risk verdict.",
            finding.subject, finding.detail
        );
        match self.forge.forge(ForgeRequest { objective }).await {
            Ok(outcome) if outcome.minted => Some(format!(
                "forged and proof-minted detector '{}' into the live capability set",
                outcome.tool.name
            )),
            Ok(outcome) => Some(format!(
                "authored detector '{}' but its proof did not mint ({}); left inactive",
                outcome.tool.name, outcome.detail
            )),
            Err(e) => {
                tracing::warn!(%e, "guardian detector forge failed");
                None
            }
        }
    }

    fn journal_proposal(&self, proposal_id: &str, kind: &str, finding: &SecurityFinding) {
        let _ = self.chronicle.record(RecordEpisodeRequest {
            kind: EpisodeKind::Decision,
            content: format!(
                "CEO Guardian submitted a {kind} proposal ({proposal_id}) for {} finding '{}'",
                finding.kind.as_str(),
                truncate(&finding.subject, 80)
            ),
            rationale: Some(finding.detail.clone()),
            source: Some("ceo_guardian".into()),
            source_ref: Some(proposal_id.to_string()),
            tags: vec!["security".into(), "guardian".into(), "governance".into()],
            importance: Some(importance_for_severity(finding.severity)),
            due_at_ms: None,
            tenant_scope: TenantScope::Global,
        });
    }

    // ── Baseline / drift detection ──────────────────────────────────────────

    /// True once at least one prior patrol has seeded the baseline. Until then,
    /// drift detection only records surfaces — it never flags "new".
    fn baseline_established(&self) -> bool {
        let store = self.store.lock();
        store
            .query_row("SELECT COUNT(*) FROM guardian_patrols", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0)
            > 0
    }

    /// Record one observed surface item in the persisted baseline and report
    /// whether it is newly appeared or changed relative to prior patrols. The
    /// per-machine baseline is what turns a signature scan into anomaly-by-change
    /// detection: a listener or credential file that was never here before is a
    /// far stronger signal than any static pattern.
    fn baseline_observe(
        &self,
        kind: &str,
        ident: &str,
        fingerprint: Option<&str>,
        baseline_ready: bool,
    ) -> BaselineDrift {
        let now = now_ms();
        let store = self.store.lock();
        let prior: Option<Option<String>> = store
            .query_row(
                "SELECT fingerprint FROM guardian_baseline WHERE kind = ?1 AND ident = ?2",
                params![kind, ident],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .unwrap_or(None);
        let prior_existed = prior.is_some();
        let prior_fp = prior.flatten();
        let drift = decide_drift(
            baseline_ready,
            prior_existed,
            prior_fp.as_deref(),
            fingerprint,
        );
        let _ = store.execute(
            "INSERT INTO guardian_baseline \
             (kind, ident, fingerprint, first_seen_ms, last_seen_ms, seen_count) \
             VALUES (?1, ?2, ?3, ?4, ?4, 1) \
             ON CONFLICT(kind, ident) DO UPDATE SET \
                last_seen_ms = ?4, seen_count = seen_count + 1, \
                fingerprint = COALESCE(?3, fingerprint)",
            params![kind, ident, fingerprint, now],
        );
        drift
    }

    // ── Remediation executor (closing the loop) ──────────────────────────────

    /// Carry out the remediations the court has APPROVED. This is the only place
    /// the Guardian takes a host-altering action, and it is doubly gated: the
    /// governance court must have approved the proposal, AND destructive actions
    /// (process termination) additionally require `ASTRA_GUARDIAN_EXECUTE=on`.
    /// Without that flag a destructive remediation is recorded as `planned` (the
    /// exact command it would run) and left for a human; non-destructive
    /// remediations are recorded as `advisory`. Idempotent — each proposal is
    /// handled once (planned ones are re-checked so enabling the flag later runs
    /// them).
    pub async fn execute_approved_remediations(&self) -> Result<Vec<RemediationRecord>, AppError> {
        let execute_enabled = env_on("ASTRA_GUARDIAN_EXECUTE", false);
        let proposals = self.desk.proposals(200)?;
        let mut out = Vec::new();
        for proposal in proposals {
            if proposal.status != ProposalStatus::Approved || !is_guardian_remediation(&proposal) {
                continue;
            }
            if self.remediation_handled(&proposal.proposal_id) {
                continue;
            }
            let finding: Option<SecurityFinding> = proposal
                .payload
                .as_ref()
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            let record = match finding {
                Some(f) => self.carry_out(&proposal, &f, execute_enabled).await?,
                None => self.record_remediation(
                    &proposal.proposal_id,
                    "unknown",
                    "skipped",
                    "approved remediation carried no finding payload",
                )?,
            };
            out.push(record);
        }
        Ok(out)
    }

    /// Execute (or plan) one approved remediation, choosing the action from the
    /// finding. Process termination is the only destructive action, and only runs
    /// under the explicit execute flag; everything else is advisory.
    async fn carry_out(
        &self,
        proposal: &ProjectProposal,
        finding: &SecurityFinding,
        execute_enabled: bool,
    ) -> Result<RemediationRecord, AppError> {
        // The one destructive action: terminate a malware-classified process.
        if finding.kind == FindingKind::Malware {
            if let Some(pid) = parse_pid_evidence(&finding.evidence) {
                let cmd = if cfg!(windows) {
                    format!("taskkill /PID {pid} /F /T")
                } else {
                    format!("kill -9 {pid}")
                };
                if !execute_enabled {
                    let detail = format!(
                        "APPROVED but NOT executed: would run `{cmd}` on malware finding '{}'. \
                         Set ASTRA_GUARDIAN_EXECUTE=on to let the Guardian carry out approved \
                         destructive remediations.",
                        finding.subject
                    );
                    return self.record_remediation(
                        &proposal.proposal_id,
                        "terminate_process",
                        "planned",
                        &detail,
                    );
                }
                let result = self
                    .device
                    .execute("shell_exec", &json!({ "command": cmd }));
                let (mode, detail) = match &result {
                    Ok(v) => {
                        let exit = v.get("exit_code").and_then(Value::as_i64);
                        if exit == Some(0) {
                            let _ = self.desk.mark_executed(&proposal.proposal_id);
                            (
                                "executed",
                                format!(
                                    "ran `{cmd}` (exit 0) on approved malware finding '{}'",
                                    finding.subject
                                ),
                            )
                        } else {
                            ("failed", format!("`{cmd}` returned exit {exit:?}"))
                        }
                    }
                    Err(e) => ("failed", format!("device refused/failed `{cmd}`: {e}")),
                };
                let record = self.record_remediation(
                    &proposal.proposal_id,
                    "terminate_process",
                    mode,
                    &detail,
                )?;
                let _ = self.chronicle.record(RecordEpisodeRequest {
                    kind: EpisodeKind::Decision,
                    content: format!(
                        "CEO Guardian remediation ({mode}) on approved proposal {}: {detail}",
                        proposal.proposal_id
                    ),
                    rationale: Some(finding.detail.clone()),
                    source: Some("ceo_guardian".into()),
                    source_ref: Some(proposal.proposal_id.clone()),
                    tags: vec!["security".into(), "guardian".into(), "remediation".into()],
                    importance: Some(0.9),
                    due_at_ms: None,
                    tenant_scope: TenantScope::Global,
                });
                return Ok(record);
            }
        }
        // Non-destructive / non-actionable: advisory. Safe containment was already
        // recorded at detection time; the actionable step needs a human.
        let detail = format!(
            "approved remediation for {} finding '{}' requires manual action (rotate/remove/\
             reconfigure); the Guardian recorded safe containment at detection time and takes no \
             destructive host action here.",
            finding.kind.as_str(),
            finding.subject
        );
        self.record_remediation(&proposal.proposal_id, "advisory", "advisory", &detail)
    }

    /// A proposal is "handled" once it has a terminal record; a `planned` record
    /// is re-checked so enabling the execute flag later runs it.
    fn remediation_handled(&self, proposal_id: &str) -> bool {
        let store = self.store.lock();
        store
            .query_row(
                "SELECT 1 FROM guardian_remediations WHERE proposal_id = ?1 AND mode <> 'planned'",
                params![proposal_id],
                |_| Ok(()),
            )
            .optional()
            .unwrap_or(None)
            .is_some()
    }

    fn record_remediation(
        &self,
        proposal_id: &str,
        action: &str,
        mode: &str,
        detail: &str,
    ) -> Result<RemediationRecord, AppError> {
        let record = RemediationRecord {
            proposal_id: proposal_id.to_string(),
            action: action.to_string(),
            mode: mode.to_string(),
            detail: detail.to_string(),
            created_at_ms: now_ms(),
        };
        let store = self.store.lock();
        store
            .execute(
                "INSERT OR REPLACE INTO guardian_remediations \
                 (proposal_id, action, mode, detail, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.proposal_id,
                    record.action,
                    record.mode,
                    record.detail,
                    record.created_at_ms
                ],
            )
            .map_err(sql_err)?;
        Ok(record)
    }

    /// Recent remediation records, newest first.
    pub fn recent_remediations(&self, limit: usize) -> Result<Vec<RemediationRecord>, AppError> {
        let limit = limit.clamp(1, 500);
        let store = self.store.lock();
        let mut stmt = store
            .prepare(
                "SELECT proposal_id, action, mode, detail, created_at_ms \
                 FROM guardian_remediations ORDER BY created_at_ms DESC LIMIT ?1",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(RemediationRecord {
                    proposal_id: row.get(0)?,
                    action: row.get(1)?,
                    mode: row.get(2)?,
                    detail: row.get(3)?,
                    created_at_ms: row.get(4)?,
                })
            })
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(sql_err)?);
        }
        Ok(out)
    }

    // ── Persistence + read APIs ─────────────────────────────────────────────

    fn record_finding(&self, finding: &SecurityFinding) -> Result<(), AppError> {
        let payload = serde_json::to_string(finding)
            .map_err(|e| AppError::Internal(format!("finding serialize failed: {e}")))?;
        let store = self.store.lock();
        store
            .execute(
                "INSERT OR REPLACE INTO guardian_findings \
                 (finding_id, payload, severity, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
                params![
                    finding.finding_id,
                    payload,
                    finding.severity as i64,
                    finding.created_at_ms
                ],
            )
            .map_err(sql_err)?;
        // Bound the table so a long-lived host never grows it without limit.
        store
            .execute(
                "DELETE FROM guardian_findings WHERE finding_id NOT IN \
                 (SELECT finding_id FROM guardian_findings ORDER BY created_at_ms DESC LIMIT ?1)",
                params![RECENT_FINDINGS_KEEP as i64],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn record_patrol(&self, report: &GuardianPatrolReport) -> Result<(), AppError> {
        let payload = serde_json::to_string(report)
            .map_err(|e| AppError::Internal(format!("patrol serialize failed: {e}")))?;
        let store = self.store.lock();
        store
            .execute(
                "INSERT OR REPLACE INTO guardian_patrols \
                 (patrol_id, payload, created_at_ms) VALUES (?1, ?2, ?3)",
                params![report.patrol_id, payload, report.finished_at_ms],
            )
            .map_err(sql_err)?;
        // Prune baseline surfaces not seen in the retention window, so ephemeral
        // sockets/files can't grow the table without bound. A surface that returns
        // after this window is legitimately flagged as new again.
        let cutoff = report.finished_at_ms - BASELINE_RETENTION_MS;
        store
            .execute(
                "DELETE FROM guardian_baseline WHERE last_seen_ms < ?1",
                params![cutoff],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    /// Most recent findings, newest first.
    pub fn recent_findings(&self, limit: usize) -> Result<Vec<SecurityFinding>, AppError> {
        let limit = limit.clamp(1, RECENT_FINDINGS_KEEP);
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM guardian_findings ORDER BY created_at_ms DESC LIMIT ?1")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            let payload = row.map_err(sql_err)?;
            if let Ok(finding) = serde_json::from_str::<SecurityFinding>(&payload) {
                out.push(finding);
            }
        }
        Ok(out)
    }

    /// Most recent patrol reports, newest first.
    pub fn recent_patrols(&self, limit: usize) -> Result<Vec<GuardianPatrolReport>, AppError> {
        let limit = limit.clamp(1, 200);
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM guardian_patrols ORDER BY created_at_ms DESC LIMIT ?1")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            let payload = row.map_err(sql_err)?;
            if let Ok(report) = serde_json::from_str::<GuardianPatrolReport>(&payload) {
                out.push(report);
            }
        }
        Ok(out)
    }

    /// Aggregate recent findings into one legible read on the machine's current
    /// security posture — a severity-weighted risk score, a health grade, and the
    /// open-remediation count. Only findings inside the recency window count, so
    /// the posture reflects *now*, not all history.
    pub fn posture(&self) -> Result<GuardianPosture, AppError> {
        let now = now_ms();
        let cutoff = now - POSTURE_WINDOW_MS;
        let findings = self.recent_findings(RECENT_FINDINGS_KEEP)?;
        let recent: Vec<&SecurityFinding> = findings
            .iter()
            .filter(|f| f.created_at_ms >= cutoff)
            .collect();

        let mut by_severity: BTreeMap<String, u64> = BTreeMap::new();
        let mut weighted = 0.0_f64;
        let mut peak: Option<GuardianSeverity> = None;
        let mut open_remediations = 0u64;
        for f in &recent {
            *by_severity.entry(format!("{:?}", f.severity)).or_insert(0) += 1;
            weighted += severity_weight(f.severity);
            peak = Some(match peak {
                Some(p) if p >= f.severity => p,
                _ => f.severity,
            });
            if matches!(
                f.remediation,
                RemediationMode::ApprovalRequired | RemediationMode::SelfModificationProposed
            ) {
                open_remediations += 1;
            }
        }
        // Saturate: ~5 High-equivalent findings drive risk to the ceiling.
        let risk = (weighted / POSTURE_SATURATION).clamp(0.0, 1.0);
        let health = 1.0 - risk;
        Ok(GuardianPosture {
            health: round2(health),
            risk: round2(risk),
            grade: grade_for(health).to_string(),
            recent_by_severity: by_severity,
            recent_findings: recent.len() as u64,
            open_remediations,
            peak_severity: peak,
            computed_at_ms: now,
        })
    }

    /// A one-line posture summary the CEO folds into its executive deliberation
    /// so security posture actively drives self-evolution. Never errors.
    #[must_use]
    pub fn posture_summary(&self) -> String {
        match self.posture() {
            Ok(p) => format!(
                "security grade {} (health {:.2}, risk {:.2}); {} recent finding(s), {} awaiting \
                 remediation, peak {}",
                p.grade,
                p.health,
                p.risk,
                p.recent_findings,
                p.open_remediations,
                p.peak_severity
                    .map(|s| format!("{s:?}"))
                    .unwrap_or_else(|| "none".into()),
            ),
            Err(_) => "security posture unavailable".into(),
        }
    }

    /// A status snapshot for the dashboard/route.
    pub fn status(&self) -> Result<CeoGuardianStatus, AppError> {
        let store = self.store.lock();
        let total_patrols: u64 = store
            .query_row("SELECT COUNT(*) FROM guardian_patrols", [], |r| r.get(0))
            .map_err(sql_err)?;
        let total_findings: u64 = store
            .query_row("SELECT COUNT(*) FROM guardian_findings", [], |r| r.get(0))
            .map_err(sql_err)?;
        let last_patrol_at_ms: Option<i64> = store
            .query_row("SELECT MAX(created_at_ms) FROM guardian_patrols", [], |r| {
                r.get(0)
            })
            .map_err(sql_err)?;
        let peak_raw: Option<i64> = store
            .query_row("SELECT MAX(severity) FROM guardian_findings", [], |r| {
                r.get(0)
            })
            .map_err(sql_err)?;
        let baseline_size: u64 = store
            .query_row("SELECT COUNT(*) FROM guardian_baseline", [], |r| r.get(0))
            .map_err(sql_err)?;
        drop(store);

        let open_proposals = self
            .desk
            .proposals(500)
            .map(|list| {
                list.into_iter()
                    .filter(|p| {
                        p.submitted_by == "ceo" && p.title.to_lowercase().contains("guardian")
                            || p.title.starts_with("Security remediation:")
                    })
                    .count() as u64
            })
            .unwrap_or(0);

        Ok(CeoGuardianStatus {
            status: "operational".into(),
            model_backed: self.model_backed,
            scan_roots: self
                .config
                .scan_roots
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            self_modification_enabled: self.config.self_modification_enabled,
            capability_forging_enabled: self.config.capability_forging_enabled,
            total_patrols,
            total_findings,
            open_proposals,
            baseline_size,
            last_patrol_at_ms,
            peak_severity: peak_raw.and_then(severity_from_i64),
        })
    }
}

// ── free helpers ────────────────────────────────────────────────────────────

fn sql_err(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("ceo_guardian persistence failed: {error}"))
}

fn env_on(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.as_str(), "1" | "true" | "on" | "yes" | "TRUE"),
        Err(_) => default,
    }
}

/// Resolve the directories the Guardian may inspect. Defaults to the project's
/// own workspace (the current dir), which is honest and safe; overridable via
/// `ASTRA_GUARDIAN_SCAN_ROOTS` (`;`- or `,`-separated). The Guardian's own data
/// dir is always excluded so it never flags its own containment manifests.
fn resolve_scan_roots(own_dir: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = match std::env::var("ASTRA_GUARDIAN_SCAN_ROOTS") {
        Ok(raw) => raw
            .split([';', ','])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect(),
        Err(_) => std::env::current_dir().into_iter().collect(),
    };
    let own = own_dir
        .canonicalize()
        .unwrap_or_else(|_| own_dir.to_path_buf());
    roots.retain(|r| {
        let c = r.canonicalize().unwrap_or_else(|_| r.clone());
        !c.starts_with(&own)
    });
    roots.sort();
    roots.dedup();
    roots
}

/// Parse one process row into `(name, pid)`. Handles Windows `tasklist /fo csv
/// /nh` (`"image","pid",...`) and Unix `ps -eo pid,comm,...` (`pid comm ...`).
fn parse_process_row(line: &str) -> (String, Option<u32>) {
    let line = line.trim();
    if line.starts_with('"') {
        // CSV: "image name","pid","session","session#","mem"
        let fields: Vec<String> = line
            .split("\",\"")
            .map(|f| f.trim_matches('"').trim().to_string())
            .collect();
        let name = fields.first().cloned().unwrap_or_default();
        let pid = fields.get(1).and_then(|p| p.parse::<u32>().ok());
        (name, pid)
    } else {
        // Whitespace: pid comm %cpu %mem
        let mut it = line.split_whitespace();
        let pid = it.next().and_then(|p| p.parse::<u32>().ok());
        let name = it.next().unwrap_or("").to_string();
        (name, pid)
    }
}

/// True for the destructive remediation proposals the Guardian submits (as
/// opposed to hardening self-modifications, which the CEO's `execute_upgrade`
/// path handles). Matches the title `propose_remediation` writes.
fn is_guardian_remediation(proposal: &ProjectProposal) -> bool {
    matches!(proposal.kind, UpgradeKind::Policy)
        && proposal.title.starts_with("Security remediation:")
}

/// Recover a target PID from a finding's evidence (`pid=Some(1234)`).
fn parse_pid_evidence(evidence: &[String]) -> Option<u32> {
    for entry in evidence {
        if let Some(rest) = entry.strip_prefix("pid=Some(") {
            if let Some(num) = rest.strip_suffix(')') {
                if let Ok(pid) = num.parse::<u32>() {
                    return Some(pid);
                }
            }
        }
    }
    None
}

/// A running host process, snapshotted once per patrol.
struct HostProcess {
    name: String,
    pid: Option<u32>,
    /// The raw process-list row, kept for evidence.
    raw: String,
}

/// The process that owns a listening socket, resolved from the PID snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ListenerOwner {
    /// PID resolved to a live process image.
    Named(String),
    /// The socket carries a PID, but it is not in the live process list.
    UnknownPid(u32),
    /// No PID was available for the socket (e.g. Unix `ss` without privileges).
    None,
}

/// Correlate a socket's PID to its owning process image (unit-tested).
fn resolve_owner(
    pid: Option<u32>,
    pid_map: &std::collections::HashMap<u32, String>,
) -> ListenerOwner {
    match pid {
        Some(pid) => match pid_map.get(&pid) {
            Some(name) => ListenerOwner::Named(name.clone()),
            None => ListenerOwner::UnknownPid(pid),
        },
        None => ListenerOwner::None,
    }
}

/// One parsed network-connection row.
struct ConnRow {
    local: String,
    /// The remote/peer endpoint (empty for listeners with no peer).
    foreign: String,
    is_listen: bool,
    pid: Option<u32>,
}

/// Parse a `netstat -ano` (Windows) or `ss -tunap` (Unix) row into the fields
/// the network scan needs. Address tokens are those containing ':'; the first is
/// the local endpoint and the second is the foreign/peer endpoint. On Windows the
/// trailing token is the PID; on Unix the trailing token is a process descriptor
/// that simply won't parse to a number.
fn parse_conn_row(line: &str) -> ConnRow {
    let is_listen = line.to_ascii_lowercase().contains("listen");
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let addrs: Vec<&str> = tokens.iter().copied().filter(|t| t.contains(':')).collect();
    let local = addrs.first().copied().unwrap_or("").to_string();
    let foreign = addrs.get(1).copied().unwrap_or("").to_string();
    let pid = tokens.last().and_then(|t| t.parse::<u32>().ok());
    ConnRow {
        local,
        foreign,
        is_listen,
        pid,
    }
}

/// True when a local endpoint is bound to every interface (reachable off-host)
/// rather than to loopback.
fn binds_all_interfaces(local: &str) -> bool {
    local.starts_with("0.0.0.0:")
        || local.starts_with("[::]:")
        || local.starts_with(":::")
        || local.starts_with("*:")
        || local.starts_with("0:0:0:0:0:0:0:0:")
}

/// True when a foreign endpoint is a real, routable remote peer — not loopback,
/// a private/link-local range, a wildcard, or the unspecified address. Used to
/// decide whether an outbound connection is worth surfacing.
fn is_public_endpoint(addr: &str) -> bool {
    let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(addr);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() || host == "*" || host == "0.0.0.0" || host == "::" || host == "::1" {
        return false;
    }
    if host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.starts_with("fe80")
        || host.starts_with("fc")
        || host.starts_with("fd")
    {
        return false;
    }
    // 172.16.0.0 – 172.31.255.255 private range.
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some(octet) = rest.split('.').next().and_then(|s| s.parse::<u8>().ok()) {
            if (16..=31).contains(&octet) {
                return false;
            }
        }
    }
    true
}

/// Heavy build/VCS/cache directories the filesystem walk skips — they hold no
/// user secrets and would balloon the walk. Note: `.ssh`/`.aws`-style secret
/// dirs are deliberately NOT skipped.
fn should_skip_dir(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "node_modules"
            | "target"
            | ".git"
            | ".hg"
            | ".svn"
            | ".cargo"
            | ".rustup"
            | "vendor"
            | "dist"
            | "build"
            | "__pycache__"
            | ".venv"
            | "venv"
            | ".next"
            | ".nuxt"
            | "obj"
            | ".idea"
            | ".vscode"
            | "site-packages"
    )
}

/// Whether an observed surface item drifted from the per-machine baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BaselineDrift {
    /// In the baseline unchanged, or the baseline is still seeding.
    None,
    /// Never seen in a prior patrol.
    New,
    /// Seen before, but its fingerprint changed.
    Changed,
}

/// Pure drift decision (unit-tested). During seeding (`!baseline_ready`) nothing
/// is ever flagged, so the first patrol only records the surface.
fn decide_drift(
    baseline_ready: bool,
    prior_existed: bool,
    prior_fp: Option<&str>,
    current_fp: Option<&str>,
) -> BaselineDrift {
    if !baseline_ready {
        return BaselineDrift::None;
    }
    if !prior_existed {
        return BaselineDrift::New;
    }
    match (prior_fp, current_fp) {
        (Some(a), Some(b)) if a != b => BaselineDrift::Changed,
        _ => BaselineDrift::None,
    }
}

/// Malware-like process image / command patterns. Deliberately conservative.
fn looks_like_malware(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "mimikatz",
        "powershell -enc",
        "rundll32",
        "regsvr32",
        "certutil",
        "bitsadmin",
        "mshta",
        "ncat",
        "nc.exe",
        "\\temp\\",
        "appdata\\local\\temp",
    ]
    .iter()
    .any(|needle| n.contains(needle))
}

/// Filenames that commonly hold secrets/config worth reading and scanning.
fn is_sensitive_filename(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if is_high_value_filename(name) {
        return true;
    }
    n.ends_with(".env")
        || n.starts_with(".env")
        || n.ends_with(".pem")
        || n.ends_with(".key")
        || n.ends_with(".ppk")
        || n.ends_with(".pfx")
        || n.ends_with(".p12")
        || n == ".npmrc"
        || n == ".netrc"
        || n == ".pgpass"
        || n == ".git-credentials"
        || n == "credentials"
        || n == "config"
        || n.contains("secret")
        || n.contains("password")
        || n.contains("token")
}

/// High-value credential material — flagged even without a clear-text hit.
fn is_high_value_filename(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "id_rsa"
        || n == "id_ed25519"
        || n == "id_ecdsa"
        || n == "id_dsa"
        || n == "wallet.dat"
        || n == ".env"
        || n.ends_with(".env")
}

/// Risk weight per severity, used to aggregate the posture score.
fn severity_weight(severity: GuardianSeverity) -> f64 {
    match severity {
        GuardianSeverity::Informational => 0.05,
        GuardianSeverity::Low => 0.15,
        GuardianSeverity::Moderate => 0.40,
        GuardianSeverity::High => 0.70,
        GuardianSeverity::Critical => 1.00,
    }
}

/// A letter grade from a health score in [0,1].
fn grade_for(health: f64) -> &'static str {
    if health >= 0.9 {
        "A"
    } else if health >= 0.75 {
        "B"
    } else if health >= 0.6 {
        "C"
    } else if health >= 0.4 {
        "D"
    } else {
        "F"
    }
}

fn round2(value: f64) -> f64 {
    (value.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

fn importance_for_severity(severity: GuardianSeverity) -> f64 {
    match severity {
        GuardianSeverity::Informational | GuardianSeverity::Low => 0.45,
        GuardianSeverity::Moderate => 0.65,
        GuardianSeverity::High => 0.82,
        GuardianSeverity::Critical => 0.93,
    }
}

fn severity_from_i64(value: i64) -> Option<GuardianSeverity> {
    match value {
        0 => Some(GuardianSeverity::Informational),
        1 => Some(GuardianSeverity::Low),
        2 => Some(GuardianSeverity::Moderate),
        3 => Some(GuardianSeverity::High),
        4 => Some(GuardianSeverity::Critical),
        _ => None,
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        value.chars().take(max).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_tasklist_row() {
        let (name, pid) =
            parse_process_row("\"chrome.exe\",\"4321\",\"Console\",\"1\",\"150,000 K\"");
        assert_eq!(name, "chrome.exe");
        assert_eq!(pid, Some(4321));
    }

    #[test]
    fn parses_unix_ps_row() {
        let (name, pid) = parse_process_row("  1234 sshd  0.1  0.5");
        assert_eq!(name, "sshd");
        assert_eq!(pid, Some(1234));
    }

    #[test]
    fn flags_malware_patterns_but_not_ordinary_processes() {
        assert!(looks_like_malware("mimikatz.exe"));
        assert!(looks_like_malware(
            "C:\\Users\\x\\AppData\\Local\\Temp\\a.exe"
        ));
        assert!(!looks_like_malware("chrome.exe"));
    }

    #[test]
    fn sensitive_filename_detection() {
        assert!(is_sensitive_filename(".env"));
        assert!(is_sensitive_filename("id_rsa"));
        assert!(is_sensitive_filename("server.pem"));
        assert!(is_high_value_filename("wallet.dat"));
        assert!(!is_sensitive_filename("README.md"));
    }

    #[test]
    fn parses_windows_netstat_row_and_flags_all_interface_listener() {
        let row = "  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1044";
        let conn = parse_conn_row(row);
        assert!(conn.is_listen);
        assert_eq!(conn.local, "0.0.0.0:135");
        assert_eq!(conn.pid, Some(1044));
        assert!(binds_all_interfaces(&conn.local));
    }

    #[test]
    fn loopback_listener_is_not_all_interfaces() {
        let conn = parse_conn_row("  TCP    127.0.0.1:5432   0.0.0.0:0   LISTENING   900");
        assert!(conn.is_listen);
        assert!(!binds_all_interfaces(&conn.local));
    }

    #[test]
    fn unix_ss_row_has_no_numeric_pid_but_parses_local() {
        let row = "tcp   LISTEN 0      128    0.0.0.0:22         0.0.0.0:*    users:((\"sshd\",pid=800,fd=3))";
        let conn = parse_conn_row(row);
        assert!(conn.is_listen);
        assert_eq!(conn.local, "0.0.0.0:22");
        assert_eq!(conn.pid, None); // trailing token is a process descriptor
    }

    #[test]
    fn walk_skips_heavy_dirs_but_not_secret_dirs() {
        assert!(should_skip_dir("node_modules"));
        assert!(should_skip_dir("target"));
        assert!(should_skip_dir(".git"));
        assert!(!should_skip_dir(".ssh"));
        assert!(!should_skip_dir(".aws"));
        assert!(!should_skip_dir("src"));
    }

    #[test]
    fn public_endpoint_excludes_private_and_loopback() {
        assert!(is_public_endpoint("93.184.216.34:443"));
        assert!(is_public_endpoint(
            "[2606:2800:220:1:248:1893:25c8:1946]:443"
        ));
        assert!(!is_public_endpoint("127.0.0.1:5432"));
        assert!(!is_public_endpoint("10.0.0.5:80"));
        assert!(!is_public_endpoint("192.168.1.10:445"));
        assert!(!is_public_endpoint("172.16.5.4:8080"));
        assert!(is_public_endpoint("172.32.5.4:8080")); // 172.32 is outside the private range
        assert!(!is_public_endpoint("0.0.0.0:0"));
        assert!(!is_public_endpoint("*:*"));
    }

    #[test]
    fn parse_conn_row_extracts_foreign_endpoint() {
        let conn = parse_conn_row(
            "  TCP    192.168.1.5:52341      93.184.216.34:443      ESTABLISHED     4321",
        );
        assert!(!conn.is_listen);
        assert_eq!(conn.local, "192.168.1.5:52341");
        assert_eq!(conn.foreign, "93.184.216.34:443");
        assert_eq!(conn.pid, Some(4321));
    }

    #[test]
    fn posture_grade_tracks_health() {
        assert_eq!(grade_for(1.0), "A");
        assert_eq!(grade_for(0.8), "B");
        assert_eq!(grade_for(0.62), "C");
        assert_eq!(grade_for(0.45), "D");
        assert_eq!(grade_for(0.1), "F");
        assert!(
            severity_weight(GuardianSeverity::Critical) > severity_weight(GuardianSeverity::Low)
        );
    }

    #[test]
    fn parse_pid_evidence_recovers_target_pid() {
        assert_eq!(
            parse_pid_evidence(&["owner=x".into(), "pid=Some(4321)".into()]),
            Some(4321)
        );
        assert_eq!(parse_pid_evidence(&["pid=None".into()]), None);
        assert_eq!(parse_pid_evidence(&["nothing".into()]), None);
    }

    #[test]
    fn resolve_owner_correlates_pid_to_process() {
        let mut map = std::collections::HashMap::new();
        map.insert(4321u32, "svchost.exe".to_string());
        assert_eq!(
            resolve_owner(Some(4321), &map),
            ListenerOwner::Named("svchost.exe".into())
        );
        assert_eq!(
            resolve_owner(Some(9999), &map),
            ListenerOwner::UnknownPid(9999)
        );
        assert_eq!(resolve_owner(None, &map), ListenerOwner::None);
    }

    #[test]
    fn drift_only_fires_after_baseline_is_established() {
        // Seeding: never flags, whatever the state.
        assert_eq!(decide_drift(false, false, None, None), BaselineDrift::None);
        assert_eq!(
            decide_drift(false, true, Some("a"), Some("b")),
            BaselineDrift::None
        );
        // Established baseline: a never-seen item is New.
        assert_eq!(decide_drift(true, false, None, None), BaselineDrift::New);
        // Known item, unchanged fingerprint → None; changed → Changed.
        assert_eq!(
            decide_drift(true, true, Some("a"), Some("a")),
            BaselineDrift::None
        );
        assert_eq!(
            decide_drift(true, true, Some("a"), Some("b")),
            BaselineDrift::Changed
        );
        // Known listener with no fingerprint on either side → not a change.
        assert_eq!(decide_drift(true, true, None, None), BaselineDrift::None);
    }

    #[test]
    fn severity_roundtrips_through_i64() {
        for sev in [
            GuardianSeverity::Informational,
            GuardianSeverity::Low,
            GuardianSeverity::Moderate,
            GuardianSeverity::High,
            GuardianSeverity::Critical,
        ] {
            assert_eq!(severity_from_i64(sev as i64), Some(sev));
        }
    }
}

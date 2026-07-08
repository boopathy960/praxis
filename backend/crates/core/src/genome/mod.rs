//! Genome OS — an AI-native operating system for companies.
//!
//! Operations ship as **genomes**: verified, forkable behaviours, each a
//! certified contract `(E, R, C)` — an [`Envelope`](envelope::Envelope) of hard,
//! decidable invariants proven logically, a [`Residual`](model::Residual) of
//! judgment bounded statistically by adaptive conformal prediction, and a
//! [`Certificate`](model::Certificate) that proves both, anchored as a real claim
//! in the [proof economy](crate::proof_economy). A swarm executes them through
//! the [runtime]; a human reshapes one by stating a deviation in plain language
//! ([projection::reshape]); the commons forks and composes them as a refinement
//! lattice ([commons]); they improve from evidence under constrained best-arm
//! identification ([evolution]); outcomes are priced by Shapley value with
//! royalties up the fork tree ([pricing]); and the warranty layer is
//! correlated-risk underwriting with concentration limits ([underwriting]).
//!
//! This module is the service that owns the store and wires those pieces to the
//! rest of the system — the same proof economy, device layer, and agentic loop
//! every other organ already trusts. The brief's beachhead is seeded on first
//! boot: billing and revenue operations (invoicing, dunning, reconciliation,
//! refunds), where "correct" is mathematically checkable and every run quietly
//! populates the proof cache and the commons.

pub mod commons;
pub mod conformal;
pub mod envelope;
pub mod evolution;
pub mod model;
pub mod pricing;
pub mod projection;
pub mod runtime;
pub mod underwriting;

#[cfg(test)]
pub(crate) mod testkit;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::agentic_loop::AgenticLoop;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::proof_economy::{AttackRequest, ClaimKind, ProofEconomy, ProposeRequest};

use commons::{CompositionReport, LatticeGraph, RefinementCheck};
use conformal::ConformalState;
use envelope::{Envelope, Invariant, OpValue};
use evolution::{Arm, EvolutionReport};
use model::{
    Certificate, Contract, Genome, GenomeMetrics, GenomeStatus, GenomeSummary, Residual, VarSpec,
};
use pricing::{Contributor, PricingReport};
use projection::{MachineApi, ReshapePatch, UiSchema};
use runtime::{PendingSignoff, RunReport, RunRequest};
use underwriting::{DEFAULT_CONCENTRATION_CAP, RiskReport};

/// The Genome OS service.
#[derive(Clone)]
pub struct GenomeOsService {
    store: Arc<Mutex<Connection>>,
    economy: ProofEconomy,
    #[allow(dead_code)]
    device: Arc<DeviceCapabilities>,
    /// Optional agentic reasoner for the residual judgment (never the guarantee).
    agentic_loop: Option<AgenticLoop>,
    data_root: String,
    cap: f64,
    /// Whether the seeded genomes have had their certificates anchored yet.
    /// Anchoring runs device-backed proof checks, so it is deferred out of
    /// construction and done once, lazily, on the first read — keeping startup
    /// free of device IO.
    anchored: Arc<AtomicBool>,
}

/// The result of forking a genome.
#[derive(Debug, Clone, Serialize)]
pub struct ForkOutcome {
    pub genome: Genome,
    pub refinement: RefinementCheck,
    pub patch: ReshapePatch,
}

/// The re-verified state of a genome's certificate.
#[derive(Debug, Clone, Serialize)]
pub struct CertificateReport {
    pub genome_id: String,
    pub certificate: Certificate,
    pub verify_pass: bool,
    pub verify_detail: String,
    pub claim_status: String,
}

/// The console's landing dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub total_genomes: usize,
    pub minted: usize,
    pub certified: usize,
    pub quarantined: usize,
    pub operations: usize,
    pub generations: u32,
    pub total_runs: u64,
    pub admitted: u64,
    pub blocked: u64,
    pub human_signoffs: u64,
    pub pending_signoffs: usize,
    pub total_outcome_value: f64,
    pub total_premium: f64,
    pub systemic_risk_score: f64,
    pub concentration_breaches: usize,
    pub mean_certificate_validity: f64,
}

impl GenomeOsService {
    /// Build the service over its sqlite store, wired to the proof economy and
    /// device layer. Seeds the billing beachhead on first boot.
    pub fn new(
        data_dir: impl AsRef<std::path::Path>,
        economy: ProofEconomy,
        device: Arc<DeviceCapabilities>,
    ) -> Result<Self, AppError> {
        let dir = data_dir.as_ref();
        std::fs::create_dir_all(dir)
            .map_err(|error| AppError::Internal(format!("failed to create genome dir: {error}")))?;
        std::fs::create_dir_all(dir.join("ledgers")).map_err(|error| {
            AppError::Internal(format!("failed to create genome ledger dir: {error}"))
        })?;
        let path = dir.join("genome.sqlite");
        let connection = Connection::open(&path)
            .map_err(|error| AppError::Internal(format!("failed to open genome store: {error}")))?;
        Self::init_schema(&connection)?;

        let cap = std::env::var("ASTRA_GENOME_CONCENTRATION_CAP")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| value.clamp(0.05, 1.0))
            .unwrap_or(DEFAULT_CONCENTRATION_CAP);

        let service = Self {
            store: Arc::new(Mutex::new(connection)),
            economy,
            device,
            agentic_loop: None,
            data_root: dir.to_string_lossy().to_string(),
            cap,
            anchored: Arc::new(AtomicBool::new(false)),
        };

        if service.count_genomes()? == 0 {
            // Seed as drafts; certificates are anchored lazily on first access so
            // construction performs no device IO.
            for genome in seed_genomes() {
                service.persist_genome(&genome)?;
            }
            tracing::info!(
                "genome commons seeded with the billing beachhead (certificates anchor lazily)"
            );
        }
        Ok(service)
    }

    /// Anchor every still-unanchored genome's certificate exactly once. The
    /// `swap` makes the first caller the sole anchorer; later callers are a cheap
    /// atomic read. This is what keeps device-backed proof checks out of
    /// `AppState::new` while still presenting certified genomes to the first
    /// request that needs them.
    fn ensure_anchored(&self) {
        if self.anchored.swap(true, Ordering::SeqCst) {
            return;
        }
        match self.list_raw() {
            Ok(genomes) => {
                for mut genome in genomes {
                    if genome.certificate.logical_status == "unanchored" {
                        self.anchor_certificate(&mut genome);
                        let _ = self.persist_genome(&genome);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "deferred certificate anchoring failed; will retry");
                self.anchored.store(false, Ordering::SeqCst);
            }
        }
    }

    /// Attach the agentic loop, so a genome's residual judgment can be reasoned
    /// by the same verified loop the rest of the system uses.
    #[must_use]
    pub fn with_loop(mut self, agentic_loop: AgenticLoop) -> Self {
        self.agentic_loop = Some(agentic_loop);
        self
    }

    fn init_schema(connection: &Connection) -> Result<(), AppError> {
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS genomes (
                    genome_id TEXT PRIMARY KEY,
                    operation TEXT NOT NULL,
                    parent TEXT,
                    status TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS genome_signoffs (
                    signoff_id TEXT PRIMARY KEY,
                    genome_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS genome_arms (
                    arm_key TEXT PRIMARY KEY,
                    operation TEXT NOT NULL,
                    payload TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS genome_runs (
                    run_id TEXT PRIMARY KEY,
                    genome_id TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    status TEXT NOT NULL,
                    outcome REAL NOT NULL,
                    payload TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS genome_idem (
                    genome_id TEXT NOT NULL,
                    idem_key TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    PRIMARY KEY (genome_id, idem_key)
                 );",
            )
            .map_err(sql_err)
    }

    // ── persistence ────────────────────────────────────────────────────

    fn count_genomes(&self) -> Result<i64, AppError> {
        let store = self.store.lock();
        store
            .query_row("SELECT COUNT(*) FROM genomes", [], |row| row.get(0))
            .map_err(sql_err)
    }

    fn persist_genome(&self, genome: &Genome) -> Result<(), AppError> {
        let payload = serde_json::to_string(genome).map_err(json_err)?;
        let store = self.store.lock();
        store
            .execute(
                "INSERT INTO genomes (genome_id, operation, parent, status, payload, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(genome_id) DO UPDATE SET
                    operation = excluded.operation,
                    parent = excluded.parent,
                    status = excluded.status,
                    payload = excluded.payload,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    genome.genome_id,
                    genome.operation,
                    genome.parent,
                    genome.status.label(),
                    payload,
                    genome.created_at_ms,
                    genome.updated_at_ms,
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    /// Load one genome by id (anchoring the seeded commons on first access).
    pub fn get(&self, genome_id: &str) -> Result<Genome, AppError> {
        self.ensure_anchored();
        self.get_raw(genome_id)
    }

    fn get_raw(&self, genome_id: &str) -> Result<Genome, AppError> {
        let store = self.store.lock();
        let payload: String = store
            .query_row(
                "SELECT payload FROM genomes WHERE genome_id = ?1",
                params![genome_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::NotFound(format!("genome {genome_id} not found")))?;
        serde_json::from_str(&payload).map_err(json_err)
    }

    /// Load the whole commons (anchoring the seeded commons on first access).
    pub fn list(&self) -> Result<Vec<Genome>, AppError> {
        self.ensure_anchored();
        self.list_raw()
    }

    fn list_raw(&self) -> Result<Vec<Genome>, AppError> {
        let store = self.store.lock();
        let mut statement = store
            .prepare("SELECT payload FROM genomes ORDER BY created_at_ms ASC")
            .map_err(sql_err)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut genomes = Vec::new();
        for row in rows {
            let payload = row.map_err(sql_err)?;
            if let Ok(genome) = serde_json::from_str::<Genome>(&payload) {
                genomes.push(genome);
            }
        }
        Ok(genomes)
    }

    /// Compact summaries for list/graph views.
    pub fn summaries(&self) -> Result<Vec<GenomeSummary>, AppError> {
        Ok(self.list()?.iter().map(Genome::summary).collect())
    }

    // ── certificate anchoring (the proof-economy bridge) ───────────────

    /// Anchor a genome's certificate: compile its envelope to a real device-run
    /// [`Check`](crate::verification::Check), stake it as a claim in the proof
    /// economy, run an adversarial round, and reflect the resulting status on the
    /// genome. A genome whose envelope check holds against attack *mints*; one
    /// whose check fails is refuted and quarantined — the certificate is earned,
    /// not asserted.
    fn anchor_certificate(&self, genome: &mut Genome) {
        let violations_path = runtime::violation_ledger_path(&self.data_root, &genome.genome_id);
        let check = genome.envelope.compile_to_check(&violations_path);
        genome.certificate.envelope_check = check.clone();

        let statement = format!(
            "genome '{}' never violates its envelope on operation {}",
            genome.name, genome.operation
        );
        let proposed = self.economy.propose(ProposeRequest {
            statement,
            kind: ClaimKind::Assertion,
            proposer: "genome_os".into(),
            verification: check,
            evidence: vec![format!("operation:{}", genome.operation)],
            depends_on: Vec::new(),
        });

        match proposed {
            Ok(claim) => {
                let claim_id = claim.claim_id.clone();
                let pass = self
                    .economy
                    .verify(&claim_id)
                    .map(|report| report.pass)
                    .unwrap_or(false);
                let status_label = if pass {
                    self.try_mint(&claim_id)
                } else {
                    format!("{:?}", claim.status).to_lowercase()
                };
                genome.certificate.claim_id = Some(claim_id);
                genome.certificate.logical_status = status_label.clone();
                genome.certificate.envelope_holds = pass;
                genome.certificate.issued_at_ms = now_ms();
                runtime::refresh_certificate(&mut genome.certificate, &genome.residual.conformal);
                genome.status = match (pass, status_label.as_str()) {
                    (true, "minted") => GenomeStatus::Minted,
                    (true, _) => GenomeStatus::Certified,
                    (false, _) => GenomeStatus::Quarantined,
                };
            }
            Err(error) => {
                tracing::warn!(%error, genome = %genome.genome_id, "certificate anchoring failed");
                genome.certificate.logical_status = "unanchored".into();
                runtime::refresh_certificate(&mut genome.certificate, &genome.residual.conformal);
            }
        }
    }

    /// Drive the adversarial mint round: attacks that fail to refute a true claim
    /// burn the attacker and carry the claim to Minted. Returns the final status.
    fn try_mint(&self, claim_id: &str) -> String {
        let mut label = "proposed".to_string();
        for attacker in ["auditor_alpha", "auditor_beta", "auditor_gamma"] {
            match self.economy.attack(
                claim_id,
                AttackRequest {
                    attacker: attacker.into(),
                    note: "genome certificate adversarial round".into(),
                    counter: None,
                },
            ) {
                Ok(claim) => {
                    label = format!("{:?}", claim.status).to_lowercase();
                    if label != "proposed" {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        label
    }

    /// Re-verify a genome's certificate against reality (re-runs the device check)
    /// and persists the refreshed certificate.
    pub fn certificate(&self, genome_id: &str) -> Result<CertificateReport, AppError> {
        let mut genome = self.get(genome_id)?;
        let (pass, detail, claim_status) = match &genome.certificate.claim_id {
            Some(claim_id) => {
                let report = self.economy.verify(claim_id)?;
                let claim_status = self
                    .economy
                    .get_claim(claim_id)
                    .map(|claim| format!("{:?}", claim.status).to_lowercase())
                    .unwrap_or_else(|_| "unknown".into());
                (report.pass, report.detail, claim_status)
            }
            None => (
                genome.certificate.envelope_holds,
                "certificate not anchored".into(),
                "unanchored".into(),
            ),
        };
        genome.certificate.envelope_holds = pass;
        genome.certificate.logical_status = claim_status.clone();
        runtime::refresh_certificate(&mut genome.certificate, &genome.residual.conformal);
        if !pass {
            genome.status = GenomeStatus::Quarantined;
        } else if genome.status == GenomeStatus::Quarantined {
            genome.status = GenomeStatus::Certified;
        }
        self.persist_genome(&genome)?;
        Ok(CertificateReport {
            genome_id: genome.genome_id.clone(),
            certificate: genome.certificate.clone(),
            verify_pass: pass,
            verify_detail: detail,
            claim_status,
        })
    }

    // ── compile / fork / compose ───────────────────────────────────────

    /// Compile plain-language intent into an operational genome and certify it.
    pub fn compile(
        &self,
        intent: &str,
        name: Option<String>,
        operation: Option<String>,
    ) -> Result<Genome, AppError> {
        let intent = intent.trim();
        if intent.is_empty() {
            return Err(AppError::Validation("intent is empty".into()));
        }
        let mut genome = compile_intent(intent, name, operation);
        self.anchor_certificate(&mut genome);
        self.persist_genome(&genome)?;
        Ok(genome)
    }

    /// Fork a genome by stating deviations in plain language. The fork must be a
    /// refinement (preserve the parent's guarantees while adding constraints);
    /// otherwise it is rejected with the unmet guarantees explained.
    pub fn fork(
        &self,
        parent_id: &str,
        deviations: &str,
        name: Option<String>,
    ) -> Result<ForkOutcome, AppError> {
        let parent = self.get(parent_id)?;
        let patch = projection::reshape(&parent, deviations);
        if !patch.understood {
            return Err(AppError::Validation(patch.message));
        }
        if !patch.is_refinement {
            return Err(AppError::Validation(format!(
                "fork is not a valid refinement — it loosens a guarantee: {}",
                patch.message
            )));
        }
        let refinement = commons::check_refinement(&parent, &patch.new_envelope, patch.new_epsilon);
        let now = now_ms();
        let mut lineage = parent.lineage.clone();
        lineage.push(parent.genome_id.clone());
        let mut child = parent.clone();
        child.genome_id = new_id("genome");
        child.name = name.unwrap_or_else(|| format!("{} · fork", parent.name));
        child.intent = format!("{} — deviating: {}", parent.intent, deviations.trim());
        child.envelope = patch.new_envelope.clone();
        child.contract = Contract {
            assumptions: parent.contract.assumptions.clone(),
            guarantees: patch.new_envelope.clone(),
        };
        child.residual = Residual {
            description: parent.residual.description.clone(),
            judgment_var: parent.residual.judgment_var.clone(),
            approved_center: parent.residual.approved_center,
            approved_halfwidth: parent.residual.approved_halfwidth,
            // A fork starts a fresh, cold-start calibration at the new ε.
            conformal: ConformalState::new(patch.new_epsilon),
        };
        child.parent = Some(parent.genome_id.clone());
        child.lineage = lineage;
        child.generation = parent.generation + 1;
        child.status = GenomeStatus::Draft;
        child.description_bits = refinement.description_bits;
        child.deviation_bits = refinement.deviation_bits;
        child.metrics = GenomeMetrics::default();
        child.created_at_ms = now;
        child.updated_at_ms = now;
        child.version = 1;
        self.anchor_certificate(&mut child);
        self.persist_genome(&child)?;
        Ok(ForkOutcome {
            genome: child,
            refinement,
            patch,
        })
    }

    /// Decide whether two genomes compose safely (assume-guarantee).
    pub fn compose(
        &self,
        upstream_id: &str,
        downstream_id: &str,
    ) -> Result<CompositionReport, AppError> {
        let upstream = self.get(upstream_id)?;
        let downstream = self.get(downstream_id)?;
        Ok(commons::compose(&upstream, &downstream))
    }

    /// Preview how a plain-language reshape would change a genome.
    pub fn reshape_preview(&self, genome_id: &str, text: &str) -> Result<ReshapePatch, AppError> {
        let genome = self.get(genome_id)?;
        Ok(projection::reshape(&genome, text))
    }

    /// Project a genome's human UI on demand.
    pub fn projection(&self, genome_id: &str) -> Result<UiSchema, AppError> {
        Ok(projection::project_ui(&self.get(genome_id)?))
    }

    /// A genome's MCP-style machine interface.
    pub fn machine_api(&self, genome_id: &str) -> Result<MachineApi, AppError> {
        Ok(projection::machine_api(&self.get(genome_id)?))
    }

    // ── run ────────────────────────────────────────────────────────────

    /// Execute one operation of a genome through the full safety pipeline.
    pub fn run(&self, genome_id: &str, request: RunRequest) -> Result<RunReport, AppError> {
        let mut genome = self.get(genome_id)?;
        let all = self.list()?;
        let verdict =
            underwriting::admits_more_traffic(&all, &genome.operation, &genome.genome_id, self.cap);

        // Idempotency: a retry with a seen key is a no-op.
        let already = match &request.idempotency_key {
            Some(key) => self.idempotency_seen(genome_id, key)?,
            None => false,
        };

        let segment = request.segment.clone().unwrap_or_else(|| "global".into());
        let outcome = runtime::execute(&mut genome, &request, verdict, already);

        // Record the idempotency key once an action actually applies.
        if let Some(key) = &request.idempotency_key {
            if outcome.report.status == "executed" || outcome.report.status == "pending_signoff" {
                self.record_idempotency(genome_id, key)?;
            }
        }

        // Update network share + premium from the live commons, so the projection
        // and certificate reflect concentration and warranty cost.
        self.refresh_genome_risk(&mut genome, &all);
        runtime::refresh_certificate(&mut genome.certificate, &genome.residual.conformal);
        self.persist_genome(&genome)?;

        // Feed the evolution bandit and persist any sign-off.
        if let Some((reward, violated)) = outcome.bandit_reward {
            self.record_arm(&genome.operation, &segment, genome_id, reward, violated)?;
        }
        if let Some(signoff) = &outcome.signoff {
            self.persist_signoff(signoff)?;
        }
        self.record_run(&outcome.report)?;
        Ok(outcome.report)
    }

    fn idempotency_seen(&self, genome_id: &str, key: &str) -> Result<bool, AppError> {
        let store = self.store.lock();
        let count: i64 = store
            .query_row(
                "SELECT COUNT(*) FROM genome_idem WHERE genome_id = ?1 AND idem_key = ?2",
                params![genome_id, key],
                |row| row.get(0),
            )
            .map_err(sql_err)?;
        Ok(count > 0)
    }

    fn record_idempotency(&self, genome_id: &str, key: &str) -> Result<(), AppError> {
        let store = self.store.lock();
        store
            .execute(
                "INSERT OR IGNORE INTO genome_idem (genome_id, idem_key, created_at_ms) VALUES (?1, ?2, ?3)",
                params![genome_id, key, now_ms()],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn record_run(&self, report: &RunReport) -> Result<(), AppError> {
        let payload = serde_json::to_string(report).map_err(json_err)?;
        let store = self.store.lock();
        store
            .execute(
                "INSERT INTO genome_runs (run_id, genome_id, operation, status, outcome, payload, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![report.run_id, report.genome_id, report.operation, report.status, report.outcome_value, payload, report.ran_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    /// Recent run audit, newest first.
    pub fn recent_runs(&self, limit: usize) -> Result<Vec<RunReport>, AppError> {
        let store = self.store.lock();
        let mut statement = store
            .prepare("SELECT payload FROM genome_runs ORDER BY created_at_ms DESC LIMIT ?1")
            .map_err(sql_err)?;
        let rows = statement
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut runs = Vec::new();
        for row in rows {
            if let Ok(report) = serde_json::from_str::<RunReport>(&row.map_err(sql_err)?) {
                runs.push(report);
            }
        }
        Ok(runs)
    }

    fn refresh_genome_risk(&self, genome: &mut Genome, all: &[Genome]) {
        let report = underwriting::assess(all, self.cap);
        if let Some(line) = report.premiums.iter().find(|p| p.id == genome.genome_id) {
            genome.metrics.premium = line.premium;
        }
        let op_total: u64 = all
            .iter()
            .filter(|g| g.operation == genome.operation)
            .map(|g| g.metrics.runs)
            .sum::<u64>()
            .max(1);
        genome.metrics.network_share = genome.metrics.runs as f64 / op_total as f64;
    }

    // ── evolution (best-arm) ───────────────────────────────────────────

    fn arm_key(operation: &str, segment: &str, genome_id: &str) -> String {
        format!("{operation}|{segment}|{genome_id}")
    }

    fn record_arm(
        &self,
        operation: &str,
        segment: &str,
        genome_id: &str,
        reward: f64,
        violated: bool,
    ) -> Result<(), AppError> {
        let key = Self::arm_key(operation, segment, genome_id);
        let store = self.store.lock();
        let existing: Option<String> = store
            .query_row(
                "SELECT payload FROM genome_arms WHERE arm_key = ?1",
                params![key],
                |row| row.get(0),
            )
            .ok();
        let mut arm = existing
            .and_then(|payload| serde_json::from_str::<Arm>(&payload).ok())
            .unwrap_or_else(|| Arm::new(genome_id, genome_id, segment));
        arm.record(reward, violated);
        let payload = serde_json::to_string(&arm).map_err(json_err)?;
        store
            .execute(
                "INSERT INTO genome_arms (arm_key, operation, payload) VALUES (?1, ?2, ?3)
                 ON CONFLICT(arm_key) DO UPDATE SET payload = excluded.payload",
                params![key, operation, payload],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn arms_for(&self, operation: &str, segment: &str) -> Result<Vec<Arm>, AppError> {
        let store = self.store.lock();
        let mut statement = store
            .prepare("SELECT payload FROM genome_arms WHERE operation = ?1")
            .map_err(sql_err)?;
        let rows = statement
            .query_map(params![operation], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut arms = Vec::new();
        for row in rows {
            if let Ok(arm) = serde_json::from_str::<Arm>(&row.map_err(sql_err)?) {
                if arm.segment == segment {
                    arms.push(arm);
                }
            }
        }
        Ok(arms)
    }

    /// Run a best-arm evaluation over an operation's variant genomes for a
    /// segment, returning the curse-corrected recommendation.
    pub fn evolve(
        &self,
        operation: &str,
        segment: Option<String>,
    ) -> Result<EvolutionReport, AppError> {
        let segment = segment.unwrap_or_else(|| "global".into());
        let arms = self.arms_for(operation, &segment)?;
        if arms.is_empty() {
            return Err(AppError::NotFound(format!(
                "no evolution evidence yet for operation {operation} / segment {segment} — run some genomes first"
            )));
        }
        // The incumbent is the most-run genome on this operation.
        let incumbent = self
            .list()?
            .into_iter()
            .filter(|g| g.operation == operation)
            .max_by_key(|g| g.metrics.runs)
            .map(|g| g.genome_id);
        Ok(evolution::evaluate(
            &arms,
            incumbent.as_deref(),
            operation,
            &segment,
        ))
    }

    // ── pricing (Shapley) ──────────────────────────────────────────────

    /// Price a metered outcome by Shapley value over the genome and its fork-tree
    /// lineage, with royalties flowing up to the authors who created the prior.
    pub fn price(
        &self,
        genome_id: &str,
        total_value: f64,
        collaborators: Vec<String>,
    ) -> Result<PricingReport, AppError> {
        let genome = self.get(genome_id)?;
        let mut contributors = Vec::new();
        // The genome itself, carrying its lineage so royalties find their targets.
        contributors.push(Contributor {
            id: genome.genome_id.clone(),
            label: genome.name.clone(),
            weight: 3.0,
            lineage: genome.lineage.clone(),
            is_human: false,
        });
        // Its ancestors, as contributors that can receive royalties.
        for (depth, ancestor_id) in genome.lineage.iter().rev().enumerate() {
            if let Ok(ancestor) = self.get(ancestor_id) {
                contributors.push(Contributor {
                    id: ancestor.genome_id.clone(),
                    label: format!("{} (ancestor)", ancestor.name),
                    weight: (1.0 / (depth as f64 + 2.0)).max(0.1),
                    lineage: Vec::new(),
                    is_human: false,
                });
            }
        }
        // Listed collaborator genomes in the execution graph.
        for collaborator_id in collaborators {
            if let Ok(collaborator) = self.get(&collaborator_id) {
                contributors.push(Contributor {
                    id: collaborator.genome_id.clone(),
                    label: collaborator.name.clone(),
                    weight: 1.5,
                    lineage: collaborator.lineage.clone(),
                    is_human: false,
                });
            }
        }
        // The human-in-the-loop, weighted by how often this genome needs sign-off.
        let human_weight = if genome.metrics.runs > 0 {
            (genome.metrics.human_signoffs as f64 / genome.metrics.runs as f64).clamp(0.05, 1.0)
        } else {
            0.2
        };
        contributors.push(Contributor {
            id: "human_reviewer".into(),
            label: "human reviewer".into(),
            weight: human_weight,
            lineage: Vec::new(),
            is_human: true,
        });
        Ok(pricing::price(&contributors, total_value))
    }

    // ── underwriting (risk) ────────────────────────────────────────────

    /// Assess the commons' correlated risk and concentration, persisting the
    /// per-genome premium and network share back onto each genome.
    pub fn risk(&self) -> Result<RiskReport, AppError> {
        let mut all = self.list()?;
        let report = underwriting::assess(&all, self.cap);
        for genome in &mut all {
            let changed_premium = report
                .premiums
                .iter()
                .find(|p| p.id == genome.genome_id)
                .map(|p| p.premium);
            let share = report
                .concentration
                .iter()
                .flat_map(|c| c.entries.iter())
                .find(|e| e.id == genome.genome_id)
                .map(|e| e.share);
            let mut dirty = false;
            if let Some(premium) = changed_premium {
                if (genome.metrics.premium - premium).abs() > 1e-6 {
                    genome.metrics.premium = premium;
                    dirty = true;
                }
            }
            if let Some(share) = share {
                if (genome.metrics.network_share - share).abs() > 1e-6 {
                    genome.metrics.network_share = share;
                    dirty = true;
                }
            }
            if dirty {
                let _ = self.persist_genome(genome);
            }
        }
        Ok(report)
    }

    // ── sign-offs ──────────────────────────────────────────────────────

    fn persist_signoff(&self, signoff: &PendingSignoff) -> Result<(), AppError> {
        let payload = serde_json::to_string(signoff).map_err(json_err)?;
        let store = self.store.lock();
        store
            .execute(
                "INSERT INTO genome_signoffs (signoff_id, genome_id, status, payload, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(signoff_id) DO UPDATE SET status = excluded.status, payload = excluded.payload",
                params![signoff.signoff_id, signoff.genome_id, signoff.status, payload, signoff.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    /// The human sign-off queue — residual judgments that escaped the conformal
    /// set and need a person.
    pub fn signoffs(&self, status: Option<&str>) -> Result<Vec<PendingSignoff>, AppError> {
        let store = self.store.lock();
        let mut statement = store
            .prepare("SELECT payload FROM genome_signoffs ORDER BY created_at_ms DESC LIMIT 200")
            .map_err(sql_err)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut signoffs = Vec::new();
        for row in rows {
            if let Ok(signoff) = serde_json::from_str::<PendingSignoff>(&row.map_err(sql_err)?) {
                if status.is_none_or(|wanted| signoff.status == wanted) {
                    signoffs.push(signoff);
                }
            }
        }
        Ok(signoffs)
    }

    /// Resolve a sign-off: a human approves or rejects the flagged judgment.
    pub fn resolve_signoff(
        &self,
        signoff_id: &str,
        approve: bool,
        note: Option<String>,
    ) -> Result<PendingSignoff, AppError> {
        let store = self.store.lock();
        let payload: String = store
            .query_row(
                "SELECT payload FROM genome_signoffs WHERE signoff_id = ?1",
                params![signoff_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::NotFound(format!("sign-off {signoff_id} not found")))?;
        drop(store);
        let mut signoff: PendingSignoff = serde_json::from_str(&payload).map_err(json_err)?;
        if signoff.status != "pending" {
            return Err(AppError::Conflict(format!(
                "sign-off is already {}",
                signoff.status
            )));
        }
        signoff.status = if approve {
            "approved".into()
        } else {
            "rejected".into()
        };
        if let Some(note) = note {
            signoff.actor = format!("{} ({note})", signoff.actor);
        }
        self.persist_signoff(&signoff)?;
        Ok(signoff)
    }

    /// An external monitor reports a real envelope breach: stamp the genome's
    /// violation ledger (so the certificate's device check fails) and re-verify,
    /// which quarantines the genome. The system's own tripwire.
    pub fn report_violation(
        &self,
        genome_id: &str,
        detail: &str,
    ) -> Result<CertificateReport, AppError> {
        let mut genome = self.get(genome_id)?;
        // Write to the exact path string the certificate's compiled check reads,
        // so the device re-verification finds the marker on the next pass.
        let path = runtime::violation_ledger_path(&self.data_root, &genome.genome_id);
        let record = runtime::violation_record(&genome.genome_id, detail);
        let mut existing = std::fs::read_to_string(&path).unwrap_or_default();
        existing.push_str(&record);
        std::fs::write(&path, existing).map_err(|error| {
            AppError::Internal(format!("failed to write violation ledger: {error}"))
        })?;
        genome.metrics.violations += 1;
        genome.status = GenomeStatus::Quarantined;
        self.persist_genome(&genome)?;
        // Re-verify, which now fails the device check and quarantines the genome.
        let mut report = self.certificate(genome_id)?;
        report.verify_detail = format!("violation recorded: {detail}; {}", report.verify_detail);
        Ok(report)
    }

    // ── commons graph + overview ───────────────────────────────────────

    /// The refinement lattice the console renders.
    pub fn commons_graph(&self) -> Result<LatticeGraph, AppError> {
        Ok(commons::lattice_graph(&self.list()?))
    }

    /// The landing dashboard aggregate.
    pub fn overview(&self) -> Result<Overview, AppError> {
        let genomes = self.list()?;
        let risk = underwriting::assess(&genomes, self.cap);
        let pending = self.signoffs(Some("pending"))?.len();
        let mut operations = std::collections::BTreeSet::new();
        let mut total_runs = 0u64;
        let mut admitted = 0u64;
        let mut blocked = 0u64;
        let mut human_signoffs = 0u64;
        let mut total_outcome = 0.0;
        let mut validity_sum = 0.0;
        let (mut minted, mut certified, mut quarantined) = (0, 0, 0);
        for genome in &genomes {
            operations.insert(genome.operation.clone());
            total_runs += genome.metrics.runs;
            admitted += genome.metrics.admitted;
            blocked += genome.metrics.blocked;
            human_signoffs += genome.metrics.human_signoffs;
            total_outcome += genome.metrics.outcome_value;
            validity_sum += if genome.certificate.is_valid() {
                1.0
            } else {
                0.0
            };
            match genome.status {
                GenomeStatus::Minted => minted += 1,
                GenomeStatus::Certified => certified += 1,
                GenomeStatus::Quarantined => quarantined += 1,
                _ => {}
            }
        }
        let generations = genomes.iter().map(|g| g.generation).max().unwrap_or(0) + 1;
        let concentration_breaches = risk.concentration.iter().filter(|c| c.breached).count();
        let mean_validity = if genomes.is_empty() {
            0.0
        } else {
            validity_sum / genomes.len() as f64
        };
        Ok(Overview {
            total_genomes: genomes.len(),
            minted,
            certified,
            quarantined,
            operations: operations.len(),
            generations,
            total_runs,
            admitted,
            blocked,
            human_signoffs,
            pending_signoffs: pending,
            total_outcome_value: total_outcome,
            total_premium: risk.total_premium,
            systemic_risk_score: risk.systemic_risk_score,
            concentration_breaches,
            mean_certificate_validity: mean_validity,
        })
    }
}

fn sql_err(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("genome store error: {error}"))
}

fn json_err(error: serde_json::Error) -> AppError {
    AppError::Internal(format!("genome serialization error: {error}"))
}

// ── compilation: plain-language intent → an operational genome ─────────

/// A deterministic intent compiler. It recognises the billing beachhead families
/// and otherwise produces a generic money-conserving, idempotent operation with a
/// judgment residual — always certifiable, so the commons is never empty.
fn compile_intent(intent: &str, name: Option<String>, operation: Option<String>) -> Genome {
    let lower = intent.to_lowercase();
    let operation = operation.unwrap_or_else(|| infer_operation(&lower));
    let display_name = name.unwrap_or_else(|| derive_name(&operation, intent));
    match operation.as_str() {
        "billing.invoice" => invoice_genome(&display_name, intent),
        "billing.dunning" => dunning_genome(&display_name, intent),
        "billing.reconciliation" => reconciliation_genome(&display_name, intent),
        "billing.refund" => refund_genome(&display_name, intent),
        _ => generic_genome(&operation, &display_name, intent),
    }
}

fn infer_operation(lower: &str) -> String {
    if lower.contains("dunning")
        || lower.contains("overdue")
        || lower.contains("chase")
        || lower.contains("collect")
    {
        "billing.dunning".into()
    } else if lower.contains("reconcil")
        || lower.contains("close the month")
        || lower.contains("ledger")
    {
        "billing.reconciliation".into()
    } else if lower.contains("refund") || lower.contains("chargeback") {
        "billing.refund".into()
    } else if lower.contains("invoice") || lower.contains("bill") {
        "billing.invoice".into()
    } else {
        "operation.generic".into()
    }
}

fn derive_name(operation: &str, intent: &str) -> String {
    let short: String = intent.chars().take(48).collect();
    format!("{} — {}", operation, short.trim())
}

fn base_genome(
    operation: &str,
    name: &str,
    intent: &str,
    variables: Vec<VarSpec>,
    envelope: Envelope,
    guarantees: Envelope,
    residual: Residual,
) -> Genome {
    let now = now_ms();
    let description_bits = envelope.description_bits();
    Genome {
        genome_id: new_id("genome"),
        name: name.to_string(),
        intent: intent.to_string(),
        operation: operation.to_string(),
        variables,
        envelope: envelope.clone(),
        residual,
        contract: Contract {
            assumptions: Envelope::default(),
            guarantees,
        },
        certificate: Certificate::unanchored(envelope.compile_to_check("pending")),
        parent: None,
        lineage: Vec::new(),
        generation: 0,
        status: GenomeStatus::Draft,
        description_bits,
        deviation_bits: description_bits,
        metrics: GenomeMetrics::default(),
        created_at_ms: now,
        updated_at_ms: now,
        version: 1,
    }
}

fn invoice_genome(name: &str, intent: &str) -> Genome {
    let variables = vec![
        VarSpec::money("amount", "Invoice amount"),
        VarSpec::money("contract_value", "Contract ceiling"),
        VarSpec::count("invoice_id", "Invoice id"),
        VarSpec::flag("customer_active", "Customer active"),
        VarSpec {
            default: Some(OpValue::Money(0.0)),
            ..VarSpec::money("discount_pct", "Discount %")
        },
    ];
    let envelope = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "amount".into(),
            inflow: "contract_value".into(),
        },
        Invariant::Idempotency {
            key: "invoice_id".into(),
        },
        Invariant::Requires {
            flag: "customer_active".into(),
        },
        Invariant::Bounded {
            var: "discount_pct".into(),
            lo: 0.0,
            hi: 30.0,
        },
    ]);
    let guarantees = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "amount".into(),
            inflow: "contract_value".into(),
        },
        Invariant::Idempotency {
            key: "invoice_id".into(),
        },
    ]);
    let residual = Residual::new(
        "how much to bill within the contract ceiling",
        "amount",
        1000.0,
        400.0,
        0.1,
    );
    base_genome(
        "billing.invoice",
        name,
        intent,
        variables,
        envelope,
        guarantees,
        residual,
    )
}

fn dunning_genome(name: &str, intent: &str) -> Genome {
    let variables = vec![
        VarSpec::money("balance_due", "Balance due"),
        VarSpec::money("paid", "Amount paid"),
        VarSpec::money("refund", "Goodwill credit"),
        VarSpec::count("reminders_sent", "Reminders sent"),
        VarSpec::count("invoice_id", "Invoice id"),
        VarSpec::flag("account_frozen", "Account frozen"),
        VarSpec::tag("segment", "Customer segment"),
    ];
    let envelope = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "refund".into(),
            inflow: "paid".into(),
        },
        Invariant::Idempotency {
            key: "invoice_id".into(),
        },
        Invariant::RateLimit {
            counter: "reminders_sent".into(),
            max: 5.0,
        },
        Invariant::Forbids {
            flag: "account_frozen".into(),
        },
    ]);
    let guarantees = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "refund".into(),
            inflow: "paid".into(),
        },
        Invariant::RateLimit {
            counter: "reminders_sent".into(),
            max: 5.0,
        },
    ]);
    let residual = Residual::new(
        "the tone, timing, and goodwill of the next dunning touch",
        "refund",
        25.0,
        25.0,
        0.1,
    );
    base_genome(
        "billing.dunning",
        name,
        intent,
        variables,
        envelope,
        guarantees,
        residual,
    )
}

fn reconciliation_genome(name: &str, intent: &str) -> Genome {
    let variables = vec![
        VarSpec::money("ledger_delta", "Ledger delta"),
        VarSpec::money("tolerance", "Allowed tolerance"),
        VarSpec::count("statement_id", "Statement id"),
        VarSpec::flag("books_locked", "Books locked"),
    ];
    let envelope = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "ledger_delta".into(),
            inflow: "tolerance".into(),
        },
        Invariant::Idempotency {
            key: "statement_id".into(),
        },
        Invariant::Forbids {
            flag: "books_locked".into(),
        },
    ]);
    let guarantees = Envelope::new(vec![Invariant::MoneyConservation {
        outflow: "ledger_delta".into(),
        inflow: "tolerance".into(),
    }]);
    let residual = Residual::new(
        "which unmatched lines to auto-clear vs escalate",
        "ledger_delta",
        0.0,
        50.0,
        0.05,
    );
    base_genome(
        "billing.reconciliation",
        name,
        intent,
        variables,
        envelope,
        guarantees,
        residual,
    )
}

fn refund_genome(name: &str, intent: &str) -> Genome {
    let variables = vec![
        VarSpec::money("refund", "Refund amount"),
        VarSpec::money("paid", "Amount paid"),
        VarSpec::count("charge_id", "Charge id"),
        VarSpec::flag("eligible", "Refund eligible"),
        VarSpec::tag("reason", "Refund reason"),
    ];
    let envelope = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "refund".into(),
            inflow: "paid".into(),
        },
        Invariant::Idempotency {
            key: "charge_id".into(),
        },
        Invariant::Requires {
            flag: "eligible".into(),
        },
        Invariant::Eligibility {
            var: "reason".into(),
            allowed: vec![
                "defect".into(),
                "duplicate".into(),
                "cancellation".into(),
                "goodwill".into(),
            ],
        },
    ]);
    let guarantees = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "refund".into(),
            inflow: "paid".into(),
        },
        Invariant::Idempotency {
            key: "charge_id".into(),
        },
    ]);
    let residual = Residual::new(
        "the wise refund within eligibility and conservation",
        "refund",
        50.0,
        30.0,
        0.1,
    );
    base_genome(
        "billing.refund",
        name,
        intent,
        variables,
        envelope,
        guarantees,
        residual,
    )
}

fn generic_genome(operation: &str, name: &str, intent: &str) -> Genome {
    let variables = vec![
        VarSpec::money("outflow", "Value released"),
        VarSpec::money("inflow", "Value authorized"),
        VarSpec::count("operation_id", "Operation id"),
        VarSpec::flag("authorized", "Authorized"),
    ];
    let envelope = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "outflow".into(),
            inflow: "inflow".into(),
        },
        Invariant::Idempotency {
            key: "operation_id".into(),
        },
        Invariant::Requires {
            flag: "authorized".into(),
        },
    ]);
    let guarantees = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "outflow".into(),
            inflow: "inflow".into(),
        },
        Invariant::Idempotency {
            key: "operation_id".into(),
        },
    ]);
    let residual = Residual::new(
        "the discretionary judgment inside the envelope",
        "outflow",
        100.0,
        50.0,
        0.1,
    );
    base_genome(
        operation, name, intent, variables, envelope, guarantees, residual,
    )
}

/// The billing beachhead: the genomes seeded on first boot.
fn seed_genomes() -> Vec<Genome> {
    vec![
        invoice_genome(
            "Invoice Issuance",
            "Issue invoices within the contract ceiling, never double-billing the same invoice id, only for active customers.",
        ),
        dunning_genome(
            "Overdue Dunning",
            "Chase overdue invoices with escalating but rate-limited reminders, never refunding more than was paid and never touching frozen accounts.",
        ),
        reconciliation_genome(
            "Month-End Reconciliation",
            "Reconcile the ledger within tolerance and never post while the books are locked.",
        ),
        refund_genome(
            "Refund Adjudication",
            "Issue eligible refunds within conservation, idempotent per charge, only for ratified reasons.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};

    fn service() -> GenomeOsService {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        let dir = std::env::temp_dir().join(format!("genome-test-{}", new_id("svc")));
        GenomeOsService::new(dir, economy, device).unwrap()
    }

    #[test]
    fn seeds_billing_beachhead_and_certifies() {
        let service = service();
        let genomes = service.list().unwrap();
        assert_eq!(genomes.len(), 4, "four beachhead genomes are seeded");
        // Every seeded genome's envelope check holds (no violations yet) so they
        // are certified or minted, never quarantined.
        for genome in &genomes {
            assert!(
                genome.certificate.envelope_holds,
                "{} should certify",
                genome.name
            );
            assert!(matches!(
                genome.status,
                GenomeStatus::Minted | GenomeStatus::Certified
            ));
        }
    }

    #[test]
    fn fork_must_be_a_refinement() {
        let service = service();
        let dunning = service
            .list()
            .unwrap()
            .into_iter()
            .find(|g| g.operation == "billing.dunning")
            .unwrap();
        // A refining deviation succeeds.
        let outcome = service
            .fork(&dunning.genome_id, "require manager approval", None)
            .unwrap();
        assert_eq!(
            outcome.genome.parent.as_deref(),
            Some(dunning.genome_id.as_str())
        );
        assert_eq!(outcome.genome.generation, 1);
        assert!(outcome.refinement.is_refinement);
    }

    #[test]
    fn run_blocks_money_violation_and_meters_safe_runs() {
        let service = service();
        let invoice = service
            .list()
            .unwrap()
            .into_iter()
            .find(|g| g.operation == "billing.invoice")
            .unwrap();
        // Safe run: amount within the contract ceiling, active customer.
        let safe = RunRequest {
            inputs: [
                ("amount".to_string(), OpValue::Money(800.0)),
                ("contract_value".to_string(), OpValue::Money(1000.0)),
                ("customer_active".to_string(), OpValue::Flag(true)),
                ("discount_pct".to_string(), OpValue::Money(5.0)),
            ]
            .into_iter()
            .collect(),
            judgment: Some(800.0),
            outcome_value: Some(800.0),
            ..Default::default()
        };
        let report = service.run(&invoice.genome_id, safe).unwrap();
        assert_eq!(report.status, "executed");

        // Unsafe run: amount exceeds the ceiling → blocked by the envelope gate.
        let unsafe_run = RunRequest {
            inputs: [
                ("amount".to_string(), OpValue::Money(5000.0)),
                ("contract_value".to_string(), OpValue::Money(1000.0)),
                ("customer_active".to_string(), OpValue::Flag(true)),
                ("discount_pct".to_string(), OpValue::Money(5.0)),
            ]
            .into_iter()
            .collect(),
            outcome_value: Some(5000.0),
            ..Default::default()
        };
        let blocked = service.run(&invoice.genome_id, unsafe_run).unwrap();
        assert_eq!(blocked.status, "blocked");
    }

    #[test]
    fn overview_reports_seeded_commons() {
        let service = service();
        let overview = service.overview().unwrap();
        assert_eq!(overview.total_genomes, 4);
        assert!(overview.operations >= 4);
    }
}

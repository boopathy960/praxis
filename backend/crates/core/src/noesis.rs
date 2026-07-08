//! Noēsis — the understanding organ: compress, imagine, and pay rent in proof.
//!
//! Every other organ in this system bottoms out in one primitive — a
//! [`Check`](crate::verification::Check) that *actually ran*. A
//! [belief](crate::active_inference) predicts one check's pass/fail; a
//! [claim](crate::proof_economy) is minted when a check survives attack. That
//! makes Astra a superb **verifier**. What it has lacked are the two faculties
//! that separate a verifier from a *thinker*:
//!
//!   1. **Generalization.** Nothing predicts the outcome of a check it has never
//!      run. The system has a *memory of facts*, not a *model of its world*.
//!   2. **Imagination.** Active inference's own honest boundary is that "every
//!      number traces back to a check that actually ran" — epistemically pure,
//!      and a cage: the system cannot ask "what *would* happen if?" without
//!      paying real device cost.
//!
//! Noēsis adds both, grounded in the same incorruptible substrate so it can
//! never drift into ungrounded fantasy. Where the [proof economy]
//! (crate::proof_economy) mints **facts that survived attack**, Noēsis mints
//! **theories that survived being wrong** — compact programs that predict the
//! outcomes of checks they were never trained on. Three mechanisms make it work:
//!
//!   * **Compression as currency.** A theory's worth is its description-length
//!     reduction over the minted corpus: `bits_explained − len(theory) −
//!     penalty(mispredicted)`. A short rule that reproduces many verified facts
//!     earns a lot; a lookup table as big as the data it explains earns nothing.
//!     This is Solomonoff/Hutter induction run as a market — shortest program
//!     that survives wins, the razor enforced economically.
//!   * **Refutation by risky prediction.** A theory is not attacked by re-running
//!     its own check (that's how the proof economy tests a claim); it is attacked
//!     by making a confident prediction about a check it was **never given**,
//!     running that check for real, and being **refuted if reality disagrees**.
//!     Popperian falsifiability, executable — the test that it *generalized*
//!     rather than memorized.
//!   * **Dream-debt.** Every imagined prediction not yet settled against a real
//!     check is *unsecured debt*. The system tracks an **epistemic leverage
//!     ratio** = behavior resting on imagination ÷ behavior resting on minted
//!     truth, so it can dream freely and still never let an untested conjecture
//!     drive the world. One inspectable scalar, like resting free energy.
//!
//! The closed loop: minted claims → **compress** into a theory → **conjecture**
//! risky predictions about un-run checks (the imagination, booked as debt) →
//! **settle** the high-value ones against reality → survivors mint, mispredictions
//! refute, and the debt is paid down. The first organ that uses the reasoner not
//! to *act* but to *theorize and dream*.
//!
//! Honest boundary: a "theory" here is a reasoner-authored rule whose description
//! length is approximated by its serialized size, not a Kolmogorov-optimal
//! program; "compression" is bits-of-checks-explained minus that proxy length,
//! not true algorithmic information; and "imagination" is the theory's own
//! forecast over un-run checks — exactly as good as the theory. That is *why*
//! every confident, untested conjecture is booked as debt and must settle against
//! a check that actually runs, through the same device layer, before it is allowed
//! to count as secured. Conjectures are restricted to reversible (read-only)
//! checks so imagination can never mutate the world on its own.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};

use crate::asc2::ReasoningExecutor;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::proof_economy::{Claim, ProofEconomy};
use crate::verification::Check;

// ── Compression-market constants (the deliberate, bounded MDL proxy) ────────
/// Each correctly predicted fact is worth this many "bits saved".
const BITS_PER_FACT: f64 = 1.0;
/// Description-length cost charged per character of a theory's program. The
/// proxy for Kolmogorov complexity: shorter rules are cheaper, so generality is
/// rewarded over enumeration.
const DESCRIPTION_BIT_PER_CHAR: f64 = 0.02;
/// Ceiling on a theory's description cost — a very long program is maximally
/// penalized but does not run away.
const DESCRIPTION_BITS_CAP: f64 = 256.0;
/// A mispredicted training fact costs more than a correct one earns: a theory
/// that lies about the corpus is worse than one that says nothing.
const MISPREDICT_PENALTY: f64 = 2.0;
/// Risky out-of-sample predictions a theory must get right (with none wrong, and
/// positive compression) to mint. Mirrors the proof economy's mint threshold.
const MINT_SURVIVED_PREDICTIONS: u32 = 2;
/// Wrong out-of-sample predictions that refute a theory outright.
const REFUTE_FAILED_PREDICTIONS: u32 = 2;
/// Fewest verified facts before compression is meaningful.
const MIN_CORPUS: usize = 2;
const DEFAULT_TRAIN_LIMIT: usize = 80;
const MAX_TRAIN_LIMIT: usize = 400;
const MAX_CONJECTURES: usize = 32;

/// A theory's standing — mirrors [`ClaimStatus`](crate::proof_economy::ClaimStatus).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TheoryStatus {
    /// Compressed and scored, awaiting risky out-of-sample tests.
    Proposed,
    /// Compresses the corpus and survived enough risky predictions — minted.
    Minted,
    /// Reality contradicted too many of its predictions.
    Refuted,
}

/// A compact, falsifiable model that predicts `Check` outcomes it was never given.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theory {
    pub theory_id: String,
    /// One-line, human-readable summary of what the theory claims.
    pub statement: String,
    /// The compact rule itself — its serialized length is its description cost.
    pub program: String,
    pub proposer: String,
    /// Minted claim ids the theory correctly reproduced over the training corpus.
    pub explains: Vec<String>,
    /// Approximate description length of `program` (the MDL cost).
    pub description_bits: f64,
    /// Bits saved by correctly predicting training facts.
    pub bits_explained: f64,
    /// `bits_explained − description_bits − penalty(mispredicted)`. Positive ⇒
    /// the theory is a genuine compression of what is known.
    pub compression_gain: f64,
    /// Training facts predicted correctly / incorrectly (scored by re-running).
    pub correct: u32,
    pub mispredicted: u32,
    /// Risky out-of-sample predictions that held / that reality contradicted.
    pub survived_refutations: u32,
    pub failed_refutations: u32,
    pub status: TheoryStatus,
    pub created_at_ms: i64,
    pub minted_at_ms: Option<i64>,
}

/// A risky forecast: the theory's prediction about a check it has NOT run. Until
/// `settled`, it is dream-debt — imagination the world has not yet confirmed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conjecture {
    pub conjecture_id: String,
    pub theory_id: String,
    pub statement: String,
    /// The un-run experiment. Always reversible (read-only) — imagination may
    /// never mutate the world.
    pub check: Check,
    pub predicted_holds: bool,
    /// How strongly the theory believes this — the weight of the debt it carries.
    pub confidence: f64,
    pub rationale: String,
    /// Value-of-information proxy: confidence per unit of check cost.
    pub expected_info: f64,
    pub settled: bool,
    pub observed_holds: Option<bool>,
    pub correct: Option<bool>,
    pub created_at_ms: i64,
    pub settled_at_ms: Option<i64>,
}

/// The one scalar governance can cap: how much of the system's standing rests on
/// imagination it has not yet paid for.
#[derive(Debug, Clone, Serialize)]
pub struct EpistemicLeverage {
    /// Σ confidence of unsettled conjectures.
    pub dream_debt: f64,
    /// Σ confidence of settled-correct conjectures + minted theories' bits.
    pub secured: f64,
    /// `dream_debt / (dream_debt + secured)`. 0 ⇒ everything is paid for.
    pub ratio: f64,
    pub unsettled_conjectures: usize,
    pub settled_conjectures: usize,
    pub minted_theories: usize,
}

/// The outcome of settling one conjecture against reality.
#[derive(Debug, Clone, Serialize)]
pub struct SettleReport {
    pub conjecture_id: String,
    pub theory_id: String,
    pub predicted_holds: bool,
    pub observed_holds: bool,
    pub correct: bool,
    pub theory_status: TheoryStatus,
    pub survived_refutations: u32,
    pub failed_refutations: u32,
    pub detail: String,
}

/// A snapshot of the organ's overall state.
#[derive(Debug, Clone, Serialize)]
pub struct NoesisStatus {
    pub theories: usize,
    pub minted_theories: usize,
    pub proposed_theories: usize,
    pub refuted_theories: usize,
    pub conjectures: usize,
    pub open_frontier: usize,
    /// Σ compression_gain across minted theories — total understanding banked.
    pub total_minted_compression: f64,
    pub leverage_ratio: f64,
}

/// Ask Noēsis to compress the verified ledger into a theory.
#[derive(Debug, Clone, Deserialize)]
pub struct CompressRequest {
    #[serde(default)]
    pub proposer: String,
    #[serde(default)]
    pub train_limit: Option<usize>,
}

/// Ask a theory to imagine risky predictions.
#[derive(Debug, Clone, Deserialize)]
pub struct ConjectureRequest {
    #[serde(default)]
    pub count: Option<usize>,
}

/// The understanding organ. Holds the durable theory/conjecture ledger, the proof
/// economy it compresses, the reasoner that authors theories, and the device
/// layer that settles every conjecture for real.
#[derive(Clone)]
pub struct Noesis {
    store: Arc<Mutex<Connection>>,
    device: Arc<DeviceCapabilities>,
    economy: ProofEconomy,
    reasoner: Arc<dyn ReasoningExecutor>,
}

/// One verified fact handed to the reasoner under an opaque reference, so the
/// model never sees — or can fabricate — internal claim ids.
struct CorpusEntry {
    reference: String,
    claim: Claim,
    currently_holds: bool,
}

impl Noesis {
    pub fn new(
        data_dir: impl AsRef<Path>,
        device: Arc<DeviceCapabilities>,
        economy: ProofEconomy,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("noesis.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create noesis dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open noesis ledger: {e}")))?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
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
        let connection = Connection::open_in_memory().map_err(sql_err)?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
            economy,
            reasoner,
        })
    }

    /// Compress the verified ledger into the shortest theory that reproduces it.
    /// Pulls minted claims (which hold) and refuted claims (which fail) as a
    /// labelled corpus, asks the reasoner for a compact predictive rule, then
    /// **scores it against reality** by re-running each predicted fact's check
    /// for real. The score is description-length reduction — bits explained minus
    /// the program's length minus a penalty for lies — so a short, general rule
    /// over many facts wins and an enumeration earns nothing.
    pub async fn compress(&self, request: CompressRequest) -> Result<Theory, AppError> {
        let proposer = {
            let p = request.proposer.trim();
            if p.is_empty() {
                "noesis".to_string()
            } else {
                p.to_string()
            }
        };
        let train_limit = request
            .train_limit
            .unwrap_or(DEFAULT_TRAIN_LIMIT)
            .clamp(MIN_CORPUS, MAX_TRAIN_LIMIT);

        // The corpus: minted facts (label: holds) and refuted facts (label: fails).
        // Both labels are present, so the rule must actually discriminate.
        let mut entries: Vec<CorpusEntry> = Vec::new();
        for claim in self.economy.ledger()? {
            if entries.len() >= train_limit {
                break;
            }
            entries.push(CorpusEntry {
                reference: format!("c{}", entries.len()),
                claim,
                currently_holds: true,
            });
        }
        for claim in self.economy.refuted_claims()? {
            if entries.len() >= train_limit {
                break;
            }
            entries.push(CorpusEntry {
                reference: format!("c{}", entries.len()),
                claim,
                currently_holds: false,
            });
        }
        if entries.len() < MIN_CORPUS {
            return Err(AppError::Validation(format!(
                "need at least {MIN_CORPUS} verified facts to compress; the ledger yielded {}",
                entries.len()
            )));
        }

        let corpus_json: Vec<Value> = entries
            .iter()
            .map(|e| {
                json!({
                    "ref": e.reference,
                    "statement": e.claim.statement,
                    "check": serde_json::to_value(&e.claim.verification).unwrap_or(Value::Null),
                    "currently_holds": e.currently_holds,
                })
            })
            .collect();
        let user = format!(
            "Here are {} verified facts. Each is a re-runnable check that currently holds (true) \
             or fails (false).\n\nFACTS:\n{}\n\nFind the SHORTEST general rule (\"theory\") that \
             predicts, for each fact, whether its check holds. Reward = facts explained minus the \
             length of your theory: be maximally compact and general, never an enumeration.\n\n\
             Return STRICT JSON only, no prose:\n{}",
            entries.len(),
            serde_json::to_string_pretty(&corpus_json).unwrap_or_default(),
            COMPRESS_SCHEMA,
        );
        let raw = self
            .reasoner
            .complete(COMPRESS_SYSTEM.to_string(), user)
            .await?;
        let parsed: RawTheory = parse_json(&raw).ok_or_else(|| {
            AppError::Internal("could not parse a theory from the reasoner".into())
        })?;

        let program = parsed.program.trim().to_string();
        if program.is_empty() {
            return Err(AppError::Internal(
                "the reasoner returned an empty theory program".into(),
            ));
        }

        // Score against reality: re-run each predicted fact's check for real.
        let by_ref: HashMap<&str, &CorpusEntry> =
            entries.iter().map(|e| (e.reference.as_str(), e)).collect();
        let mut correct = 0u32;
        let mut mispredicted = 0u32;
        let mut explains: Vec<String> = Vec::new();
        for pred in &parsed.predictions {
            let Some(entry) = by_ref.get(pred.reference.as_str()) else {
                continue;
            };
            let outcome = crate::verification::run(&entry.claim.verification, &self.device, None);
            if outcome.pass == pred.holds {
                correct += 1;
                explains.push(entry.claim.claim_id.clone());
            } else {
                mispredicted += 1;
            }
        }

        let description_bits =
            (program.chars().count() as f64 * DESCRIPTION_BIT_PER_CHAR).min(DESCRIPTION_BITS_CAP);
        let bits_explained = correct as f64 * BITS_PER_FACT;
        let compression_gain =
            bits_explained - description_bits - (mispredicted as f64 * MISPREDICT_PENALTY);

        let statement = {
            let s = parsed.statement.trim();
            if s.is_empty() {
                program.clone()
            } else {
                s.to_string()
            }
        };
        let theory = Theory {
            theory_id: new_id("theory"),
            statement,
            program,
            proposer,
            explains,
            description_bits,
            bits_explained,
            compression_gain,
            correct,
            mispredicted,
            survived_refutations: 0,
            failed_refutations: 0,
            status: TheoryStatus::Proposed,
            created_at_ms: now_ms(),
            minted_at_ms: None,
        };
        self.save_theory(&theory)?;
        Ok(theory)
    }

    /// Imagine: ask a theory for risky, falsifiable predictions about checks it
    /// was never given. Each becomes an unsettled [`Conjecture`] — dream-debt —
    /// on the Ignorance Frontier, ranked by value of information. Only reversible
    /// (read-only) checks are accepted, so imagination can never mutate the world.
    pub async fn conjecture(
        &self,
        theory_id: &str,
        count: usize,
    ) -> Result<Vec<Conjecture>, AppError> {
        let theory = self.get_theory(theory_id)?;
        let count = count.clamp(1, MAX_CONJECTURES);
        let user = format!(
            "THEORY: {}\nRULE: {}\n\nProduce up to {} NEW, read-only checks this rule makes a \
             confident, falsifiable prediction about — checks NOT already trivially known, where \
             the rule could be proven WRONG. Use ONLY non-mutating checks (never write, move, or \
             delete). Valid check shapes:\n{}\n\nReturn STRICT JSON only, no prose:\n{}",
            theory.statement, theory.program, count, CHECK_SHAPES, CONJECTURE_SCHEMA,
        );
        let raw = self
            .reasoner
            .complete(CONJECTURE_SYSTEM.to_string(), user)
            .await?;
        let parsed: RawConjectureBatch = parse_json(&raw).ok_or_else(|| {
            AppError::Internal("could not parse conjectures from the reasoner".into())
        })?;

        let now = now_ms();
        let mut out = Vec::new();
        for rc in parsed.conjectures.into_iter().take(count) {
            if !rc.check.is_reversible() {
                tracing::warn!(
                    theory = %theory.theory_id,
                    "noesis dropped a non-reversible conjecture (imagination may not mutate the world)"
                );
                continue;
            }
            let confidence = rc.confidence.unwrap_or(0.6).clamp(0.0, 1.0);
            let expected_info = confidence / rc.check.cost().max(0.1);
            let conjecture = Conjecture {
                conjecture_id: new_id("conj"),
                theory_id: theory.theory_id.clone(),
                statement: rc.statement.trim().to_string(),
                check: rc.check,
                predicted_holds: rc.predicted_holds,
                confidence,
                rationale: rc.rationale.trim().to_string(),
                expected_info,
                settled: false,
                observed_holds: None,
                correct: None,
                created_at_ms: now,
                settled_at_ms: None,
            };
            self.save_conjecture(&conjecture)?;
            out.push(conjecture);
        }
        Ok(out)
    }

    /// Settle a conjecture against reality: run its (reversible) check for real
    /// and compare to the prediction. A correct prediction is a survived
    /// refutation that pays down dream-debt; a wrong one is a failed refutation.
    /// A theory that compresses and survives [`MINT_SURVIVED_PREDICTIONS`] risky
    /// predictions with none wrong is **minted**; one that fails
    /// [`REFUTE_FAILED_PREDICTIONS`] is **refuted**.
    pub fn settle(&self, conjecture_id: &str) -> Result<SettleReport, AppError> {
        let mut conjecture = self.load_conjecture(conjecture_id)?;
        if conjecture.settled {
            let theory = self.get_theory(&conjecture.theory_id)?;
            return Ok(SettleReport {
                conjecture_id: conjecture.conjecture_id,
                theory_id: theory.theory_id,
                predicted_holds: conjecture.predicted_holds,
                observed_holds: conjecture.observed_holds.unwrap_or(false),
                correct: conjecture.correct.unwrap_or(false),
                theory_status: theory.status,
                survived_refutations: theory.survived_refutations,
                failed_refutations: theory.failed_refutations,
                detail: "already settled".into(),
            });
        }
        if !conjecture.check.is_reversible() {
            return Err(AppError::Validation(
                "a non-reversible conjecture cannot be auto-settled".into(),
            ));
        }

        // Device call outside the store lock — a slow check never blocks the ledger.
        let outcome = crate::verification::run(&conjecture.check, &self.device, None);
        let observed = outcome.pass;
        let correct = observed == conjecture.predicted_holds;
        let now = now_ms();
        conjecture.settled = true;
        conjecture.observed_holds = Some(observed);
        conjecture.correct = Some(correct);
        conjecture.settled_at_ms = Some(now);
        self.save_conjecture(&conjecture)?;

        let mut theory = self.get_theory(&conjecture.theory_id)?;
        if correct {
            theory.survived_refutations += 1;
        } else {
            theory.failed_refutations += 1;
        }
        if theory.status == TheoryStatus::Proposed {
            if !correct && theory.failed_refutations >= REFUTE_FAILED_PREDICTIONS {
                theory.status = TheoryStatus::Refuted;
            } else if theory.compression_gain > 0.0
                && theory.survived_refutations >= MINT_SURVIVED_PREDICTIONS
                && theory.failed_refutations == 0
            {
                theory.status = TheoryStatus::Minted;
                theory.minted_at_ms = Some(now);
            }
        }
        self.save_theory(&theory)?;

        Ok(SettleReport {
            conjecture_id: conjecture.conjecture_id,
            theory_id: theory.theory_id.clone(),
            predicted_holds: conjecture.predicted_holds,
            observed_holds: observed,
            correct,
            theory_status: theory.status,
            survived_refutations: theory.survived_refutations,
            failed_refutations: theory.failed_refutations,
            detail: outcome.detail,
        })
    }

    /// The Ignorance Frontier: unsettled conjectures, highest value-of-information
    /// first — what the system most wants to find out next.
    pub fn frontier(&self, limit: usize) -> Result<Vec<Conjecture>, AppError> {
        let limit = limit.clamp(1, 500);
        let mut open: Vec<Conjecture> = self
            .all_conjectures()?
            .into_iter()
            .filter(|c| !c.settled)
            .collect();
        open.sort_by(|a, b| {
            b.expected_info
                .partial_cmp(&a.expected_info)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        open.truncate(limit);
        Ok(open)
    }

    /// The epistemic leverage ratio — how much of the system's standing rests on
    /// imagination it has not yet paid for.
    pub fn leverage(&self) -> Result<EpistemicLeverage, AppError> {
        let conjectures = self.all_conjectures()?;
        let dream_debt: f64 = conjectures
            .iter()
            .filter(|c| !c.settled)
            .map(|c| c.confidence)
            .sum();
        let settled_secured: f64 = conjectures
            .iter()
            .filter(|c| c.settled && c.correct == Some(true))
            .map(|c| c.confidence)
            .sum();
        let unsettled = conjectures.iter().filter(|c| !c.settled).count();
        let settled = conjectures.len() - unsettled;

        let theories = self.all_theories()?;
        let minted: Vec<&Theory> = theories
            .iter()
            .filter(|t| t.status == TheoryStatus::Minted)
            .collect();
        let minted_secured: f64 = minted.iter().map(|t| t.bits_explained).sum();
        let secured = settled_secured + minted_secured;
        let total = dream_debt + secured;
        let ratio = if total > 0.0 { dream_debt / total } else { 0.0 };

        Ok(EpistemicLeverage {
            dream_debt,
            secured,
            ratio,
            unsettled_conjectures: unsettled,
            settled_conjectures: settled,
            minted_theories: minted.len(),
        })
    }

    /// Every theory, newest first.
    pub fn theories(&self) -> Result<Vec<Theory>, AppError> {
        self.all_theories()
    }

    pub fn get_theory(&self, theory_id: &str) -> Result<Theory, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM noesis_theories WHERE theory_id=?1",
                [theory_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("theory {theory_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    pub fn get_conjecture(&self, conjecture_id: &str) -> Result<Conjecture, AppError> {
        self.load_conjecture(conjecture_id)
    }

    /// A snapshot of the organ's overall state.
    pub fn status(&self) -> Result<NoesisStatus, AppError> {
        let theories = self.all_theories()?;
        let minted: Vec<&Theory> = theories
            .iter()
            .filter(|t| t.status == TheoryStatus::Minted)
            .collect();
        let proposed = theories
            .iter()
            .filter(|t| t.status == TheoryStatus::Proposed)
            .count();
        let refuted = theories
            .iter()
            .filter(|t| t.status == TheoryStatus::Refuted)
            .count();
        let conjectures = self.all_conjectures()?;
        let open_frontier = conjectures.iter().filter(|c| !c.settled).count();
        let total_minted_compression = minted.iter().map(|t| t.compression_gain).sum();
        let leverage_ratio = self.leverage()?.ratio;
        Ok(NoesisStatus {
            theories: theories.len(),
            minted_theories: minted.len(),
            proposed_theories: proposed,
            refuted_theories: refuted,
            conjectures: conjectures.len(),
            open_frontier,
            total_minted_compression,
            leverage_ratio,
        })
    }

    // ── persistence ─────────────────────────────────────────────────────────

    fn save_theory(&self, theory: &Theory) -> Result<(), AppError> {
        let payload = serde_json::to_string(theory).map_err(ser_err)?;
        let status = status_tag(theory.status);
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO noesis_theories (theory_id, payload, status, created_at_ms) VALUES (?1,?2,?3,?4)",
                params![theory.theory_id, payload, status, theory.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn all_theories(&self) -> Result<Vec<Theory>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM noesis_theories ORDER BY created_at_ms DESC LIMIT 500")
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

    fn save_conjecture(&self, conjecture: &Conjecture) -> Result<(), AppError> {
        let payload = serde_json::to_string(conjecture).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO noesis_conjectures (conjecture_id, payload, theory_id, settled, created_at_ms) VALUES (?1,?2,?3,?4,?5)",
                params![
                    conjecture.conjecture_id,
                    payload,
                    conjecture.theory_id,
                    conjecture.settled as i64,
                    conjecture.created_at_ms
                ],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn load_conjecture(&self, conjecture_id: &str) -> Result<Conjecture, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM noesis_conjectures WHERE conjecture_id=?1",
                [conjecture_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("conjecture {conjecture_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    fn all_conjectures(&self) -> Result<Vec<Conjecture>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare(
                "SELECT payload FROM noesis_conjectures ORDER BY created_at_ms DESC LIMIT 2000",
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
}

// ── reasoner I/O contracts ──────────────────────────────────────────────────

const COMPRESS_SYSTEM: &str = "You are Noēsis, the theory-synthesizer of a verified intelligence. You are given verified \
     facts, each a deterministic check that currently holds or fails. Your job is to find the \
     SHORTEST general rule that reproduces them — Occam's razor as an objective. A rule that \
     subsumes many facts in few words is worth far more than an enumeration. You output strict \
     JSON only.";

const COMPRESS_SCHEMA: &str = r#"{
  "statement": "<one sentence describing the theory>",
  "program": "<the shortest general rule that predicts the facts; keep it compact>",
  "predictions": [ { "ref": "<fact ref>", "holds": true_or_false }, ... ]
}"#;

const CONJECTURE_SYSTEM: &str = "You are Noēsis, imagining the consequences of a theory. You propose NEW, read-only checks the \
     theory makes a confident, falsifiable prediction about — risky predictions that could prove \
     the theory wrong. Never propose a check that writes, moves, or deletes anything. You output \
     strict JSON only.";

const CHECK_SHAPES: &str = r#"  {"type":"file_exists","path":"..."}
  {"type":"file_contains","path":"...","substring":"..."}
  {"type":"path_absent","path":"..."}
  {"type":"command_succeeds","command":"<read-only command>"}
  {"type":"command_fails","command":"<read-only command>"}
  {"type":"shell_output_contains","command":"<read-only command>","substring":"..."}
  {"type":"trivial","pass":true_or_false}"#;

const CONJECTURE_SCHEMA: &str = r#"{
  "conjectures": [
    {
      "statement": "<what is being predicted>",
      "check": { "type": "...", ... },
      "predicted_holds": true_or_false,
      "confidence": 0.0_to_1.0,
      "rationale": "<why the theory predicts this>"
    }, ...
  ]
}"#;

#[derive(Debug, Deserialize)]
struct RawTheory {
    #[serde(default)]
    statement: String,
    #[serde(default)]
    program: String,
    #[serde(default)]
    predictions: Vec<RawPrediction>,
}

#[derive(Debug, Deserialize)]
struct RawPrediction {
    #[serde(rename = "ref", default)]
    reference: String,
    #[serde(default)]
    holds: bool,
}

#[derive(Debug, Deserialize)]
struct RawConjectureBatch {
    #[serde(default)]
    conjectures: Vec<RawConjecture>,
}

#[derive(Debug, Deserialize)]
struct RawConjecture {
    #[serde(default)]
    statement: String,
    check: Check,
    #[serde(default)]
    predicted_holds: bool,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    rationale: String,
}

/// Tolerant JSON extraction: reasoning models often wrap their answer in prose or
/// markdown fences. Take the outermost `{`…`}` span and parse that.
fn parse_json<T: serde::de::DeserializeOwned>(raw: &str) -> Option<T> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&raw[start..=end]).ok()
}

const SCHEMA: &str = "PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS noesis_theories (
        theory_id TEXT PRIMARY KEY, payload TEXT NOT NULL,
        status TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS noesis_conjectures (
        conjecture_id TEXT PRIMARY KEY, payload TEXT NOT NULL,
        theory_id TEXT NOT NULL, settled INTEGER NOT NULL, created_at_ms INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS noesis_conj_theory ON noesis_conjectures(theory_id);";

fn status_tag(status: TheoryStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "proposed".into())
}

fn sql_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("noesis sql error: {e}"))
}
fn ser_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("noesis serialize error: {e}"))
}
fn de_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("noesis decode error: {e}"))
}

/// Spawn the continuous understanding reflex on its own thread+runtime (mirrors
/// [active inference](crate::active_inference::spawn_active_inference)). Gated
/// behind `ASTRA_NOESIS=on`; every `ASTRA_NOESIS_INTERVAL_SECS` (default 240, min
/// 30) it compresses the verified ledger into a theory, and — if the theory is a
/// genuine compression — imagines a few risky predictions and settles the
/// reversible ones against reality, logging the epistemic leverage. Needs a
/// configured remote model to be useful (the stub cannot author theories).
pub fn spawn_noesis(engine: Noesis) {
    let enabled = std::env::var("ASTRA_NOESIS")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!(
            "noesis disabled (set ASTRA_NOESIS=on to enable the compress→imagine→settle reflex)"
        );
        return;
    }
    let interval = std::env::var("ASTRA_NOESIS_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s >= 30)
        .unwrap_or(240);
    std::thread::Builder::new()
        .name("astra-noesis".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                loop {
                    match engine
                        .compress(CompressRequest {
                            proposer: "reflex".into(),
                            train_limit: None,
                        })
                        .await
                    {
                        Ok(theory) => {
                            tracing::info!(
                                theory = %theory.theory_id,
                                gain = theory.compression_gain,
                                correct = theory.correct,
                                mispredicted = theory.mispredicted,
                                "noesis compressed the ledger into a theory"
                            );
                            if theory.compression_gain > 0.0 {
                                match engine.conjecture(&theory.theory_id, 3).await {
                                    Ok(conjectures) => {
                                        for conjecture in &conjectures {
                                            match engine.settle(&conjecture.conjecture_id) {
                                                Ok(report) => tracing::info!(
                                                    conjecture = %report.conjecture_id,
                                                    correct = report.correct,
                                                    status = ?report.theory_status,
                                                    "noesis settled a conjecture against reality"
                                                ),
                                                Err(error) => tracing::warn!(
                                                    error = %error,
                                                    "noesis could not settle a conjecture"
                                                ),
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(error = %error, "noesis conjecture failed")
                                    }
                                }
                            }
                            if let Ok(leverage) = engine.leverage() {
                                tracing::info!(
                                    dream_debt = leverage.dream_debt,
                                    secured = leverage.secured,
                                    ratio = leverage.ratio,
                                    "noesis epistemic leverage"
                                );
                            }
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "noesis compress skipped")
                        }
                    }
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            });
        })
        .expect("failed to spawn noesis thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc2::{BoxFuture, ReasoningProposal, ReasoningRequest};
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};
    use crate::proof_economy::{AttackRequest, ClaimKind, ProofEconomy, ProposeRequest};
    use std::collections::VecDeque;

    /// A reasoner that replays a fixed script of `complete` responses.
    struct ScriptedReasoner {
        replies: Mutex<VecDeque<String>>,
    }
    impl ScriptedReasoner {
        fn new(replies: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(replies.into_iter().map(str::to_string).collect()),
            })
        }
    }
    impl ReasoningExecutor for ScriptedReasoner {
        fn name(&self) -> &str {
            "scripted"
        }
        fn execute(
            &self,
            _request: ReasoningRequest,
        ) -> BoxFuture<Result<ReasoningProposal, AppError>> {
            Box::pin(async { Err(AppError::Internal("execute unused in noesis tests".into())) })
        }
        fn complete(&self, _system: String, _user: String) -> BoxFuture<Result<String, AppError>> {
            let next = self.replies.lock().pop_front().unwrap_or_default();
            Box::pin(async move { Ok(next) })
        }
    }

    fn economy() -> ProofEconomy {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        ProofEconomy::in_memory(device).expect("economy")
    }

    /// Mint a claim by surviving the two required attacks.
    fn mint(econ: &ProofEconomy, statement: &str) -> String {
        let claim = econ
            .propose(ProposeRequest {
                statement: statement.into(),
                kind: ClaimKind::Assertion,
                proposer: "seed".into(),
                verification: Check::Trivial { pass: true },
                evidence: vec![],
                depends_on: vec![],
            })
            .expect("propose");
        for who in ["a", "b"] {
            econ.attack(
                &claim.claim_id,
                AttackRequest {
                    attacker: who.into(),
                    note: "x".into(),
                    counter: None,
                },
            )
            .expect("attack");
        }
        claim.claim_id
    }

    /// Create a claim that is refuted on arrival (its check fails).
    fn refute_on_arrival(econ: &ProofEconomy, statement: &str) {
        econ.propose(ProposeRequest {
            statement: statement.into(),
            kind: ClaimKind::Assertion,
            proposer: "liar".into(),
            verification: Check::Trivial { pass: false },
            evidence: vec![],
            depends_on: vec![],
        })
        .expect("propose");
    }

    fn noesis_with(econ: ProofEconomy, replies: Vec<&str>) -> Noesis {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        Noesis::in_memory(device, econ, ScriptedReasoner::new(replies)).expect("noesis")
    }

    #[actix_web::test]
    async fn compress_scores_a_theory_against_the_verified_corpus() {
        let econ = economy();
        mint(&econ, "the host can echo");
        refute_on_arrival(&econ, "the impossible holds");
        // c0 = minted (holds), c1 = refuted (fails).
        let reply = r#"{"statement":"minted facts hold, refuted facts fail",
            "program":"holds iff minted",
            "predictions":[{"ref":"c0","holds":true},{"ref":"c1","holds":false}]}"#;
        let noesis = noesis_with(econ, vec![reply]);

        let theory = noesis
            .compress(CompressRequest {
                proposer: "tester".into(),
                train_limit: None,
            })
            .await
            .expect("compress");

        assert_eq!(theory.correct, 2, "both facts predicted correctly");
        assert_eq!(theory.mispredicted, 0);
        assert_eq!(theory.bits_explained, 2.0);
        assert!(
            theory.compression_gain > 0.0,
            "a short rule over two facts should be a net compression, got {}",
            theory.compression_gain
        );
        assert_eq!(theory.status, TheoryStatus::Proposed);
        assert_eq!(theory.explains.len(), 2);
    }

    #[actix_web::test]
    async fn surviving_risky_predictions_mints_a_theory() {
        let econ = economy();
        mint(&econ, "fact one");
        refute_on_arrival(&econ, "fact two");
        let compress_reply = r#"{"statement":"t","program":"r",
            "predictions":[{"ref":"c0","holds":true},{"ref":"c1","holds":false}]}"#;
        // Two risky predictions the world will confirm (trivial-true, predicted true).
        let conjecture_reply = r#"{"conjectures":[
            {"statement":"a","check":{"type":"trivial","pass":true},"predicted_holds":true,"confidence":0.9,"rationale":"x"},
            {"statement":"b","check":{"type":"trivial","pass":true},"predicted_holds":true,"confidence":0.8,"rationale":"y"}
        ]}"#;
        let noesis = noesis_with(econ, vec![compress_reply, conjecture_reply]);

        let theory = noesis
            .compress(CompressRequest::default_for_test())
            .await
            .expect("compress");
        assert!(theory.compression_gain > 0.0);

        let conjectures = noesis
            .conjecture(&theory.theory_id, 2)
            .await
            .expect("conjecture");
        assert_eq!(conjectures.len(), 2, "two reversible conjectures accepted");

        for conjecture in &conjectures {
            noesis.settle(&conjecture.conjecture_id).expect("settle");
        }
        let minted = noesis.get_theory(&theory.theory_id).expect("theory");
        assert_eq!(minted.status, TheoryStatus::Minted, "survivor mints");
        assert_eq!(minted.survived_refutations, 2);
        assert_eq!(minted.failed_refutations, 0);
    }

    #[actix_web::test]
    async fn a_wrong_prediction_is_a_failed_refutation_and_blocks_mint() {
        let econ = economy();
        mint(&econ, "fact one");
        refute_on_arrival(&econ, "fact two");
        let compress_reply = r#"{"statement":"t","program":"r",
            "predictions":[{"ref":"c0","holds":true},{"ref":"c1","holds":false}]}"#;
        // The theory predicts this check holds, but the world says it fails.
        let conjecture_reply = r#"{"conjectures":[
            {"statement":"wrong","check":{"type":"trivial","pass":false},"predicted_holds":true,"confidence":0.9,"rationale":"x"}
        ]}"#;
        let noesis = noesis_with(econ, vec![compress_reply, conjecture_reply]);

        let theory = noesis
            .compress(CompressRequest::default_for_test())
            .await
            .expect("compress");
        let conjectures = noesis
            .conjecture(&theory.theory_id, 1)
            .await
            .expect("conjecture");
        let report = noesis
            .settle(&conjectures[0].conjecture_id)
            .expect("settle");

        assert!(!report.correct, "reality contradicted the prediction");
        let after = noesis.get_theory(&theory.theory_id).expect("theory");
        assert_eq!(after.failed_refutations, 1);
        assert_eq!(after.survived_refutations, 0);
        assert_eq!(
            after.status,
            TheoryStatus::Proposed,
            "one failure does not yet refute, but it does block minting"
        );
    }

    #[actix_web::test]
    async fn dream_debt_falls_as_conjectures_settle() {
        let econ = economy();
        mint(&econ, "fact one");
        refute_on_arrival(&econ, "fact two");
        let compress_reply = r#"{"statement":"t","program":"r",
            "predictions":[{"ref":"c0","holds":true},{"ref":"c1","holds":false}]}"#;
        let conjecture_reply = r#"{"conjectures":[
            {"statement":"a","check":{"type":"trivial","pass":true},"predicted_holds":true,"confidence":0.9,"rationale":"x"},
            {"statement":"b","check":{"type":"trivial","pass":true},"predicted_holds":true,"confidence":0.8,"rationale":"y"}
        ]}"#;
        let noesis = noesis_with(econ, vec![compress_reply, conjecture_reply]);

        let theory = noesis
            .compress(CompressRequest::default_for_test())
            .await
            .expect("compress");
        let conjectures = noesis
            .conjecture(&theory.theory_id, 2)
            .await
            .expect("conjecture");

        let before = noesis.leverage().expect("leverage");
        assert!(before.dream_debt > 0.0, "unsettled conjectures are debt");
        assert_eq!(before.unsettled_conjectures, 2);

        for conjecture in &conjectures {
            noesis.settle(&conjecture.conjecture_id).expect("settle");
        }
        let after = noesis.leverage().expect("leverage");
        assert_eq!(after.dream_debt, 0.0, "settling pays the debt down");
        assert_eq!(after.unsettled_conjectures, 0);
        assert_eq!(after.settled_conjectures, 2);
    }

    impl CompressRequest {
        fn default_for_test() -> Self {
            Self {
                proposer: "tester".into(),
                train_limit: None,
            }
        }
    }
}

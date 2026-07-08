//! Continuous Active Inference — the generative model, surprise, and attention.
//!
//! The [Sentinel](crate::sentinel) answers "*when* did a property break?" by
//! re-running a check and recording the flip. That is reactive: it samples
//! everything on a fixed sweep and tells you, after the fact, what changed. What
//! the system has lacked is a layer that *predicts* what reality will show before
//! it looks, *notices its own error* when reality disagrees, and *decides where to
//! look next* so it is least surprised for the least effort. That layer is active
//! inference.
//!
//! This is the engineering analog of Karl Friston's free-energy principle, made
//! concrete on the one substrate this system already trusts — a re-runnable
//! [`Check`](crate::verification::Check) executed for real through the
//! [device layer](crate::device_agent):
//!
//!   * A **belief** is a prediction about the world: "this check will pass",
//!     carried with a **precision** (the model's confidence / inverse variance).
//!   * **Perception** is one cycle: for each belief, predict the outcome, sample
//!     it for real, and measure **surprise** — the Shannon information of the
//!     observed outcome under the prediction, `-ln p(observed)`. The sum of
//!     surprise across beliefs is the system's **variational free energy**: a
//!     single scalar for "how wrong is my model of the world right now?".
//!   * **Learning** minimizes free energy by updating beliefs: a confirmed
//!     prediction *hardens* (precision rises toward certainty); a violated one
//!     *softens* (precision collapses) and, if reality keeps contradicting it, the
//!     belief is **revised** — its prediction flips to match the world.
//!   * **Active inference** is choosing where to sample next to most reduce
//!     expected future surprise: an [`AttentionPlan`] ranks beliefs by the
//!     information a sample would resolve (epistemic value) plus their volatility
//!     (pragmatic value), per unit of check cost. The system forages its own
//!     attention instead of blindly sweeping everything.
//!
//! Run continuously (the background cycle), this is a mind that is never not
//! predicting: it holds a model of its world, is genuinely surprised when the
//! world moves, re-models on the spot, and spends its attention where its
//! uncertainty is highest. Minted [proof-economy](crate::proof_economy) claims can
//! be seeded as high-precision beliefs, so settled knowledge becomes the model's
//! confident priors.
//!
//! Honest boundary: "surprise" here is Bernoulli surprise over a check's
//! pass/fail, and "precision" is a bounded scalar updated by a deliberate,
//! legible rule — not a full hierarchical Gaussian generative model, and not a
//! calibrated probability. The grounding is real execution; the dynamics are a
//! faithful but simplified analog chosen so every number traces back to a check
//! that actually ran.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::asc2::ReasoningExecutor;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::proof_economy::ProofEconomy;
use crate::verification::Check;

// ── Generative-model dynamics (the deliberate, bounded learning rule) ──────
/// Resting precision a fresh belief starts with (sigmoid(1.0) ≈ 0.73 confidence).
const BASE_PRECISION: f64 = 1.0;
/// Ceiling on precision (sigmoid(6) ≈ 0.9975) — the model never becomes certain.
const PRECISION_CAP: f64 = 6.0;
/// Floor on precision — a collapsed belief keeps a sliver of confidence.
const PRECISION_FLOOR: f64 = 0.05;
/// How fast a confirmed prediction hardens.
const LEARN_RATE: f64 = 0.6;
/// Below this precision after a surprise, the belief is revised (prediction flips).
const FLIP_THRESHOLD: f64 = 0.25;
/// Smoothing for the running prediction-error average (volatility).
const EMA_ALPHA: f64 = 0.3;

/// Where a belief came from — for telemetry and to keep settled knowledge (minted
/// claims) visibly distinct from hypotheses the model is still testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeliefOrigin {
    /// Registered directly through the API.
    Declared,
    /// Seeded from a minted [proof-economy](crate::proof_economy) claim.
    MintedClaim,
    /// Seeded from a [Sentinel](crate::sentinel) watch under continuous re-verify.
    Watch,
}

/// One prediction the model holds about the world, with the confidence (precision)
/// it is held at and the running record of how surprising it has been.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Belief {
    pub belief_id: String,
    pub statement: String,
    /// The re-runnable predicate the belief is grounded in — sampled every cycle.
    pub check: Check,
    /// The model's current prediction: will the check pass?
    pub predicted_holding: bool,
    /// Confidence / inverse variance. High precision ⇒ a violation is very
    /// surprising; low precision ⇒ the model is unsure and a sample is informative.
    pub precision: f64,
    /// The most recent cycle's surprise (nats) — this belief's free-energy share.
    pub free_energy: f64,
    /// Exponential moving average of prediction error in `0.0..=1.0` (volatility).
    pub surprise_ema: f64,
    /// How many times the belief has been sampled.
    pub observations: u64,
    /// How many of those observations violated the prediction.
    pub surprises: u32,
    pub last_detail: String,
    pub origin: BeliefOrigin,
    /// The minted claim this belief mirrors, when seeded from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    pub registered_at_ms: i64,
    pub last_observed_at_ms: i64,
    /// A retired belief is skipped by the perception cycle but kept for history.
    pub active: bool,
}

/// A prediction error: the instant the world disagreed with the model. The
/// actionable output of a cycle — and the trigger the [neural orchestrator]
/// (crate::neural_orchestration) routes on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurpriseEvent {
    pub belief_id: String,
    pub statement: String,
    /// What the model predicted *before* this observation.
    pub predicted_holding: bool,
    /// What reality actually showed.
    pub observed_holding: bool,
    /// Surprise in nats, `-ln p(observed)` — larger when a confident belief breaks.
    pub prediction_error: f64,
    pub precision_before: f64,
    pub precision_after: f64,
    /// True when the contradiction was strong enough that the belief was revised
    /// (its prediction flipped to match the world).
    pub flipped: bool,
    pub detail: String,
    pub at_ms: i64,
}

/// What to place under the generative model.
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterBelief {
    pub statement: String,
    pub check: Check,
    /// The prior prediction. When omitted, the belief is calibrated to the world
    /// on arrival (it predicts whatever the first sample shows).
    #[serde(default)]
    pub expect_holding: Option<bool>,
    /// Optional starting precision (clamped). Higher ⇒ a stronger prior.
    #[serde(default)]
    pub prior_precision: Option<f64>,
}

/// The outcome of one perception cycle over every active belief.
#[derive(Debug, Clone, Serialize)]
pub struct PerceptionReport {
    pub beliefs_sampled: usize,
    /// Sum of surprise across all sampled beliefs — the system's free energy now.
    pub total_free_energy: f64,
    pub mean_precision: f64,
    /// Only the beliefs the world contradicted this cycle — the signal worth acting on.
    pub surprises: Vec<SurpriseEvent>,
    pub cycle_at_ms: i64,
}

/// One belief ranked for attention: where sampling would most reduce expected
/// future surprise per unit of effort.
#[derive(Debug, Clone, Serialize)]
pub struct AttentionItem {
    pub belief_id: String,
    pub statement: String,
    /// Epistemic value: how much uncertainty a sample would resolve (peaks when
    /// the model is maximally unsure, precision ≈ 0).
    pub epistemic_value: f64,
    /// Pragmatic value: how volatile the belief has been (recent error average).
    pub volatility: f64,
    /// Rough cost of the underlying check (from [`Check::cost`]).
    pub check_cost: f64,
    /// Policy score = (epistemic + pragmatic) / cost. Higher ⇒ sample sooner; this
    /// is the action that most reduces expected free energy per unit effort.
    pub salience: f64,
    pub precision: f64,
}

/// The system's attention policy — beliefs ordered by where to look next.
#[derive(Debug, Clone, Serialize)]
pub struct AttentionPlan {
    pub candidates: Vec<AttentionItem>,
    pub note: String,
}

/// A point in the free-energy history — the trace of the system's surprise over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeEnergyPoint {
    pub point_id: String,
    pub free_energy: f64,
    pub beliefs: usize,
    pub surprises: usize,
    pub at_ms: i64,
}

/// A snapshot of the model's overall state — its "state of mind".
#[derive(Debug, Clone, Serialize)]
pub struct MindState {
    pub beliefs: usize,
    pub active_beliefs: usize,
    pub mean_precision: f64,
    /// Sum of each active belief's last free-energy share.
    pub resting_free_energy: f64,
    pub mean_volatility: f64,
    pub cycles_logged: usize,
}

/// The continuous active-inference engine. Holds the durable belief/free-energy
/// ledger and the device layer that samples every belief for real.
#[derive(Clone)]
pub struct ActiveInference {
    store: Arc<Mutex<Connection>>,
    device: Arc<DeviceCapabilities>,
    /// Attached so settled knowledge (minted claims) can seed confident priors.
    economy: Option<ProofEconomy>,
    /// Attached so the model can reason about its own prediction errors.
    reasoner: Option<Arc<dyn ReasoningExecutor>>,
}

impl ActiveInference {
    pub fn new(
        data_dir: impl AsRef<Path>,
        device: Arc<DeviceCapabilities>,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("active_inference.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::Internal(format!("failed to create active inference dir: {e}"))
            })?;
        }
        let connection = Connection::open(&path).map_err(|e| {
            AppError::Internal(format!("failed to open active inference ledger: {e}"))
        })?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
            economy: None,
            reasoner: None,
        })
    }

    /// Test/in-memory variant.
    pub fn in_memory(device: Arc<DeviceCapabilities>) -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(sql_err)?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
            economy: None,
            reasoner: None,
        })
    }

    /// Attach the proof economy so minted claims can seed confident priors.
    #[must_use]
    pub fn with_economy(mut self, economy: ProofEconomy) -> Self {
        self.economy = Some(economy);
        self
    }

    /// Attach a reasoner so the model can hypothesize about its own surprises.
    #[must_use]
    pub fn with_reasoner(mut self, reasoner: Arc<dyn ReasoningExecutor>) -> Self {
        self.reasoner = Some(reasoner);
        self
    }

    /// Place a prediction under the generative model. The check is sampled once on
    /// arrival to calibrate the belief (and to register an immediate surprise if a
    /// supplied prior already disagrees with the world).
    pub fn register_belief(&self, request: RegisterBelief) -> Result<Belief, AppError> {
        let statement = request.statement.trim().to_string();
        if statement.is_empty() {
            return Err(AppError::Validation("belief statement is empty".into()));
        }
        let now = now_ms();
        let outcome = self.run_check(&request.check);
        let predicted_holding = request.expect_holding.unwrap_or(outcome.pass);
        let precision = request
            .prior_precision
            .unwrap_or(BASE_PRECISION)
            .clamp(PRECISION_FLOOR, PRECISION_CAP);
        let mut belief = Belief {
            belief_id: new_id("belief"),
            statement,
            check: request.check,
            predicted_holding,
            precision,
            free_energy: 0.0,
            surprise_ema: 0.0,
            observations: 0,
            surprises: 0,
            last_detail: String::new(),
            origin: BeliefOrigin::Declared,
            claim_id: None,
            registered_at_ms: now,
            last_observed_at_ms: now,
            active: true,
        };
        // Integrate the arrival sample so a wrong prior is surprising immediately.
        self.integrate(&mut belief, &outcome, now);
        self.save_belief(&belief)?;
        Ok(belief)
    }

    /// Seed minted [proof-economy](crate::proof_economy) claims as high-precision
    /// beliefs: settled, adversarially-survived knowledge becomes the model's
    /// confident priors, and the model is surprised the instant one stops holding.
    /// Idempotent — a claim already mirrored is skipped. Returns how many were
    /// newly seeded.
    pub fn seed_from_economy(&self, limit: usize) -> Result<usize, AppError> {
        let economy = self.economy.clone().ok_or_else(|| {
            AppError::Validation("no proof economy attached to seed beliefs from".into())
        })?;
        let existing: std::collections::HashSet<String> = self
            .all_beliefs()?
            .into_iter()
            .filter_map(|b| b.claim_id)
            .collect();
        let now = now_ms();
        let mut seeded = 0usize;
        for claim in economy.ledger()?.into_iter().take(limit.clamp(1, 1000)) {
            if existing.contains(&claim.claim_id) {
                continue;
            }
            let outcome = self.run_check(&claim.verification);
            // A minted claim is trusted: predict it holds, at high precision.
            let mut belief = Belief {
                belief_id: new_id("belief"),
                statement: claim.statement.clone(),
                check: claim.verification.clone(),
                predicted_holding: true,
                precision: PRECISION_CAP * 0.6,
                free_energy: 0.0,
                surprise_ema: 0.0,
                observations: 0,
                surprises: 0,
                last_detail: String::new(),
                origin: BeliefOrigin::MintedClaim,
                claim_id: Some(claim.claim_id.clone()),
                registered_at_ms: now,
                last_observed_at_ms: now,
                active: true,
            };
            self.integrate(&mut belief, &outcome, now);
            self.save_belief(&belief)?;
            seeded += 1;
        }
        Ok(seeded)
    }

    /// One perception cycle: predict, sample, and learn from every active belief.
    /// Returns the total free energy and the beliefs the world contradicted. The
    /// device calls happen outside the store lock so a slow check never blocks the
    /// ledger.
    pub fn cycle(&self) -> Result<PerceptionReport, AppError> {
        let beliefs = self.active_beliefs()?;
        let count = beliefs.len();
        let now = now_ms();
        let mut total_free_energy = 0.0;
        let mut precision_sum = 0.0;
        let mut surprises = Vec::new();
        for mut belief in beliefs {
            let (surprise, event) = self.observe(&mut belief, now);
            total_free_energy += surprise;
            precision_sum += belief.precision;
            if let Some(event) = event {
                surprises.push(event);
            }
            self.save_belief(&belief)?;
        }
        let point = FreeEnergyPoint {
            point_id: new_id("fe"),
            free_energy: total_free_energy,
            beliefs: count,
            surprises: surprises.len(),
            at_ms: now,
        };
        self.save_point(&point)?;
        Ok(PerceptionReport {
            beliefs_sampled: count,
            total_free_energy,
            mean_precision: if count > 0 {
                precision_sum / count as f64
            } else {
                0.0
            },
            surprises,
            cycle_at_ms: now,
        })
    }

    /// Re-sample a single belief now (e.g. to confirm a correction took).
    pub fn cycle_one(&self, belief_id: &str) -> Result<Belief, AppError> {
        let mut belief = self.get_belief(belief_id)?;
        let now = now_ms();
        self.observe(&mut belief, now);
        self.save_belief(&belief)?;
        Ok(belief)
    }

    /// The attention policy: rank active beliefs by where a sample would most
    /// reduce expected future surprise per unit of check cost. This is the
    /// system's epistemic foraging — what to look at next, and why.
    pub fn attention_plan(&self, limit: usize) -> Result<AttentionPlan, AppError> {
        let limit = limit.clamp(1, 200);
        let mut items: Vec<AttentionItem> = self
            .active_beliefs()?
            .into_iter()
            .map(|belief| {
                let p = sigmoid(belief.precision);
                // Epistemic value peaks when the model is maximally unsure (p≈0.5).
                let epistemic_value = 1.0 - (2.0 * p - 1.0).abs();
                let volatility = belief.surprise_ema;
                let check_cost = belief.check.cost().max(0.1);
                let salience = (epistemic_value + volatility) / check_cost;
                AttentionItem {
                    belief_id: belief.belief_id,
                    statement: belief.statement,
                    epistemic_value,
                    volatility,
                    check_cost,
                    salience,
                    precision: belief.precision,
                }
            })
            .collect();
        items.sort_by(|a, b| {
            b.salience
                .partial_cmp(&a.salience)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        items.truncate(limit);
        Ok(AttentionPlan {
            note: "beliefs ordered by expected free-energy reduction per unit cost \
                   (epistemic + pragmatic value / check cost)"
                .into(),
            candidates: items,
        })
    }

    /// Retire (or revive) a belief. Retired beliefs are skipped by the cycle but
    /// kept for their history.
    pub fn set_active(&self, belief_id: &str, active: bool) -> Result<Belief, AppError> {
        let mut belief = self.get_belief(belief_id)?;
        belief.active = active;
        self.save_belief(&belief)?;
        Ok(belief)
    }

    /// Ask the attached reasoner to hypothesize *why* a surprising belief broke —
    /// the model reflecting on its own prediction error. Needs a configured
    /// reasoner; the hypothesis is a fallible suggestion, never ground truth.
    pub async fn explain_surprise(&self, belief_id: &str) -> Result<String, AppError> {
        let belief = self.get_belief(belief_id)?;
        let reasoner = self.reasoner.clone().ok_or_else(|| {
            AppError::Validation("no reasoner attached to explain surprises".into())
        })?;
        let system = "You are the reflective layer of an active-inference system. Given a belief \
             whose prediction the world just contradicted, hypothesize the most likely cause of \
             the prediction error and one concrete action that would resolve it. Be brief and \
             concrete; you are proposing, not asserting truth."
            .to_string();
        let user = format!(
            "Belief: {}\nPredicted the check would {}.\nLatest observation: {}\nWhy was the model \
             surprised, and what one action would most reduce future surprise?",
            belief.statement,
            if belief.predicted_holding {
                "hold"
            } else {
                "fail"
            },
            belief.last_detail
        );
        reasoner.complete(system, user).await
    }

    /// A snapshot of the model's overall state.
    pub fn status(&self) -> Result<MindState, AppError> {
        let all = self.all_beliefs()?;
        let active: Vec<&Belief> = all.iter().filter(|b| b.active).collect();
        let active_count = active.len();
        let mean_precision = if active_count > 0 {
            active.iter().map(|b| b.precision).sum::<f64>() / active_count as f64
        } else {
            0.0
        };
        let mean_volatility = if active_count > 0 {
            active.iter().map(|b| b.surprise_ema).sum::<f64>() / active_count as f64
        } else {
            0.0
        };
        let resting_free_energy = active.iter().map(|b| b.free_energy).sum::<f64>();
        Ok(MindState {
            beliefs: all.len(),
            active_beliefs: active_count,
            mean_precision,
            resting_free_energy,
            mean_volatility,
            cycles_logged: self.count_points()?,
        })
    }

    pub fn beliefs(&self) -> Result<Vec<Belief>, AppError> {
        self.all_beliefs()
    }

    pub fn free_energy_log(&self, limit: usize) -> Result<Vec<FreeEnergyPoint>, AppError> {
        let limit = limit.clamp(1, 2000) as i64;
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM ai_free_energy ORDER BY at_ms DESC LIMIT ?1")
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

    pub fn get_belief(&self, belief_id: &str) -> Result<Belief, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM ai_beliefs WHERE belief_id=?1",
                [belief_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("belief {belief_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    // ── perception kernel (the only place beliefs change) ───────────────────

    /// Sample a belief for real and learn from the observation.
    fn observe(&self, belief: &mut Belief, now: i64) -> (f64, Option<SurpriseEvent>) {
        let outcome = self.run_check(&belief.check);
        self.integrate(belief, &outcome, now)
    }

    /// The free-energy-minimizing update, given an already-run observation:
    /// compute surprise, harden a confirmed belief / soften (and maybe revise) a
    /// contradicted one, and accumulate volatility.
    fn integrate(
        &self,
        belief: &mut Belief,
        outcome: &crate::verification::CheckOutcome,
        now: i64,
    ) -> (f64, Option<SurpriseEvent>) {
        let observed = outcome.pass;
        let predicted_before = belief.predicted_holding;
        let precision_before = belief.precision;
        let p_pred = sigmoid(belief.precision);
        let matched = observed == predicted_before;
        let p_obs = if matched { p_pred } else { 1.0 - p_pred };
        let surprise = -(p_obs.max(1e-6)).ln();
        let error = if matched { 0.0 } else { 1.0 };
        belief.surprise_ema = (1.0 - EMA_ALPHA) * belief.surprise_ema + EMA_ALPHA * error;

        let mut flipped = false;
        if matched {
            // Confirmed: harden toward (but never reaching) certainty.
            belief.precision = (belief.precision + LEARN_RATE * (1.0 - p_pred)).min(PRECISION_CAP);
        } else {
            // Contradicted: collapse confidence, and revise the belief if reality
            // has pushed it past the flip threshold.
            belief.surprises += 1;
            belief.precision = (belief.precision * 0.5).max(PRECISION_FLOOR);
            if belief.precision <= FLIP_THRESHOLD {
                belief.predicted_holding = observed;
                belief.precision = (BASE_PRECISION * 0.5).max(PRECISION_FLOOR);
                flipped = true;
            }
        }
        belief.observations += 1;
        belief.free_energy = surprise;
        belief.last_detail = outcome.detail.clone();
        belief.last_observed_at_ms = now;

        let event = (!matched).then(|| SurpriseEvent {
            belief_id: belief.belief_id.clone(),
            statement: belief.statement.clone(),
            predicted_holding: predicted_before,
            observed_holding: observed,
            prediction_error: surprise,
            precision_before,
            precision_after: belief.precision,
            flipped,
            detail: outcome.detail.clone(),
            at_ms: now,
        });
        (surprise, event)
    }

    fn run_check(&self, check: &Check) -> crate::verification::CheckOutcome {
        crate::verification::run(check, &self.device, None)
    }

    // ── persistence ─────────────────────────────────────────────────────────

    fn active_beliefs(&self) -> Result<Vec<Belief>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM ai_beliefs WHERE active=1 ORDER BY registered_at_ms ASC LIMIT 2000")
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

    fn all_beliefs(&self) -> Result<Vec<Belief>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM ai_beliefs ORDER BY registered_at_ms DESC LIMIT 2000")
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

    fn save_belief(&self, belief: &Belief) -> Result<(), AppError> {
        let payload = serde_json::to_string(belief).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO ai_beliefs (belief_id, payload, active, registered_at_ms) VALUES (?1,?2,?3,?4)",
                params![belief.belief_id, payload, belief.active as i64, belief.registered_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn save_point(&self, point: &FreeEnergyPoint) -> Result<(), AppError> {
        let payload = serde_json::to_string(point).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO ai_free_energy (point_id, payload, at_ms) VALUES (?1,?2,?3)",
                params![point.point_id, payload, point.at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn count_points(&self) -> Result<usize, AppError> {
        let store = self.store.lock();
        let count: i64 = store
            .query_row("SELECT COUNT(*) FROM ai_free_energy", [], |row| row.get(0))
            .map_err(sql_err)?;
        Ok(count as usize)
    }
}

const SCHEMA: &str = "PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS ai_beliefs (
        belief_id TEXT PRIMARY KEY, payload TEXT NOT NULL,
        active INTEGER NOT NULL, registered_at_ms INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS ai_free_energy (
        point_id TEXT PRIMARY KEY, payload TEXT NOT NULL, at_ms INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS ai_free_energy_at ON ai_free_energy(at_ms);";

#[must_use]
fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Spawn the continuous perception loop on its own thread+runtime (mirrors the
/// Sentinel). Gated behind `ASTRA_ACTIVE_INFERENCE=on`; runs a cycle every
/// `ASTRA_ACTIVE_INFERENCE_INTERVAL_SECS` (default 90, min 15). Each surprise is
/// logged — the moment the system's model of its world was proven wrong.
pub fn spawn_active_inference(engine: ActiveInference) {
    let enabled = std::env::var("ASTRA_ACTIVE_INFERENCE")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!(
            "active inference disabled (set ASTRA_ACTIVE_INFERENCE=on to enable continuous perception)"
        );
        return;
    }
    let interval = std::env::var("ASTRA_ACTIVE_INFERENCE_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s >= 15)
        .unwrap_or(90);
    std::thread::Builder::new()
        .name("astra-active-inference".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                loop {
                    match engine.cycle() {
                        Ok(report) => {
                            for event in &report.surprises {
                                tracing::warn!(
                                    belief = %event.belief_id,
                                    statement = %event.statement,
                                    error = event.prediction_error,
                                    flipped = event.flipped,
                                    "active inference: the model was SURPRISED"
                                );
                            }
                            tracing::info!(
                                beliefs = report.beliefs_sampled,
                                free_energy = report.total_free_energy,
                                surprises = report.surprises.len(),
                                "active inference cycle complete"
                            );
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "active inference cycle failed")
                        }
                    }
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            });
        })
        .expect("failed to spawn active inference thread");
}

fn sql_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("active inference sql error: {e}"))
}
fn ser_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("active inference serialize error: {e}"))
}
fn de_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("active inference decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};

    fn engine() -> ActiveInference {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        ActiveInference::in_memory(device).expect("engine")
    }

    #[test]
    fn confirmed_belief_hardens_and_stays_unsurprised() {
        let engine = engine();
        let belief = engine
            .register_belief(RegisterBelief {
                statement: "the host can echo".into(),
                check: Check::Trivial { pass: true },
                expect_holding: None,
                prior_precision: None,
            })
            .unwrap();
        let start_precision = belief.precision;
        // Several confirming cycles: precision rises, no surprises.
        for _ in 0..5 {
            let report = engine.cycle().unwrap();
            assert!(report.surprises.is_empty());
        }
        let after = engine.get_belief(&belief.belief_id).unwrap();
        assert!(
            after.precision > start_precision,
            "a repeatedly-confirmed belief should harden"
        );
        assert_eq!(after.surprises, 0);
        assert!(after.surprise_ema < 0.05);
    }

    #[test]
    fn model_is_surprised_then_revises_when_reality_flips() {
        let engine = engine();
        let dir = std::env::temp_dir().join(format!("astra-ai-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("state.txt");
        std::fs::write(&file, "value present").unwrap();
        let path = file.to_string_lossy().to_string();

        let belief = engine
            .register_belief(RegisterBelief {
                statement: "state file holds the value".into(),
                check: Check::FileContains {
                    path: path.clone(),
                    substring: "present".into(),
                },
                expect_holding: None,
                prior_precision: None,
            })
            .unwrap();
        assert!(belief.predicted_holding, "calibrated to a holding world");
        // Harden the belief so the contradiction is genuinely surprising.
        for _ in 0..4 {
            engine.cycle().unwrap();
        }

        // Reality moves out from under the model.
        std::fs::write(&file, "value gone").unwrap();
        let report = engine.cycle().unwrap();
        assert_eq!(report.surprises.len(), 1, "the model should be surprised");
        assert!(report.total_free_energy > 0.0);
        assert!(report.surprises[0].prediction_error > 0.0);

        // Keep contradicting it until the belief is revised to match the world.
        for _ in 0..6 {
            engine.cycle().unwrap();
        }
        let revised = engine.get_belief(&belief.belief_id).unwrap();
        assert!(
            !revised.predicted_holding,
            "a persistently-contradicted belief should be revised (flip its prediction)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_prior_is_surprising_on_arrival() {
        let engine = engine();
        // Assert the opposite of reality: the check passes but we predict failure.
        let belief = engine
            .register_belief(RegisterBelief {
                statement: "this will not pass (but it does)".into(),
                check: Check::Trivial { pass: true },
                expect_holding: Some(false),
                prior_precision: Some(2.0),
            })
            .unwrap();
        assert_eq!(belief.surprises, 1, "a wrong prior is surprising at once");
        assert!(belief.free_energy > 0.0);
    }

    #[test]
    fn attention_prefers_the_uncertain_and_volatile_belief() {
        let engine = engine();
        // A rock-solid, cheap, always-true belief: low epistemic value.
        engine
            .register_belief(RegisterBelief {
                statement: "settled".into(),
                check: Check::Trivial { pass: true },
                expect_holding: None,
                prior_precision: Some(PRECISION_CAP),
            })
            .unwrap();
        for _ in 0..3 {
            engine.cycle().unwrap();
        }
        // A volatile belief: a wrong, low-precision prior that keeps being contradicted.
        let volatile = engine
            .register_belief(RegisterBelief {
                statement: "volatile".into(),
                check: Check::Trivial { pass: true },
                expect_holding: Some(false),
                prior_precision: Some(0.3),
            })
            .unwrap();

        let plan = engine.attention_plan(10).unwrap();
        assert_eq!(
            plan.candidates[0].belief_id, volatile.belief_id,
            "the uncertain/volatile belief should be sampled first"
        );
    }

    #[test]
    fn seed_from_economy_is_idempotent() {
        use crate::proof_economy::{ProofEconomy, ProposeRequest};
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        // Mint a claim by surviving the required attacks.
        let claim = economy
            .propose(ProposeRequest {
                statement: "echo works".into(),
                kind: crate::proof_economy::ClaimKind::Assertion,
                proposer: "seed".into(),
                verification: Check::CommandSucceeds {
                    command: "echo hi".into(),
                },
                evidence: vec![],
                depends_on: vec![],
            })
            .unwrap();
        for who in ["a", "b"] {
            economy
                .attack(
                    &claim.claim_id,
                    crate::proof_economy::AttackRequest {
                        attacker: who.into(),
                        note: "x".into(),
                        counter: None,
                    },
                )
                .unwrap();
        }
        let engine = ActiveInference::in_memory(device)
            .unwrap()
            .with_economy(economy);
        let first = engine.seed_from_economy(100).unwrap();
        assert_eq!(first, 1, "the minted claim becomes a belief");
        let second = engine.seed_from_economy(100).unwrap();
        assert_eq!(second, 0, "seeding is idempotent");
        let belief = &engine.beliefs().unwrap()[0];
        assert_eq!(belief.origin, BeliefOrigin::MintedClaim);
        assert!(belief.predicted_holding);
    }
}

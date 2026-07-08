//! The Sentinel — continuous re-verification and the drift ledger.
//!
//! The [proof economy](crate::proof_economy) mints a claim once it survives
//! adversarial attack and then, by design, **never re-litigates it** — a minted
//! claim is permanent knowledge. That is exactly right for a settled fact, but a
//! whole class of real-world guarantees are not settled facts: they are
//! *standing properties of a changing world* that must hold **continuously**, and
//! whose value is in catching the **instant they stop holding**:
//!
//!   * a reproducible result the moment a dependency update breaks it,
//!   * an impossibility ("this package makes no outbound call") the instant a new
//!     version quietly adds the exfiltration path,
//!   * a compliance control the second a config drifts out of bounds,
//!   * a vibe-coder's intent ("never leak a user email") the instant a future
//!     change violates it.
//!
//! The Sentinel is that missing primitive. It re-runs a watched claim's `Check`
//! on a sweep and records a [`DriftEvent`] — a timestamped record of the exact
//! moment a property flipped from holding to broken (or back). Where the economy
//! answers "is this true, under attack, right now?", the Sentinel answers "*when*
//! did this stop being true?" — the question every continuous-verification domain
//! in the [continuum](crate::continuum) actually asks.
//!
//! It runs every check for real through the same [device layer](crate::device_agent)
//! the economy uses, so a watch is never an opinion — it is reality, re-sampled.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::verification::Check;

/// Which verification domain a watch belongs to — for filtering, telemetry, and
/// routing drift alerts. Mirrors the [continuum](crate::continuum) verticals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    /// A reproducible computational finding (idea 1).
    Reproducibility,
    /// A behavioral guarantee/prohibition about a dependency (idea 2).
    SupplyChain,
    /// A continuously-attacked compliance control (idea 3).
    Compliance,
    /// A mined behavioral spec of an opaque system (idea 4).
    SpecMining,
    /// A banked proven impossibility / negative result (idea 5).
    Impossibility,
    /// A vibe-coder's plain-language safety intent (idea 6).
    Guardrail,
    /// An AI-in-production guardrail / eval, continuously red-teamed.
    AiSafety,
    /// A data-quality / data-contract expectation over a warehouse.
    DataContract,
    /// A behavioral-equivalence claim for a verified migration.
    Equivalence,
    /// An operational fact backed by a re-runnable check (the KB that can't lie).
    Knowledge,
    /// A proof-gated golden path (platform-engineering self-service).
    GoldenPath,
    /// Anything else placed under continuous watch.
    Other,
}

/// A claim placed under continuous re-verification. The `check` is snapshotted so
/// the Sentinel re-runs the *same* proof the economy adjudicated, decoupled from
/// the economy's own lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watch {
    pub watch_id: String,
    /// The economy claim this mirrors, when the watch was born from one.
    pub claim_id: Option<String>,
    pub domain: Domain,
    pub label: String,
    pub check: Check,
    /// True while the watched property currently holds (its check passes).
    pub holding: bool,
    /// How many times the check has been re-run.
    pub sweeps: u64,
    /// How many times the property has flipped (drift events emitted).
    pub transitions: u32,
    pub last_detail: String,
    pub registered_at_ms: i64,
    pub last_checked_at_ms: i64,
    /// A watch stops being swept once retired.
    pub active: bool,
}

/// The instant a watched property flipped — the moment reproducibility broke, the
/// second a control drifted, the instant a backdoor's behavior appeared. This is
/// the durable, timestamped record that a one-time badge fundamentally cannot
/// give you.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftEvent {
    pub event_id: String,
    pub watch_id: String,
    pub domain: Domain,
    pub label: String,
    /// Whether the property was holding *before* this sweep.
    pub was_holding: bool,
    /// Whether it holds *now*. `was_holding && !now_holding` is a regression
    /// (the guarantee broke); `!was_holding && now_holding` is a recovery.
    pub now_holding: bool,
    pub detail: String,
    pub at_ms: i64,
}

impl DriftEvent {
    /// A guarantee that *broke* — the alert every domain cares about most.
    #[must_use]
    pub fn is_regression(&self) -> bool {
        self.was_holding && !self.now_holding
    }
}

/// What to place under watch.
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterWatch {
    #[serde(default)]
    pub claim_id: Option<String>,
    pub domain: Domain,
    pub label: String,
    pub check: Check,
}

/// The outcome of one continuous-verification sweep.
#[derive(Debug, Clone, Serialize)]
pub struct SweepReport {
    pub watched: usize,
    pub checked: usize,
    pub still_holding: usize,
    pub broke: usize,
    pub recovered: usize,
    /// Only the watches that flipped this sweep — the actionable signal.
    pub events: Vec<DriftEvent>,
}

/// The Sentinel. Holds the durable watch/drift ledger and the device layer that
/// re-runs every check for real.
#[derive(Clone)]
pub struct Sentinel {
    store: Arc<Mutex<Connection>>,
    device: Arc<DeviceCapabilities>,
}

impl Sentinel {
    pub fn new(
        data_dir: impl AsRef<Path>,
        device: Arc<DeviceCapabilities>,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("sentinel.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create sentinel dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open sentinel ledger: {e}")))?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
        })
    }

    /// Test/in-memory variant.
    pub fn in_memory(device: Arc<DeviceCapabilities>) -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(sql_err)?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
        })
    }

    /// Place a claim's check under continuous re-verification. The check is run
    /// once on arrival to seed the watch's `holding` state; subsequent sweeps
    /// detect drift from that baseline.
    pub fn register(&self, request: RegisterWatch) -> Result<Watch, AppError> {
        let label = request.label.trim().to_string();
        if label.is_empty() {
            return Err(AppError::Validation("watch label is empty".into()));
        }
        let outcome = self.run_check(&request.check);
        let now = now_ms();
        let watch = Watch {
            watch_id: new_id("watch"),
            claim_id: request.claim_id,
            domain: request.domain,
            label,
            check: request.check,
            holding: outcome.pass,
            sweeps: 1,
            transitions: 0,
            last_detail: outcome.detail,
            registered_at_ms: now,
            last_checked_at_ms: now,
            active: true,
        };
        self.save_watch(&watch)?;
        Ok(watch)
    }

    /// Re-run every active watch's check. Any flip from the prior state appends a
    /// durable [`DriftEvent`] stamped with the moment it happened. The device
    /// calls happen *outside* the store lock so a slow check never blocks the
    /// ledger.
    pub fn sweep(&self) -> Result<SweepReport, AppError> {
        let watches = self.active_watches()?;
        let watched = watches.len();
        let mut still_holding = 0usize;
        let mut broke = 0usize;
        let mut recovered = 0usize;
        let mut events = Vec::new();
        let now = now_ms();

        for mut watch in watches {
            let outcome = self.run_check(&watch.check);
            let was = watch.holding;
            let now_holding = outcome.pass;
            watch.sweeps += 1;
            watch.last_checked_at_ms = now;
            watch.last_detail = outcome.detail.clone();
            if now_holding {
                still_holding += 1;
            }
            if was != now_holding {
                watch.holding = now_holding;
                watch.transitions += 1;
                if was && !now_holding {
                    broke += 1;
                } else {
                    recovered += 1;
                }
                let event = DriftEvent {
                    event_id: new_id("drift"),
                    watch_id: watch.watch_id.clone(),
                    domain: watch.domain,
                    label: watch.label.clone(),
                    was_holding: was,
                    now_holding,
                    detail: outcome.detail,
                    at_ms: now,
                };
                self.save_drift(&event)?;
                events.push(event);
            }
            self.save_watch(&watch)?;
        }

        Ok(SweepReport {
            watched,
            checked: watched,
            still_holding,
            broke,
            recovered,
            events,
        })
    }

    /// Re-run a single watch immediately (e.g. to confirm a fix). Records drift
    /// just like a full sweep.
    pub fn sweep_one(&self, watch_id: &str) -> Result<Watch, AppError> {
        let mut watch = self.get_watch(watch_id)?;
        let outcome = self.run_check(&watch.check);
        let was = watch.holding;
        let now = now_ms();
        watch.sweeps += 1;
        watch.last_checked_at_ms = now;
        watch.last_detail = outcome.detail.clone();
        if was != outcome.pass {
            watch.holding = outcome.pass;
            watch.transitions += 1;
            let event = DriftEvent {
                event_id: new_id("drift"),
                watch_id: watch.watch_id.clone(),
                domain: watch.domain,
                label: watch.label.clone(),
                was_holding: was,
                now_holding: outcome.pass,
                detail: outcome.detail,
                at_ms: now,
            };
            self.save_drift(&event)?;
        }
        self.save_watch(&watch)?;
        Ok(watch)
    }

    /// Retire (or revive) a watch. Retired watches are skipped by sweeps but kept
    /// for their drift history.
    pub fn set_active(&self, watch_id: &str, active: bool) -> Result<Watch, AppError> {
        let mut watch = self.get_watch(watch_id)?;
        watch.active = active;
        self.save_watch(&watch)?;
        Ok(watch)
    }

    pub fn get_watch(&self, watch_id: &str) -> Result<Watch, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM sentinel_watches WHERE watch_id=?1",
                [watch_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("watch {watch_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    /// All watches, newest first, optionally filtered to one domain.
    pub fn watches(&self, domain: Option<Domain>) -> Result<Vec<Watch>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare(
                "SELECT payload FROM sentinel_watches ORDER BY registered_at_ms DESC LIMIT 1000",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            let watch: Watch = serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?;
            if domain.map_or(true, |d| d == watch.domain) {
                out.push(watch);
            }
        }
        Ok(out)
    }

    /// The drift ledger — the timestamped history of when properties broke or
    /// recovered. Optionally scoped to one watch. Newest first.
    pub fn drift_log(
        &self,
        watch_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DriftEvent>, AppError> {
        let limit = limit.clamp(1, 2000) as i64;
        let store = self.store.lock();
        let mut out = Vec::new();
        match watch_id {
            Some(id) => {
                let mut stmt = store
                    .prepare("SELECT payload FROM sentinel_drift WHERE watch_id=?1 ORDER BY at_ms DESC LIMIT ?2")
                    .map_err(sql_err)?;
                let rows = stmt
                    .query_map(params![id, limit], |row| row.get::<_, String>(0))
                    .map_err(sql_err)?;
                for row in rows {
                    out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
                }
            }
            None => {
                let mut stmt = store
                    .prepare("SELECT payload FROM sentinel_drift ORDER BY at_ms DESC LIMIT ?1")
                    .map_err(sql_err)?;
                let rows = stmt
                    .query_map(params![limit], |row| row.get::<_, String>(0))
                    .map_err(sql_err)?;
                for row in rows {
                    out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
                }
            }
        }
        Ok(out)
    }

    // ── check execution (the arbiter of truth) ──────────────────────────

    fn run_check(&self, check: &Check) -> crate::verification::CheckOutcome {
        crate::verification::run(check, &self.device, None)
    }

    // ── persistence ─────────────────────────────────────────────────────

    fn active_watches(&self) -> Result<Vec<Watch>, AppError> {
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM sentinel_watches WHERE active=1 ORDER BY registered_at_ms ASC LIMIT 1000")
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

    fn save_watch(&self, watch: &Watch) -> Result<(), AppError> {
        let payload = serde_json::to_string(watch).map_err(ser_err)?;
        let domain = domain_tag(watch.domain);
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO sentinel_watches (watch_id, payload, domain, active, registered_at_ms) VALUES (?1,?2,?3,?4,?5)",
                params![watch.watch_id, payload, domain, watch.active as i64, watch.registered_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }

    fn save_drift(&self, event: &DriftEvent) -> Result<(), AppError> {
        let payload = serde_json::to_string(event).map_err(ser_err)?;
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO sentinel_drift (event_id, watch_id, payload, at_ms) VALUES (?1,?2,?3,?4)",
                params![event.event_id, event.watch_id, payload, event.at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }
}

const SCHEMA: &str = "PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS sentinel_watches (
        watch_id TEXT PRIMARY KEY, payload TEXT NOT NULL, domain TEXT NOT NULL,
        active INTEGER NOT NULL, registered_at_ms INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS sentinel_drift (
        event_id TEXT PRIMARY KEY, watch_id TEXT NOT NULL, payload TEXT NOT NULL,
        at_ms INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS sentinel_drift_watch ON sentinel_drift(watch_id);";

fn domain_tag(domain: Domain) -> String {
    serde_json::to_value(domain)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "other".into())
}

/// Spawn the continuous sweeper on its own thread+runtime (mirrors the proof
/// conductor). Gated behind `ASTRA_SENTINEL=on`; sweeps every
/// `ASTRA_SENTINEL_INTERVAL_SECS` (default 120, min 15). Each regression is
/// logged at warn level — the alert that a standing guarantee just broke.
pub fn spawn_sentinel(sentinel: Sentinel) {
    let enabled = std::env::var("ASTRA_SENTINEL")
        .map(|v| matches!(v.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!("sentinel disabled (set ASTRA_SENTINEL=on to enable continuous re-verify)");
        return;
    }
    let interval = std::env::var("ASTRA_SENTINEL_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s >= 15)
        .unwrap_or(120);
    std::thread::Builder::new()
        .name("astra-sentinel".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                loop {
                    match sentinel.sweep() {
                        Ok(report) => {
                            for event in report.events.iter().filter(|e| e.is_regression()) {
                                tracing::warn!(
                                    watch = %event.watch_id,
                                    label = %event.label,
                                    detail = %event.detail,
                                    "sentinel: a standing guarantee BROKE"
                                );
                            }
                            tracing::info!(
                                watched = report.watched,
                                broke = report.broke,
                                recovered = report.recovered,
                                "sentinel sweep complete"
                            );
                        }
                        Err(error) => tracing::warn!(error = %error, "sentinel sweep failed"),
                    }
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            });
        })
        .expect("failed to spawn sentinel thread");
}

fn sql_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("sentinel sql error: {e}"))
}
fn ser_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("sentinel serialize error: {e}"))
}
fn de_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("sentinel decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};

    fn sentinel() -> Sentinel {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        Sentinel::in_memory(device).expect("sentinel")
    }

    #[test]
    fn watch_holds_then_records_the_moment_it_breaks() {
        let sentinel = sentinel();
        let dir = std::env::temp_dir().join(format!("astra-sentinel-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("result.txt");
        std::fs::write(&file, "effect size 0.42").unwrap();
        let path = file.to_string_lossy().to_string();

        // A reproducible finding placed under continuous watch — it holds now.
        let watch = sentinel
            .register(RegisterWatch {
                claim_id: None,
                domain: Domain::Reproducibility,
                label: "effect reproduces".into(),
                check: Check::FileContains {
                    path: path.clone(),
                    substring: "0.42".into(),
                },
            })
            .unwrap();
        assert!(watch.holding, "should hold on arrival");

        // A sweep with the world unchanged: no drift.
        let report = sentinel.sweep().unwrap();
        assert_eq!(report.broke, 0);
        assert_eq!(report.still_holding, 1);
        assert!(report.events.is_empty());

        // Reality moves: a dependency update changes the result.
        std::fs::write(&file, "effect size 0.05").unwrap();
        let report = sentinel.sweep().unwrap();
        assert_eq!(report.broke, 1, "drift should be detected");
        assert_eq!(report.events.len(), 1);
        assert!(report.events[0].is_regression());

        // The exact moment reproducibility broke is in the durable ledger.
        let log = sentinel.drift_log(Some(&watch.watch_id), 10).unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].was_holding && !log[0].now_holding);
        assert!(log[0].at_ms > 0);

        let after = sentinel.get_watch(&watch.watch_id).unwrap();
        assert!(!after.holding);
        assert_eq!(after.transitions, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn impossibility_watch_is_stable_while_the_op_keeps_failing() {
        let sentinel = sentinel();
        // "this binary cannot run here" — CommandFails holds while it stays absent.
        let watch = sentinel
            .register(RegisterWatch {
                claim_id: None,
                domain: Domain::SupplyChain,
                label: "no exfiltration path".into(),
                check: Check::CommandFails {
                    command: "astra_nonexistent_zzz --exfiltrate".into(),
                },
            })
            .unwrap();
        assert!(watch.holding);
        let report = sentinel.sweep().unwrap();
        assert_eq!(report.broke, 0);
        assert_eq!(report.still_holding, 1);
    }

    #[test]
    fn retired_watch_is_skipped() {
        let sentinel = sentinel();
        let watch = sentinel
            .register(RegisterWatch {
                claim_id: None,
                domain: Domain::Other,
                label: "trivial".into(),
                check: Check::Trivial { pass: true },
            })
            .unwrap();
        sentinel.set_active(&watch.watch_id, false).unwrap();
        let report = sentinel.sweep().unwrap();
        assert_eq!(report.watched, 0, "retired watches are not swept");
    }

    #[test]
    fn domain_filter_partitions_watches() {
        let sentinel = sentinel();
        for (domain, label) in [
            (Domain::Compliance, "no public bucket"),
            (Domain::Guardrail, "no email leak"),
        ] {
            sentinel
                .register(RegisterWatch {
                    claim_id: None,
                    domain,
                    label: label.into(),
                    check: Check::Trivial { pass: true },
                })
                .unwrap();
        }
        assert_eq!(sentinel.watches(Some(Domain::Compliance)).unwrap().len(), 1);
        assert_eq!(sentinel.watches(Some(Domain::Guardrail)).unwrap().len(), 1);
        assert_eq!(sentinel.watches(None).unwrap().len(), 2);
    }
}

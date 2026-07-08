use std::fs::File;
use std::io::{self, BufReader, ErrorKind};

use actix_cors::Cors;
use actix_web::{
    App, HttpResponse, HttpServer, dev::Service, http::header, middleware::DefaultHeaders, web,
};
use astra_core::common::new_id;
use astra_server::app_config;
use rustls::ServerConfig;
use tracing_subscriber::EnvFilter;

#[actix_web::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let state = match astra_core::AppState::new(astra_core::common::AppConfig::from_env()) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("astra-server failed to start: {error}");
            return Err(io::Error::new(ErrorKind::InvalidInput, error.to_string()));
        }
    };

    spawn_autonomy_worker(state.clone());
    spawn_heal_worker(state.clone());
    spawn_governance_worker(state.clone());
    spawn_ceo_worker(state.clone());
    spawn_ceo_guardian_worker(state.clone());
    astra_core::telegram::spawn_telegram_worker(state.clone());
    astra_core::proof_round::spawn_proof_conductor(state.proof_conductor.clone());
    astra_core::sentinel::spawn_sentinel(state.continuum.sentinel());
    // The perception layer: continuous active inference (predict → sense →
    // surprise → learn → forage attention), running forever in the background.
    astra_core::active_inference::spawn_active_inference(state.active_inference.clone());
    // The coordination layer: the neural orchestration reflex reads the live free
    // energy + drift, routes them through the plastic connectome, and reinforces
    // the pathway by whether surprise fell — closing the perceive→route→learn loop.
    astra_core::neural_orchestration::spawn_neural_orchestrator(
        state.neural_orchestrator.clone(),
        state.active_inference.clone(),
        state.continuum.sentinel(),
    );
    // The understanding layer: Noēsis compresses the verified ledger into theories,
    // imagines risky predictions about checks it never ran, and settles them against
    // reality — minting survivors, refuting mispredictions, tracking dream-debt.
    astra_core::noesis::spawn_noesis(state.noesis.clone());
    // Observe → diagnose → heal: turn Sentinel regressions into Crucible root-cause
    // inquiries (why did the guarantee break?), then repair the cause via the
    // capability self-heal — autonomous, no request and no human.
    spawn_self_diagnosis_worker(state.clone());
    let host = state.config.host.clone();
    let port = state.config.port;
    let cors_origins = state.config.cors_allowed_origins.clone();
    let tls_cert_path = state.config.tls_cert_path.clone();
    let tls_key_path = state.config.tls_key_path.clone();

    let server =
        HttpServer::new(move || {
            let cors = build_cors(&cors_origins);
            App::new()
                .app_data(web::Data::new(state.clone()))
                .app_data(web::JsonConfig::default().limit(1024 * 1024).error_handler(
                    |error, _req| {
                        actix_web::error::InternalError::from_response(
                            error,
                            HttpResponse::BadRequest().json(serde_json::json!({
                                "ok": false,
                                "error": "invalid json payload",
                            })),
                        )
                        .into()
                    },
                ))
                .wrap(
                    DefaultHeaders::new()
                        .add((header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
                        .add((header::X_FRAME_OPTIONS, "DENY"))
                        .add((header::REFERRER_POLICY, "no-referrer"))
                        .add((
                            header::HeaderName::from_static("x-httpa-transport"),
                            "https-preferred",
                        )),
                )
                .wrap_fn(|req, srv| {
                    let request_id = new_id("req");
                    let fut = srv.call(req);
                    async move {
                        let mut response = fut.await?;
                        response.headers_mut().insert(
                            header::HeaderName::from_static("x-request-id"),
                            header::HeaderValue::from_str(&request_id)
                                .unwrap_or_else(|_| header::HeaderValue::from_static("invalid")),
                        );
                        Ok(response)
                    }
                })
                .wrap(cors)
                .configure(app_config)
        })
        .bind((host.clone(), port))?;

    if let (Some(cert_path), Some(key_path)) = (tls_cert_path, tls_key_path) {
        server.bind_rustls_0_23((host, port + 1), load_rustls_config(&cert_path, &key_path)?)?
    } else {
        server
    }
    .run()
    .await
}

/// Starts the background autonomy worker: a dedicated OS thread that ticks the
/// autonomy queue on a fixed cadence so enqueued objectives are fabricated and
/// executed without a request driving each step, and runs the chronicle's
/// memory lifecycle (decay, archival, insight promotion) on the same cadence.
/// Controlled by `ASTRA_AUTONOMY_ENABLED` (default on) and
/// `ASTRA_AUTONOMY_INTERVAL_SECS`.
fn spawn_autonomy_worker(state: astra_core::AppState) {
    let enabled = std::env::var("ASTRA_AUTONOMY_ENABLED")
        .map(|value| !matches!(value.as_str(), "0" | "false" | "FALSE" | "no"))
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let interval_secs = std::env::var("ASTRA_AUTONOMY_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(30)
        .max(1);
    std::thread::Builder::new()
        .name("astra-autonomy-worker".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(interval_secs));
                let report = state.autonomy.tick(4);
                if report.processed > 0 {
                    tracing::info!(
                        processed = report.processed,
                        "autonomy worker advanced queued objectives"
                    );
                }
                match state.chronicle.consolidate() {
                    Ok(consolidation)
                        if consolidation.archived > 0 || consolidation.insights_promoted > 0 =>
                    {
                        tracing::info!(
                            archived = consolidation.archived,
                            insights = consolidation.insights_promoted,
                            "chronicle consolidated memory"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "chronicle consolidation failed"),
                }
            }
        })
        .expect("failed to spawn autonomy worker thread");
}

/// Starts the autonomous self-heal worker: on a fixed cadence it scans the Forge
/// and the Architect for regressed capabilities — tools/agents whose failure rate
/// climbed, or whose minted proof no longer verifies because reality drifted — and
/// re-proves a bounded batch of each through their `heal_round` entry points. The
/// capability fabric repairs itself with no request and no human.
///
/// Gated behind `ASTRA_HEAL_AUTONOMY=on` (default off — re-authoring needs a
/// configured remote LLM to be useful). Cadence `ASTRA_HEAL_INTERVAL_SECS`
/// (default 300, min 30); per-fabric batch `ASTRA_HEAL_BATCH` (default 4) bounds
/// how many regressions are healed per tick so a storm can't hammer the model.
fn spawn_heal_worker(state: astra_core::AppState) {
    let enabled = std::env::var("ASTRA_HEAL_AUTONOMY")
        .map(|value| matches!(value.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!("capability self-heal disabled (set ASTRA_HEAL_AUTONOMY=on to enable)");
        return;
    }
    let interval_secs = std::env::var("ASTRA_HEAL_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 30)
        .unwrap_or(300);
    let batch = std::env::var("ASTRA_HEAL_BATCH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(4);
    std::thread::Builder::new()
        .name("astra-heal-worker".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                loop {
                    // Sleep first so the fabric is warm before the first scan.
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
                    let tools = state.forge.heal_round(batch).await;
                    let agents = state.architect.heal_round(batch).await;
                    if !tools.is_empty() || !agents.is_empty() {
                        let healed = tools.iter().filter(|outcome| outcome.healed).count()
                            + agents.iter().filter(|outcome| outcome.healed).count();
                        tracing::info!(
                            tools_scanned = tools.len(),
                            agents_scanned = agents.len(),
                            healed,
                            "capability self-heal tick complete"
                        );
                    }
                }
            });
        })
        .expect("failed to spawn heal worker thread");
}

/// The autonomous self-diagnosis reflex — observe → diagnose → heal, closed.
///
/// Each tick it reads the [Sentinel](astra_core::sentinel)'s drift log and, for
/// each NEW regression whose watched guarantee is *still* broken, opens a
/// [Crucible](astra_core::crucible) inquiry that asks **why** — forming competing
/// hypotheses and confirming the cause with real read-only checks — then ticks the
/// capability self-heal so confirmed regressions are repaired at the cause (heal
/// now consults the Crucible). This is the piece that makes "continuously fix the
/// problems on the machine" fully autonomous rather than on-demand.
///
/// Gated behind `ASTRA_SELF_DIAGNOSIS=on` (needs a configured remote LLM to
/// reason and to re-author fixes). Cadence `ASTRA_SELF_DIAGNOSIS_INTERVAL_SECS`
/// (default 180, min 30); `ASTRA_SELF_DIAGNOSIS_BATCH` (default 3, min 1) bounds
/// how many fresh regressions are diagnosed per tick so a drift storm cannot
/// hammer the model.
fn spawn_self_diagnosis_worker(state: astra_core::AppState) {
    let enabled = std::env::var("ASTRA_SELF_DIAGNOSIS")
        .map(|value| matches!(value.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!(
            "self-diagnosis disabled (set ASTRA_SELF_DIAGNOSIS=on to enable observe→diagnose→heal)"
        );
        return;
    }
    let interval_secs = std::env::var("ASTRA_SELF_DIAGNOSIS_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 30)
        .unwrap_or(180);
    let batch = std::env::var("ASTRA_SELF_DIAGNOSIS_BATCH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(3);
    std::thread::Builder::new()
        .name("astra-self-diagnosis".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                use std::collections::HashSet;
                // Regression events already diagnosed this run, so each tick only
                // reasons about genuinely new breakage.
                let mut seen: HashSet<String> = HashSet::new();
                let sentinel = state.continuum.sentinel();
                loop {
                    // Sleep first so the Sentinel has swept at least once.
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval_secs)).await;

                    let regressions = match sentinel.drift_log(None, 100) {
                        Ok(log) => log
                            .into_iter()
                            .filter(|event| event.is_regression())
                            .collect::<Vec<_>>(),
                        Err(error) => {
                            tracing::warn!(%error, "self-diagnosis could not read the drift log");
                            continue;
                        }
                    };
                    // Snapshot watches once to attach the failing check as context
                    // and to skip regressions that have already recovered.
                    let watches = sentinel.watches(None).unwrap_or_default();

                    let mut diagnosed = 0usize;
                    for event in regressions {
                        if diagnosed >= batch {
                            break;
                        }
                        if !seen.insert(event.event_id.clone()) {
                            continue; // already diagnosed this exact regression
                        }
                        let watch = watches.iter().find(|w| w.watch_id == event.watch_id);
                        // Only spend reasoning on guarantees that are STILL broken.
                        if matches!(watch, Some(w) if w.holding) {
                            continue;
                        }
                        let failing_check = watch
                            .map(|w| serde_json::to_string(&w.check).unwrap_or_default())
                            .unwrap_or_default();
                        let problem = format!(
                            "A verified guarantee regressed in the {:?} domain: '{}'. Its check no \
                             longer holds.",
                            event.domain, event.label
                        );
                        let context = format!(
                            "watch_id={}; failing check={}; sweep detail={}",
                            event.watch_id, failing_check, event.detail
                        );
                        match state
                            .crucible
                            .investigate(astra_core::crucible::InvestigateRequest {
                                problem,
                                context,
                                mint: false,
                            })
                            .await
                        {
                            Ok(inquiry) => {
                                diagnosed += 1;
                                tracing::info!(
                                    inquiry = %inquiry.inquiry_id,
                                    watch = %event.watch_id,
                                    status = ?inquiry.status,
                                    root_cause =
                                        inquiry.root_cause.as_deref().unwrap_or("(unconfirmed)"),
                                    "self-diagnosis: root-caused a regression"
                                );
                            }
                            Err(error) => tracing::warn!(
                                %error,
                                watch = %event.watch_id,
                                "self-diagnosis investigate failed"
                            ),
                        }
                    }

                    // Diagnosis done — now let the capability fabric repair the
                    // cause (forge.heal consults the Crucible internally).
                    if diagnosed > 0 {
                        let tools = state.forge.heal_round(batch).await;
                        let agents = state.architect.heal_round(batch).await;
                        let healed = tools.iter().filter(|outcome| outcome.healed).count()
                            + agents.iter().filter(|outcome| outcome.healed).count();
                        tracing::info!(
                            diagnosed,
                            healed,
                            "self-diagnosis tick complete (observe → diagnose → heal)"
                        );
                    }

                    // Bound the seen-set so a long-lived process never leaks memory.
                    if seen.len() > 5000 {
                        seen.clear();
                    }
                }
            });
        })
        .expect("failed to spawn self-diagnosis worker thread");
}

/// The Supreme Court of Justice's continuous patrol. On a fixed cadence the army
/// sweeps the sandbox border (sealing escape/intrusion attempts) and the police
/// sweep the project audit (sealing agents whose dangerous actions were denied).
/// Default ON; gate with `ASTRA_GOVERNANCE_ENABLED`, cadence
/// `ASTRA_GOVERNANCE_INTERVAL_SECS` (default 15, min 1).
fn spawn_governance_worker(state: astra_core::AppState) {
    let enabled = std::env::var("ASTRA_GOVERNANCE_ENABLED")
        .map(|value| !matches!(value.as_str(), "0" | "false" | "FALSE" | "no" | "off"))
        .unwrap_or(true);
    if !enabled {
        tracing::info!("governance patrols disabled (ASTRA_GOVERNANCE_ENABLED=off)");
        return;
    }
    let interval_secs = std::env::var("ASTRA_GOVERNANCE_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(15);
    std::thread::Builder::new()
        .name("astra-governance-worker".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(interval_secs));
                let army = state.governance.army_patrol();
                let police = state.governance.police_patrol();
                match (&army, &police) {
                    (Ok(a), Ok(p)) if a.enforced > 0 || p.enforced > 0 => {
                        tracing::warn!(
                            army_enforced = a.enforced,
                            police_enforced = p.enforced,
                            alert = %a.alert_level,
                            "governance patrol sealed offenders"
                        );
                    }
                    _ => {
                        if let Err(error) = &army {
                            tracing::warn!(%error, "army patrol failed");
                        }
                        if let Err(error) = &police {
                            tracing::warn!(%error, "police patrol failed");
                        }
                    }
                }
                // The court also adjudicates any upgrade proposals the CEO has
                // submitted. This is deterministic (the constitutional screen,
                // no model), so it is safe to run on every tick.
                match state.governance.review_pending() {
                    Ok(reviews) if !reviews.is_empty() => {
                        let approved = reviews.iter().filter(|r| r.approved).count();
                        tracing::info!(
                            reviewed = reviews.len(),
                            approved,
                            terminated = reviews.len() - approved,
                            "governance ruled on pending proposals"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "proposal review failed"),
                }
            }
        })
        .expect("failed to spawn governance worker thread");
}

/// The CEO's autonomous cognition loop: on a fixed cadence it self-thinks and
/// self-evolves — drafting one upgrade proposal and submitting it to governance —
/// and then executes any proposal the court has already approved (spawning the
/// agents/tools it authorized). The court's approval is still the gate: the CEO
/// never spawns anything the governance has not signed off on.
///
/// Gated behind `ASTRA_CEO_AUTONOMY=on` (default OFF — autonomous proposing needs
/// a configured remote model, and autonomous spawning should be opt-in). Cadence
/// `ASTRA_CEO_INTERVAL_SECS` (default 120, min 15). Bounded: one proposal and at
/// most one execution per tick.
fn spawn_ceo_worker(state: astra_core::AppState) {
    let enabled = std::env::var("ASTRA_CEO_AUTONOMY")
        .map(|value| matches!(value.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!("CEO autonomy disabled (set ASTRA_CEO_AUTONOMY=on to enable)");
        return;
    }
    let interval_secs = std::env::var("ASTRA_CEO_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 15)
        .unwrap_or(120);
    std::thread::Builder::new()
        .name("astra-ceo-worker".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                loop {
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
                    // Self-think + self-evolve: draft and submit one proposal.
                    match state.ceo.self_evolve().await {
                        Ok(outcome) => tracing::info!(
                            evolved = outcome.evolved,
                            decision = %outcome.decision,
                            "CEO self-evolution round"
                        ),
                        Err(error) => tracing::warn!(%error, "CEO self-evolution failed"),
                    }
                    // Execute the oldest court-approved, not-yet-executed proposal.
                    let approved = state
                        .governance
                        .proposals(200)
                        .map(|proposals| {
                            proposals
                                .into_iter()
                                .filter(|p| {
                                    p.status == astra_core::governance::ProposalStatus::Approved
                                })
                                .map(|p| p.proposal_id)
                                .last()
                        })
                        .unwrap_or(None);
                    if let Some(proposal_id) = approved {
                        match state.ceo.execute_upgrade(&proposal_id).await {
                            Ok(outcome) => tracing::info!(
                                proposal = %outcome.proposal_id,
                                spawned = outcome.spawned.len(),
                                "CEO executed an approved upgrade"
                            ),
                            Err(error) => tracing::warn!(%error, "CEO upgrade execution failed"),
                        }
                    }
                }
            });
        })
        .expect("failed to spawn ceo worker thread");
}

/// The CEO's continuous OS-security patrol — the piece that makes the executive
/// *continuously monitor the operating system*. On a fixed cadence it turns the
/// device eyes on the host, scores what it sees through the OS Guardian (malware,
/// loopholes, data leaks), applies the safe half of every fix itself, and routes
/// every destructive remediation or hardening self-modification through the
/// governance desk for the court's approval. Nothing host-altering is executed
/// autonomously.
///
/// Gated behind `ASTRA_CEO_GUARDIAN=on` (default OFF — scanning the host and
/// drafting fixes should be opt-in). Cadence `ASTRA_CEO_GUARDIAN_INTERVAL_SECS`
/// (default 240, min 30). Scan roots default to the project workspace; override
/// with `ASTRA_GUARDIAN_SCAN_ROOTS`. Drafting self-mods / forging detectors also
/// requires a configured remote model (`ASTRA_GUARDIAN_SELF_MOD`,
/// `ASTRA_GUARDIAN_FORGE`).
fn spawn_ceo_guardian_worker(state: astra_core::AppState) {
    let enabled = std::env::var("ASTRA_CEO_GUARDIAN")
        .map(|value| matches!(value.as_str(), "1" | "true" | "on" | "yes" | "TRUE"))
        .unwrap_or(false);
    if !enabled {
        tracing::info!(
            "CEO OS-security patrol disabled (set ASTRA_CEO_GUARDIAN=on to enable continuous \
             monitoring)"
        );
        return;
    }
    let interval_secs = std::env::var("ASTRA_CEO_GUARDIAN_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 30)
        .unwrap_or(240);
    std::thread::Builder::new()
        .name("astra-ceo-guardian".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                loop {
                    // Sleep first so the host is warm and other organs have booted.
                    actix_web::rt::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
                    match state.ceo.guardian_patrol().await {
                        Ok(report) => {
                            if report.counts.findings > 0 {
                                tracing::warn!(
                                    findings = report.counts.findings,
                                    auto_healed = report.counts.auto_healed,
                                    approvals = report.counts.approvals_requested,
                                    self_mods = report.counts.self_mods_proposed,
                                    peak = ?report.peak_severity,
                                    "CEO guardian patrol surfaced security findings"
                                );
                            } else {
                                tracing::info!(
                                    processes = report.counts.processes_scanned,
                                    files = report.counts.files_scanned,
                                    "CEO guardian patrol clean"
                                );
                            }
                        }
                        Err(error) => tracing::warn!(%error, "CEO guardian patrol failed"),
                    }
                    // Close the loop: carry out any remediation the court has since
                    // approved (destructive actions still require ASTRA_GUARDIAN_EXECUTE=on).
                    match state.ceo.guardian_execute_remediations().await {
                        Ok(records) if !records.is_empty() => {
                            let executed = records.iter().filter(|r| r.mode == "executed").count();
                            tracing::info!(
                                handled = records.len(),
                                executed,
                                "CEO guardian processed approved remediations"
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(%error, "CEO guardian remediation pass failed")
                        }
                    }
                }
            });
        })
        .expect("failed to spawn ceo guardian worker thread");
}

fn build_cors(allowed_origins: &[String]) -> Cors {
    let mut cors = Cors::default()
        .allow_any_header()
        .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"])
        .max_age(3600);
    for origin in allowed_origins {
        cors = cors.allowed_origin(origin);
    }
    cors
}

fn load_rustls_config(cert_path: &str, key_path: &str) -> io::Result<ServerConfig> {
    let cert_file = &mut BufReader::new(File::open(cert_path)?);
    let key_file = &mut BufReader::new(File::open(key_path)?);

    let cert_chain = rustls_pemfile::certs(cert_file)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?;
    let private_key = rustls_pemfile::private_key(key_file)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "missing private key"))?;

    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))
}

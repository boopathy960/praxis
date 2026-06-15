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
    astra_core::telegram::spawn_telegram_worker(state.clone());
    astra_core::proof_round::spawn_proof_conductor(state.proof_conductor.clone());
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

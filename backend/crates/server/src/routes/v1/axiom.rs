//! Axiom — the proof-scheduled OS kernel (Phase 0). See `AXIOM.md`.
//!
//! Components register with claims from the proof economy; dispatch is the
//! proof-scheduled syscall (uncaged call / guarded domain / process cage);
//! resync is the dispatcher re-reading the economy + drift ledger; and the
//! shell is `axsh`, the kernel's built-in command line.

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    axiom::{RegisterComponent, shell::Axsh},
    common::{ApiResponse, AppError, run_blocking_io},
};
use serde::Deserialize;
use serde_json::Value;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/axiom/kernel", web::get().to(kernel_info))
        .route("/axiom/components", web::post().to(register))
        .route("/axiom/components", web::get().to(components))
        .route("/axiom/components/{id}", web::get().to(get_component))
        .route("/axiom/components/{id}/dispatch", web::post().to(dispatch))
        .route("/axiom/resync", web::post().to(resync))
        .route("/axiom/transitions", web::get().to(transitions))
        .route("/axiom/bench", web::post().to(bench))
        .route("/axiom/shell", web::post().to(shell));
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct BenchRequest {
    #[serde(default)]
    iters: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ShellRequest {
    line: String,
}

/// Kernel identity: version, phase, and the live tier census.
async fn kernel_info(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let info = run_blocking_io(move || kernel.kernel_info())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(info)))
}

/// Admit a component with the economy claims it ships with. The tier it gets
/// is whatever those proofs buy today — never more.
async fn register(
    state: web::Data<AppState>,
    body: web::Json<RegisterComponent>,
) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let request = body.into_inner();
    let component = run_blocking_io(move || kernel.register(request))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(component)))
}

async fn components(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let list = run_blocking_io(move || kernel.components())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(list)))
}

async fn get_component(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let id = path.into_inner();
    let lookup = id.clone();
    let component = run_blocking_io(move || kernel.find(&lookup))?
        .ok_or_else(|| AppError::NotFound(format!("component {id}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(component)))
}

/// The proof-scheduled syscall: the tier decides whether this call is an
/// uncaged function call, a guarded domain crossing, or a full OS process.
async fn dispatch(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<Value>,
) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let id = path.into_inner();
    let args = body.into_inner();
    let report = run_blocking_io(move || kernel.dispatch(&id, &args))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// Re-read the economy and the drift ledger; promote/demote components live.
async fn resync(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let report = run_blocking_io(move || kernel.resync())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// The tier ledger — every moment execution privilege moved.
async fn transitions(
    state: web::Data<AppState>,
    query: web::Query<LimitQuery>,
) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let limit = query.limit.unwrap_or(50);
    let list = run_blocking_io(move || kernel.transitions(limit))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(list)))
}

/// Publish the falsifiable Phase-0 numbers: the same no-op through all three
/// mechanisms — the cage tax, measured.
async fn bench(
    state: web::Data<AppState>,
    body: web::Json<BenchRequest>,
) -> Result<HttpResponse, AppError> {
    let kernel = state.axiom.clone();
    let iters = body.into_inner().iters.unwrap_or(1000);
    let report = run_blocking_io(move || kernel.bench(iters))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// axsh — one line in, text out. The same interpreter a TTY would get.
async fn shell(
    state: web::Data<AppState>,
    body: web::Json<ShellRequest>,
) -> Result<HttpResponse, AppError> {
    let shell = Axsh::new(state.axiom.clone());
    let line = body.into_inner().line;
    let output = run_blocking_io(move || shell.exec(&line))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(output)))
}

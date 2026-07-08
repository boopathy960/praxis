//! The Noēsis HTTP surface — compress, imagine, settle.
//!
//! `compress` and `conjecture` drive the reasoner and are awaited directly.
//! `settle` runs a (reversible) check for real through the device layer, so it is
//! dispatched on a blocking thread. The frontier, leverage, theory, and status
//! reads are cheap.

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError, run_blocking_io},
    noesis::{CompressRequest, ConjectureRequest},
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/noesis/compress", web::post().to(compress))
        .route("/noesis/theories", web::get().to(theories))
        .route("/noesis/theories/{id}", web::get().to(get_theory))
        .route(
            "/noesis/theories/{id}/conjecture",
            web::post().to(conjecture),
        )
        .route("/noesis/conjectures/{id}/settle", web::post().to(settle))
        .route("/noesis/frontier", web::get().to(frontier))
        .route("/noesis/leverage", web::get().to(leverage))
        .route("/noesis/status", web::get().to(status));
}

/// Compress the verified ledger into the shortest theory that reproduces it.
async fn compress(
    state: web::Data<AppState>,
    body: web::Json<CompressRequest>,
) -> Result<HttpResponse, AppError> {
    let theory = state.noesis.compress(body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(theory)))
}

/// Every theory the organ holds, newest first.
async fn theories(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let list = state.noesis.theories()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(list)))
}

async fn get_theory(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let theory = state.noesis.get_theory(&path)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(theory)))
}

/// Imagine: ask a theory for risky, falsifiable predictions about un-run checks.
async fn conjecture(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ConjectureRequest>,
) -> Result<HttpResponse, AppError> {
    let count = body.count.unwrap_or(5);
    let conjectures = state.noesis.conjecture(&path.into_inner(), count).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(conjectures)))
}

/// Settle a conjecture against reality: run its check and update the theory.
async fn settle(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let engine = state.noesis.clone();
    let id = path.into_inner();
    let report = run_blocking_io(move || engine.settle(&id))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct FrontierQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// The Ignorance Frontier: unsettled conjectures, highest value-of-information first.
async fn frontier(
    state: web::Data<AppState>,
    query: web::Query<FrontierQuery>,
) -> Result<HttpResponse, AppError> {
    let open = state.noesis.frontier(query.limit.unwrap_or(50))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(open)))
}

/// The epistemic leverage ratio — how much standing rests on unpaid imagination.
async fn leverage(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let leverage = state.noesis.leverage()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(leverage)))
}

/// A snapshot of the organ's overall state.
async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let snapshot = state.noesis.status()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(snapshot)))
}

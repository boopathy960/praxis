//! The Active Inference HTTP surface — the generative model, surprise, and
//! attention.
//!
//! Registering a belief and running a perception cycle sample checks for real
//! through the device layer, so they are dispatched on a blocking thread. The
//! plan, status, belief, and free-energy reads are cheap. `explain` drives the
//! reasoner and is awaited directly.

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    active_inference::RegisterBelief,
    common::{ApiResponse, AppError, run_blocking_io},
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/active-inference/beliefs", web::post().to(register_belief))
        .route("/active-inference/beliefs", web::get().to(beliefs))
        .route("/active-inference/beliefs/{id}", web::get().to(get_belief))
        .route(
            "/active-inference/beliefs/{id}/observe",
            web::post().to(observe_one),
        )
        .route(
            "/active-inference/beliefs/{id}/explain",
            web::post().to(explain_surprise),
        )
        .route("/active-inference/cycle", web::post().to(cycle))
        .route("/active-inference/attention", web::get().to(attention))
        .route(
            "/active-inference/seed-from-economy",
            web::post().to(seed_from_economy),
        )
        .route("/active-inference/free-energy", web::get().to(free_energy))
        .route("/active-inference/status", web::get().to(status));
}

/// Place a prediction under the generative model. Its check runs once on arrival.
async fn register_belief(
    state: web::Data<AppState>,
    body: web::Json<RegisterBelief>,
) -> Result<HttpResponse, AppError> {
    let engine = state.active_inference.clone();
    let req = body.into_inner();
    let belief = run_blocking_io(move || engine.register_belief(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(belief)))
}

/// Every belief the model holds, newest first.
async fn beliefs(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let list = state.active_inference.beliefs()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(list)))
}

async fn get_belief(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let belief = state.active_inference.get_belief(&path)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(belief)))
}

/// Run one perception cycle: predict, sample, and learn from every active belief.
async fn cycle(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let engine = state.active_inference.clone();
    let report = run_blocking_io(move || engine.cycle())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// Re-sample a single belief now (e.g. to confirm a correction took).
async fn observe_one(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let engine = state.active_inference.clone();
    let id = path.into_inner();
    let belief = run_blocking_io(move || engine.cycle_one(&id))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(belief)))
}

#[derive(Debug, Deserialize)]
struct AttentionQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// The attention policy: where to look next, and why.
async fn attention(
    state: web::Data<AppState>,
    query: web::Query<AttentionQuery>,
) -> Result<HttpResponse, AppError> {
    let plan = state
        .active_inference
        .attention_plan(query.limit.unwrap_or(20))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(plan)))
}

#[derive(Debug, Deserialize)]
struct SeedRequest {
    #[serde(default)]
    limit: Option<usize>,
}

/// Seed minted proof-economy claims as high-precision beliefs.
async fn seed_from_economy(
    state: web::Data<AppState>,
    body: web::Json<SeedRequest>,
) -> Result<HttpResponse, AppError> {
    let engine = state.active_inference.clone();
    let limit = body.limit.unwrap_or(200);
    let seeded = run_blocking_io(move || engine.seed_from_economy(limit))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({ "seeded": seeded }))))
}

#[derive(Debug, Deserialize)]
struct FreeEnergyQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// The free-energy history — the trace of the system's surprise over time.
async fn free_energy(
    state: web::Data<AppState>,
    query: web::Query<FreeEnergyQuery>,
) -> Result<HttpResponse, AppError> {
    let log = state
        .active_inference
        .free_energy_log(query.limit.unwrap_or(200))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(log)))
}

/// A snapshot of the model's overall state — its "state of mind".
async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let mind = state.active_inference.status()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(mind)))
}

/// Ask the reasoner to hypothesize why a surprising belief broke. Needs a
/// configured remote model.
async fn explain_surprise(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let explanation = state.active_inference.explain_surprise(&path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        serde_json::json!({ "belief_id": path.into_inner(), "hypothesis": explanation }),
    )))
}

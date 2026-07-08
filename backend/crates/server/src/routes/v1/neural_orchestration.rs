//! The Neural Orchestration HTTP surface — the plastic connectome that routes
//! signals across the cognitive organs and rewires from outcomes.
//!
//! Every endpoint here is an in-memory graph operation over the connectome
//! ledger; none touch the device layer, so they are cheap reads/writes. The
//! continuous routing reflex runs in the background (`spawn_neural_orchestrator`);
//! these routes are the manual surface onto the same machinery.

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    neural_orchestration::{NodeKind, ReinforceRequest, StimulusRequest},
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/neural/connectome", web::get().to(connectome))
        .route("/neural/seed", web::post().to(seed))
        .route("/neural/connect", web::post().to(connect))
        .route("/neural/stimulate", web::post().to(stimulate))
        .route("/neural/reinforce", web::post().to(reinforce))
        .route("/neural/episodes", web::get().to(episodes))
        .route("/neural/status", web::get().to(status));
}

/// The whole connectome — organs and synapses with their current weights.
async fn connectome(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let graph = state.neural_orchestrator.connectome()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(graph)))
}

/// Bootstrap (idempotently) the innate organs and synapses.
async fn seed(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let graph = state.neural_orchestrator.seed_default_connectome()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(graph)))
}

#[derive(Debug, Deserialize)]
struct ConnectRequest {
    from: NodeKind,
    to: NodeKind,
    weight: f64,
}

/// Wire (or re-weight) a synapse between two organs.
async fn connect(
    state: web::Data<AppState>,
    body: web::Json<ConnectRequest>,
) -> Result<HttpResponse, AppError> {
    let req = body.into_inner();
    let edge = state
        .neural_orchestrator
        .connect(req.from, req.to, req.weight)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(edge)))
}

/// Route a stimulus through the connectome and return the organs to engage.
async fn stimulate(
    state: web::Data<AppState>,
    body: web::Json<StimulusRequest>,
) -> Result<HttpResponse, AppError> {
    let plan = state.neural_orchestrator.stimulate(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(plan)))
}

/// Apply reward-modulated Hebbian learning to the synapses that fired in an episode.
async fn reinforce(
    state: web::Data<AppState>,
    body: web::Json<ReinforceRequest>,
) -> Result<HttpResponse, AppError> {
    let report = state.neural_orchestrator.reinforce(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct EpisodesQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// Recent stimulus→plan episodes — the network's routing history.
async fn episodes(
    state: web::Data<AppState>,
    query: web::Query<EpisodesQuery>,
) -> Result<HttpResponse, AppError> {
    let list = state
        .neural_orchestrator
        .episodes(query.limit.unwrap_or(100))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(list)))
}

/// A snapshot of the network's structure & plasticity.
async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let net = state.neural_orchestrator.status()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(net)))
}

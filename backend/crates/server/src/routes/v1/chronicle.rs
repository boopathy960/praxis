use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    chronicle::{RecallRequest, RecordEpisodeRequest},
    common::{ApiResponse, AppError},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/chronicle/episodes", web::post().to(record_episode))
        .route("/chronicle/episodes", web::get().to(list_episodes))
        .route("/chronicle/episodes/{episode_id}", web::get().to(get_episode))
        .route("/chronicle/recall", web::post().to(recall))
        .route("/chronicle/commitments", web::get().to(commitments))
        .route(
            "/chronicle/commitments/{episode_id}/resolve",
            web::post().to(resolve_commitment),
        )
        .route("/chronicle/consolidate", web::post().to(consolidate))
        .route("/chronicle/brief", web::get().to(brief))
        .route("/chronicle/stats", web::get().to(stats));
}

async fn record_episode(
    state: web::Data<AppState>,
    body: web::Json<RecordEpisodeRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Created().json(ApiResponse::ok(state.chronicle.record(body.into_inner())?)))
}

async fn list_episodes(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.chronicle.list_episodes()))
}

async fn get_episode(
    state: web::Data<AppState>,
    episode_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chronicle.get_episode(&episode_id)?)))
}

/// Natural-language recall: "what did I learn / decide about X, and why?"
async fn recall(
    state: web::Data<AppState>,
    body: web::Json<RecallRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chronicle.recall(body.into_inner())?)))
}

async fn commitments(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.chronicle.commitments()))
}

async fn resolve_commitment(
    state: web::Data<AppState>,
    episode_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.chronicle.resolve_commitment(&episode_id)?,
    )))
}

/// One lifecycle tick: decay, archive, and promote recurring themes into
/// insights.
async fn consolidate(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.chronicle.consolidate()?)))
}

/// What matters now: memory synthesis plus live subsystem activity, in one
/// place.
async fn brief(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "chronicle": state.chronicle.brief(),
        "autonomy": state.autonomy.stats(),
        "weave": state.weave.stats(),
    })))
}

async fn stats(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.chronicle.stats()))
}

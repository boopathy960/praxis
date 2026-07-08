use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    learning::RecordLearningEpisodeRequest,
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/learning/episodes", web::post().to(record_episode))
        .route("/learning/episodes", web::get().to(episodes))
        .route("/learning/stats", web::get().to(stats));
}

async fn record_episode(
    state: web::Data<AppState>,
    body: web::Json<RecordLearningEpisodeRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.learning.record(body.into_inner())?)))
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    #[serde(default)]
    limit: Option<usize>,
}

async fn episodes(
    state: web::Data<AppState>,
    query: web::Query<LimitQuery>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.learning.list(query.limit.unwrap_or(100)),
    )))
}

async fn stats(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.learning.stats())))
}

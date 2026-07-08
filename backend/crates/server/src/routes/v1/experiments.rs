use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError, run_blocking_io},
    experiments::CreateExperimentRequest,
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/experiments", web::post().to(create))
        .route("/experiments", web::get().to(list))
        .route("/experiments/{id}", web::get().to(get));
}

async fn create(
    state: web::Data<AppState>,
    body: web::Json<CreateExperimentRequest>,
) -> Result<HttpResponse, AppError> {
    let factory = state.experiments.clone();
    let request = body.into_inner();
    let run = run_blocking_io(move || factory.create_and_run(request))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(run)))
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    #[serde(default)]
    limit: Option<usize>,
}

async fn list(
    state: web::Data<AppState>,
    query: web::Query<LimitQuery>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.experiments.list(query.limit.unwrap_or(100)),
    )))
}

async fn get(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.experiments.get(&path)?)))
}

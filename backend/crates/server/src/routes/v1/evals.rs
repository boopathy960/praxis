use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError, run_blocking_io},
    evals::{AttachEvalSuiteRequest, CreateEvalSuiteRequest, RunEvalSuiteRequest},
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/evals/suites", web::post().to(create_suite))
        .route("/evals/suites", web::get().to(suites))
        .route("/evals/suites/{id}", web::get().to(get_suite))
        .route("/evals/suites/{id}/run", web::post().to(run_suite))
        .route("/evals/suites/{id}/attach", web::post().to(attach_suite))
        .route("/evals/runs", web::get().to(runs))
        .route("/evals/attachments", web::get().to(attachments));
}

async fn create_suite(
    state: web::Data<AppState>,
    body: web::Json<CreateEvalSuiteRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.evals.create_suite(body.into_inner())?,
    )))
}

async fn suites(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.evals.suites())))
}

async fn get_suite(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.evals.get_suite(&path)?)))
}

async fn run_suite(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<RunEvalSuiteRequest>,
) -> Result<HttpResponse, AppError> {
    let evals = state.evals.clone();
    let device = state.device.clone();
    let suite_id = path.into_inner();
    let request = body.into_inner();
    let scorecard = run_blocking_io(move || evals.run_suite(&suite_id, request, &device))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(scorecard)))
}

async fn attach_suite(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<AttachEvalSuiteRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.evals.attach(&path, body.into_inner())?,
    )))
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    #[serde(default)]
    limit: Option<usize>,
}

async fn runs(
    state: web::Data<AppState>,
    query: web::Query<LimitQuery>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.evals.runs(query.limit.unwrap_or(100)),
    )))
}

async fn attachments(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.evals.attachments())))
}

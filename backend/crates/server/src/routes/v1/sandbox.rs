use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    sandbox::{SandboxAction, SandboxNormalizeRequest},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/sandbox/status", web::get().to(status))
        .route("/sandbox/policies", web::get().to(policies))
        .route(
            "/sandbox/actions/normalize",
            web::post().to(normalize_action),
        )
        .route("/sandbox/actions/evaluate", web::post().to(evaluate_action))
        .route("/sandbox/actions/execute", web::post().to(execute_action))
        .route("/sandbox/audit", web::get().to(audit))
        .route("/sandbox/threats", web::get().to(threats));
}

async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.sandbox.status()?)))
}

async fn policies(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.sandbox.policies())))
}

async fn normalize_action(
    state: web::Data<AppState>,
    body: web::Json<SandboxNormalizeRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.sandbox.normalize(body.into_inner())?)))
}

async fn evaluate_action(
    state: web::Data<AppState>,
    body: web::Json<SandboxAction>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.sandbox.evaluate(body.into_inner()))))
}

async fn execute_action(
    state: web::Data<AppState>,
    body: web::Json<SandboxAction>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(
        state.sandbox.guard(body.into_inner(), true)?,
    )))
}

async fn audit(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.sandbox.audit(200)?)))
}

/// The intrusion threat-intel feed: every breakout / honeypot-decoy attempt the
/// perimeter caught, newest first — for the governance layer to review.
async fn threats(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.sandbox.threat_events(200)?)))
}

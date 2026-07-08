//! The Crucible HTTP surface — root-cause reasoning: why, and the real fix.
//!
//! `investigate`, `diagnose`, and `deepen` drive the reasoner and run read-only
//! discriminating checks through the device layer; they are awaited directly. The
//! inquiry reads are cheap.

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    crucible::InvestigateRequest,
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/crucible/investigate", web::post().to(investigate))
        .route("/crucible/diagnose", web::post().to(diagnose))
        .route("/crucible/inquiries", web::get().to(inquiries))
        .route("/crucible/inquiries/{id}", web::get().to(get_inquiry))
        .route("/crucible/inquiries/{id}/deepen", web::post().to(deepen));
}

/// Investigate a problem end-to-end: diagnose the root cause, propose the real
/// fix, and prove its postcondition. Set `mint` to stake the proven fix.
async fn investigate(
    state: web::Data<AppState>,
    body: web::Json<InvestigateRequest>,
) -> Result<HttpResponse, AppError> {
    let inquiry = state.crucible.investigate(body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(inquiry)))
}

#[derive(Debug, Deserialize)]
struct DiagnoseRequest {
    problem: String,
    #[serde(default)]
    context: String,
}

/// Diagnose only: form competing hypotheses, run their discriminating tests, and
/// name the confirmed root cause. No fix is proposed — just the grounded "why".
async fn diagnose(
    state: web::Data<AppState>,
    body: web::Json<DiagnoseRequest>,
) -> Result<HttpResponse, AppError> {
    let req = body.into_inner();
    let inquiry = state.crucible.diagnose(&req.problem, &req.context).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(inquiry)))
}

/// The five-whys recursion: interrogate the confirmed cause itself.
async fn deepen(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let inquiry = state.crucible.deepen(&path.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(inquiry)))
}

#[derive(Debug, Deserialize)]
struct InquiriesQuery {
    #[serde(default)]
    limit: Option<usize>,
}

/// Recent inquiries, newest first.
async fn inquiries(
    state: web::Data<AppState>,
    query: web::Query<InquiriesQuery>,
) -> Result<HttpResponse, AppError> {
    let list = state.crucible.inquiries(query.limit.unwrap_or(50))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(list)))
}

async fn get_inquiry(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let inquiry = state.crucible.get_inquiry(&path)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(inquiry)))
}

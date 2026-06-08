use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/artifacts/{artifact_id}", web::get().to(get_artifact))
        .route(
            "/artifacts/{artifact_id}/safety",
            web::get().to(get_artifact_safety),
        )
        .route(
            "/artifacts/{artifact_id}/purge",
            web::post().to(purge_artifact),
        );
}

async fn get_artifact(
    state: web::Data<AppState>,
    artifact_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let report = state.artifacts.get_report(&artifact_id)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "artifact_id": artifact_id.into_inner(),
        "path": report.path,
        "content_hash": report.content_hash,
        "safety_status": report.status,
        "warnings": report.warnings,
    }))))
}

async fn get_artifact_safety(
    state: web::Data<AppState>,
    artifact_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.artifacts.get_report(&artifact_id)?)))
}

async fn purge_artifact(
    state: web::Data<AppState>,
    artifact_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.artifacts.purge(&artifact_id)?)))
}

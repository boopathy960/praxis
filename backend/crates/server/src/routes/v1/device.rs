use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
};
use serde::Deserialize;
use serde_json::Value;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/device/status", web::get().to(status))
        .route("/device/live", web::get().to(live))
        .route("/device/audit", web::get().to(audit))
        .route("/device/execute", web::post().to(execute))
        .route("/device/crawl", web::post().to(crawl))
        .route("/device/search", web::post().to(search));
}

/// Supervisor health: in-flight count, lifetime totals, audit-log size, and
/// the policy the agents are operating under.
async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let supervisor = state.device.supervisor();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "stats": supervisor.stats(),
        "policy": state.device.policy(),
    }))))
}

/// What the agents are doing on the device right now — the continuous watch.
async fn live(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.device.supervisor().live())))
}

/// Recent sealed, hash-chained records of finished device operations.
async fn audit(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.device.supervisor().recent_audit(200))))
}

#[derive(Debug, Deserialize)]
struct DeviceExecuteRequest {
    tool: String,
    #[serde(default)]
    input: Value,
}

/// Directly invoke one supervised device tool. Runs the real operation on the
/// host under the watchdog and returns the observation plus its op id.
async fn execute(
    state: web::Data<AppState>,
    body: web::Json<DeviceExecuteRequest>,
) -> Result<HttpResponse, AppError> {
    let DeviceExecuteRequest { tool, input } = body.into_inner();
    let device = state.device.clone();
    // The shell path can block for the full command timeout, so run it off the
    // actix worker pool.
    let result = astra_core::common::run_blocking_io(move || {
        device.execute(&tool, &input).map_err(AppError::Validation)
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

/// Governed deep crawl: HTTPA receipts + safe fetch + semantic rendering +
/// link-following, over the seed URLs. Body:
/// `{ "urls": ["https://…"], "query": "…", "max_depth": 1, "max_pages": 8 }`.
async fn crawl(
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> Result<HttpResponse, AppError> {
    let input = body.into_inner();
    let device = state.device.clone();
    let result = astra_core::common::run_blocking_io(move || {
        device
            .execute("deep_crawl", &input)
            .map_err(AppError::Validation)
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

/// Web discovery via SearXNG: turn a query into result URLs (the seeds the
/// crawl pipeline consumes). Body: `{ "query": "…", "count": 10 }`.
async fn search(
    state: web::Data<AppState>,
    body: web::Json<Value>,
) -> Result<HttpResponse, AppError> {
    let input = body.into_inner();
    let device = state.device.clone();
    let result = astra_core::common::run_blocking_io(move || {
        device
            .execute("web_search", &input)
            .map_err(AppError::Validation)
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

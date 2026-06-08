use std::collections::BTreeMap;

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, HealthSnapshot, now_ms},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/health", web::get().to(health))
        .route("/ready", web::get().to(ready));
}

async fn health(state: web::Data<AppState>) -> HttpResponse {
    let snapshot = HealthSnapshot {
        service: state.config.service_name.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        environment: state.config.environment.clone(),
        dependencies: BTreeMap::new(),
        timestamp_ms: now_ms(),
    };
    HttpResponse::Ok().json(ApiResponse::ok(snapshot))
}

async fn ready(_state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "status": "ready",
        "mode": "personal_assistant",
    })))
}

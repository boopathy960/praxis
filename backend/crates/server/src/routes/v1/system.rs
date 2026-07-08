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

async fn ready(state: web::Data<AppState>) -> HttpResponse {
    let eval_runs = state.evals.runs(100);
    let eval_pass_rate = if eval_runs.is_empty() {
        1.0
    } else {
        eval_runs.iter().filter(|run| run.pass).count() as f64 / eval_runs.len() as f64
    };
    let mind = state.active_inference.status().ok();
    let leverage = state.noesis.leverage().ok();
    let sandbox = state.sandbox.status().ok();
    HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "status": "ready",
        "mode": "personal_assistant",
        "intelligence": {
            "verified_success_rate": eval_pass_rate,
            "free_energy": mind.as_ref().map(|state| state.resting_free_energy).unwrap_or(0.0),
            "dream_debt_ratio": leverage.as_ref().map(|value| value.ratio).unwrap_or(0.0),
            "eval_pass_rate": eval_pass_rate,
            "unsafe_attempts_blocked": sandbox.as_ref().map(|value| value.threat_events).unwrap_or(0),
            "capability_growth": {
                "forged_tools": state.forge.catalog().len(),
                "agent_specs": state.architect.catalog().len(),
                "learning_episodes": state.learning.stats().episodes
            }
        }
    })))
}

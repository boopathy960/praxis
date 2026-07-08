use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    curriculum::CurriculumRequest,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/curriculum/next", web::post().to(next))
        .route("/curriculum/status", web::get().to(status));
}

async fn next(
    state: web::Data<AppState>,
    body: web::Json<CurriculumRequest>,
) -> Result<HttpResponse, AppError> {
    let learning = state.learning.list(200);
    let open_claims = state.proof_economy.open_claims().unwrap_or_default();
    let eval_runs = state.evals.runs(100);
    let attention = state.active_inference.attention_plan(20).ok();
    let plan = state.curriculum.propose(
        &learning,
        &open_claims,
        &eval_runs,
        attention.as_ref(),
        body.limit.unwrap_or(20),
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(plan)))
}

async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "learning": state.learning.stats(),
        "open_claims": state.proof_economy.open_claims().map(|claims| claims.len()).unwrap_or(0),
        "eval_runs": state.evals.runs(100).len(),
    }))))
}

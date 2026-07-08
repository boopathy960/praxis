use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    autonomy::EnqueueJobRequest,
    common::{ApiResponse, AppError, new_id},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/autonomy/jobs", web::post().to(enqueue_job))
        .route("/autonomy/jobs", web::get().to(list_jobs))
        .route("/autonomy/jobs/{job_id}", web::get().to(get_job))
        .route("/autonomy/tick", web::post().to(tick))
        .route("/autonomy/stats", web::get().to(stats));
}

/// Enqueues an objective for the autonomous worker to fabricate an agent and
/// run on its own cadence (or immediately via `/autonomy/tick`). The enqueue
/// passes the sandbox reference monitor and is notarized with an HTTPA
/// receipt like every other ingress.
async fn enqueue_job(
    state: web::Data<AppState>,
    body: web::Json<EnqueueJobRequest>,
) -> Result<HttpResponse, AppError> {
    let job = state.autonomy.enqueue(body.into_inner())?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "autonomy_job",
            job.objective.clone(),
            vec!["agent_fabricator".into(), "deep_research".into()],
            serde_json::json!({
                "job_id": job.job_id,
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    let httpa_receipt = state.httpa.record_receipt(
        &new_id("httpa_trace"),
        "autonomy_job",
        &serde_json::json!({
            "job_id": job.job_id,
            "objective": job.objective,
        }),
    )?;
    Ok(
        HttpResponse::Accepted().json(ApiResponse::ok(serde_json::json!({
            "job": job,
            "sandbox_receipt": sandbox,
            "httpa_receipt": httpa_receipt,
        }))),
    )
}

async fn list_jobs(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.autonomy.list_jobs()))
}

async fn get_job(
    state: web::Data<AppState>,
    job_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.autonomy.get_job(&job_id)?)))
}

/// Manually drives one autonomy tick (the background worker does this on a
/// timer). Runs agent executions — including deep research — so it is kept off
/// the async worker pool.
async fn tick(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let autonomy = state.autonomy.clone();
    let report = web::block(move || autonomy.tick(8))
        .await
        .map_err(|error| AppError::Internal(format!("autonomy tick task failed: {error}")))?;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(report)))
}

async fn stats(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.autonomy.stats()))
}

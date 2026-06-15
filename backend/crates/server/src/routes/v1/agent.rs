use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    agentic_loop::LoopRequest,
    common::{ApiResponse, AppError},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/agent/run", web::post().to(run));
}

/// Run the Verified Agentic Loop: the LLM plans the next action, the device
/// layer executes it under supervision, and the runtime proves each mutating
/// action's declared postcondition before reporting success. Body:
/// `{ "objective": "...", "allow_tools": ["fs_read", ...], "max_iterations": 8 }`.
async fn run(
    state: web::Data<AppState>,
    body: web::Json<LoopRequest>,
) -> Result<HttpResponse, AppError> {
    let result = state.agentic_loop.run(body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

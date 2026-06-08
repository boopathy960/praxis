use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    asc2::{Asc2BenchmarkRequest, Asc2MissionRequest},
    common::{ApiResponse, AppError},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/asc2/missions", web::post().to(create_mission))
        .route("/asc2/missions/{mission_id}", web::get().to(get_mission))
        .route("/asc2/status", web::get().to(status))
        .route("/asc2/benchmarks", web::post().to(run_benchmark))
        .route("/asc2/benchmarks/latest", web::get().to(latest_benchmark))
        .route(
            "/asc2/self-modifications",
            web::get().to(list_self_modifications),
        );
}

async fn create_mission(
    state: web::Data<AppState>,
    body: web::Json<Asc2MissionRequest>,
) -> Result<HttpResponse, AppError> {
    let request = body.into_inner();
    let sandbox_action = state.sandbox.action_for_activity(
        request.activity_kind.clone(),
        request.objective.clone(),
        request.requested_tools.clone(),
        serde_json::json!({
            "side_effecting": request.side_effecting,
            "sensitive": request.sensitive,
            "protocol": "httpa",
            "semantic_rendering_engine": "required_for_network_research_and_data_collection",
        }),
        request.owner_authorized,
    );
    let mission = state.asc2.execute_mission(request.clone()).await?;
    let sandbox = state
        .sandbox
        .guard(sandbox_action, request.side_effecting)?;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(
        state.asc2.attach_sandbox(&mission.mission_id, sandbox)?,
    )))
}

async fn get_mission(
    state: web::Data<AppState>,
    mission_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.get_mission(&mission_id)?)))
}

async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.status()?)))
}

async fn run_benchmark(
    state: web::Data<AppState>,
    body: web::Json<Asc2BenchmarkRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(
        state.asc2.run_benchmark(body.into_inner()).await?,
    )))
}

async fn latest_benchmark(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.latest_benchmark()?)))
}

async fn list_self_modifications(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.self_modifications()?)))
}

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    agent_runtime::{CreateActivityExecutionRequest, RiskTier},
    asc2::Asc2MissionRequest,
    common::{ApiResponse, AppError, new_id},
    semantic_render::SemanticRenderRequest,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/render/analyze", web::post().to(analyze))
        .route("/render/stats", web::get().to(stats));
}

async fn analyze(
    state: web::Data<AppState>,
    body: web::Json<SemanticRenderRequest>,
) -> Result<HttpResponse, AppError> {
    let request = body.into_inner();
    let report = {
        let mut engine = state.semantic_render.write();
        engine.render(request)?
    };
    let trace_id = new_id("httpa_trace");
    let orchestration =
        state
            .agent_runtime
            .orchestrate_activity(CreateActivityExecutionRequest {
                activity_kind: "semantic_render".into(),
                objective: report.normalized_url.clone(),
                httpa_trace_id: trace_id.clone(),
                device_id_hash: None,
                requested_tools: vec![
                    "semantic_extractor".into(),
                    "content_hasher".into(),
                    "safety_gate".into(),
                    "receipt_notary".into(),
                ],
                resource_limits: Default::default(),
                risk_tier: if report.warnings.is_empty() {
                    RiskTier::Low
                } else {
                    RiskTier::Moderate
                },
                owner_authorized: true,
                payload: serde_json::to_value(&report).map_err(|error| {
                    AppError::Internal(format!("render orchestration payload failed: {error}"))
                })?,
            })?;
    let asc2_mission = state
        .asc2
        .execute_mission(Asc2MissionRequest {
            objective: orchestration.objective.clone(),
            activity_kind: orchestration.activity_kind.clone(),
            requested_tools: orchestration.requested_tools.clone(),
            sensitive: false,
            owner_authorized: true,
            side_effecting: false,
            action_token: None,
        })
        .await?;
    let orchestration = state
        .agent_runtime
        .attach_asc2(&orchestration.execution_id, asc2_mission.diagnostics)?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "semantic_render",
            orchestration.objective.clone(),
            orchestration.requested_tools.clone(),
            serde_json::json!({
                "render_id": &report.render_id,
                "normalized_url": &report.normalized_url,
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    let orchestration = state
        .agent_runtime
        .attach_sandbox_to_activity(&orchestration.execution_id, sandbox.clone())?;
    let httpa_receipt = state.httpa.record_receipt(
        &trace_id,
        "semantic_render",
        &serde_json::json!({
            "render_id": report.render_id,
            "orchestration": orchestration,
        }),
    )?;
    let mut value = serde_json::to_value(report).map_err(|error| {
        AppError::Internal(format!("render response serialization failed: {error}"))
    })?;
    value["orchestration"] = serde_json::to_value(orchestration).map_err(|error| {
        AppError::Internal(format!("orchestration serialization failed: {error}"))
    })?;
    value["httpa_receipt"] = serde_json::to_value(httpa_receipt).map_err(|error| {
        AppError::Internal(format!("HTTPA receipt serialization failed: {error}"))
    })?;
    value["sandbox_receipt"] = serde_json::to_value(sandbox).map_err(|error| {
        AppError::Internal(format!("sandbox receipt serialization failed: {error}"))
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(value)))
}

async fn stats(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.semantic_render.read().stats())))
}

use actix_web::{HttpRequest, HttpResponse, http::header, web};
use astra_core::{
    AppState,
    agent_runtime::{CreateActivityExecutionRequest, RiskTier},
    asc2::Asc2MissionRequest,
    assistant::AssistantCommandRequest,
    common::{ApiResponse, AppError, new_id},
    os_guardian::{DataLeakSignal, LeakVector},
    sandbox::{SandboxActionKind, SandboxTaint},
};

const HEADER_ADMIN_TOKEN: &str = "x-astra-admin-token";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/assistant/commands", web::post().to(create_command))
        .route(
            "/assistant/commands/{command_id}",
            web::get().to(get_command),
        );
}

async fn create_command(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<AssistantCommandRequest>,
) -> Result<HttpResponse, AppError> {
    authorize_assistant_write(&state, &request)?;
    let command = body.into_inner();
    let dlp = state.os_guardian.analyze_dlp_signal(DataLeakSignal {
        source_process: "assistant_search_bar".into(),
        subject: command.text.clone(),
        destination: None,
        owner_authorized: true,
        data_classes: Vec::new(),
        leak_vectors: vec![LeakVector::Clipboard],
        metadata: Default::default(),
        content_sample: Some(command.text.clone()),
        observed_at_ms: None,
    })?;
    let response = state
        .assistant_commands
        .create_command(command, &dlp)?;
    let trace_id = new_id("httpa_trace");
    let orchestration =
        state
            .agent_runtime
            .orchestrate_activity(CreateActivityExecutionRequest {
                activity_kind: "assistant_command".into(),
                objective: response.normalized_text.clone(),
                httpa_trace_id: trace_id.clone(),
                device_id_hash: Some(response.device_id_hash.clone()),
                requested_tools: vec![
                    "command_planner".into(),
                    "action_allowlist".into(),
                    "safety_gate".into(),
                    "receipt_notary".into(),
                ],
                resource_limits: Default::default(),
                risk_tier: command_risk_tier(&response),
                owner_authorized: true,
                payload: serde_json::to_value(&response).map_err(|error| {
                    AppError::Internal(format!("assistant orchestration payload failed: {error}"))
                })?,
            })?;
    let asc2_mission = state
        .asc2
        .execute_mission(Asc2MissionRequest {
            objective: response.normalized_text.clone(),
            activity_kind: "assistant_command".into(),
            requested_tools: orchestration.requested_tools.clone(),
            sensitive: response.action_plan.requires_approval,
            owner_authorized: true,
            side_effecting: response.action_plan.executable,
            action_token: None,
        })
        .await?;
    let orchestration = state
        .agent_runtime
        .attach_asc2(&orchestration.execution_id, asc2_mission.diagnostics)?;
    let sandbox_action = if matches!(
        response.action_plan.action_kind,
        astra_core::assistant::AssistantActionKind::OpenUrl
            | astra_core::assistant::AssistantActionKind::SearchWeb
    ) {
        state.sandbox.action_for_network_fetch(
            response
                .action_plan
                .target
                .clone()
                .unwrap_or_else(|| response.normalized_text.clone()),
            vec![SandboxTaint::Public],
        )
    } else {
        state.sandbox.action_for_activity(
            "assistant_command",
            response.normalized_text.clone(),
            orchestration.requested_tools.clone(),
            serde_json::to_value(&response).map_err(|error| {
                AppError::Internal(format!("assistant sandbox payload failed: {error}"))
            })?,
            true,
        )
    };
    let sandbox_effectful = response.action_plan.executable
        || matches!(sandbox_action.kind, SandboxActionKind::NetworkFetch);
    let sandbox = state.sandbox.guard(sandbox_action, sandbox_effectful)?;
    let orchestration = state
        .agent_runtime
        .attach_sandbox_to_activity(&orchestration.execution_id, sandbox)?;
    let httpa_receipt = state.httpa.record_receipt(
        &trace_id,
        "assistant_command",
        &serde_json::json!({
            "command_id": response.command_id,
            "orchestration": orchestration,
        }),
    )?;
    let response = state.assistant_commands.attach_orchestration(
        &response.command_id,
        orchestration,
        httpa_receipt,
    )?;
    super::remember(
        &state,
        astra_core::chronicle::EpisodeKind::Decision,
        format!(
            "assistant command '{}' classified (executable={}, requires_approval={})",
            response.normalized_text,
            response.action_plan.executable,
            response.action_plan.requires_approval,
        ),
        "assistant",
        Some(response.command_id.clone()),
        vec!["assistant".into()],
        0.6,
    );
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(response)))
}

async fn get_command(
    state: web::Data<AppState>,
    command_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.assistant_commands.get_command(&command_id)?,
    )))
}

fn authorize_assistant_write(
    state: &web::Data<AppState>,
    request: &HttpRequest,
) -> Result<(), AppError> {
    if state.config.is_development() {
        return Ok(());
    }

    let configured = state
        .config
        .admin_token
        .as_deref()
        .ok_or(AppError::Unauthorized)?;
    let provided = request
        .headers()
        .get(header::HeaderName::from_static(HEADER_ADMIN_TOKEN))
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    if astra_core::common::constant_time_token_eq(provided, configured) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn command_risk_tier(response: &astra_core::assistant::AssistantCommandResponse) -> RiskTier {
    if matches!(
        response.status,
        astra_core::assistant::AssistantCommandStatus::Blocked
    ) {
        RiskTier::Critical
    } else if response.action_plan.requires_approval {
        RiskTier::High
    } else {
        RiskTier::Low
    }
}

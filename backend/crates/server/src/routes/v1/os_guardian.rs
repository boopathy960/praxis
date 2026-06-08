use actix_web::{HttpRequest, HttpResponse, http::header, web};
use astra_core::{
    AppState,
    agent_runtime::{CreateActivityExecutionRequest, RiskTier},
    asc2::Asc2MissionRequest,
    common::{ApiResponse, AppError, new_id},
    os_guardian::{DataLeakSignal, GuardianSeverity, SubmitGuardianEventRequest},
};

const HEADER_ADMIN_TOKEN: &str = "x-astra-admin-token";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/os-guardian/status", web::get().to(status))
        .route("/os-guardian/events", web::post().to(submit_event))
        .route("/os-guardian/events", web::get().to(recent_events))
        .route("/os-guardian/ledger/verify", web::get().to(verify_ledger))
        .route("/os-guardian/dlp/analyze", web::post().to(analyze_dlp))
        .route("/os-guardian/dlp/policy", web::get().to(dlp_policy))
        .route("/os-guardian/dlp/decoys", web::get().to(dlp_decoys))
        .route("/os-guardian/dlp/pending", web::get().to(dlp_pending));
}

async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.os_guardian.status())))
}

async fn submit_event(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<SubmitGuardianEventRequest>,
) -> Result<HttpResponse, AppError> {
    authorize_guardian_write(&state, &request)?;
    let event = state.os_guardian.submit_event(body.into_inner())?;
    let trace_id = new_id("httpa_trace");
    let orchestration =
        state
            .agent_runtime
            .orchestrate_activity(CreateActivityExecutionRequest {
                activity_kind: "os_guardian_event".into(),
                objective: event.event.subject.clone(),
                httpa_trace_id: trace_id.clone(),
                device_id_hash: None,
                requested_tools: vec![
                    "telemetry_classifier".into(),
                    "threat_evaluator".into(),
                    "safety_gate".into(),
                    "receipt_notary".into(),
                ],
                resource_limits: Default::default(),
                risk_tier: risk_for_guardian_severity(event.evaluation.severity),
                owner_authorized: true,
                payload: serde_json::to_value(&event).map_err(|error| {
                    AppError::Internal(format!("guardian orchestration payload failed: {error}"))
                })?,
            })?;
    let asc2_mission = state
        .asc2
        .execute_mission(Asc2MissionRequest {
            objective: orchestration.objective.clone(),
            activity_kind: orchestration.activity_kind.clone(),
            requested_tools: orchestration.requested_tools.clone(),
            sensitive: matches!(orchestration.risk_tier, RiskTier::High | RiskTier::Critical),
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
            "os_guardian_event",
            orchestration.objective.clone(),
            orchestration.requested_tools.clone(),
            serde_json::json!({
                "event_id": &event.event.event_id,
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
        "os_guardian_event",
        &serde_json::json!({
            "event_id": event.event.event_id,
            "orchestration": orchestration,
        }),
    )?;
    let mut value = serde_json::to_value(event).map_err(|error| {
        AppError::Internal(format!("guardian response serialization failed: {error}"))
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
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(value)))
}

async fn recent_events(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.os_guardian.recent_events())))
}

async fn verify_ledger(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.os_guardian.verify_ledger())))
}

async fn analyze_dlp(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<DataLeakSignal>,
) -> Result<HttpResponse, AppError> {
    authorize_guardian_write(&state, &request)?;
    let analysis = state.os_guardian.analyze_dlp_signal(body.into_inner())?;
    let trace_id = new_id("httpa_trace");
    let orchestration =
        state
            .agent_runtime
            .orchestrate_activity(CreateActivityExecutionRequest {
                activity_kind: "dlp_analysis".into(),
                objective: analysis.signal.subject.clone(),
                httpa_trace_id: trace_id.clone(),
                device_id_hash: None,
                requested_tools: vec![
                    "dlp_classifier".into(),
                    "redaction_policy".into(),
                    "safety_gate".into(),
                    "receipt_notary".into(),
                ],
                resource_limits: Default::default(),
                risk_tier: risk_for_guardian_severity(analysis.verdict.severity),
                owner_authorized: true,
                payload: serde_json::to_value(&analysis).map_err(|error| {
                    AppError::Internal(format!("DLP orchestration payload failed: {error}"))
                })?,
            })?;
    let asc2_mission = state
        .asc2
        .execute_mission(Asc2MissionRequest {
            objective: orchestration.objective.clone(),
            activity_kind: orchestration.activity_kind.clone(),
            requested_tools: orchestration.requested_tools.clone(),
            sensitive: true,
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
            "dlp_analysis",
            orchestration.objective.clone(),
            orchestration.requested_tools.clone(),
            serde_json::json!({
                "signal_id": &analysis.signal.signal_id,
                "protocol": "httpa",
                "taint_policy": "secrets_never_cross_network_or_message_boundary",
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
        "dlp_analysis",
        &serde_json::json!({
            "signal_id": analysis.signal.signal_id,
            "orchestration": orchestration,
        }),
    )?;
    let mut value = serde_json::to_value(analysis).map_err(|error| {
        AppError::Internal(format!("DLP response serialization failed: {error}"))
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
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(value)))
}

async fn dlp_policy(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.os_guardian.dlp_policy())))
}

async fn dlp_decoys(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.os_guardian.dlp_decoy_policies())))
}

async fn dlp_pending(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.os_guardian.pending_dlp_decisions())))
}

fn authorize_guardian_write(
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

    if provided == configured {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn risk_for_guardian_severity(severity: GuardianSeverity) -> RiskTier {
    match severity {
        GuardianSeverity::Informational | GuardianSeverity::Low => RiskTier::Low,
        GuardianSeverity::Moderate => RiskTier::Moderate,
        GuardianSeverity::High => RiskTier::High,
        GuardianSeverity::Critical => RiskTier::Critical,
    }
}

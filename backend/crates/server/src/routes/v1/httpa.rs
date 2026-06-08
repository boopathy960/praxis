use actix_web::{HttpRequest, HttpResponse, http::header, web};
use astra_core::{
    AppState,
    agent_runtime::{ActivityStatus, CreateActivityExecutionRequest, RiskTier},
    asc2::Asc2MissionRequest,
    common::{ApiResponse, AppError, new_id},
    httpa::{CreateHttpaSessionRequest, HttpaIntentRequest, HttpaIntentResponse, HttpaService},
};

const HEADER_ADMIN_TOKEN: &str = "x-astra-admin-token";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/httpa/capabilities", web::get().to(capabilities))
        .route("/httpa/sessions", web::post().to(create_session))
        .route("/httpa/intents", web::post().to(submit_intent))
        .route("/httpa/receipts/{receipt_id}", web::get().to(get_receipt))
        .route("/httpa/ledger/verify", web::get().to(verify_ledger));
}

async fn capabilities() -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(HttpaService::capabilities()))
}

async fn create_session(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<CreateHttpaSessionRequest>,
) -> Result<HttpResponse, AppError> {
    authorize_write(&state, &request)?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(
        state
            .httpa
            .create_session(body.into_inner(), !state.config.is_development())?,
    )))
}

async fn submit_intent(
    state: web::Data<AppState>,
    body: web::Json<HttpaIntentRequest>,
) -> Result<HttpResponse, AppError> {
    let intent = body.into_inner();
    let session = state
        .httpa
        .verify_session(&intent.session_id, &intent.session_token)?;
    let device = state.httpa.device_for_hash(&session.device_id_hash)?;
    let activity_allowed = device
        .action_allowlist
        .iter()
        .any(|allowed| allowed == &intent.activity_kind);
    let trace_id = new_id("httpa_trace");
    let execution = state
        .agent_runtime
        .orchestrate_activity(CreateActivityExecutionRequest {
            activity_kind: intent.activity_kind.clone(),
            objective: intent.intent.clone(),
            httpa_trace_id: trace_id.clone(),
            device_id_hash: Some(session.device_id_hash.clone()),
            requested_tools: intent.requested_tools,
            resource_limits: intent.resource_limits,
            risk_tier: infer_risk_tier(&intent.intent),
            owner_authorized: device.owner_enrolled && activity_allowed,
            payload: intent.payload,
        })?;
    let asc2_mission = state
        .asc2
        .execute_mission(Asc2MissionRequest {
            objective: execution.objective.clone(),
            activity_kind: execution.activity_kind.clone(),
            requested_tools: execution.requested_tools.clone(),
            sensitive: matches!(execution.risk_tier, RiskTier::High | RiskTier::Critical),
            owner_authorized: device.owner_enrolled && activity_allowed,
            side_effecting: !execution.requested_tools.is_empty(),
            action_token: None,
        })
        .await?;
    let execution = state
        .agent_runtime
        .attach_asc2(&execution.execution_id, asc2_mission.diagnostics)?;
    let sandbox_action = state.sandbox.action_for_activity(
        execution.activity_kind.clone(),
        execution.objective.clone(),
        execution.requested_tools.clone(),
        serde_json::json!({
            "session_id": &intent.session_id,
            "device_id_hash": &session.device_id_hash,
            "protocol": "httpa",
            "semantic_rendering_engine": "required_for_network_intake",
        }),
        device.owner_enrolled && activity_allowed,
    );
    let sandbox = state
        .sandbox
        .guard(sandbox_action, !execution.requested_tools.is_empty())?;
    let execution = state
        .agent_runtime
        .attach_sandbox_to_activity(&execution.execution_id, sandbox)?;
    let receipt = state.httpa.record_receipt(
        &trace_id,
        &format!("httpa_intent:{}", intent.activity_kind),
        &serde_json::json!({
            "session_id": intent.session_id,
            "device_id_hash": session.device_id_hash,
            "execution": execution,
        }),
    )?;
    let status = serde_json::to_value(execution.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "failed".into());
    Ok(
        HttpResponse::Accepted().json(ApiResponse::ok(HttpaIntentResponse {
            intent_id: new_id("httpa_intent"),
            status,
            trace_id,
            receipt_id: receipt.receipt_id,
            receipt_hash: receipt.block_hash,
            approval_required: matches!(execution.status, ActivityStatus::ApprovalRequired),
            blocked_reason: execution.blocked_reason,
            orchestration_id: execution.execution_id,
        })),
    )
}

async fn get_receipt(
    state: web::Data<AppState>,
    receipt_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.httpa.get_receipt(&receipt_id)?)))
}

async fn verify_ledger(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.httpa.verify_ledger())))
}

fn authorize_write(state: &web::Data<AppState>, request: &HttpRequest) -> Result<(), AppError> {
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

fn infer_risk_tier(intent: &str) -> RiskTier {
    let intent = intent.to_ascii_lowercase();
    if contains_any(
        &intent,
        &[
            "dump password",
            "private key",
            "seed phrase",
            "disable security",
            "stealth",
            "fingerprint evasion",
        ],
    ) {
        RiskTier::Critical
    } else if contains_any(
        &intent,
        &[
            "kill process",
            "quarantine",
            "registry",
            "firewall",
            "patch",
            "wallet",
            "transaction",
        ],
    ) {
        RiskTier::High
    } else {
        RiskTier::Low
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

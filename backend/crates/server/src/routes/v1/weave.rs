use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError, new_id},
    weave::{IntentStatus, SubmitIntentRequest},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/weave/intents", web::post().to(submit_intent))
        .route("/weave/intents", web::get().to(list_intents))
        .route("/weave/intents/{intent_id}", web::get().to(get_intent))
        .route("/weave/stats", web::get().to(stats));
}

/// Submits an intent to the Intent Substrate, which composes a weave plan and
/// materializes it (fabricates an agent and runs its first governed
/// execution). Materialization can deep-research, so it runs on the blocking
/// pool and never stalls an async worker. Like every other ingress, the result
/// passes the sandbox reference monitor and is notarized with an HTTPA
/// receipt.
async fn submit_intent(
    state: web::Data<AppState>,
    body: web::Json<SubmitIntentRequest>,
) -> Result<HttpResponse, AppError> {
    let weave = state.weave.clone();
    let request = body.into_inner();
    let mut intent = web::block(move || weave.submit(request))
        .await
        .map_err(|error| AppError::Internal(format!("weave intent task failed: {error}")))??;

    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "weave_intent",
            intent.description.clone(),
            intent.tools_used.clone(),
            serde_json::json!({
                "intent_id": intent.intent_id,
                "kind": intent.kind,
                "sources": intent.sources,
                "connector_bindings": intent.connector_bindings,
                "continuity_job_id": intent.continuity_job_id,
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    // The fabricated execution carries the sandbox receipt like every other
    // governed run.
    if let Some(execution_id) = intent.execution_id.clone() {
        let receipt = state
            .agent_runtime
            .attach_sandbox_to_execution(&execution_id, sandbox.clone())?;
        if intent.status == IntentStatus::Woven {
            intent.summary = Some(format!(
                "{} (sandbox: {})",
                intent.summary.unwrap_or_default(),
                receipt.status
            ));
        }
    }
    let httpa_receipt = state.httpa.record_receipt(
        &new_id("httpa_trace"),
        "weave_intent",
        &serde_json::json!({
            "intent_id": intent.intent_id,
            "status": intent.status,
            "agent_id": intent.agent_id,
            "execution_id": intent.execution_id,
        }),
    )?;
    Ok(
        HttpResponse::Created().json(ApiResponse::ok(serde_json::json!({
            "intent": intent,
            "sandbox_receipt": sandbox,
            "httpa_receipt": httpa_receipt,
        }))),
    )
}

async fn list_intents(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.weave.list_intents()))
}

async fn get_intent(
    state: web::Data<AppState>,
    intent_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.weave.get_intent(&intent_id)?)))
}

async fn stats(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.weave.stats()))
}

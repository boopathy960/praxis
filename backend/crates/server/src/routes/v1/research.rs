use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    agent_runtime::{CreateActivityExecutionRequest, RiskTier},
    asc2::Asc2MissionRequest,
    common::{ApiResponse, AppError, new_id},
    research::ResearchJobRequest,
    sandbox::{SandboxDecisionOutcome, SandboxTaint},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/research/jobs", web::post().to(create_job))
        .route("/research/jobs/{job_id}", web::get().to(get_job))
        .route(
            "/research/jobs/{job_id}/readiness",
            web::get().to(get_job_readiness),
        )
        .route(
            "/research/domain-policies",
            web::get().to(list_domain_policies),
        );
}

async fn create_job(
    state: web::Data<AppState>,
    body: web::Json<ResearchJobRequest>,
) -> Result<HttpResponse, AppError> {
    let payload = body.into_inner();
    let mut sandbox_receipts = Vec::new();
    for url in &payload.urls {
        let receipt = state.sandbox.guard(
            state
                .sandbox
                .action_for_network_fetch(url.clone(), vec![SandboxTaint::Public]),
            true,
        )?;
        if !matches!(
            receipt.decision.outcome,
            SandboxDecisionOutcome::Allow | SandboxDecisionOutcome::Rewrite
        ) || !receipt.executed
        {
            return Err(AppError::Forbidden(format!(
                "research URL fetch blocked by HTTPA sandbox: {}",
                receipt.decision.reason
            )));
        }
        sandbox_receipts.push(receipt);
    }
    let job = state.research.create_job(payload).await?;
    let artifact_safety_reports = job
        .artifacts
        .iter()
        .map(|artifact| {
            state.artifacts.scan_file(
                &artifact.artifact_id,
                &artifact.path,
                None,
                Some(&artifact.content_type),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let trace_id = new_id("httpa_trace");
    let orchestration =
        state
            .agent_runtime
            .orchestrate_activity(CreateActivityExecutionRequest {
                activity_kind: "research".into(),
                objective: job.summary.clone(),
                httpa_trace_id: trace_id.clone(),
                device_id_hash: None,
                requested_tools: vec![
                    "source_fetcher".into(),
                    "citation_ranker".into(),
                    "artifact_scanner".into(),
                    "safety_gate".into(),
                    "receipt_notary".into(),
                ],
                resource_limits: Default::default(),
                risk_tier: if job.errors.is_empty() {
                    RiskTier::Low
                } else {
                    RiskTier::Moderate
                },
                owner_authorized: true,
                payload: serde_json::to_value(&job).map_err(|error| {
                    AppError::Internal(format!("research orchestration payload failed: {error}"))
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
            "research",
            orchestration.objective.clone(),
            orchestration.requested_tools.clone(),
            serde_json::json!({
                "job_id": job.id,
                "protocol": "httpa",
                "semantic_rendering_engine": "required_for_collection_and_source_normalization",
            }),
            true,
        ),
        false,
    )?;
    sandbox_receipts.push(sandbox.clone());
    let orchestration = state
        .agent_runtime
        .attach_sandbox_to_activity(&orchestration.execution_id, sandbox)?;
    let httpa_receipt = state.httpa.record_receipt(
        &trace_id,
        "research",
        &serde_json::json!({
            "job_id": job.id,
            "orchestration": orchestration,
            "artifact_safety_reports": artifact_safety_reports,
        }),
    )?;
    let mut value = serde_json::to_value(job).map_err(|error| {
        AppError::Internal(format!("research response serialization failed: {error}"))
    })?;
    value["artifact_safety_reports"] =
        serde_json::to_value(artifact_safety_reports).map_err(|error| {
            AppError::Internal(format!("artifact safety serialization failed: {error}"))
        })?;
    value["orchestration"] = serde_json::to_value(orchestration).map_err(|error| {
        AppError::Internal(format!("orchestration serialization failed: {error}"))
    })?;
    value["httpa_receipt"] = serde_json::to_value(httpa_receipt).map_err(|error| {
        AppError::Internal(format!("HTTPA receipt serialization failed: {error}"))
    })?;
    value["sandbox_receipts"] = serde_json::to_value(sandbox_receipts).map_err(|error| {
        AppError::Internal(format!("sandbox receipt serialization failed: {error}"))
    })?;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(value)))
}

async fn get_job(
    state: web::Data<AppState>,
    job_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.research.get_job(&job_id)?)))
}

async fn get_job_readiness(
    state: web::Data<AppState>,
    job_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.research.readiness(&job_id)?)))
}

async fn list_domain_policies(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.research.list_domain_policies())))
}

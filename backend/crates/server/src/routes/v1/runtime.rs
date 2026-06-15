use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    agent_runtime::{
        CreateAgentExecutionRequest, CreateGeneratedAgentRequest, CreateGeneratedToolRequest,
        FabricateAgentRequest, SpawnSwarmRequest,
    },
    common::{ApiResponse, AppError},
};

const OWNER_PRINCIPAL: &str = "owner";
/// Default and ceiling for the long-poll swarm wait, kept under proxy/client
/// timeouts.
const SWARM_WAIT_DEFAULT_MS: u64 = 30_000;
const SWARM_WAIT_MAX_MS: u64 = 120_000;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/runtime/generated-agents", web::post().to(create_agent))
        .route("/runtime/generated-agents", web::get().to(list_agents))
        .route("/runtime/generated-tools", web::post().to(create_tool))
        .route("/runtime/generated-tools", web::get().to(list_tools))
        .route(
            "/agents/{agent_id}/executions",
            web::post().to(create_execution),
        )
        .route(
            "/agents/executions/{execution_id}",
            web::get().to(get_execution),
        )
        .route(
            "/runtime/activities/{execution_id}",
            web::get().to(get_activity),
        )
        .route("/runtime/metrics", web::get().to(runtime_metrics))
        .route("/agents/executions", web::get().to(list_executions))
        .route("/runtime/fabricate", web::post().to(fabricate_agent))
        .route("/runtime/swarms", web::post().to(spawn_swarm))
        .route("/runtime/swarms/{swarm_id}", web::get().to(swarm_status))
        .route(
            "/runtime/swarms/{swarm_id}/wait",
            web::get().to(wait_for_swarm),
        );
}

/// Spawns up to thousands of agent executions in one call (`replicate` per
/// spec). Admission queues task state and returns immediately; the swarm
/// scheduler interleaves tool steps over a measured number of worker lanes,
/// so the HTTP worker and the CPU stay responsive even on 2-vCPU hosts.
async fn spawn_swarm(
    state: web::Data<AppState>,
    body: web::Json<SpawnSwarmRequest>,
) -> Result<HttpResponse, AppError> {
    let request = body.into_inner();
    let sandbox_action = state.sandbox.action_for_tool_execution(
        None,
        None,
        "agent_runtime_swarm",
        serde_json::json!({
            "agent_specs": request.agents.len(),
            "requested_replicas": request
                .agents
                .iter()
                .map(|spec| spec.replicate.max(1))
                .sum::<usize>(),
            "protocol": "httpa",
        }),
        Some("owner_authorized_session".into()),
    );
    let runtime = state.agent_runtime.clone();
    let receipt = web::block(move || runtime.spawn_swarm(request, OWNER_PRINCIPAL))
        .await
        .map_err(|error| AppError::Internal(format!("swarm spawn task failed: {error}")))??;
    let sandbox = state.sandbox.guard(sandbox_action, true)?;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(serde_json::json!({
        "swarm": receipt,
        "sandbox": sandbox,
    }))))
}

async fn swarm_status(
    state: web::Data<AppState>,
    swarm_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.agent_runtime.swarm_status(&swarm_id)?,
    )))
}

#[derive(serde::Deserialize)]
struct SwarmWaitQuery {
    timeout_ms: Option<u64>,
}

/// Long-polls until the swarm completes or the timeout elapses, returning
/// the live status either way. The condvar wait parks a blocking-pool
/// thread without consuming CPU.
async fn wait_for_swarm(
    state: web::Data<AppState>,
    swarm_id: web::Path<String>,
    query: web::Query<SwarmWaitQuery>,
) -> Result<HttpResponse, AppError> {
    let timeout = std::time::Duration::from_millis(
        query
            .timeout_ms
            .unwrap_or(SWARM_WAIT_DEFAULT_MS)
            .min(SWARM_WAIT_MAX_MS),
    );
    let runtime = state.agent_runtime.clone();
    let swarm_id = swarm_id.into_inner();
    let status = web::block(move || runtime.wait_for_swarm(&swarm_id, timeout))
        .await
        .map_err(|error| AppError::Internal(format!("swarm wait task failed: {error}")))??;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(status)))
}

/// Fabricates a custom agent plus canary-tested custom tools for a complex
/// objective; the result is immediately executable via the executions route.
async fn fabricate_agent(
    state: web::Data<AppState>,
    body: web::Json<FabricateAgentRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Created().json(ApiResponse::ok(
        state.agent_runtime.fabricate_for_objective(body.into_inner())?,
    )))
}

async fn runtime_metrics(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.agent_runtime.runtime_metrics())))
}

async fn list_executions(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.agent_runtime.list_executions())))
}

async fn create_agent(
    state: web::Data<AppState>,
    body: web::Json<CreateGeneratedAgentRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Created().json(ApiResponse::ok(
        state.agent_runtime.create_agent(body.into_inner())?,
    )))
}

async fn list_agents(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.agent_runtime.list_agents())))
}

async fn create_tool(
    state: web::Data<AppState>,
    body: web::Json<CreateGeneratedToolRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Created().json(ApiResponse::ok(
        state.agent_runtime.create_tool(body.into_inner())?,
    )))
}

async fn list_tools(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.agent_runtime.list_tools())))
}

async fn create_execution(
    state: web::Data<AppState>,
    agent_id: web::Path<String>,
    body: web::Json<CreateAgentExecutionRequest>,
) -> Result<HttpResponse, AppError> {
    let request = body.into_inner();
    let agent_id = agent_id.into_inner();
    let sandbox_action = state.sandbox.action_for_tool_execution(
        None,
        None,
        "agent_runtime_generated_tool",
        serde_json::json!({
            "agent_id": &agent_id,
            "input": &request.input,
            "requested_tools": &request.requested_tools,
            "resource_limits": &request.resource_limits,
            "protocol": "httpa",
        }),
        Some("owner_authorized_session".into()),
    );
    // Tool pipelines may invoke deep_research (live fetches); keep them off the
    // async worker pool.
    let runtime = state.agent_runtime.clone();
    let exec_agent_id = agent_id.clone();
    let receipt = web::block(move || {
        runtime.create_execution(&exec_agent_id, OWNER_PRINCIPAL, request)
    })
    .await
    .map_err(|error| AppError::Internal(format!("agent execution task failed: {error}")))??;
    let sandbox = state.sandbox.guard(sandbox_action, true)?;
    let receipt = state
        .agent_runtime
        .attach_sandbox_to_execution(&receipt.execution_id, sandbox)?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(receipt)))
}

async fn get_execution(
    state: web::Data<AppState>,
    execution_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.agent_runtime.get_execution(&execution_id)?,
    )))
}

async fn get_activity(
    state: web::Data<AppState>,
    execution_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.agent_runtime.get_activity(&execution_id)?,
    )))
}

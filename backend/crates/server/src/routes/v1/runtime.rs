use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    agent_runtime::{
        CreateAgentExecutionRequest, CreateGeneratedAgentRequest, CreateGeneratedToolRequest,
    },
    common::{ApiResponse, AppError},
};

const OWNER_PRINCIPAL: &str = "owner";

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
        );
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
    let sandbox_action = state.sandbox.action_for_tool_execution(
        None,
        None,
        "agent_runtime_generated_tool",
        serde_json::json!({
            "agent_id": agent_id.as_str(),
            "input": &request.input,
            "requested_tools": &request.requested_tools,
            "resource_limits": &request.resource_limits,
            "protocol": "httpa",
        }),
        Some("owner_authorized_session".into()),
    );
    let receipt = state
        .agent_runtime
        .create_execution(&agent_id, OWNER_PRINCIPAL, request)?;
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

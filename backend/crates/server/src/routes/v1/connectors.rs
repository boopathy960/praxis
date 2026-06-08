use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    connectors::{ConnectConnectorRequest, ConnectorProvider, ConnectorResearchRequest},
};

const OWNER_PRINCIPAL: &str = "owner";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/connectors", web::get().to(list_connectors))
        .route(
            "/connectors/{provider}/connect",
            web::post().to(connect_provider),
        )
        .route(
            "/connectors/{provider}/disconnect",
            web::post().to(disconnect_provider),
        )
        .route(
            "/connectors/{provider}/research",
            web::post().to(research_provider),
        )
        .route("/connectors/jobs/{job_id}", web::get().to(get_job));
}

async fn list_connectors(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "profiles": state.connectors.profiles(),
        "accounts": state.connectors.accounts_for(OWNER_PRINCIPAL),
    }))))
}

async fn connect_provider(
    state: web::Data<AppState>,
    provider: web::Path<String>,
    body: web::Json<ConnectConnectorRequest>,
) -> Result<HttpResponse, AppError> {
    let provider = ConnectorProvider::from_path(&provider)?;
    let account = state
        .connectors
        .connect(OWNER_PRINCIPAL, provider, body.into_inner());
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "connector_connect",
            format!("connect connector {:?}", provider),
            vec!["connector_adapter".into(), "httpa_gateway".into()],
            serde_json::json!({
                "provider": format!("{:?}", provider),
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    Ok(
        HttpResponse::Created().json(ApiResponse::ok(serde_json::json!({
            "account": account,
            "sandbox_receipt": sandbox,
        }))),
    )
}

async fn disconnect_provider(
    state: web::Data<AppState>,
    provider: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let provider = ConnectorProvider::from_path(&provider)?;
    let account = state.connectors.disconnect(OWNER_PRINCIPAL, provider)?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "connector_disconnect",
            format!("disconnect connector {:?}", provider),
            vec!["connector_adapter".into(), "httpa_gateway".into()],
            serde_json::json!({
                "provider": format!("{:?}", provider),
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "account": account,
        "sandbox_receipt": sandbox,
    }))))
}

async fn research_provider(
    state: web::Data<AppState>,
    provider: web::Path<String>,
    body: web::Json<ConnectorResearchRequest>,
) -> Result<HttpResponse, AppError> {
    let provider = ConnectorProvider::from_path(&provider)?;
    let job = state
        .connectors
        .research(OWNER_PRINCIPAL, provider, body.into_inner())?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "connector_research",
            format!("research connector {:?}", provider),
            vec![
                "connector_adapter".into(),
                "httpa_gateway".into(),
                "semantic_render".into(),
            ],
            serde_json::json!({
                "provider": format!("{:?}", provider),
                "protocol": "httpa",
                "semantic_rendering_engine": "required_for_connector_research",
            }),
            true,
        ),
        false,
    )?;
    Ok(
        HttpResponse::Accepted().json(ApiResponse::ok(serde_json::json!({
            "job": job,
            "sandbox_receipt": sandbox,
        }))),
    )
}

async fn get_job(
    state: web::Data<AppState>,
    job_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.connectors.get_job(&job_id)?)))
}

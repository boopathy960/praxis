use actix_web::{HttpRequest, HttpResponse, web};
use astra_core::{
    AppState,
    asc2::Asc2MissionRequest,
    common::{ApiResponse, AppError},
    nexus::{
        AddMemberRequest, AgentKind, ApproveActionRequest, ConnectIntegrationRequest,
        CreateBusinessBrainInterviewRequest, CreateOrganizationRequest, CreateWorkflowRequest,
        IngestBusinessEventRequest, RejectActionRequest, RunAgentRequest,
        SubmitBusinessBrainAnswersRequest, UpdateAgentConfigurationRequest,
        UpdateOrganizationPlanRequest,
    },
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/nexus/agents/catalog", web::get().to(agent_catalog))
        .route("/nexus/plans", web::get().to(plan_catalog))
        .route(
            "/nexus/integrations/catalog",
            web::get().to(integration_catalog),
        )
        .route("/nexus/organizations", web::post().to(create_organization))
        .route("/nexus/organizations", web::get().to(list_organizations))
        .route(
            "/nexus/organizations/{organization_id}",
            web::get().to(get_organization),
        )
        .route(
            "/nexus/organizations/{organization_id}/dashboard",
            web::get().to(dashboard),
        )
        .route(
            "/nexus/organizations/{organization_id}/revenue-opportunities",
            web::get().to(list_revenue_opportunities),
        )
        .route(
            "/nexus/organizations/{organization_id}/business-brain/interviews",
            web::post().to(create_business_brain_interview),
        )
        .route(
            "/nexus/organizations/{organization_id}/business-brain/interviews/{interview_id}/answers",
            web::post().to(submit_business_brain_answers),
        )
        .route(
            "/nexus/organizations/{organization_id}/business-brain/interviews/{interview_id}/plan",
            web::post().to(generate_business_brain_plan),
        )
        .route(
            "/nexus/organizations/{organization_id}/plan",
            web::put().to(update_plan),
        )
        .route(
            "/nexus/organizations/{organization_id}/members",
            web::post().to(add_member),
        )
        .route(
            "/nexus/organizations/{organization_id}/members",
            web::get().to(list_members),
        )
        .route(
            "/nexus/organizations/{organization_id}/agents",
            web::get().to(list_agents),
        )
        .route(
            "/nexus/organizations/{organization_id}/agents/{agent_key}",
            web::put().to(update_agent),
        )
        .route(
            "/nexus/organizations/{organization_id}/agents/{agent_key}/runs",
            web::post().to(run_agent),
        )
        .route(
            "/nexus/organizations/{organization_id}/runs/{run_id}",
            web::get().to(get_run),
        )
        .route(
            "/nexus/organizations/{organization_id}/integrations",
            web::post().to(connect_integration),
        )
        .route(
            "/nexus/organizations/{organization_id}/integrations",
            web::get().to(list_integrations),
        )
        .route(
            "/nexus/organizations/{organization_id}/integrations/{integration_id}",
            web::delete().to(disconnect_integration),
        )
        .route(
            "/nexus/organizations/{organization_id}/events",
            web::post().to(ingest_event),
        )
        .route(
            "/nexus/organizations/{organization_id}/events",
            web::get().to(list_events),
        )
        .route(
            "/nexus/organizations/{organization_id}/workflows",
            web::post().to(create_workflow),
        )
        .route(
            "/nexus/organizations/{organization_id}/workflows",
            web::get().to(list_workflows),
        )
        .route(
            "/nexus/organizations/{organization_id}/actions",
            web::get().to(list_actions),
        )
        .route(
            "/nexus/organizations/{organization_id}/actions/{action_id}/approve",
            web::post().to(approve_action),
        )
        .route(
            "/nexus/organizations/{organization_id}/actions/{action_id}/reject",
            web::post().to(reject_action),
        );
}

async fn agent_catalog(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.nexus.agent_catalog()))
}

async fn plan_catalog(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.nexus.plan_catalog()))
}

async fn integration_catalog(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.nexus.integration_catalog()))
}

async fn create_organization(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<CreateOrganizationRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Created().json(ApiResponse::ok(
        state
            .nexus
            .create_organization(&principal(&request, &state)?, body.into_inner())?,
    )))
}

async fn list_organizations(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_organizations(&principal(&request, &state)?)?,
    )))
}

async fn get_organization(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .get_organization(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn dashboard(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .dashboard(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn list_revenue_opportunities(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_revenue_opportunities(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn create_business_brain_interview(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
    body: web::Json<CreateBusinessBrainInterviewRequest>,
) -> Result<HttpResponse, AppError> {
    let principal = principal(&request, &state)?;
    let interview = state.nexus.start_business_brain_interview(
        &principal,
        &organization_id,
        body.into_inner(),
    )?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "business_brain_interview",
            "start Business Brain onboarding interview",
            vec!["httpa_gateway".into(), "business_brain".into()],
            serde_json::json!({
                "organization_id": organization_id.as_str(),
                "interview_id": &interview.interview_id,
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    Ok(
        HttpResponse::Created().json(ApiResponse::ok(serde_json::json!({
            "interview": interview,
            "sandbox_receipt": sandbox,
        }))),
    )
}

async fn submit_business_brain_answers(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<SubmitBusinessBrainAnswersRequest>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, interview_id) = path.into_inner();
    let principal = principal(&request, &state)?;
    let interview = state.nexus.submit_business_brain_answers(
        &principal,
        &organization_id,
        &interview_id,
        body.into_inner(),
    )?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "business_brain_answers",
            "record Business Brain interview answers",
            vec!["httpa_gateway".into(), "business_brain".into()],
            serde_json::json!({
                "organization_id": &organization_id,
                "interview_id": &interview.interview_id,
                "answered_questions": interview.answers.len(),
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "interview": interview,
        "sandbox_receipt": sandbox,
    }))))
}

async fn generate_business_brain_plan(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, interview_id) = path.into_inner();
    let principal = principal(&request, &state)?;
    let interview =
        state
            .nexus
            .generate_business_brain_plan(&principal, &organization_id, &interview_id)?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "business_brain_plan",
            "generate Business Brain 90-day revenue plan",
            vec!["httpa_gateway".into(), "business_brain".into()],
            serde_json::json!({
                "organization_id": &organization_id,
                "interview_id": &interview.interview_id,
                "plan_id": interview.plan.as_ref().map(|plan| plan.plan_id.as_str()),
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "interview": interview,
        "sandbox_receipt": sandbox,
    }))))
}

async fn update_plan(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
    body: web::Json<UpdateOrganizationPlanRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(state.nexus.update_plan(
            &principal(&request, &state)?,
            &organization_id,
            body.into_inner(),
        )?)),
    )
}

async fn add_member(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
    body: web::Json<AddMemberRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(
        HttpResponse::Created().json(ApiResponse::ok(state.nexus.add_member(
            &principal(&request, &state)?,
            &organization_id,
            body.into_inner(),
        )?)),
    )
}

async fn list_members(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_members(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn list_agents(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_agent_configs(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn update_agent(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<UpdateAgentConfigurationRequest>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, agent_key) = path.into_inner();
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(state.nexus.update_agent_config(
            &principal(&request, &state)?,
            &organization_id,
            AgentKind::from_key(&agent_key)?,
            body.into_inner(),
        )?)),
    )
}

async fn run_agent(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<RunAgentRequest>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, agent_key) = path.into_inner();
    let agent = AgentKind::from_key(&agent_key)?;
    let run = state.nexus.run_agent(
        &principal(&request, &state)?,
        &organization_id,
        agent,
        body.into_inner(),
    )?;
    let run = attach_reasoning(&state, &organization_id, agent, run).await?;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(run)))
}

async fn get_run(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, run_id) = path.into_inner();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.nexus.get_run(
        &principal(&request, &state)?,
        &organization_id,
        &run_id,
    )?)))
}

async fn connect_integration(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
    body: web::Json<ConnectIntegrationRequest>,
) -> Result<HttpResponse, AppError> {
    let principal = principal(&request, &state)?;
    let integration =
        state
            .nexus
            .connect_integration(&principal, &organization_id, body.into_inner())?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "nexus_connect_integration",
            format!("connect Nexus integration {}", integration.provider),
            vec!["httpa_gateway".into(), "integration_adapter".into()],
            serde_json::json!({
                "organization_id": organization_id.as_str(),
                "integration_id": &integration.integration_id,
                "provider": &integration.provider,
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    Ok(
        HttpResponse::Created().json(ApiResponse::ok(serde_json::json!({
            "integration": integration,
            "sandbox_receipt": sandbox,
        }))),
    )
}

async fn list_integrations(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_integrations(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn disconnect_integration(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, integration_id) = path.into_inner();
    let integration = state.nexus.disconnect_integration(
        &principal(&request, &state)?,
        &organization_id,
        &integration_id,
    )?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "nexus_disconnect_integration",
            format!("disconnect Nexus integration {}", integration.provider),
            vec!["httpa_gateway".into(), "integration_adapter".into()],
            serde_json::json!({
                "organization_id": &organization_id,
                "integration_id": &integration.integration_id,
                "provider": &integration.provider,
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "integration": integration,
        "sandbox_receipt": sandbox,
    }))))
}

async fn ingest_event(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
    body: web::Json<IngestBusinessEventRequest>,
) -> Result<HttpResponse, AppError> {
    let mut result = state.nexus.ingest_event(
        &principal(&request, &state)?,
        &organization_id,
        body.into_inner(),
    )?;
    for run in &mut result.triggered_runs {
        *run = attach_reasoning(&state, &organization_id, run.agent, run.clone()).await?;
    }
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(result)))
}

async fn list_events(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_events(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn create_workflow(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
    body: web::Json<CreateWorkflowRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(
        HttpResponse::Created().json(ApiResponse::ok(state.nexus.create_workflow(
            &principal(&request, &state)?,
            &organization_id,
            body.into_inner(),
        )?)),
    )
}

async fn list_workflows(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_workflows(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn list_actions(
    state: web::Data<AppState>,
    request: HttpRequest,
    organization_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .nexus
            .list_actions(&principal(&request, &state)?, &organization_id)?,
    )))
}

async fn approve_action(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<ApproveActionRequest>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, action_id) = path.into_inner();
    let request_body = body.into_inner();
    let principal = principal(&request, &state)?;
    let action = state.nexus.approve_action(
        &principal,
        &organization_id,
        &action_id,
        request_body.clone(),
    )?;
    let action = if request_body.execute_after_approval {
        let sandbox = state.sandbox.guard(
            state.sandbox.action_for_nexus_delivery(
                organization_id.clone(),
                action.action_type.clone(),
                serde_json::json!({
                    "action_id": action.action_id,
                    "payload": action.payload,
                    "rollback": action.rollback,
                    "approved_by": principal,
                    "protocol": "httpa",
                    "semantic_rendering_engine": "required_for_external_delivery_context",
                }),
                Some("nexus_approver_token".into()),
            ),
            true,
        )?;
        state
            .nexus
            .attach_sandbox_to_action(&organization_id, &action_id, sandbox)?
    } else {
        action
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(action)))
}

async fn reject_action(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<RejectActionRequest>,
) -> Result<HttpResponse, AppError> {
    let (organization_id, action_id) = path.into_inner();
    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(state.nexus.reject_action(
            &principal(&request, &state)?,
            &organization_id,
            &action_id,
            body.into_inner(),
        )?)),
    )
}

fn principal(request: &HttpRequest, state: &AppState) -> Result<String, AppError> {
    if !state.config.is_development() {
        let configured = state
            .config
            .admin_token
            .as_deref()
            .ok_or(AppError::Unauthorized)?;
        let provided = request
            .headers()
            .get("x-astra-admin-token")
            .and_then(|value| value.to_str().ok())
            .ok_or(AppError::Unauthorized)?;
        if provided != configured {
            return Err(AppError::Unauthorized);
        }
    }
    if let Some(value) = request
        .headers()
        .get("x-nexus-principal-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(value.trim().to_string());
    }
    if state.config.is_development() {
        Ok("owner".into())
    } else {
        Err(AppError::Unauthorized)
    }
}

async fn attach_reasoning(
    state: &AppState,
    organization_id: &str,
    agent: AgentKind,
    run: astra_core::nexus::AgentRun,
) -> Result<astra_core::nexus::AgentRun, AppError> {
    let findings = run
        .findings
        .iter()
        .map(|finding| format!("{}: {}", finding.title, finding.explanation))
        .collect::<Vec<_>>()
        .join("\n");
    let sensitive = matches!(
        agent,
        AgentKind::HrOs
            | AgentKind::ComplianceOs
            | AgentKind::LegalOs
            | AgentKind::FinanceOs
            | AgentKind::EmployeeChurnRadar
            | AgentKind::BusinessBrain
            | AgentKind::DataProductCreator
            | AgentKind::StrategicAcquirerIntelligence
            | AgentKind::CompetitiveWeaknessExploiter
    );
    let mission = state
        .asc2
        .execute_mission(Asc2MissionRequest {
            objective: format!(
                "Act as the Nexus {} business automation agent. Verify the evidence-backed findings, identify limitations, and recommend the safest next decision.\n{}",
                agent.key(),
                findings
            ),
            activity_kind: format!("nexus_{}", agent.key()),
            requested_tools: Vec::new(),
            sensitive,
            owner_authorized: false,
            side_effecting: false,
            action_token: None,
        })
        .await?;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            format!("nexus_{}", agent.key()),
            mission.request.objective.clone(),
            Vec::new(),
            serde_json::json!({
                "organization_id": organization_id,
                "run_id": &run.run_id,
                "protocol": "httpa",
                "semantic_rendering_engine": "required_for_business_evidence_context",
            }),
            false,
        ),
        false,
    )?;
    let mission = state.asc2.attach_sandbox(&mission.mission_id, sandbox)?;
    state.nexus.attach_asc2_reasoning(
        organization_id,
        &run.run_id,
        mission.diagnostics,
        mission.answer,
    )
}

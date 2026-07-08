use actix_web::{HttpRequest, HttpResponse, http::header, web};
use astra_core::{
    AppState,
    asc2::{
        Asc2BenchmarkRequest, Asc2MissionRequest, AssessRequest, AutonomousSelfModRequest,
        IdeationRequest, SelfModificationCandidate,
    },
    common::{ApiResponse, AppError},
};
use serde::Deserialize;

const HEADER_ADMIN_TOKEN: &str = "x-astra-admin-token";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/asc2/missions", web::post().to(create_mission))
        .route("/asc2/missions/{mission_id}", web::get().to(get_mission))
        .route("/asc2/status", web::get().to(status))
        .route("/asc2/benchmarks", web::post().to(run_benchmark))
        .route("/asc2/benchmarks/latest", web::get().to(latest_benchmark))
        .route(
            "/asc2/self-modifications",
            web::get().to(list_self_modifications),
        )
        .route(
            "/asc2/self-modifications",
            web::post().to(stage_self_modification),
        )
        .route(
            "/asc2/self-modifications/autonomous",
            web::post().to(autonomous_self_modification),
        )
        // Runtime role prompts — live self-modification with no rebuild.
        .route("/asc2/prompts", web::get().to(list_role_prompts))
        .route("/asc2/prompts", web::post().to(set_role_prompt))
        .route(
            "/asc2/prompts/autonomous",
            web::post().to(autonomous_role_prompt),
        )
        .route("/asc2/prompts/revert", web::post().to(revert_role_prompt))
        .route("/asc2/ideate", web::post().to(ideate))
        .route("/asc2/assess", web::post().to(assess));
}

async fn create_mission(
    state: web::Data<AppState>,
    body: web::Json<Asc2MissionRequest>,
) -> Result<HttpResponse, AppError> {
    let request = body.into_inner();
    let sandbox_action = state.sandbox.action_for_activity(
        request.activity_kind.clone(),
        request.objective.clone(),
        request.requested_tools.clone(),
        serde_json::json!({
            "side_effecting": request.side_effecting,
            "sensitive": request.sensitive,
            "protocol": "httpa",
            "semantic_rendering_engine": "required_for_network_research_and_data_collection",
        }),
        request.owner_authorized,
    );
    let mission = state.asc2.execute_mission(request.clone()).await?;
    let sandbox = state
        .sandbox
        .guard(sandbox_action, request.side_effecting)?;
    super::remember(
        &state,
        astra_core::chronicle::EpisodeKind::Event,
        format!(
            "asc2 mission '{}' executed for activity '{}'",
            request.objective, request.activity_kind
        ),
        "asc2",
        Some(mission.mission_id.clone()),
        vec!["asc2".into()],
        0.5,
    );
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(
        state.asc2.attach_sandbox(&mission.mission_id, sandbox)?,
    )))
}

async fn get_mission(
    state: web::Data<AppState>,
    mission_id: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.get_mission(&mission_id)?)))
}

async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.status()?)))
}

async fn run_benchmark(
    state: web::Data<AppState>,
    body: web::Json<Asc2BenchmarkRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(
        state.asc2.run_benchmark(body.into_inner()).await?,
    )))
}

async fn latest_benchmark(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.latest_benchmark()?)))
}

async fn list_self_modifications(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.self_modifications()?)))
}

/// Validate and **stage** a candidate self-modification: the server formats,
/// tests, and release-builds the candidate workspace and stages the binary. This
/// direct path NEVER promotes — promotion is authorized only by the governance
/// court (via the CEO's `SelfModification` proposal flow), not by a human or an
/// env flag. Still admin-gated, and gated again inside by a *promotable*
/// benchmark, a changed-path allowlist, and workspace/rollback existence checks.
/// Body: `{ "workspace_path": "...", "changed_paths": ["asc2/prompts/..."], "current_binary": "..." }`.
async fn stage_self_modification(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<SelfModificationCandidate>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let candidate = body.into_inner();
    let asc2 = state.asc2.clone();
    // fmt/test/`build --release` is heavy and spawns processes — keep it off the
    // async worker pool. `authorized = false`: this direct/admin path may stage
    // and validate, but NEVER promotes — only the governance court (via the CEO)
    // can authorize a promotion.
    let record = web::block(move || asc2.validate_and_stage_candidate(candidate, false))
        .await
        .map_err(|error| {
            AppError::Internal(format!("self-modification staging task failed: {error}"))
        })??;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(record)))
}

/// Run the autonomous self-modification loop: the model proposes one bounded
/// change, it's applied + driven through the full hardened pipeline, and reverted
/// if any gate rejects it. Admin-gated. Body:
/// `{ "goal": "...", "workspace_path": "...", "current_binary": "..." }`.
async fn autonomous_self_modification(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<AutonomousSelfModRequest>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let outcome = state
        .asc2
        .autonomous_self_modification(body.into_inner())
        .await?;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(outcome)))
}

/// Autonomous ideation: generate diverse candidate approaches to a problem,
/// critique each adversarially, and return them ranked with the survivors.
/// Body: `{ "problem": "...", "candidates": 4 }`.
async fn ideate(
    state: web::Data<AppState>,
    body: web::Json<IdeationRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.asc2.autonomous_ideation(body.into_inner()).await?,
    )))
}

/// Metacognition: assess the model's certainty on a question via self-consistency
/// (answer it several ways, measure agreement, abstain if not answerable).
/// Body: `{ "question": "...", "samples": 4 }`.
async fn assess(
    state: web::Data<AppState>,
    body: web::Json<AssessRequest>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.asc2.metacognitive_assess(body.into_inner()).await?,
    )))
}

/// The current panel role prompts (planner/solver/verifier) and whether each is
/// overridden by a runtime file. Read-only.
async fn list_role_prompts(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.asc2.role_prompts())))
}

#[derive(Deserialize)]
struct SetPromptBody {
    role: String,
    content: String,
}

/// Set a runtime override prompt for a panel role (admin-gated). Takes effect on
/// the next mission — no rebuild. Body: `{ "role": "planner", "content": "..." }`.
async fn set_role_prompt(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<SetPromptBody>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let body = body.into_inner();
    let record = state.asc2.set_role_prompt(&body.role, &body.content)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(record)))
}

#[derive(Deserialize)]
struct AutoPromptBody {
    role: String,
    goal: String,
}

/// Autonomous live self-modification (admin-gated): the model authors an improved
/// system prompt for a panel role and it is applied immediately, with a backup
/// kept for rollback. Body: `{ "role": "planner", "goal": "..." }`.
async fn autonomous_role_prompt(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<AutoPromptBody>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let body = body.into_inner();
    let record = state
        .asc2
        .autonomous_role_prompt_update(&body.role, &body.goal)
        .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(record)))
}

#[derive(Deserialize)]
struct RevertPromptBody {
    role: String,
}

/// Revert a role prompt to its prior override or the compiled default (admin-gated).
async fn revert_role_prompt(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<RevertPromptBody>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let record = state.asc2.revert_role_prompt(&body.into_inner().role)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(record)))
}

/// Admin gate for state-changing privileged routes: open in development, and in
/// production requires a constant-time match of the `x-astra-admin-token` header
/// against the configured token. Mirrors `os_guardian::authorize_guardian_write`.
fn authorize_admin(state: &web::Data<AppState>, request: &HttpRequest) -> Result<(), AppError> {
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

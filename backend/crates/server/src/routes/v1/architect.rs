use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    architect::{ComposeRequest, RunSpecRequest},
    chronicle::EpisodeKind,
    common::{ApiResponse, AppError},
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/architect/agents", web::post().to(compose))
        .route("/architect/agents", web::get().to(catalog))
        .route("/architect/agents/{name}", web::get().to(get_spec))
        .route("/architect/agents/{name}/run", web::post().to(run_spec))
        .route("/architect/agents/{name}/heal", web::post().to(heal))
        .route("/architect/health", web::get().to(health));
}

/// Compose a reusable agent for a goal: the LLM designs a spec from the native
/// and forged tools, the spec is run for real through the verified loop on its
/// canary, and its success contract is staked in the proof economy. The agent is
/// reusable (Active) only if it provably achieved its canary. Body:
/// `{ "goal": "..." }`.
async fn compose(
    state: web::Data<AppState>,
    body: web::Json<ComposeRequest>,
) -> Result<HttpResponse, AppError> {
    let outcome = state.architect.compose(body.into_inner()).await?;
    super::remember(
        &state,
        EpisodeKind::Event,
        format!(
            "composed agent '{}' ({}) — {}",
            outcome.spec.name,
            if outcome.minted { "minted/active" } else { "probation" },
            outcome.detail
        ),
        "architect",
        Some(outcome.spec.spec_id.clone()),
        if outcome.minted {
            vec!["architect".into(), "minted".into()]
        } else {
            vec!["architect".into(), "probation".into()]
        },
        if outcome.minted { 0.7 } else { 0.5 },
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(outcome)))
}

/// The living registry of agents: the current spec for each name.
async fn catalog(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.architect.catalog())))
}

async fn get_spec(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.architect.get(&path)?)))
}

/// Run a minted agent on a fresh objective, with its proven role and tool
/// envelope. The run updates the agent's health (the healer's signal).
/// Body: `{ "objective": "...", "max_iterations": 6 }`.
async fn run_spec(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<RunSpecRequest>,
) -> Result<HttpResponse, AppError> {
    let result = state.architect.run_spec(&path, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

/// Heal a regressed agent: diagnose, re-compose, re-prove on its canary, and
/// hot-swap — quarantining the broken version.
async fn heal(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let outcome = state.architect.heal(&path).await?;
    super::remember(
        &state,
        EpisodeKind::Learning,
        format!("healed agent '{}' — {}", outcome.name, outcome.detail),
        "architect",
        Some(outcome.new_spec.spec_id.clone()),
        if outcome.healed {
            vec!["architect".into(), "heal".into()]
        } else {
            vec!["architect".into(), "heal".into(), "failure".into()]
        },
        0.7,
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(outcome)))
}

/// Fabric health: which agents warrant a heal alongside the full registry.
async fn health(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "heal_targets": state.architect.heal_targets(),
        "agents": state.architect.catalog(),
    }))))
}

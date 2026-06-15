use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    chronicle::EpisodeKind,
    common::{ApiResponse, AppError, run_blocking_io},
    forge::ForgeRequest,
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/forge/tools", web::post().to(forge_tool))
        .route("/forge/tools", web::get().to(catalog))
        .route("/forge/tools/{name}", web::get().to(get_tool))
        .route("/forge/tools/{name}/execute", web::post().to(execute))
        .route("/forge/tools/{name}/heal", web::post().to(heal))
        .route("/forge/health", web::get().to(health));
}

/// Forge a new capability from a gap: the LLM authors a recipe over the governed
/// primitives, it runs on a canary, and its contract is staked in the proof
/// economy. The tool goes live (Active/callable) only if its proof mints.
/// Body: `{ "objective": "..." }`.
async fn forge_tool(
    state: web::Data<AppState>,
    body: web::Json<ForgeRequest>,
) -> Result<HttpResponse, AppError> {
    let outcome = state.forge.forge(body.into_inner()).await?;
    super::remember(
        &state,
        EpisodeKind::Event,
        format!(
            "forged tool '{}' ({}) — {}",
            outcome.tool.name,
            if outcome.minted { "minted/active" } else { "probation" },
            outcome.detail
        ),
        "forge",
        Some(outcome.tool.tool_id.clone()),
        if outcome.minted {
            vec!["forge".into(), "minted".into()]
        } else {
            vec!["forge".into(), "probation".into()]
        },
        if outcome.minted { 0.7 } else { 0.5 },
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(outcome)))
}

/// The living registry: the current forged tool for each name.
async fn catalog(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.forge.catalog())))
}

async fn get_tool(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.forge.get(&path)?)))
}

#[derive(Debug, Deserialize)]
struct ExecuteRequest {
    #[serde(default)]
    input: serde_json::Value,
}

/// Run a forged tool by name (only Active/proof-minted tools run). The call
/// updates the tool's health, which is the healer's signal.
/// Body: `{ "input": { ... } }`.
async fn execute(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ExecuteRequest>,
) -> Result<HttpResponse, AppError> {
    let forge = state.forge.clone();
    let name = path.into_inner();
    let input = body.into_inner().input;
    let output =
        run_blocking_io(move || forge.execute(&name, &input).map_err(AppError::Validation))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(output)))
}

/// Heal a regressed forged tool: diagnose, regenerate, re-prove against the same
/// contract, and hot-swap — quarantining the broken version.
async fn heal(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let outcome = state.forge.heal(&path).await?;
    super::remember(
        &state,
        EpisodeKind::Learning,
        format!(
            "healed forged tool '{}' — {}",
            outcome.name, outcome.detail
        ),
        "forge",
        Some(outcome.new_tool.tool_id.clone()),
        if outcome.healed {
            vec!["forge".into(), "heal".into()]
        } else {
            vec!["forge".into(), "heal".into(), "failure".into()]
        },
        0.7,
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(outcome)))
}

/// Fabric health: which forged tools warrant a heal (regressed by failure rate,
/// or whose minted proof no longer verifies) alongside the full registry.
async fn health(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(serde_json::json!({
        "heal_targets": state.forge.heal_targets(),
        "tools": state.forge.catalog(),
    }))))
}

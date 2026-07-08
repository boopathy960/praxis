//! Genome OS HTTP surface — the product API.
//!
//! The genome is simultaneously the knowledge, the worker's instructions, the
//! human's interface, the machine's API, and the unit billed against, so this
//! one route group exposes all five views: compile intent into genomes, fork and
//! compose them across the commons, run them through the envelope-gated swarm
//! runtime, project a human UI or a machine API on demand, evolve them under
//! best-arm identification, price outcomes by Shapley value, and underwrite the
//! correlated risk. Mutating routes are admin-gated outside development, matching
//! the rest of the system.

use actix_web::{HttpRequest, HttpResponse, web};
use astra_core::{
    AppState,
    chronicle::EpisodeKind,
    common::{ApiResponse, AppError, constant_time_token_eq, run_blocking_io},
    genome::runtime::RunRequest,
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/genome/overview", web::get().to(overview))
        .route("/genome/compile", web::post().to(compile))
        .route("/genome/genomes", web::get().to(list))
        .route("/genome/genomes/{id}", web::get().to(get_genome))
        .route("/genome/genomes/{id}/fork", web::post().to(fork))
        .route(
            "/genome/genomes/{id}/certificate",
            web::get().to(certificate),
        )
        .route("/genome/genomes/{id}/run", web::post().to(run))
        .route("/genome/genomes/{id}/projection", web::get().to(projection))
        .route("/genome/genomes/{id}/reshape", web::post().to(reshape))
        .route(
            "/genome/genomes/{id}/machine-api",
            web::get().to(machine_api),
        )
        .route("/genome/genomes/{id}/price", web::post().to(price))
        .route(
            "/genome/genomes/{id}/report-violation",
            web::post().to(report_violation),
        )
        .route("/genome/compose", web::post().to(compose))
        .route(
            "/genome/operations/{operation}/evolve",
            web::get().to(evolve),
        )
        .route("/genome/commons", web::get().to(commons))
        .route("/genome/risk", web::get().to(risk))
        .route("/genome/runs", web::get().to(runs))
        .route("/genome/signoffs", web::get().to(signoffs))
        .route(
            "/genome/signoffs/{id}/resolve",
            web::post().to(resolve_signoff),
        );
}

/// Admin gate matching the house pattern: open in development, admin-token-gated
/// in production for any mutating operation.
fn admin_gate(request: &HttpRequest, state: &AppState) -> Result<(), AppError> {
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
        .get("x-astra-admin-token")
        .and_then(|value| value.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    if !constant_time_token_eq(provided, configured) {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

// ── reads ──────────────────────────────────────────────────────────────

async fn overview(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let service = state.genome.clone();
    let overview = run_blocking_io(move || service.overview())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(overview)))
}

async fn list(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.genome.list()?)))
}

async fn get_genome(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.genome.get(&path)?)))
}

async fn projection(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.genome.projection(&path)?)))
}

async fn machine_api(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.genome.machine_api(&path)?)))
}

async fn commons(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.genome.commons_graph()?)))
}

async fn certificate(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let service = state.genome.clone();
    let id = path.into_inner();
    let report = run_blocking_io(move || service.certificate(&id))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

async fn risk(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let service = state.genome.clone();
    let report = run_blocking_io(move || service.risk())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct RunsQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

async fn runs(
    state: web::Data<AppState>,
    query: web::Query<RunsQuery>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.genome.recent_runs(query.limit.min(500))?,
    )))
}

#[derive(Debug, Deserialize)]
struct SignoffQuery {
    #[serde(default)]
    status: Option<String>,
}

async fn signoffs(
    state: web::Data<AppState>,
    query: web::Query<SignoffQuery>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.genome.signoffs(query.status.as_deref())?,
    )))
}

#[derive(Debug, Deserialize)]
struct EvolveQuery {
    #[serde(default)]
    segment: Option<String>,
}

async fn evolve(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<EvolveQuery>,
) -> Result<HttpResponse, AppError> {
    let report = state.genome.evolve(&path, query.into_inner().segment)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── reshape preview (read-only interpretation) ──────────────────────────

#[derive(Debug, Deserialize)]
struct ReshapeRequest {
    text: String,
}

async fn reshape(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<ReshapeRequest>,
) -> Result<HttpResponse, AppError> {
    let patch = state.genome.reshape_preview(&path, &body.text)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(patch)))
}

// ── mutations ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CompileRequest {
    intent: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    operation: Option<String>,
}

async fn compile(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<CompileRequest>,
) -> Result<HttpResponse, AppError> {
    admin_gate(&request, &state)?;
    let service = state.genome.clone();
    let body = body.into_inner();
    let intent = body.intent.clone();
    let genome = run_blocking_io(move || service.compile(&body.intent, body.name, body.operation))?;
    super::remember(
        &state,
        EpisodeKind::Event,
        format!(
            "compiled genome '{}' ({}) from intent: {}",
            genome.name, genome.operation, intent
        ),
        "genome",
        Some(genome.genome_id.clone()),
        vec![
            "genome".into(),
            "compile".into(),
            genome.status.label().into(),
        ],
        0.6,
    );
    Ok(HttpResponse::Created().json(ApiResponse::ok(genome)))
}

#[derive(Debug, Deserialize)]
struct ForkRequest {
    deviations: String,
    #[serde(default)]
    name: Option<String>,
}

async fn fork(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ForkRequest>,
) -> Result<HttpResponse, AppError> {
    admin_gate(&request, &state)?;
    let service = state.genome.clone();
    let parent_id = path.into_inner();
    let body = body.into_inner();
    let deviations = body.deviations.clone();
    let outcome = run_blocking_io(move || service.fork(&parent_id, &body.deviations, body.name))?;
    super::remember(
        &state,
        EpisodeKind::Event,
        format!(
            "forked genome '{}' (gen {}) — deviation: {} (prior carried {:.0}%)",
            outcome.genome.name,
            outcome.genome.generation,
            deviations,
            outcome.refinement.prior_leverage * 100.0
        ),
        "genome",
        Some(outcome.genome.genome_id.clone()),
        vec!["genome".into(), "fork".into(), "refinement".into()],
        0.6,
    );
    Ok(HttpResponse::Created().json(ApiResponse::ok(outcome)))
}

async fn run(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<RunRequest>,
) -> Result<HttpResponse, AppError> {
    admin_gate(&request, &state)?;
    let service = state.genome.clone();
    let id = path.into_inner();
    let run_request = body.into_inner();
    let report = run_blocking_io(move || service.run(&id, run_request))?;
    super::remember(
        &state,
        EpisodeKind::Event,
        format!(
            "ran genome operation {} → {} ({})",
            report.operation, report.status, report.detail
        ),
        "genome",
        Some(report.genome_id.clone()),
        vec!["genome".into(), "run".into(), report.status.clone()],
        if report.status == "blocked" { 0.7 } else { 0.4 },
    );
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct ComposeRequest {
    upstream: String,
    downstream: String,
}

async fn compose(
    state: web::Data<AppState>,
    body: web::Json<ComposeRequest>,
) -> Result<HttpResponse, AppError> {
    let body = body.into_inner();
    let report = state.genome.compose(&body.upstream, &body.downstream)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct PriceRequest {
    total_value: f64,
    #[serde(default)]
    collaborators: Vec<String>,
}

async fn price(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<PriceRequest>,
) -> Result<HttpResponse, AppError> {
    let service = state.genome.clone();
    let id = path.into_inner();
    let body = body.into_inner();
    let report = run_blocking_io(move || service.price(&id, body.total_value, body.collaborators))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct ViolationRequest {
    detail: String,
}

async fn report_violation(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ViolationRequest>,
) -> Result<HttpResponse, AppError> {
    admin_gate(&request, &state)?;
    let service = state.genome.clone();
    let id = path.into_inner();
    let detail = body.into_inner().detail;
    let report = run_blocking_io(move || service.report_violation(&id, &detail))?;
    super::remember(
        &state,
        EpisodeKind::Learning,
        format!(
            "envelope violation reported on genome {} — quarantined",
            report.genome_id
        ),
        "genome",
        Some(report.genome_id.clone()),
        vec!["genome".into(), "violation".into(), "quarantine".into()],
        0.9,
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct ResolveRequest {
    approve: bool,
    #[serde(default)]
    note: Option<String>,
}

async fn resolve_signoff(
    state: web::Data<AppState>,
    request: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ResolveRequest>,
) -> Result<HttpResponse, AppError> {
    admin_gate(&request, &state)?;
    let body = body.into_inner();
    let signoff = state
        .genome
        .resolve_signoff(&path, body.approve, body.note)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(signoff)))
}

use actix_web::{HttpRequest, HttpResponse, http::header, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    governance::ProposalDraft,
};
use serde::Deserialize;

const HEADER_ADMIN_TOKEN: &str = "x-astra-admin-token";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/ceo/status", web::get().to(status))
        .route("/ceo/journal", web::get().to(journal))
        // The executive front door: ask the CEO a question and it marshals the
        // whole project to answer it. Open (not admin-gated) like agent/mission.
        .route("/ceo/query", web::post().to(query))
        .route("/ceo/query/{id}", web::get().to(query_status))
        .route("/ceo/queries", web::get().to(query_list))
        .route("/ceo/think", web::post().to(think))
        .route("/ceo/self-evolve", web::post().to(self_evolve))
        .route("/ceo/self-modify", web::post().to(self_modify))
        .route("/ceo/propose", web::post().to(propose))
        .route("/ceo/execute", web::post().to(execute))
        // The continuous OS-security patrol (the security cortex).
        .route("/ceo/guardian/status", web::get().to(guardian_status))
        .route("/ceo/guardian/posture", web::get().to(guardian_posture))
        .route("/ceo/guardian/findings", web::get().to(guardian_findings))
        .route("/ceo/guardian/patrols", web::get().to(guardian_patrols))
        .route("/ceo/guardian/patrol", web::post().to(guardian_patrol))
        .route(
            "/ceo/guardian/remediations",
            web::get().to(guardian_remediations),
        )
        .route(
            "/ceo/guardian/remediate",
            web::post().to(guardian_remediate),
        );
}

/// The CEO's standing under governance: its 49% authority, what it controls and
/// what it does not, and its proposal/thought tallies. Read-only.
async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.status()?)))
}

/// The CEO's private journal of thoughts, submissions, and executions.
async fn journal(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.journal_entries(200)?)))
}

#[derive(Deserialize)]
struct QueryBody {
    query: String,
}

/// Ask the CEO a question. It orchestrates the whole project — recall → route →
/// verified loop → proof → continuous re-verification, proposing any missing
/// capability to governance — on a background worker, returning a job to poll.
/// 202 Accepted with the queued job; poll `GET /ceo/query/{id}` for the answer.
async fn query(
    state: web::Data<AppState>,
    body: web::Json<QueryBody>,
) -> Result<HttpResponse, AppError> {
    let job = state.ceo.submit_query(&body.into_inner().query)?;
    Ok(HttpResponse::Accepted().json(ApiResponse::ok(job)))
}

/// Poll one query job — its status, and the verified answer once finished.
async fn query_status(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.get_query(&path)?)))
}

/// Recent query jobs, newest first.
async fn query_list(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.query_jobs(100)?)))
}

#[derive(Deserialize, Default)]
struct ThinkBody {
    #[serde(default)]
    focus: String,
}

/// One round of executive self-thinking (admin-gated — it calls the model).
async fn think(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: Option<web::Json<ThinkBody>>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let focus = body.map(|b| b.into_inner().focus).unwrap_or_default();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.think(&focus).await?)))
}

/// One self-evolution round: think, then draft + submit an upgrade proposal to
/// governance (admin-gated). It never approves or executes on its own.
async fn self_evolve(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.self_evolve().await?)))
}

#[derive(Deserialize)]
struct SelfModifyBody {
    goal: String,
}

/// One directed self-modification round (admin-gated trigger): the CEO drafts a
/// concrete code change and submits it to governance as a `SelfModification`
/// proposal. Nothing is applied until the court approves and the CEO executes —
/// the approval, not this call, is the permission.
async fn self_modify(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<SelfModifyBody>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.ceo.self_modify(&body.into_inner().goal).await?,
    )))
}

/// Submit an explicitly-authored upgrade proposal to governance (admin-gated).
async fn propose(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<ProposalDraft>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.propose(body.into_inner())?)))
}

#[derive(Deserialize)]
struct ExecuteBody {
    proposal_id: String,
}

/// Execute a court-approved upgrade — spawn the agents/tools it authorized
/// (admin-gated). Errors if governance has not approved the proposal: that is
/// the enforced "only after approval may the CEO spawn" gate.
async fn execute(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<ExecuteBody>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .ceo
            .execute_upgrade(&body.into_inner().proposal_id)
            .await?,
    )))
}

/// The security cortex's posture: model backing, scan roots, patrol/finding
/// counts, and the worst severity seen. Read-only.
async fn guardian_status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.guardian().status()?)))
}

/// The machine's current security posture: severity-weighted risk, health grade,
/// and open-remediation count aggregated from recent findings. Read-only.
async fn guardian_posture(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.guardian().posture()?)))
}

/// Recent security findings, newest first (default 100).
async fn guardian_findings(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.guardian().recent_findings(100)?)))
}

/// Recent patrol reports, newest first (default 50).
async fn guardian_patrols(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.guardian().recent_patrols(50)?)))
}

/// Trigger one full security patrol now (admin-gated — it scans the host and may
/// draft fixes through the model). Returns the patrol report. Destructive
/// remediations are submitted to governance, never executed here.
async fn guardian_patrol(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.ceo.guardian_patrol().await?)))
}

/// Recent remediation records — what the executor executed, planned, or advised
/// for court-approved remediations. Read-only.
async fn guardian_remediations(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.ceo.guardian().recent_remediations(100)?,
    )))
}

/// Run the remediation executor now (admin-gated): carry out remediations the
/// court has approved. Destructive actions (process termination) still require
/// `ASTRA_GUARDIAN_EXECUTE=on`; without it they are recorded as `planned`.
async fn guardian_remediate(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state.ceo.guardian_execute_remediations().await?,
    )))
}

/// Admin gate: open in development, production requires a constant-time match of
/// the `x-astra-admin-token` header. Mirrors the governance routes.
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

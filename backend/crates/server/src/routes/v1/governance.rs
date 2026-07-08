use actix_web::{HttpRequest, HttpResponse, http::header, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
};
use serde::Deserialize;

const HEADER_ADMIN_TOKEN: &str = "x-astra-admin-token";

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/governance/status", web::get().to(status))
        .route("/governance/ledger", web::get().to(ledger))
        .route("/governance/proposals", web::get().to(proposals))
        .route("/governance/review", web::post().to(review))
        .route("/governance/patrol", web::post().to(patrol))
        .route("/governance/lockdown", web::post().to(lockdown));
}

/// The single governance authority's current posture (alert level, units,
/// lockdown state, total enforcements). Read-only.
async fn status(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.governance.status()?)))
}

/// The immutable enforcement ledger (army/police seals, court lockdowns).
async fn ledger(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.governance.ledger(200)?)))
}

/// The project-upgrade proposals the court has on record (newest first).
async fn proposals(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.governance.proposals(200)?)))
}

#[derive(Deserialize, Default)]
struct ReviewBody {
    /// Review one proposal; omit to adjudicate every pending proposal at once.
    #[serde(default)]
    proposal_id: Option<String>,
}

/// The court rules on proposals (admin-gated) — the 51% in action. With a
/// `proposal_id` it rules on that one; without, it adjudicates all pending
/// proposals. Approves human-safe, in-authority upgrades; terminates the rest.
async fn review(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: Option<web::Json<ReviewBody>>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let proposal_id = body.and_then(|b| b.into_inner().proposal_id);
    let payload = match proposal_id {
        Some(id) => serde_json::json!({ "reviewed": [state.governance.review_proposal(&id)?] }),
        None => serde_json::json!({ "reviewed": state.governance.review_pending()? }),
    };
    Ok(HttpResponse::Ok().json(ApiResponse::ok(payload)))
}

/// Run one army + police patrol cycle now (admin-gated). The army seals border
/// breaches; the police seals agents whose dangerous actions were denied.
async fn patrol(
    state: web::Data<AppState>,
    request: HttpRequest,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    let governance = state.governance.clone();
    let reports = astra_core::common::run_blocking_io(move || {
        let army = governance.army_patrol()?;
        let police = governance.police_patrol()?;
        Ok(serde_json::json!({ "army": army, "police": police }))
    })?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(reports)))
}

#[derive(Deserialize)]
struct LockdownBody {
    reason: String,
}

/// Declare a system-wide lockdown — the court alone may do this (admin-gated).
/// Seals every caller out of the sandbox for the lockdown window.
async fn lockdown(
    state: web::Data<AppState>,
    request: HttpRequest,
    body: web::Json<LockdownBody>,
) -> Result<HttpResponse, AppError> {
    authorize_admin(&state, &request)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        state
            .governance
            .declare_lockdown(&body.into_inner().reason)?,
    )))
}

/// Admin gate: open in development, production requires a constant-time match of
/// the `x-astra-admin-token` header. Mirrors the other privileged routes.
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

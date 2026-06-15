use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError, run_blocking_io},
    proof_economy::{AttackRequest, ProposeRequest},
    proof_round::RoundConfig,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/proof/claims", web::post().to(propose))
        .route("/proof/claims/{id}", web::get().to(get_claim))
        .route("/proof/claims/{id}/attack", web::post().to(attack))
        .route("/proof/claims/{id}/verify", web::get().to(verify))
        .route("/proof/ledger", web::get().to(ledger))
        .route("/proof/open", web::get().to(open))
        .route("/proof/next-experiment", web::get().to(next_experiment))
        .route("/proof/round", web::post().to(round))
        .route("/proof/accounts/{id}", web::get().to(account));
}

/// Run one self-driving round: the LLM proposes claims and refutes them; only
/// claims that survive the economy's checks under attack are minted.
async fn round(
    state: web::Data<AppState>,
    body: web::Json<RoundConfig>,
) -> Result<HttpResponse, AppError> {
    let report = state.proof_conductor.run_round(body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// Stake reputation on a new machine-checkable claim. Its check is run for real
/// on arrival; a falsehood is born refuted and the stake is burned.
async fn propose(
    state: web::Data<AppState>,
    body: web::Json<ProposeRequest>,
) -> Result<HttpResponse, AppError> {
    let economy = state.proof_economy.clone();
    let request = body.into_inner();
    let claim = run_blocking_io(move || economy.propose(request))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(claim)))
}

/// Stake reputation on breaking a claim. The house re-runs its check; a
/// successful refutation transfers the proposer's stake to the attacker.
async fn attack(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<AttackRequest>,
) -> Result<HttpResponse, AppError> {
    let economy = state.proof_economy.clone();
    let id = path.into_inner();
    let request = body.into_inner();
    let claim = run_blocking_io(move || economy.attack(&id, request))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(claim)))
}

/// The cheap public proof: re-run a claim's check. No stakes move.
async fn verify(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let economy = state.proof_economy.clone();
    let id = path.into_inner();
    let report = run_blocking_io(move || economy.verify(&id))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

async fn get_claim(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.proof_economy.get_claim(&path)?)))
}

/// The minted ledger — verified knowledge that compounds and never rots.
async fn ledger(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.proof_economy.ledger()?)))
}

async fn open(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.proof_economy.open_claims()?)))
}

/// The highest value-of-information open claim to resolve next.
async fn next_experiment(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.proof_economy.next_experiment()?)))
}

async fn account(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(state.proof_economy.account(&path)?)))
}

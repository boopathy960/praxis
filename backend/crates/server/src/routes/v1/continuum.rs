//! The Continuum HTTP surface — six verification domains over one proof economy,
//! plus the continuous re-verification (Sentinel) controls.
//!
//! Enrollment endpoints run their claim's `Check` for real on arrival (so they
//! do real device I/O and are dispatched on a blocking thread); the query and
//! ledger endpoints are cheap reads. Spec mining drives the LLM proposer/refuter
//! round and is awaited directly.

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError, run_blocking_io},
    continuum::{
        AdversaryRequest, AiGuardrailRequest, ControlRequest, DataContractRequest, DeadEndQuery,
        DependencyContractRequest, EquivalenceRequest, EvalSuiteRequest, GoldenPathRequest,
        GuardrailQuery, GuardrailRequest, ImpossibilityRequest, KnowledgeFactRequest,
        KnowledgeQuery, MineRequest, ReproRequest,
    },
    sentinel::Domain,
};
use serde::Deserialize;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg
        // 1. Reproducibility
        .route(
            "/continuum/reproducibility",
            web::post().to(enroll_reproducibility),
        )
        // 2. Supply-chain behavioral contracts
        .route("/continuum/supply-chain", web::post().to(enroll_contract))
        // 3. Continuous adversarial compliance
        .route("/continuum/compliance", web::post().to(enroll_control))
        // 4. Verified-spec mining
        .route("/continuum/spec-mining", web::post().to(mine_spec))
        // 5. The registry of proven impossibilities
        .route(
            "/continuum/impossibility",
            web::post().to(register_impossibility),
        )
        .route(
            "/continuum/impossibility",
            web::get().to(impossibility_registry),
        )
        .route(
            "/continuum/impossibility/query",
            web::post().to(is_known_dead_end),
        )
        // 6. Vibe-coder guardrails
        .route("/continuum/guardrails", web::post().to(enroll_guardrail))
        .route(
            "/continuum/guardrails/query",
            web::post().to(query_guardrails),
        )
        // 7. AI-in-production safety stack (red-team + eval + guardrails)
        .route(
            "/continuum/ai-safety/guardrails",
            web::post().to(enroll_ai_guardrails),
        )
        .route(
            "/continuum/ai-safety/evals",
            web::post().to(enroll_eval_suite),
        )
        // 8. Verified data-quality & data contracts
        .route(
            "/continuum/data-contract",
            web::post().to(enroll_data_contract),
        )
        // 9. Verified migration / modernization (behavioral equivalence)
        .route("/continuum/equivalence", web::post().to(enroll_equivalence))
        // 10. A knowledge base that cannot lie
        .route("/continuum/knowledge", web::post().to(record_fact))
        .route("/continuum/knowledge", web::get().to(knowledge_base))
        .route("/continuum/knowledge/query", web::post().to(recall_fact))
        // 11. Platform-engineering team in a box (golden paths)
        .route("/continuum/golden-path", web::post().to(enroll_golden_path))
        // The shared adversary: continuous red team / partition hunter
        .route("/continuum/adversary", web::post().to(run_adversary))
        // The continuous layer (Sentinel) surfaced
        .route("/continuum/watches", web::get().to(watches))
        .route("/continuum/watches/{id}", web::get().to(get_watch))
        .route("/continuum/watches/{id}/sweep", web::post().to(sweep_one))
        .route("/continuum/sweep", web::post().to(sweep))
        .route("/continuum/drift", web::get().to(drift));
}

// ── 1. Reproducibility ──────────────────────────────────────────────────

/// Turn a computational finding into a continuously re-runnable certificate of
/// reproducibility.
async fn enroll_reproducibility(
    state: web::Data<AppState>,
    body: web::Json<ReproRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let report = run_blocking_io(move || continuum.enroll_reproducibility(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── 2. Supply-chain behavioral contracts ────────────────────────────────

/// Stake a dependency's behavioral guarantees and impossibilities; both
/// re-verify on every version bump.
async fn enroll_contract(
    state: web::Data<AppState>,
    body: web::Json<DependencyContractRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let report = run_blocking_io(move || continuum.enroll_dependency_contract(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── 3. Continuous adversarial compliance ────────────────────────────────

/// Express a control as a software check and place it under continuous attack.
async fn enroll_control(
    state: web::Data<AppState>,
    body: web::Json<ControlRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let report = run_blocking_io(move || continuum.enroll_control(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── 4. Verified-spec mining ─────────────────────────────────────────────

/// Mine the true, executable spec of an opaque system by running the
/// proposer/refuter round at it. Needs a configured reasoner.
async fn mine_spec(
    state: web::Data<AppState>,
    body: web::Json<MineRequest>,
) -> Result<HttpResponse, AppError> {
    let report = state.continuum.mine_spec(body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── 5. The registry of proven impossibilities ───────────────────────────

/// Bank a negative result: "this provably cannot be done here".
async fn register_impossibility(
    state: web::Data<AppState>,
    body: web::Json<ImpossibilityRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let enrolled = run_blocking_io(move || continuum.register_impossibility(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(enrolled)))
}

/// The negative-knowledge ledger — minted and pending impossibilities.
async fn impossibility_registry(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let registry = state.continuum.impossibility_registry()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(registry)))
}

/// "Has anyone proven this won't work here?" — recall over banked dead ends.
async fn is_known_dead_end(
    state: web::Data<AppState>,
    body: web::Json<DeadEndQuery>,
) -> Result<HttpResponse, AppError> {
    let answer = state.continuum.is_known_dead_end(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(answer)))
}

// ── 6. Vibe-coder guardrails ────────────────────────────────────────────

/// Hold a plain-language safety intent as a continuously-attacked impossibility.
async fn enroll_guardrail(
    state: web::Data<AppState>,
    body: web::Json<GuardrailRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let enrolled = run_blocking_io(move || continuum.enroll_guardrail(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(enrolled)))
}

/// "Is my app safe re: …?" — recall the relevant guardrails in plain language.
async fn query_guardrails(
    state: web::Data<AppState>,
    body: web::Json<GuardrailQuery>,
) -> Result<HttpResponse, AppError> {
    let matches = state.continuum.query_guardrails(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(matches)))
}

// ── 7. AI-in-production safety stack ─────────────────────────────────────

/// Stake a policy's red-team guardrails as continuously-attacked impossibilities.
async fn enroll_ai_guardrails(
    state: web::Data<AppState>,
    body: web::Json<AiGuardrailRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let report = run_blocking_io(move || continuum.enroll_ai_guardrails(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

/// Stake an evaluation suite that re-runs on every model/prompt change.
async fn enroll_eval_suite(
    state: web::Data<AppState>,
    body: web::Json<EvalSuiteRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let report = run_blocking_io(move || continuum.enroll_eval_suite(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── 8. Verified data-quality & data contracts ───────────────────────────

/// Stake a data contract as continuously re-verified checks over the warehouse.
async fn enroll_data_contract(
    state: web::Data<AppState>,
    body: web::Json<DataContractRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let report = run_blocking_io(move || continuum.enroll_data_contract(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── 9. Verified migration / modernization ───────────────────────────────

/// Stake a behavioral-equivalence claim for a migration (Phase 2). Phase 1 is
/// `POST /continuum/spec-mining`.
async fn enroll_equivalence(
    state: web::Data<AppState>,
    body: web::Json<EquivalenceRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let enrolled = run_blocking_io(move || continuum.enroll_equivalence(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(enrolled)))
}

// ── 10. A knowledge base that cannot lie ────────────────────────────────

/// Back an operational fact with a re-runnable check so it can't go stale-silent.
async fn record_fact(
    state: web::Data<AppState>,
    body: web::Json<KnowledgeFactRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let enrolled = run_blocking_io(move || continuum.record_fact(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(enrolled)))
}

/// The knowledge base, partitioned into facts that hold vs facts that went false.
async fn knowledge_base(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let kb = state.continuum.knowledge_base()?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(kb)))
}

/// Recall facts by plain-language query, with their current truth state.
async fn recall_fact(
    state: web::Data<AppState>,
    body: web::Json<KnowledgeQuery>,
) -> Result<HttpResponse, AppError> {
    let hits = state.continuum.recall_fact(body.into_inner())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(hits)))
}

// ── 11. Platform-engineering team in a box ──────────────────────────────

/// Stake a golden path as a proof-gated, continuously-verified capability.
async fn enroll_golden_path(
    state: web::Data<AppState>,
    body: web::Json<GoldenPathRequest>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let req = body.into_inner();
    let report = run_blocking_io(move || continuum.enroll_golden_path(req))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── Shared adversary (continuous red team / partition hunter) ────────────

/// Drive one proposer/refuter round; its attack budget is spent across the whole
/// open claim pool, hardening every enrolled domain at once. Needs a reasoner.
async fn run_adversary(
    state: web::Data<AppState>,
    body: web::Json<AdversaryRequest>,
) -> Result<HttpResponse, AppError> {
    let report = state.continuum.run_adversary(body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

// ── The continuous layer (Sentinel) surfaced ────────────────────────────

#[derive(Debug, Deserialize)]
struct WatchesQuery {
    #[serde(default)]
    domain: Option<String>,
}

/// Every claim under continuous re-verification, optionally filtered by domain.
async fn watches(
    state: web::Data<AppState>,
    query: web::Query<WatchesQuery>,
) -> Result<HttpResponse, AppError> {
    let domain = match query.domain.as_deref() {
        Some(value) => Some(parse_domain(value)?),
        None => None,
    };
    let list = state.continuum.watches(domain)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(list)))
}

async fn get_watch(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let watch = state.continuum.get_watch(&path)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(watch)))
}

/// Re-verify a single watch now (e.g. to confirm a fix re-closed a guarantee).
async fn sweep_one(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let id = path.into_inner();
    let watch = run_blocking_io(move || continuum.sweep_one(&id))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(watch)))
}

/// Re-verify every active watch now and record any drift — the manual pulse of
/// the continuous loop the background sweeper runs on a timer.
async fn sweep(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let continuum = state.continuum.clone();
    let report = run_blocking_io(move || continuum.sweep())?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

#[derive(Debug, Deserialize)]
struct DriftQuery {
    #[serde(default)]
    watch: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

/// The drift ledger — the timestamped history of when standing guarantees broke
/// or recovered, optionally scoped to one watch.
async fn drift(
    state: web::Data<AppState>,
    query: web::Query<DriftQuery>,
) -> Result<HttpResponse, AppError> {
    let limit = query.limit.unwrap_or(200);
    let events = state.continuum.drift_log(query.watch.as_deref(), limit)?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(events)))
}

fn parse_domain(value: &str) -> Result<Domain, AppError> {
    Ok(match value.trim().to_ascii_lowercase().as_str() {
        "reproducibility" | "repro" => Domain::Reproducibility,
        "supply_chain" | "supply-chain" | "supply" => Domain::SupplyChain,
        "compliance" => Domain::Compliance,
        "spec_mining" | "spec-mining" | "spec" => Domain::SpecMining,
        "impossibility" => Domain::Impossibility,
        "guardrail" | "guardrails" => Domain::Guardrail,
        "ai_safety" | "ai-safety" | "aisafety" => Domain::AiSafety,
        "data_contract" | "data-contract" | "data" => Domain::DataContract,
        "equivalence" | "migration" => Domain::Equivalence,
        "knowledge" | "kb" => Domain::Knowledge,
        "golden_path" | "golden-path" | "golden" | "platform" => Domain::GoldenPath,
        "other" => Domain::Other,
        other => return Err(AppError::Validation(format!("unknown domain: {other}"))),
    })
}

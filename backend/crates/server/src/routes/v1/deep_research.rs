//! The Deep Research HTTP surface — multi-agent investigation.
//!
//! One call decomposes the question, spawns a sub-agent (a verified agentic-loop
//! run) per sub-question to scrape exact data from primary sources, optionally
//! cross-checks each datum with a second independent sub-agent, and synthesizes a
//! cited answer. It drives several model + device rounds, so it is awaited
//! directly and is long-running by design (the same shape as spec-mining).

use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    deep_research::ResearchRequest,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/research/deep", web::post().to(deep_research));
}

/// Run a deep, multi-agent investigation and return the synthesized, cited report.
/// Body: `{ "question": "…", "max_subquestions": 3, "cross_check": false }`.
async fn deep_research(
    state: web::Data<AppState>,
    body: web::Json<ResearchRequest>,
) -> Result<HttpResponse, AppError> {
    let report = state.deep_research.investigate(body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(report)))
}

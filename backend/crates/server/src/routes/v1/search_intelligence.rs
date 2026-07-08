use actix_web::{HttpResponse, web};
use astra_core::{
    AppState,
    common::{ApiResponse, AppError},
    search_intelligence::DeepSearchRequest,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/search/intelligence", web::post().to(search_intelligence))
        .route(
            "/search/intelligence/stats",
            web::get().to(search_intelligence_stats),
        )
        .route(
            "/search/intelligence/frontier",
            web::get().to(search_intelligence_frontier),
        )
        .route(
            "/search/intelligence/tick",
            web::post().to(search_intelligence_tick),
        );
}

async fn search_intelligence(
    state: web::Data<AppState>,
    body: web::Json<DeepSearchRequest>,
) -> Result<HttpResponse, AppError> {
    // Deep search can perform live governed fetches; run it on the blocking pool
    // so it never stalls an async worker.
    let service = state.search_intelligence.clone();
    let request = body.into_inner();
    let response = web::block(move || service.execute(request))
        .await
        .map_err(|error| {
            AppError::Internal(format!("search intelligence task failed: {error}"))
        })??;
    let sandbox = state.sandbox.guard(
        state.sandbox.action_for_activity(
            "search_intelligence",
            response.executive_summary.clone(),
            vec![
                "governed_crawler".into(),
                "formula_calculator".into(),
                "citation_ranker".into(),
                "certificate_builder".into(),
            ],
            serde_json::json!({
                "formula_version": response.formula_report.formula_version,
                "formula_count": response.formula_report.metrics.len(),
                "autonomous": response.crawl_plan.autonomous,
                "protocol": "httpa",
            }),
            true,
        ),
        false,
    )?;
    super::remember(
        &state,
        astra_core::chronicle::EpisodeKind::Learning,
        format!("deep search synthesized: {}", response.executive_summary),
        "search_intelligence",
        None,
        vec!["research".into(), "search".into()],
        0.6,
    );
    Ok(
        HttpResponse::Accepted().json(ApiResponse::ok(serde_json::json!({
            "result": response,
            "sandbox_receipt": sandbox,
        }))),
    )
}

async fn search_intelligence_stats(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.search_intelligence.stats()))
}

async fn search_intelligence_frontier(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(state.search_intelligence.frontier()))
}

async fn search_intelligence_tick(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Accepted().json(ApiResponse::ok(state.search_intelligence.tick(true)))
}

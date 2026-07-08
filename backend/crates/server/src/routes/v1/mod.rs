use actix_web::web;
use astra_core::{
    AppState,
    chronicle::{EpisodeKind, RecordEpisodeRequest},
    common::TenantScope,
};

mod active_inference;
mod agent;
mod architect;
mod artifacts;
mod asc2;
mod assistant;
mod autonomy;
mod axiom;
mod ceo;
mod chronicle;
mod connectors;
mod continuum;
mod crucible;
mod curriculum;
mod deep_research;
mod device;
mod evals;
mod experiments;
mod forge;
mod genome;
mod governance;
mod httpa;
mod learning;
mod neural_orchestration;
mod nexus;
mod noesis;
mod os_guardian;
mod proof;
mod render;
mod research;
mod runtime;
mod sandbox;
mod search_intelligence;
mod system;
mod weave;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1")
            .configure(system::configure)
            .configure(active_inference::configure)
            .configure(agent::configure)
            .configure(architect::configure)
            .configure(asc2::configure)
            .configure(assistant::configure)
            .configure(autonomy::configure)
            .configure(axiom::configure)
            .configure(ceo::configure)
            .configure(artifacts::configure)
            .configure(chronicle::configure)
            .configure(connectors::configure)
            .configure(continuum::configure)
            .configure(crucible::configure)
            .configure(curriculum::configure)
            .configure(deep_research::configure)
            .configure(device::configure)
            .configure(evals::configure)
            .configure(experiments::configure)
            .configure(forge::configure)
            .configure(genome::configure)
            .configure(governance::configure)
            .configure(httpa::configure)
            .configure(learning::configure)
            .configure(nexus::configure)
            .configure(neural_orchestration::configure)
            .configure(noesis::configure)
            .configure(os_guardian::configure)
            .configure(proof::configure)
            .configure(render::configure)
            .configure(research::configure)
            .configure(sandbox::configure)
            .configure(search_intelligence::configure)
            .configure(weave::configure)
            .configure(runtime::configure),
    );
}

/// Best-effort chronicle recording used by every route group, so each
/// subsystem's activity lands in the shared continuity memory. A full or
/// failing chronicle must never fail the request that triggered it.
pub(crate) fn remember(
    state: &AppState,
    kind: EpisodeKind,
    content: String,
    source: &str,
    source_ref: Option<String>,
    tags: Vec<String>,
    importance: f64,
) {
    let result = state.chronicle.record(RecordEpisodeRequest {
        kind,
        content,
        rationale: None,
        source: Some(source.to_string()),
        source_ref,
        tags,
        importance: Some(importance),
        due_at_ms: None,
        tenant_scope: TenantScope::Global,
    });
    if let Err(error) = result {
        tracing::warn!(%error, source, "chronicle recording skipped");
    }
}

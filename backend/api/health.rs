// Health check and feature status
use super::routes::EngineState;
use crate::error::AstraError;
use crate::httpa::handlers::AppState;
use actix_web::{web, HttpResponse, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn health_check(
    engine: web::Data<Arc<RwLock<EngineState>>>,
    app: web::Data<Arc<RwLock<AppState>>>,
) -> Result<HttpResponse, AstraError> {
    let engine = engine.read().await;
    let app = app.read().await;
    let uptime = chrono::Utc::now()
        .signed_duration_since(app.start_time)
        .num_seconds();
    let readiness = engine.personal_assistant.readiness();
    let degraded_checks = readiness
        .checks
        .iter()
        .filter(|check| {
            !matches!(
                check.level,
                crate::assistant::runtime::ReadinessLevel::Enforced
            )
        })
        .map(|check| {
            serde_json::json!({
                "name": check.name,
                "level": format!("{:?}", check.level).to_lowercase(),
                "detail": check.detail,
            })
        })
        .collect::<Vec<_>>();
    let operational_warnings = engine.config.operational_warnings();
    let status = if degraded_checks.is_empty() && operational_warnings.is_empty() {
        "ready"
    } else {
        "degraded"
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": status,
        "engine": "astra_core_engine",
        "version": "2.0.0",
        "protocol": "HTTPA/1.0",
        "uptime_seconds": uptime,
        "resource_profile": engine.config.resource_profile_name(),
        "execution_contract": {
            "cpu_only": engine.config.resources.cpu_only,
            "llm_enabled": engine.config.resources.llm_enabled,
            "gpu_enabled": engine.config.resources.gpu_enabled,
            "low_memory_mode": engine.config.resources.low_memory_mode,
            "max_background_jobs": engine.config.resources.max_background_jobs,
        },
        "components": {
            "httpa_bus": "operational",
            "crypto_engine": "operational",
            "agent_system": if engine.config.openclaw.enabled { "operational" } else { "disabled" },
            "assistant_runtime": "operational",
            "blockchain": "operational",
            "runtime_sessions": if app.config.engine_runtime.enabled { "operational" } else { "disabled" },
        },
        "sessions": app.session_manager.get_stats(),
        "chain": engine.chain.get_chain_info(),
        "assistant_readiness": {
            "target_grade": readiness.target_grade,
            "warning_count": readiness.warnings.len(),
            "degraded_checks": degraded_checks,
        },
        "warnings": operational_warnings,
    })))
}

pub async fn feature_status(
    engine: web::Data<Arc<RwLock<EngineState>>>,
) -> Result<HttpResponse, AstraError> {
    let (
        search_runtime,
        memory_count,
        mev_stats,
        identity_stats,
        compute_stats,
        feature_config,
        resources,
    ) = {
        let engine = engine.read().await;
        (
            Arc::clone(&engine.search_runtime),
            engine.memory.count(),
            engine.mev_shield.get_stats(),
            engine.identity.get_stats(),
            engine.compute.get_stats(),
            engine.config.features.clone(),
            engine.config.resources.clone(),
        )
    };
    let search_runtime = search_runtime
        .read()
        .map_err(|_| AstraError::Internal("search runtime lock poisoned".into()))?;
    let features = vec![
        feature_entry(
            "cognitive_memory_sync",
            "Proof-Carrying Cognitive Memory Sync",
            feature_config.cognitive_memory,
            "SIS-committed acquisition memories with quantum-secure integrity",
            serde_json::json!({ "memories": memory_count }),
        ),
        feature_entry(
            "swarm_browsing",
            "Multi-Agent Swarm Research",
            feature_config.swarm_browsing,
            "N-agent consensus for complex queries via Agent-Solver calculus",
            serde_json::json!({ "total_queries": search_runtime.swarm.total_queries() }),
        ),
        feature_entry(
            "zk_mev_shield",
            "ZK Web MEV Shield",
            feature_config.zk_mev_shield,
            "Lattice commitments to prevent front-running on purchases",
            serde_json::json!(mev_stats),
        ),
        feature_entry(
            "truth_verification",
            "Continuous Truth Verification",
            feature_config.truth_verification,
            "Paragraph-level verification with contradiction tracking",
            serde_json::json!(search_runtime.truth.get_stats()),
        ),
        feature_entry(
            "pq_identity",
            "Post-Quantum Stateless Identity",
            feature_config.pq_identity,
            "Ephemeral signing keys with forward-secure rotation",
            serde_json::json!(identity_stats),
        ),
        feature_entry(
            "delegated_compute",
            "Delegated Compute Intent Streaming",
            feature_config.delegated_compute,
            "Offload-capable compute flows guarded by runtime policy",
            serde_json::json!(compute_stats),
        ),
        feature_entry(
            "cpu_only_contract",
            "CPU-only Runtime Contract",
            resources.cpu_only && !resources.gpu_enabled,
            "Confirms the service can run without a GPU dependency",
            serde_json::json!({
                "profile": resources.profile.as_str(),
                "gpu_enabled": resources.gpu_enabled,
                "llm_enabled": resources.llm_enabled,
            }),
        ),
        feature_entry(
            "low_memory_mode",
            "Low-Memory Envelope",
            resources.low_memory_mode,
            "Keeps background concurrency and retention within compact hardware budgets",
            serde_json::json!({
                "max_background_jobs": resources.max_background_jobs,
            }),
        ),
    ];
    let active_features = features
        .iter()
        .filter(|feature| feature["status"] == "active")
        .count();

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "features": features,
        "total_features": features.len(),
        "active_features": active_features,
        "all_active": active_features == features.len(),
        "resource_profile": resources.profile.as_str(),
        "engine": "astra_core_engine/2.0",
    })))
}

fn feature_entry(
    id: &str,
    name: &str,
    enabled: bool,
    description: &str,
    stats: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "status": if enabled { "active" } else { "disabled" },
        "description": description,
        "stats": if enabled { stats } else { serde_json::json!({}) },
    })
}

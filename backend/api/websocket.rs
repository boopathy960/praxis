// ─────────────────────────────────────────────────────────────
// WebSocket — HTTPA/1.0 Streaming Handler
// ─────────────────────────────────────────────────────────────
// Full-duplex HTTPA frame streaming over WebSocket for real-time
// cognitive streams, memory deltas, and intent progress updates.

use super::routes::EngineState;
use crate::httpa::handlers::AppState;
use crate::httpa::protocol::{HttpaFrame, HttpaFrameType};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_ws::Message;
use log::{debug, info};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Handle incoming WebSocket connections for HTTPA streaming.
pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    engine: web::Data<Arc<RwLock<EngineState>>>,
    app: web::Data<Arc<RwLock<AppState>>>,
) -> Result<HttpResponse, Error> {
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?;

    info!("🔌 HTTPA WebSocket: new connection established");

    actix_rt::spawn(async move {
        // Send welcome frame
        let welcome = serde_json::json!({
            "type": "httpa_welcome",
            "protocol": "HTTPA/1.0",
            "engine": "astra_core_engine/1.0",
            "capabilities": ["cognitive_stream", "memory_delta", "intent_progress",
                             "swarm_updates", "verification_results", "heartbeat"],
            "timestamp": chrono::Utc::now().timestamp_millis(),
        });
        let _ = session
            .text(serde_json::to_string(&welcome).unwrap_or_default())
            .await;

        while let Some(Ok(msg)) = futures::StreamExt::next(&mut msg_stream).await {
            match msg {
                Message::Text(text) => {
                    let response = process_ws_frame(&text, &engine, &app).await;
                    let _ = session.text(response).await;
                }
                Message::Ping(bytes) => {
                    let _ = session.pong(&bytes).await;
                }
                Message::Close(reason) => {
                    info!("🔌 HTTPA WebSocket: connection closed: {:?}", reason);
                    break;
                }
                _ => {}
            }
        }
        let _ = session.close(None).await;
    });

    Ok(response)
}

/// Process an incoming WebSocket text frame as an HTTPA command.
async fn process_ws_frame(
    text: &str,
    engine: &web::Data<Arc<RwLock<EngineState>>>,
    app: &web::Data<Arc<RwLock<AppState>>>,
) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => {
            return serde_json::to_string(&serde_json::json!({
                "type": "httpa_error",
                "error": "invalid_json",
                "message": "Failed to parse frame as JSON",
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }))
            .unwrap_or_default();
        }
    };

    let frame_type = parsed
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match frame_type {
        // ── Heartbeat ──
        "heartbeat" => {
            let app = app.read().await;
            serde_json::to_string(&serde_json::json!({
                "type": "heartbeat_ack",
                "status": "alive",
                "uptime_seconds": chrono::Utc::now()
                    .signed_duration_since(app.start_time).num_seconds(),
                "bus_metrics": app.bus.get_metrics(),
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }))
            .unwrap_or_default()
        }

        // ── Cognitive Stream Request ──
        "cognitive_stream" => {
            let engine = engine.read().await;
            let memories = engine.memory.get_all();
            let recent: Vec<_> = memories.iter().rev().take(10).collect();

            serde_json::to_string(&serde_json::json!({
                "type": "cognitive_stream_response",
                "data": {
                    "total_memories": engine.memory.count(),
                    "recent": recent,
                },
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }))
            .unwrap_or_default()
        }

        // ── Memory Delta ──
        "memory_delta" => {
            let url = parsed.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let intent = parsed.get("intent").and_then(|v| v.as_str()).unwrap_or("");
            let context = parsed.get("context").and_then(|v| v.as_str()).unwrap_or("");

            let mut engine = engine.write().await;
            match engine.memory.commit_memory(url, intent, context) {
                Ok(result) => serde_json::to_string(&serde_json::json!({
                    "type": "memory_delta_ack",
                    "data": result,
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                }))
                .unwrap_or_default(),
                Err(e) => serde_json::to_string(&serde_json::json!({
                    "type": "httpa_error",
                    "error": "memory_commit_failed",
                    "message": e.to_string(),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                }))
                .unwrap_or_default(),
            }
        }

        // ── Intent Progress ──
        "intent_submit" => {
            let action = parsed
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("browse");
            let domain = parsed
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("web");

            // Route through bus
            {
                let app = app.read().await;
                let frame = HttpaFrame::new(
                    HttpaFrameType::Intent,
                    "ws_client",
                    serde_json::json!({"action": action, "domain": domain}),
                )
                .with_intent(domain);
                app.bus.emit(frame).await;
            }

            let mut engine = engine.write().await;
            let result = engine.agents.process_intent(action, &[]);

            serde_json::to_string(&serde_json::json!({
                "type": "intent_progress",
                "status": "completed",
                "data": result,
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }))
            .unwrap_or_default()
        }

        // ── Feature Status Query ──
        "feature_query" => {
            let (search_runtime, memory_count, mev_stats, identity_stats, compute_stats) = {
                let engine = engine.read().await;
                (
                    Arc::clone(&engine.search_runtime),
                    engine.memory.count(),
                    engine.mev_shield.get_stats(),
                    engine.identity.get_stats(),
                    engine.compute.get_stats(),
                )
            };
            let search_runtime = match search_runtime.read() {
                Ok(runtime) => runtime,
                Err(_) => {
                    return serde_json::to_string(&serde_json::json!({
                        "type": "httpa_error",
                        "error": "search_runtime_lock_poisoned",
                        "timestamp": chrono::Utc::now().timestamp_millis(),
                    }))
                    .unwrap_or_default();
                }
            };
            serde_json::to_string(&serde_json::json!({
                "type": "feature_status",
                "data": {
                    "memory": { "count": memory_count },
                    "swarm": { "queries": search_runtime.swarm.total_queries() },
                    "mev_shield": mev_stats,
                    "truth": search_runtime.truth.get_stats(),
                    "identity": identity_stats,
                    "compute": compute_stats,
                },
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }))
            .unwrap_or_default()
        }

        // ── Unknown Frame ──
        _ => {
            debug!("🔌 HTTPA WS: unknown frame type '{}'", frame_type);
            serde_json::to_string(&serde_json::json!({
                "type": "httpa_error",
                "error": "unknown_frame_type",
                "received": frame_type,
                "supported": ["heartbeat", "cognitive_stream", "memory_delta",
                              "intent_submit", "feature_query"],
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }))
            .unwrap_or_default()
        }
    }
}

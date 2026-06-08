use std::collections::BTreeMap;
use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse, Result};
use log::{info, warn};
use tokio::sync::RwLock;

use super::bus::HttpaBus;
use super::control_plane::{HttpaControlPlane, IntentLedgerEntry};
use super::envelope::*;
use super::protocol::*;
use super::session::SessionManager;
use crate::api::routes::{EngineState, SearchIntelligenceRuntime};
use crate::assistant::one_brain::{BrainExecuteRequest, BrainModeHint};
use crate::brain::{
    LightningLoop, OptimizationBatch, OptimizationObjective, SpanType, TraceSpan, TrajectoryTrace,
};
use crate::chain::ai_contract::{AIContractInvokeRequest, ContractDecision};
use crate::chain::value_protocol::{
    CognitionRewardRequest, ComplianceMode, DeviceBindingRequest, EnterpriseRiskTier,
    FinancialIntentCommitRequest, FinancialIntentRouteRequest, GuardedPaymentRequest,
    MarketProviderQuote, PaymentDisputeRequest, PaymentDisputeResolutionRequest,
    PrivacyAttestationRequest, PrivacyPreservationMode, QualityOfServiceTier, SettlementRail,
    SmartPaymentKind, StreamSettlementRequest, StreamValueRequest,
};
use crate::crypto::hash::sha3_256_hex;
use crate::error::{AstraError, AstraResult};
use crate::openclaw::agents::{MultiAgentExecution, UpdateRuntimeOptionsRequest};
use crate::openclaw::boot::BootJobRequest;
use crate::openclaw::gateway::{OpenClawGateway, RegisterGatewayAgentRequest};
use crate::runtime::SessionManager as RuntimeSessionManager;

/// Shared application state passed to all handlers.
pub struct AppState {
    pub session_manager: SessionManager,
    pub runtime_sessions: RuntimeSessionManager,
    pub config: crate::config::AppConfig,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub bus: HttpaBus,
    pub control_plane: HttpaControlPlane,
    pub learning_loop: LightningLoop,
    pub openclaw_gateway: OpenClawGateway,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::lightning_loop::LightningLoop;
    use crate::chain::chain::Chain;
    use actix_web::test::TestRequest;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_store_dir(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("astra-httpa-{label}-{nonce}"))
    }

    fn sample_intent(action: &str, domain: &str, parameters: serde_json::Value) -> IntentRequest {
        IntentRequest {
            action: action.into(),
            domain: domain.into(),
            priority: None,
            parameters,
            session_id: "test-session".into(),
            session_token: Some("test-token".into()),
            execution: HttpaExecutionPreferences::default(),
        }
    }

    #[test]
    fn handshake_validation_rejects_oversized_capability_sets() {
        let request = HandshakeRequest {
            agent_id: "agent-a".into(),
            agent_name: "Agent A".into(),
            capabilities: (0..65).map(|index| format!("cap-{index}")).collect(),
            domains: vec!["search".into()],
            resource_profile: json!({}),
            requested_clearance: "standard".into(),
            session_defaults: HttpaExecutionPreferences::default(),
        };

        assert!(matches!(
            validate_handshake_request(&request),
            Err(AstraError::HandshakeFailed(_))
        ));
    }

    #[test]
    fn admin_requests_require_bearer_token_when_configured() {
        let mut config = crate::config::AppConfig::personal_defaults();
        config.server.admin_token = Some("super-secret".into());

        let authorized = TestRequest::default()
            .insert_header(("Authorization", "Bearer super-secret"))
            .to_http_request();
        authorize_admin_request(&authorized, &config)
            .expect("matching bearer token should authorize request");

        let unauthorized = TestRequest::default().to_http_request();
        assert!(matches!(
            authorize_admin_request(&unauthorized, &config),
            Err(AstraError::AuthRequired)
        ));
    }

    #[test]
    fn admin_requests_accept_header_token_when_configured() {
        let mut config = crate::config::AppConfig::personal_defaults();
        config.server.admin_token = Some("super-secret".into());

        let authorized = TestRequest::default()
            .insert_header((HEADER_ADMIN_TOKEN, "super-secret"))
            .to_http_request();

        authorize_admin_request(&authorized, &config)
            .expect("matching header token should authorize request");
    }

    #[test]
    fn adaptive_search_intent_detection_covers_search_and_analysis_actions() {
        assert!(is_adaptive_search_intent(&sample_intent(
            "research",
            "knowledge",
            json!({"query": "battery chemistry trends"}),
        )));
        assert!(is_adaptive_search_intent(&sample_intent(
            "summarize",
            "search",
            json!({"query": "browser performance"}),
        )));
        assert!(is_adaptive_search_intent(&sample_intent(
            "analyze",
            "insights",
            json!({"query": "news sentiment"}),
        )));
        assert!(!is_adaptive_search_intent(&sample_intent(
            "click",
            "web",
            json!({"url": "https://example.com"}),
        )));
    }

    #[test]
    fn autonomous_value_intent_detection_covers_streams_and_wallet_binding() {
        assert!(is_autonomous_value_intent(&sample_intent(
            "stream_settle",
            "value",
            json!({"payer": "acct:alice", "payee": "acct:creator"}),
        )));
        assert!(is_autonomous_value_intent(&sample_intent(
            "bind_device",
            "iot",
            json!({"device_id": "fridge-7", "wallet_address": "acct:wallet"}),
        )));
        assert!(!is_autonomous_value_intent(&sample_intent(
            "navigate",
            "web",
            json!({"url": "https://example.com"}),
        )));
    }

    #[test]
    fn execution_plan_maps_ledger_modes_into_runtime_lanes() {
        let request = sample_intent("research", "search", json!({"query": "zero trust"}));
        let mut policy = HttpaExecutionPolicy::for_mode(HttpaExecutionMode::ResultOnly);
        policy.ledger_mode = HttpaLedgerMode::SovereignConsensus;
        let plan = build_execution_plan(&request, &policy);

        assert_eq!(plan.routing_lane, "autonomous-result");
        assert_eq!(plan.ledger_lane, "sovereign_consensus");
        assert_eq!(plan.identity_surface, "shielded_result_channel");
        assert_eq!(plan.transport.lane, "result_mesh");
        assert!(plan.transport.semantic_bootstrap);
    }

    #[test]
    fn httpa_search_intents_route_through_adaptive_search_engine() {
        let store_dir = test_store_dir("search-intent");
        let mut engine = EngineState::new(LightningLoop::new(store_dir.clone()));
        engine.chain = Chain::new();
        let request = sample_intent(
            "research",
            "search",
            json!({
                "query": "compare browser rendering engines and recent security trends",
                "include_news": true,
                "include_docs": true,
                "include_papers": true,
                "max_sources": 6
            }),
        );

        let result = maybe_execute_adaptive_search_intent(&mut engine, &request)
            .expect("adaptive search intent should execute")
            .expect("search intent should produce a response");

        assert_eq!(
            result.query,
            "compare browser rendering engines and recent security trends"
        );
        assert!(!result.answer.is_empty());
        assert!(!result.evidence.is_empty());
        assert!(result.ledger_proof.is_some());

        let _ = std::fs::remove_dir_all(store_dir);
    }

    #[test]
    fn httpa_cognitive_intents_can_delegate_to_one_brain() {
        let store_dir = test_store_dir("brain-intent");
        let mut engine = EngineState::new(LightningLoop::new(store_dir.clone()));
        let runtime_sessions =
            RuntimeSessionManager::new(32, 32, 16, "workspaces/astra", "rt-test");
        let _ = runtime_sessions.create_session(
            Some("test-session".into()),
            Some("workspaces/astra".into()),
            vec!["httpa".into()],
        );
        let request = sample_intent(
            "plan",
            "assistant",
            json!({
                "task": "plan a verified backend rollout",
                "domain": "architecture"
            }),
        );

        let result = maybe_execute_brain_intent(&mut engine, &runtime_sessions, &request)
            .expect("brain intent should execute")
            .expect("brain intent should return a result");

        assert_eq!(
            result.response.stage,
            crate::assistant::one_brain::BrainStage::Completed
        );
        assert_eq!(
            result.response.plan.execution_plan.goal,
            "plan a verified backend rollout"
        );

        let _ = std::fs::remove_dir_all(store_dir);
    }

    #[test]
    fn httpa_value_intents_route_through_value_protocol() {
        let store_dir = test_store_dir("value-intent");
        let mut engine = EngineState::new(LightningLoop::new(store_dir.clone()));
        engine.chain = Chain::new();
        engine.chain.create_account("acct:alice", 25.0);
        let request = sample_intent(
            "stream_settle",
            "value",
            json!({
                "payer": "acct:alice",
                "payee": "acct:creator",
                "resource": "article:premium",
                "bytes_total": 262144,
                "price_per_mb": 0.4,
                "settlement_rail": "internal_ledger",
                "compliance": "internal_only",
                "privacy_mode": "counterparty_shielded",
                "qos_tier": "premium"
            }),
        );

        let result = maybe_execute_autonomous_value_intent(&mut engine, &request, "trace-value")
            .expect("value intent should execute")
            .expect("value intent should produce a response");

        assert_eq!(
            result
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .expect("kind should be present"),
            "stream_settlement"
        );
        assert!(result
            .get("submission")
            .and_then(|value| value.get("tx_hash"))
            .and_then(serde_json::Value::as_str)
            .is_some());

        let _ = std::fs::remove_dir_all(store_dir);
    }

    #[test]
    fn httpa_guarded_payment_intents_route_through_guarded_value_flow() {
        let store_dir = test_store_dir("guarded-payment-intent");
        let mut engine = EngineState::new(LightningLoop::new(store_dir.clone()));
        engine.chain = Chain::new();
        engine.chain.create_account("acct:alice", 25.0);
        let request = sample_intent(
            "pay_user",
            "payment",
            json!({
                "sender": "acct:alice",
                "receiver": "acct:bob",
                "amount": 2.5,
                "payment_kind": "peer_transfer",
                "settlement_rail": "internal_ledger",
                "compliance": "internal_only",
                "privacy_mode": "full_disclosure",
                "confidence": 0.94,
                "risk_score": 0.08
            }),
        );

        let result =
            maybe_execute_autonomous_value_intent(&mut engine, &request, "trace-guarded-payment")
                .expect("guarded payment intent should execute")
                .expect("guarded payment intent should produce a response");

        assert_eq!(
            result
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .expect("kind should be present"),
            "guarded_payment"
        );
        assert!(result
            .get("outcome")
            .and_then(|value| value.get("submission"))
            .and_then(|value| value.get("tx_hash"))
            .and_then(serde_json::Value::as_str)
            .is_some());

        let _ = std::fs::remove_dir_all(store_dir);
    }

    #[test]
    fn httpa_payment_disputes_route_through_value_protocol() {
        let store_dir = test_store_dir("payment-dispute");
        let mut engine = EngineState::new(LightningLoop::new(store_dir.clone()));
        engine.chain = Chain::new();
        engine.chain.create_account("acct:alice", 25.0);

        let settle_request = sample_intent(
            "stream_settle",
            "value",
            json!({
                "payer": "acct:alice",
                "payee": "acct:creator",
                "resource": "article:premium",
                "bytes_total": 262144,
                "price_per_mb": 0.4,
                "settlement_rail": "internal_ledger",
                "compliance": "internal_only",
                "privacy_mode": "counterparty_shielded",
                "qos_tier": "premium"
            }),
        );
        let settlement = maybe_execute_autonomous_value_intent(
            &mut engine,
            &settle_request,
            "trace-dispute-seed",
        )
        .expect("value intent should execute")
        .expect("value intent should produce a response");
        let tx_hash = settlement
            .get("submission")
            .and_then(|value| value.get("tx_hash"))
            .and_then(serde_json::Value::as_str)
            .expect("submission tx_hash should be present")
            .to_string();

        let dispute_request = sample_intent(
            "open_dispute",
            "payment",
            json!({
                "actor": "acct:alice",
                "original_tx_hash": tx_hash,
                "reason": "service not delivered",
                "requested_amount": 0.1,
                "evidence_hashes": ["evidence:receipt-1"]
            }),
        );
        let dispute = maybe_execute_autonomous_value_intent(
            &mut engine,
            &dispute_request,
            "trace-dispute-open",
        )
        .expect("payment dispute should execute")
        .expect("payment dispute should produce a response");

        assert_eq!(
            dispute
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .expect("kind should be present"),
            "payment_dispute_open"
        );
        assert!(dispute
            .get("dispute")
            .and_then(|value| value.get("dispute_id"))
            .and_then(serde_json::Value::as_str)
            .is_some());

        let _ = std::fs::remove_dir_all(store_dir);
    }
}

/// POST /httpa/handshake — PQ-authenticated handshake.
pub async fn handshake(
    state: web::Data<Arc<RwLock<AppState>>>,
    body: web::Json<HandshakeRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let req = body.into_inner();
    validate_handshake_request(&req)?;

    info!(
        "HTTPA Handshake: agent='{}' name='{}' caps={:?}",
        req.agent_id, req.agent_name, req.capabilities
    );

    let clearance = match req.requested_clearance.as_str() {
        "public" => GovernanceClearance::Public,
        "elevated" => GovernanceClearance::Elevated,
        "privileged" => GovernanceClearance::Privileged,
        "sovereign" => GovernanceClearance::Sovereign,
        _ => GovernanceClearance::Standard,
    };

    let session =
        state
            .session_manager
            .create_session(&req.agent_id, req.capabilities.clone(), clearance)?;
    let session_defaults = resolve_session_defaults(&req, &state.config);
    state
        .control_plane
        .register_session(&session, &req.session_defaults);
    if state.config.engine_runtime.enabled {
        let runtime_session = state.runtime_sessions.create_session(
            Some(session.session_id.clone()),
            Some(format!(
                "{}/{}",
                state
                    .config
                    .engine_runtime
                    .default_workspace
                    .trim_end_matches('/'),
                req.agent_id.replace(['/', '\\', ':'], "-")
            )),
            req.capabilities.clone(),
        )?;
        state
            .control_plane
            .bind_runtime_session(&session.session_id, runtime_session.id);
    }
    if state.config.openclaw.enabled {
        let _ = state.openclaw_gateway.register_httpa_session(&session)?;
    }

    let response = HandshakeResponse {
        accepted: true,
        session_id: session.session_id,
        agent_id: req.agent_id,
        token: session.token,
        capabilities: req.capabilities,
        clearance,
        ttl_seconds: state.config.httpa.session_ttl_secs,
        protocol_version: HTTPA_VERSION.to_string(),
        session_defaults,
        supported_capabilities: supported_capabilities(&state.config),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /httpa/intent — Submit an intent for processing.
pub async fn submit_intent(
    app_state: web::Data<Arc<RwLock<AppState>>>,
    engine_state: web::Data<Arc<RwLock<EngineState>>>,
    http: HttpRequest,
    body: web::Json<IntentRequest>,
) -> Result<HttpResponse, AstraError> {
    let app = app_state.read().await;
    let req = body.into_inner();

    authorize_session_request(&http, &req, &app)?;
    app.session_manager.touch_session(&req.session_id)?;
    app.control_plane.touch_session(&req.session_id);
    if app.config.openclaw.enabled {
        app.openclaw_gateway
            .record_intent_activity(&req.session_id, &req.action);
    }

    let start = std::time::Instant::now();
    let intent_id = uuid::Uuid::new_v4().to_string()[..12].to_string();
    let trace_id = uuid::Uuid::new_v4().to_string()[..12].to_string();
    let priority = parse_priority(req.priority.as_deref());

    info!(
        "HTTPA Intent: action='{}' domain='{}' session='{}'",
        req.action, req.domain, req.session_id
    );

    let session_record = app.control_plane.get_session(&req.session_id);
    let runtime_session_id = session_record
        .as_ref()
        .and_then(|session| session.runtime_session_id.clone());
    let execution_policy = resolve_execution_policy(session_record.as_ref(), &req, &app.config);
    let execution_plan = build_execution_plan(&req, &execution_policy);
    let execution_summary = HttpaExecutionSummary {
        mode: execution_policy.mode,
        privacy: execution_policy.privacy,
        performance_tier: execution_policy.performance_tier,
        ledger_mode: execution_policy.ledger_mode,
        transport: execution_policy.transport_profile(),
        policy: execution_policy.clone(),
    };

    if let Some(runtime_session_id) = &runtime_session_id {
        let _ = app.runtime_sessions.append_user_text(
            runtime_session_id,
            format!(
                "HTTPA intent received. action={} domain={} mode={:?} privacy={:?} performance={:?} ledger={:?} parameters={}",
                req.action,
                req.domain,
                execution_policy.mode,
                execution_policy.privacy,
                execution_policy.performance_tier,
                execution_policy.ledger_mode,
                serde_json::to_string(&req.parameters).unwrap_or_else(|_| "{}".into())
            ),
        );
    }

    app.control_plane.record_intent(IntentLedgerEntry {
        intent_id: intent_id.clone(),
        session_id: req.session_id.clone(),
        action: req.action.clone(),
        domain: req.domain.clone(),
        priority,
        trace_id: trace_id.clone(),
        status: "running".into(),
        execution_mode: execution_policy.mode,
        privacy_mode: execution_policy.privacy,
        performance_tier: execution_policy.performance_tier,
        ledger_mode: execution_policy.ledger_mode,
        chain_receipt_required: execution_policy.chain_receipt_required,
        user_visible: execution_policy.user_visible,
        created_at: chrono::Utc::now(),
    });

    let frame = HttpaFrame::new(
        HttpaFrameType::Intent,
        req.session_id.clone(),
        serde_json::json!({
            "action": &req.action,
            "domain": &req.domain,
            "parameters": &req.parameters,
            "execution": &execution_summary,
            "plan": &execution_plan,
        }),
    )
    .with_session(&req.session_id)
    .with_intent(&req.domain)
    .with_priority(priority);

    app.bus.emit(frame).await;

    let contract_receipt = if let Some(contract_id) = req
        .parameters
        .get("contract_id")
        .and_then(serde_json::Value::as_str)
    {
        let requested_tools = req
            .parameters
            .get("tools")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(ToString::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let evidence = req
            .parameters
            .get("evidence")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(ToString::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let input_digest = req
            .parameters
            .get("input_digest")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                sha3_256_hex(
                    serde_json::to_string(&req.parameters)
                        .unwrap_or_else(|_| "{}".into())
                        .as_bytes(),
                )
            });
        let mut engine = engine_state.write().await;
        Some(
            engine.chain.invoke_contract(AIContractInvokeRequest {
                contract_id: contract_id.to_string(),
                caller: format!("session:{}", req.session_id),
                action: req.action.clone(),
                domain: req.domain.clone(),
                requested_tools,
                estimated_cost: req
                    .parameters
                    .get("estimated_cost")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
                confidence: req
                    .parameters
                    .get("confidence")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0),
                risk_score: req
                    .parameters
                    .get("risk_score")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
                input_digest,
                trace_id: Some(trace_id.clone()),
                session_id: Some(req.session_id.clone()),
                evidence,
                context: req
                    .parameters
                    .get("contract_context")
                    .and_then(serde_json::Value::as_object)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|(key, value)| {
                                (
                                    key.clone(),
                                    value
                                        .as_str()
                                        .map(ToString::to_string)
                                        .unwrap_or_else(|| value.to_string()),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                metadata: req.parameters.clone(),
            })?,
        )
    } else {
        None
    };

    if let Some(receipt) = contract_receipt.as_ref() {
        if matches!(
            receipt.decision,
            ContractDecision::Deny | ContractDecision::Review
        ) {
            let status = match receipt.decision {
                ContractDecision::Deny => "denied",
                ContractDecision::Review => "review_required",
                ContractDecision::Allow => "completed",
            };
            app.control_plane.complete_intent(&intent_id, status);

            if let Some(runtime_session_id) = &runtime_session_id {
                let _ = app.runtime_sessions.append_assistant_text(
                    runtime_session_id,
                    format!(
                        "Intent gated by chain contract. status={} action={} domain={}",
                        status, req.action, req.domain
                    ),
                );
            }

            return Ok(HttpResponse::Ok().json(IntentResponse {
                intent_id,
                status: status.to_string(),
                result: serde_json::json!({
                    "action": req.action,
                    "domain": req.domain,
                    "execution": execution_summary,
                    "plan": execution_plan,
                    "contract_receipt": receipt,
                    "status": status,
                }),
                trace_id,
                duration_ms: start.elapsed().as_millis() as u64,
            }));
        }
    }

    let learning_problem = build_learning_problem(&req.action, &req.domain, &req.parameters);
    let learning_domain =
        infer_learning_domain_from_intent(&req.action, &req.domain, &req.parameters);
    let openclaw_policy = if app.config.openclaw.enabled {
        Some(
            app.learning_loop
                .reasoning_policy(&learning_problem, Some(&learning_domain)),
        )
    } else {
        None
    };

    let openclaw_execution = if app.config.openclaw.enabled {
        Some(app.openclaw_gateway.execute_intent(
            &req.session_id,
            &req.action,
            &req.domain,
            &req.parameters,
            openclaw_policy.as_ref(),
        )?)
    } else {
        None
    };

    if let Some(execution) = openclaw_execution.as_ref() {
        ingest_openclaw_execution(&app.learning_loop, &req, execution, &learning_domain);
    }

    let mut engine = engine_state.write().await;
    let value_result = maybe_execute_autonomous_value_intent(&mut engine, &req, &trace_id)?;
    let search_result = if value_result.is_none() {
        maybe_execute_adaptive_search_intent(&mut engine, &req)?
    } else {
        None
    };
    let brain_result = if value_result.is_none() && search_result.is_none() {
        maybe_execute_brain_intent(&mut engine, &app.runtime_sessions, &req)?
    } else {
        None
    };
    let result = if let Some(value_result) = value_result.as_ref() {
        value_result.clone()
    } else if let Some(search_result) = search_result.as_ref() {
        serde_json::to_value(search_result)?
    } else if let Some(brain_result) = brain_result.as_ref() {
        serde_json::to_value(&brain_result.response)?
    } else {
        engine.agents.process_intent(&req.action, &[])
    };
    let chain_submission = engine
        .chain
        .record_httpa_intent(
            &req.session_id,
            &req.action,
            &req.domain,
            "completed",
            &trace_id,
            serde_json::json!({
                "parameters": req.parameters.clone(),
                "execution_policy": &execution_policy,
                "value_result_kind": value_result
                    .as_ref()
                    .and_then(|value| value.get("kind"))
                    .and_then(serde_json::Value::as_str),
                "search_ledger_trace": search_result
                    .as_ref()
                    .and_then(|value| value.ledger_proof.as_ref().map(|proof| proof.trace_id.clone())),
            }),
        )
        .ok();
    let agent_submission = engine
        .chain
        .record_agent_execution(
            "agent_controller",
            &req.action,
            &result,
            search_result
                .as_ref()
                .map(|value| value.confidence)
                .or_else(|| {
                    req.parameters
                        .get("confidence")
                        .and_then(serde_json::Value::as_f64)
                })
                .unwrap_or(0.75),
            &trace_id,
        )
        .ok();
    app.control_plane.complete_intent(&intent_id, "completed");

    let enriched = serde_json::json!({
        "action": req.action,
        "domain": req.domain,
        "status": "processed",
        "engine": "astra_core_engine/1.0",
        "execution": execution_summary,
        "plan": execution_plan,
        "agent_result": result,
        "value_result": value_result,
        "search_result": search_result,
        "brain_result": brain_result.as_ref().map(|value| &value.response),
        "openclaw_execution": openclaw_execution,
        "bus_metrics": app.bus.get_metrics(),
        "control_plane": app.control_plane.snapshot(),
        "contract_receipt": contract_receipt,
        "chain_submission": chain_submission,
        "agent_submission": agent_submission,
    });

    let response = IntentResponse {
        intent_id,
        status: "completed".to_string(),
        result: enriched,
        trace_id,
        duration_ms: start.elapsed().as_millis() as u64,
    };

    if let Some(runtime_session_id) = &runtime_session_id {
        let _ = app.runtime_sessions.append_assistant_text(
            runtime_session_id,
            format!(
                "Intent completed. status=completed action={} domain={} mode={:?} privacy={:?} ledger={:?} openclaw_execution={} value={} search={}",
                req.action,
                req.domain,
                execution_policy.mode,
                execution_policy.privacy,
                execution_policy.ledger_mode,
                openclaw_execution.is_some(),
                value_result.is_some(),
                search_result.is_some()
            ),
        );
        let _ = app
            .runtime_sessions
            .set_stop_reason(runtime_session_id, "intent_completed");
    }

    Ok(HttpResponse::Ok().json(response))
}

/// GET /httpa/status — Protocol status + active sessions.
pub async fn status(state: web::Data<Arc<RwLock<AppState>>>) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    let uptime = chrono::Utc::now()
        .signed_duration_since(state.start_time)
        .num_seconds();

    let response = serde_json::json!({
        "protocol": HTTPA_PROTOCOL_NAME,
        "version": HTTPA_VERSION,
        "full_name": HTTPA_FULL_NAME,
        "enabled": state.config.httpa.enabled,
        "session_auth_required": state.config.httpa.require_session_auth,
        "intent_retention_limit": state.config.httpa.intent_retention_limit,
        "cleanup_interval_secs": state.config.httpa.cleanup_interval_secs,
        "capabilities": supported_capabilities(&state.config),
        "sessions": state.session_manager.get_stats(),
        "runtime_sessions": state.runtime_sessions.fleet_summary(),
        "control_plane": state.control_plane.snapshot(),
        "openclaw": state.openclaw_gateway.snapshot(),
        "learning": state
            .learning_loop
            .summary(state.config.learning.summary_window),
        "uptime_seconds": uptime,
    });

    Ok(HttpResponse::Ok().json(response))
}

pub async fn capabilities(
    state: web::Data<Arc<RwLock<AppState>>>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    Ok(HttpResponse::Ok().json(supported_capabilities(&state.config)))
}

/// DELETE /httpa/sessions/{id} — Terminate a session.
pub async fn terminate_session(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let session_id = path.into_inner();
    let state = state.read().await;
    authorize_admin_or_session_request(&http, &state, &session_id)?;
    let runtime_session_id = state
        .control_plane
        .get_session(&session_id)
        .and_then(|session| session.runtime_session_id);
    state.session_manager.terminate_session(&session_id)?;
    state.control_plane.remove_session(&session_id);
    if let Some(runtime_session_id) = runtime_session_id {
        let _ = state
            .runtime_sessions
            .close_session(&runtime_session_id, "httpa_session_terminated");
    }
    if state.config.openclaw.enabled {
        state.openclaw_gateway.terminate_httpa_session(&session_id);
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "terminated": true,
        "session_id": session_id,
    })))
}

pub async fn control_plane_snapshot(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.control_plane.snapshot()))
}

pub async fn runtime_fleet_status(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.runtime_sessions.fleet_summary()))
}

pub async fn list_runtime_sessions(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.runtime_sessions.list_sessions(200)))
}

pub async fn get_runtime_session(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.runtime_sessions.get_session(&path.into_inner())?))
}

pub async fn ingest_learning_rollouts(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
    body: web::Json<OptimizationBatch>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    let summary = state.learning_loop.ingest_batch(body.into_inner())?;
    Ok(HttpResponse::Accepted().json(summary))
}

pub async fn learning_summary(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(
        state
            .learning_loop
            .summary(state.config.learning.summary_window),
    ))
}

pub async fn openclaw_overview(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.openclaw_gateway.snapshot()))
}

pub async fn list_openclaw_agents(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.openclaw_gateway.agents()))
}

pub async fn register_openclaw_agent(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
    body: web::Json<RegisterGatewayAgentRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    let agent = state.openclaw_gateway.register_agent(body.into_inner())?;
    Ok(HttpResponse::Created().json(agent))
}

pub async fn list_openclaw_sessions(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.openclaw_gateway.snapshot().sessions))
}

pub async fn list_openclaw_runtime_sessions(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.openclaw_gateway.snapshot().runtime_sessions))
}

pub async fn openclaw_runtime_observability(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.openclaw_gateway.runtime_observability()))
}

pub async fn update_openclaw_runtime_options(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
    body: web::Json<UpdateRuntimeOptionsRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    let request = body.into_inner();
    let options = state
        .openclaw_gateway
        .update_runtime_options(&request.session_id, request.patch)?;
    Ok(HttpResponse::Ok().json(options))
}

pub async fn list_openclaw_events(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    Ok(HttpResponse::Ok().json(state.openclaw_gateway.snapshot().recent_events))
}

pub async fn queue_openclaw_boot_job(
    state: web::Data<Arc<RwLock<AppState>>>,
    http: HttpRequest,
    body: web::Json<BootJobRequest>,
) -> Result<HttpResponse, AstraError> {
    let state = state.read().await;
    authorize_admin_request(&http, &state.config)?;
    let agent_id = body.agent_id.clone();
    let job = state
        .openclaw_gateway
        .queue_boot_job(body.into_inner(), Some(format!("agent/{agent_id}/boot")));
    Ok(HttpResponse::Accepted().json(job))
}

/// Configure HTTPA routes.
pub fn configure_httpa_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/httpa")
        .route("/handshake", web::post().to(handshake))
        .route("/intent", web::post().to(submit_intent))
        .route("/status", web::get().to(status))
        .route("/capabilities", web::get().to(capabilities))
        .route("/sessions/{id}", web::delete().to(terminate_session))
        .route("/control-plane", web::get().to(control_plane_snapshot))
        .route("/runtime", web::get().to(runtime_fleet_status))
        .route("/runtime/sessions", web::get().to(list_runtime_sessions))
        .route("/runtime/sessions/{id}", web::get().to(get_runtime_session))
        .route(
            "/learning/rollouts",
            web::post().to(ingest_learning_rollouts),
        )
        .route("/learning/summary", web::get().to(learning_summary))
        .route("/openclaw", web::get().to(openclaw_overview))
        .route("/openclaw/agents", web::get().to(list_openclaw_agents))
        .route("/openclaw/agents", web::post().to(register_openclaw_agent))
        .route("/openclaw/sessions", web::get().to(list_openclaw_sessions))
        .route(
            "/openclaw/runtime/sessions",
            web::get().to(list_openclaw_runtime_sessions),
        )
        .route(
            "/openclaw/runtime/observability",
            web::get().to(openclaw_runtime_observability),
        )
        .route(
            "/openclaw/runtime/options",
            web::post().to(update_openclaw_runtime_options),
        )
        .route("/openclaw/events", web::get().to(list_openclaw_events))
        .route("/openclaw/boot", web::post().to(queue_openclaw_boot_job));

    cfg.service(scope);
}

fn validate_handshake_request(request: &HandshakeRequest) -> AstraResult<()> {
    if request.agent_id.trim().is_empty() || request.agent_id.len() > 128 {
        return Err(AstraError::HandshakeFailed(
            "agent_id must be between 1 and 128 characters".into(),
        ));
    }
    if request.agent_name.trim().is_empty() || request.agent_name.len() > 128 {
        return Err(AstraError::HandshakeFailed(
            "agent_name must be between 1 and 128 characters".into(),
        ));
    }
    if request.capabilities.len() > 64 {
        return Err(AstraError::HandshakeFailed(
            "capabilities exceed the maximum allowed count".into(),
        ));
    }
    if request.domains.len() > 64 {
        return Err(AstraError::HandshakeFailed(
            "domains exceed the maximum allowed count".into(),
        ));
    }
    if request
        .capabilities
        .iter()
        .any(|capability| capability.trim().is_empty() || capability.len() > 96)
    {
        return Err(AstraError::HandshakeFailed(
            "capabilities must be non-empty and at most 96 characters".into(),
        ));
    }

    Ok(())
}

fn authorize_session_request(
    http: &HttpRequest,
    request: &IntentRequest,
    app: &AppState,
) -> AstraResult<()> {
    if !app.config.httpa.require_session_auth {
        return Ok(());
    }

    let token = request
        .session_token
        .as_deref()
        .or_else(|| extract_bearer_token(http))
        .or_else(|| header_token(http, HEADER_SESSION_TOKEN))
        .ok_or(AstraError::AuthRequired)?;

    app.session_manager
        .verify_session_token(&request.session_id, token)?;
    Ok(())
}

pub(crate) fn authorize_admin_request(
    http: &HttpRequest,
    config: &crate::config::AppConfig,
) -> AstraResult<()> {
    if let Some(expected_token) = config.server.admin_token.as_deref() {
        let provided = extract_bearer_token(http)
            .or_else(|| header_token(http, HEADER_ADMIN_TOKEN))
            .ok_or(AstraError::AuthRequired)?;

        if secrets_equal(provided, expected_token) {
            return Ok(());
        }

        return Err(AstraError::AuthRequired);
    }

    if config.deployment.local_bind_only && !config.deployment.remote_admin {
        if request_origin_is_loopback(http) {
            return Ok(());
        }
    }

    Err(AstraError::AuthRequired)
}

fn secrets_equal(left: &str, right: &str) -> bool {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();

    if left_bytes.len() != right_bytes.len() {
        return false;
    }

    let mut diff = 0u8;
    for (left_byte, right_byte) in left_bytes.iter().zip(right_bytes.iter()) {
        diff |= left_byte ^ right_byte;
    }

    diff == 0
}

fn authorize_admin_or_session_request(
    http: &HttpRequest,
    app: &AppState,
    session_id: &str,
) -> AstraResult<()> {
    if authorize_admin_request(http, &app.config).is_ok() {
        return Ok(());
    }

    if app.config.httpa.require_session_auth {
        if let Some(token) =
            extract_bearer_token(http).or_else(|| header_token(http, HEADER_SESSION_TOKEN))
        {
            app.session_manager
                .verify_session_token(session_id, token)?;
            return Ok(());
        }
    }

    Err(AstraError::AuthRequired)
}

fn extract_bearer_token(request: &HttpRequest) -> Option<&str> {
    request
        .headers()
        .get("Authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
}

fn header_token<'a>(request: &'a HttpRequest, header_name: &str) -> Option<&'a str> {
    request
        .headers()
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
}

fn request_origin_is_loopback(request: &HttpRequest) -> bool {
    request
        .peer_addr()
        .map(|peer| peer.ip().is_loopback())
        .unwrap_or(true)
}

fn parse_priority(priority: Option<&str>) -> IntentPriority {
    match priority {
        Some("low") => IntentPriority::Low,
        Some("high") => IntentPriority::High,
        Some("critical") => IntentPriority::Critical,
        _ => IntentPriority::Normal,
    }
}

fn resolve_session_defaults(
    request: &HandshakeRequest,
    config: &crate::config::AppConfig,
) -> HttpaExecutionPolicy {
    let mode = request
        .session_defaults
        .mode
        .unwrap_or_else(|| infer_mode_from_capabilities(&request.capabilities));
    enforce_personal_runtime_policy(
        HttpaExecutionPolicy::for_mode(mode).apply_preferences(&request.session_defaults),
        config,
    )
}

fn resolve_execution_policy(
    session: Option<&super::control_plane::ManagedAgentSession>,
    request: &IntentRequest,
    config: &crate::config::AppConfig,
) -> HttpaExecutionPolicy {
    let base = session
        .map(|managed| managed.default_policy.clone())
        .unwrap_or_else(|| {
            let _ = request;
            HttpaExecutionPolicy::for_mode(HttpaExecutionMode::ResultOnly)
        });

    enforce_personal_runtime_policy(base.apply_preferences(&request.execution), config)
}

fn build_execution_plan(
    request: &IntentRequest,
    policy: &HttpaExecutionPolicy,
) -> HttpaExecutionPlan {
    let _ = request;

    let routing_lane = match (policy.mode, policy.performance_tier) {
        (_, HttpaPerformanceTier::Hyperscale) => "autonomous-swarm-max".to_string(),
        (HttpaExecutionMode::ResultOnly, _) => "autonomous-result".to_string(),
        (HttpaExecutionMode::VisibleBrowse, _) => "autonomous-result".to_string(),
    };
    let transport = policy.transport_profile();

    HttpaExecutionPlan {
        routing_lane,
        session_profile: None,
        identity_surface: if policy.origin_shielding {
            "shielded_result_channel".into()
        } else {
            "identified_session".into()
        },
        ledger_lane: match policy.ledger_mode {
            HttpaLedgerMode::Transport => "transport_only".into(),
            HttpaLedgerMode::BoundIntent => "bound_intent".into(),
            HttpaLedgerMode::VerifiedEvidence => "verified_evidence".into(),
            HttpaLedgerMode::SovereignConsensus => "sovereign_consensus".into(),
            HttpaLedgerMode::LiquidStateChannel => "liquid_state_channel".into(),
            HttpaLedgerMode::OntologicalTruth => "ontological_truth".into(),
        },
        transport,
        activated_features: policy.activated_features(),
        expected_artifacts: policy.expected_artifacts(),
        max_parallel_agents: policy.max_parallel_agents,
    }
}

fn infer_mode_from_capabilities(capabilities: &[String]) -> HttpaExecutionMode {
    let _ = capabilities;
    HttpaExecutionMode::ResultOnly
}

fn supported_capabilities(config: &crate::config::AppConfig) -> HttpaProtocolCapabilities {
    let mut capabilities = HttpaProtocolCapabilities::production_defaults();
    let _ = config;
    capabilities.modes = vec![HttpaExecutionMode::ResultOnly];
    capabilities.delivery_modes = vec![HttpaDeliveryMode::FinalOnly];
    capabilities.privacy_modes = vec![
        HttpaPrivacyMode::OriginShielded,
        HttpaPrivacyMode::AnonymousDelegation,
    ];
    capabilities
}

fn enforce_personal_runtime_policy(
    policy: HttpaExecutionPolicy,
    config: &crate::config::AppConfig,
) -> HttpaExecutionPolicy {
    HttpaExecutionPolicy::for_mode(HttpaExecutionMode::ResultOnly).apply_preferences(
        &HttpaExecutionPreferences {
            mode: Some(HttpaExecutionMode::ResultOnly),
            delivery: Some(HttpaDeliveryMode::FinalOnly),
            privacy: Some(if config.privacy.origin_shielding_required {
                HttpaPrivacyMode::OriginShielded
            } else {
                policy.privacy
            }),
            performance_tier: Some(policy.performance_tier),
            ledger_mode: Some(policy.ledger_mode),
            require_chain_receipt: Some(policy.chain_receipt_required),
            max_parallel_agents: Some(policy.max_parallel_agents),
            explain_plan: Some(false),
        },
    )
}

fn maybe_execute_autonomous_value_intent(
    engine: &mut EngineState,
    request: &IntentRequest,
    trace_id: &str,
) -> AstraResult<Option<serde_json::Value>> {
    if !is_autonomous_value_intent(request) {
        return Ok(None);
    }

    let signal = format!(
        "{} {}",
        request.action.trim().to_lowercase(),
        request.domain.trim().to_lowercase()
    );

    if request.parameters.get("sender").is_some()
        && request.parameters.get("receiver").is_some()
        && request.parameters.get("amount").is_some()
        && (signal.contains("pay")
            || signal.contains("transfer")
            || signal.contains("settle")
            || signal.contains("milestone")
            || signal.contains("subscription"))
    {
        let outcome = engine.chain.settle_guarded_payment(GuardedPaymentRequest {
            sender: required_string_parameter(request, "sender")?,
            receiver: required_string_parameter(request, "receiver")?,
            amount: required_f64_parameter(request, "amount")?,
            settlement_rail: parse_settlement_rail(request.parameters.get("settlement_rail"))?,
            connector_id: optional_string_parameter(request, "connector_id"),
            compliance: parse_compliance_mode(request.parameters.get("compliance"))?,
            privacy_mode: Some(parse_privacy_mode(request.parameters.get("privacy_mode"))?),
            contract_id: optional_string_parameter(request, "contract_id"),
            payment_kind: parse_smart_payment_kind(request.parameters.get("payment_kind"))?,
            trace_id: optional_string_parameter(request, "trace_id")
                .or_else(|| Some(trace_id.to_string())),
            session_id: Some(request.session_id.clone()),
            confidence: request_f64(request, "confidence"),
            risk_score: request_f64(request, "risk_score"),
            evidence: parse_string_list(request.parameters.get("evidence"))
                .or_else(|| parse_string_list(request.parameters.get("evidence_hashes")))
                .unwrap_or_default(),
            context: parse_string_map(request.parameters.get("context")),
            metadata: request.parameters.get("metadata").cloned(),
            autocommit: request_bool(request, "autocommit").unwrap_or(true),
            fail_closed_on_review: request_bool(request, "fail_closed_on_review").unwrap_or(true),
        })?;
        return Ok(Some(serde_json::json!({
            "kind": "guarded_payment",
            "outcome": outcome,
        })));
    }

    if request.parameters.get("device_id").is_some()
        || request.parameters.get("wallet_address").is_some()
        || signal.contains("device")
        || signal.contains("wallet")
        || signal.contains("bind")
    {
        let submission = engine.chain.bind_device_wallet(DeviceBindingRequest {
            owner: required_string_parameter(request, "owner")?,
            device_id: required_string_parameter(request, "device_id")?,
            wallet_address: required_string_parameter(request, "wallet_address")?,
            settlement_rail: parse_settlement_rail(request.parameters.get("settlement_rail"))?,
            connector_id: optional_string_parameter(request, "connector_id"),
            daily_spend_limit: request_f64(request, "daily_spend_limit").unwrap_or(0.0),
            compliance: parse_compliance_mode(request.parameters.get("compliance"))?,
            privacy_mode: parse_privacy_mode(request.parameters.get("privacy_mode"))?,
            allowed_domains: parse_string_list(request.parameters.get("allowed_domains"))
                .unwrap_or_default(),
            metadata: request.parameters.get("metadata").cloned(),
            autocommit: request_bool(request, "autocommit").unwrap_or(true),
        })?;
        return Ok(Some(serde_json::json!({
            "kind": "device_binding",
            "submission": submission,
        })));
    }

    if request.parameters.get("workload_id").is_some()
        || signal.contains("reward")
        || signal.contains("cognition")
        || signal.contains("validator")
    {
        let submission = engine.chain.reward_cognition(CognitionRewardRequest {
            payer: required_string_parameter(request, "payer")?,
            beneficiary: required_string_parameter(request, "beneficiary")?,
            workload_id: required_string_parameter(request, "workload_id")?,
            routed_packets: request_u64(request, "routed_packets").unwrap_or(0),
            inference_units: request_u64(request, "inference_units").unwrap_or(0),
            uptime_seconds: request_u64(request, "uptime_seconds").unwrap_or(0),
            quality_score: request_f64(request, "quality_score").unwrap_or(0.75),
            settlement_rail: parse_settlement_rail(request.parameters.get("settlement_rail"))?,
            connector_id: optional_string_parameter(request, "connector_id"),
            compliance: parse_compliance_mode(request.parameters.get("compliance"))?,
            metadata: request.parameters.get("metadata").cloned(),
            autocommit: request_bool(request, "autocommit").unwrap_or(true),
        })?;
        return Ok(Some(serde_json::json!({
            "kind": "cognition_reward",
            "submission": submission,
        })));
    }

    if request.parameters.get("dispute_id").is_some()
        || signal.contains("dispute")
        || signal.contains("refund")
        || signal.contains("chargeback")
    {
        if request.parameters.get("dispute_id").is_some()
            || signal.contains("resolve")
            || request.parameters.get("approve_refund").is_some()
        {
            let outcome =
                engine
                    .chain
                    .resolve_payment_dispute(PaymentDisputeResolutionRequest {
                        dispute_id: required_string_parameter(request, "dispute_id")?,
                        resolver: required_string_parameter(request, "resolver")?,
                        approve_refund: request_bool(request, "approve_refund").unwrap_or(false),
                        refund_amount: request_f64(request, "refund_amount"),
                        rationale: required_string_parameter(request, "rationale")?,
                        metadata: request.parameters.get("metadata").cloned(),
                        autocommit: request_bool(request, "autocommit").unwrap_or(true),
                    })?;
            return Ok(Some(serde_json::json!({
                "kind": "payment_dispute_resolve",
                "outcome": outcome,
            })));
        }

        let evidence_hashes = parse_string_list(request.parameters.get("evidence_hashes"))
            .ok_or_else(|| {
                AstraError::InvalidTransaction("payment dispute requires evidence_hashes".into())
            })?;
        let dispute = engine.chain.open_payment_dispute(PaymentDisputeRequest {
            actor: required_string_parameter(request, "actor")?,
            original_tx_hash: required_string_parameter(request, "original_tx_hash")?,
            reason: required_string_parameter(request, "reason")?,
            requested_amount: required_f64_parameter(request, "requested_amount")?,
            evidence_hashes,
            metadata: request.parameters.get("metadata").cloned(),
        })?;
        return Ok(Some(serde_json::json!({
            "kind": "payment_dispute_open",
            "dispute": dispute,
        })));
    }

    if request.parameters.get("original_tx_hash").is_some()
        || signal.contains("privacy")
        || signal.contains("attest")
    {
        let evidence_hashes = parse_string_list(request.parameters.get("evidence_hashes"))
            .ok_or_else(|| {
                AstraError::InvalidTransaction(
                    "privacy attestation requires evidence_hashes".into(),
                )
            })?;
        let submission = engine.chain.attest_privacy(PrivacyAttestationRequest {
            actor: required_string_parameter(request, "actor")?,
            original_tx_hash: required_string_parameter(request, "original_tx_hash")?,
            settlement_rail: parse_settlement_rail(request.parameters.get("settlement_rail"))?,
            connector_id: optional_string_parameter(request, "connector_id"),
            compliance: parse_compliance_mode(request.parameters.get("compliance"))?,
            privacy_mode: parse_privacy_mode(request.parameters.get("privacy_mode"))?,
            tax_reference: required_string_parameter(request, "tax_reference")?,
            evidence_hashes,
            metadata: request.parameters.get("metadata").cloned(),
            autocommit: request_bool(request, "autocommit").unwrap_or(true),
        })?;
        return Ok(Some(serde_json::json!({
            "kind": "privacy_attestation",
            "submission": submission,
        })));
    }

    if request.parameters.get("providers").is_some()
        || signal.contains("route")
        || signal.contains("market")
        || signal.contains("commerce")
    {
        let route_request = FinancialIntentRouteRequest {
            consumer: required_string_parameter(request, "consumer")?,
            market: required_string_parameter(request, "market")?,
            objective: required_string_parameter(request, "objective")?,
            max_budget: required_f64_parameter(request, "max_budget")?,
            settlement_rail: parse_settlement_rail(request.parameters.get("settlement_rail"))?,
            connector_id: optional_string_parameter(request, "connector_id"),
            compliance: parse_compliance_mode(request.parameters.get("compliance"))?,
            risk_tier: parse_risk_tier(request.parameters.get("risk_tier"))?,
            providers: parse_market_providers(request.parameters.get("providers"))?,
            metadata: request.parameters.get("metadata").cloned(),
        };
        if request_bool(request, "commit").unwrap_or(false)
            || signal.contains("commit")
            || signal.contains("reserve")
            || signal.contains("book")
        {
            let submission =
                engine
                    .chain
                    .commit_financial_intent(FinancialIntentCommitRequest {
                        route_request,
                        autocommit: request_bool(request, "autocommit").unwrap_or(true),
                    })?;
            return Ok(Some(serde_json::json!({
                "kind": "financial_intent_commit",
                "submission": submission,
            })));
        }

        let route = engine.chain.route_financial_intent(&route_request)?;
        return Ok(Some(serde_json::json!({
            "kind": "financial_intent_route",
            "route": route,
        })));
    }

    let stream_request = StreamValueRequest {
        payer: required_string_parameter(request, "payer")?,
        payee: required_string_parameter(request, "payee")?,
        resource: required_string_parameter(request, "resource")?,
        bytes_total: required_u64_parameter(request, "bytes_total")?,
        price_per_mb: required_f64_parameter(request, "price_per_mb")?,
        settlement_rail: parse_settlement_rail(request.parameters.get("settlement_rail"))?,
        connector_id: optional_string_parameter(request, "connector_id"),
        compliance: parse_compliance_mode(request.parameters.get("compliance"))?,
        privacy_mode: parse_privacy_mode(request.parameters.get("privacy_mode"))?,
        qos_tier: parse_qos_tier(request.parameters.get("qos_tier"))?,
        trace_id: Some(trace_id.to_string()),
        preferred_chunk_bytes: request_u64(request, "preferred_chunk_bytes"),
        escrow_multiplier: request_f64(request, "escrow_multiplier"),
        metadata: request.parameters.get("metadata").cloned(),
    };
    if request_bool(request, "quote_only").unwrap_or(false) || signal.contains("quote") {
        let quote = engine.chain.quote_stream_value(&stream_request)?;
        return Ok(Some(serde_json::json!({
            "kind": "stream_quote",
            "quote": quote,
        })));
    }

    let submission = engine.chain.settle_stream_value(StreamSettlementRequest {
        quote_request: stream_request,
        autocommit: request_bool(request, "autocommit").unwrap_or(true),
    })?;
    Ok(Some(serde_json::json!({
        "kind": "stream_settlement",
        "submission": submission,
    })))
}

fn maybe_execute_adaptive_search_intent(
    engine: &mut EngineState,
    request: &IntentRequest,
) -> AstraResult<Option<crate::features::adaptive_search::AdaptiveSearchResponse>> {
    if !is_adaptive_search_intent(request) {
        return Ok(None);
    }

    let query = request
        .parameters
        .get("query")
        .or_else(|| request.parameters.get("prompt"))
        .or_else(|| request.parameters.get("objective"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AstraError::SearchFailed("search intent missing query text".into()))?;

    let search_request = crate::features::adaptive_search::AdaptiveSearchRequest {
        query: query.to_string(),
        preferred_depth: request
            .parameters
            .get("depth")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string),
        max_sources: request
            .parameters
            .get("max_sources")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize),
        include_social: request
            .parameters
            .get("include_social")
            .and_then(serde_json::Value::as_bool),
        include_news: request
            .parameters
            .get("include_news")
            .and_then(serde_json::Value::as_bool),
        include_books: request
            .parameters
            .get("include_books")
            .and_then(serde_json::Value::as_bool),
        include_papers: request
            .parameters
            .get("include_papers")
            .and_then(serde_json::Value::as_bool),
        include_docs: request
            .parameters
            .get("include_docs")
            .and_then(serde_json::Value::as_bool),
        response_mode: request
            .parameters
            .get("response_mode")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_search_response_mode),
        freshness_horizon_hours: request
            .parameters
            .get("freshness_horizon_hours")
            .and_then(serde_json::Value::as_u64),
        domain_focus: parse_string_list(request.parameters.get("domain_focus")),
        require_citations: request
            .parameters
            .get("require_citations")
            .and_then(serde_json::Value::as_bool),
        max_contradictions: request
            .parameters
            .get("max_contradictions")
            .and_then(serde_json::Value::as_u64)
            .map(|value| value as usize),
        notarize: request
            .parameters
            .get("notarize")
            .and_then(serde_json::Value::as_bool)
            .or(Some(true)),
    };
    let requested_notarize = search_request.notarize;
    let mut search_runtime = engine
        .search_runtime
        .write()
        .map_err(|_| AstraError::Internal("search runtime lock poisoned".into()))?;
    let SearchIntelligenceRuntime {
        web_search,
        deep_research,
        swarm,
        truth,
        adaptive_search,
        enhanced_swarm,
    } = &mut *search_runtime;

    let mut result = adaptive_search.execute(
        search_request,
        crate::features::adaptive_search::AdaptiveSearchDeps {
            web_search,
            deep_research,
            swarm,
            truth,
            enhanced_swarm,
        },
    )?;
    drop(search_runtime);

    if requested_notarize.unwrap_or(!result.profile.fast_path) {
        crate::features::adaptive_search::attach_ledger_proof(&mut engine.chain, &mut result)?;
    }

    Ok(Some(result))
}

fn maybe_execute_brain_intent(
    engine: &mut EngineState,
    runtime_sessions: &RuntimeSessionManager,
    request: &IntentRequest,
) -> AstraResult<Option<crate::assistant::one_brain::BrainExecutionArtifacts>> {
    let Some(mode_hint) = brain_mode_hint_for_intent(request) else {
        return Ok(None);
    };

    let task = request
        .parameters
        .get("task")
        .or_else(|| request.parameters.get("query"))
        .or_else(|| request.parameters.get("prompt"))
        .or_else(|| request.parameters.get("objective"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("{} {}", request.action, request.domain));

    Ok(Some(
        engine.execute_brain(
            BrainExecuteRequest {
                task,
                context: serde_json::json!({
                    "domain": request.domain,
                    "parameters": request.parameters,
                    "session_id": request.session_id,
                }),
                session_id: Some(request.session_id.clone()),
                mode_hint: Some(mode_hint),
                allow_mutation: false,
                require_live_retrieval: matches!(
                    mode_hint,
                    BrainModeHint::Respond | BrainModeHint::Intent
                ) && request
                    .parameters
                    .get("require_live_retrieval")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                require_verification: true,
            },
            Some(runtime_sessions),
        )?,
    ))
}

fn brain_mode_hint_for_intent(request: &IntentRequest) -> Option<BrainModeHint> {
    let action = request.action.trim().to_ascii_lowercase();
    let domain = request.domain.trim().to_ascii_lowercase();

    if action.contains("reason") || action.contains("solve") {
        Some(BrainModeHint::Reason)
    } else if action.contains("plan") || domain.contains("plan") {
        Some(BrainModeHint::Plan)
    } else if action.contains("mission")
        || action.contains("orchestrate")
        || domain.contains("mission")
    {
        Some(BrainModeHint::Mission)
    } else if action.contains("think")
        || action.contains("respond")
        || action.contains("assistant")
        || action.contains("chat")
    {
        Some(BrainModeHint::Respond)
    } else if domain.contains("assistant") || domain.contains("cognition") {
        Some(BrainModeHint::Intent)
    } else {
        None
    }
}

fn is_adaptive_search_intent(request: &IntentRequest) -> bool {
    request.domain.contains("search")
        || request.domain.contains("research")
        || request.action.contains("search")
        || request.action.contains("research")
        || request.action.contains("analyze")
}

fn is_autonomous_value_intent(request: &IntentRequest) -> bool {
    let domain = request.domain.trim().to_lowercase();
    let action = request.action.trim().to_lowercase();
    request.parameters.get("settlement_rail").is_some()
        || request.parameters.get("providers").is_some()
        || request.parameters.get("workload_id").is_some()
        || request.parameters.get("original_tx_hash").is_some()
        || request.parameters.get("dispute_id").is_some()
        || request.parameters.get("device_id").is_some()
        || request.parameters.get("wallet_address").is_some()
        || request.parameters.get("bytes_total").is_some()
        || domain.contains("value")
        || domain.contains("payment")
        || domain.contains("commerce")
        || domain.contains("wallet")
        || domain.contains("iot")
        || action.contains("stream")
        || action.contains("settle")
        || action.contains("route")
        || action.contains("reward")
        || action.contains("attest")
        || action.contains("dispute")
        || action.contains("refund")
        || action.contains("wallet")
        || action.contains("bind")
}

fn parse_settlement_rail(value: Option<&serde_json::Value>) -> AstraResult<SettlementRail> {
    match value
        .and_then(serde_json::Value::as_str)
        .map(|entry| entry.trim().to_lowercase())
        .as_deref()
    {
        Some("internal") | Some("internal_ledger") | None => Ok(SettlementRail::InternalLedger),
        Some("upi") | Some("upi_bridge") => Ok(SettlementRail::UpiBridge),
        Some("tokenized") | Some("tokenized_deposit") => Ok(SettlementRail::TokenizedDeposit),
        Some(other) => Err(AstraError::InvalidTransaction(format!(
            "unsupported settlement rail '{other}'"
        ))),
    }
}

fn parse_compliance_mode(value: Option<&serde_json::Value>) -> AstraResult<ComplianceMode> {
    match value
        .and_then(serde_json::Value::as_str)
        .map(|entry| entry.trim().to_lowercase())
        .as_deref()
    {
        Some("internal") | Some("internal_only") | None => Ok(ComplianceMode::InternalOnly),
        Some("tax") | Some("tax_aware") => Ok(ComplianceMode::TaxAware),
        Some("regulated") | Some("regulated_pseudonymous") => {
            Ok(ComplianceMode::RegulatedPseudonymous)
        }
        Some(other) => Err(AstraError::InvalidTransaction(format!(
            "unsupported compliance mode '{other}'"
        ))),
    }
}

fn parse_smart_payment_kind(value: Option<&serde_json::Value>) -> AstraResult<SmartPaymentKind> {
    match value
        .and_then(serde_json::Value::as_str)
        .map(|entry| entry.trim().to_lowercase())
        .as_deref()
    {
        Some("peer_transfer") | Some("peer") | Some("transfer") | None => {
            Ok(SmartPaymentKind::PeerTransfer)
        }
        Some("milestone_release") | Some("milestone") => Ok(SmartPaymentKind::MilestoneRelease),
        Some("subscription_renewal") | Some("subscription") | Some("renewal") => {
            Ok(SmartPaymentKind::SubscriptionRenewal)
        }
        Some("refund_return") | Some("refund") => Ok(SmartPaymentKind::RefundReturn),
        Some(other) => Err(AstraError::InvalidTransaction(format!(
            "unsupported smart payment kind '{other}'"
        ))),
    }
}

fn parse_privacy_mode(value: Option<&serde_json::Value>) -> AstraResult<PrivacyPreservationMode> {
    match value
        .and_then(serde_json::Value::as_str)
        .map(|entry| entry.trim().to_lowercase())
        .as_deref()
    {
        Some("full") | Some("full_disclosure") | None => {
            Ok(PrivacyPreservationMode::FullDisclosure)
        }
        Some("counterparty") | Some("counterparty_shielded") => {
            Ok(PrivacyPreservationMode::CounterpartyShielded)
        }
        Some("attested") | Some("attested_minimization") => {
            Ok(PrivacyPreservationMode::AttestedMinimization)
        }
        Some(other) => Err(AstraError::InvalidTransaction(format!(
            "unsupported privacy mode '{other}'"
        ))),
    }
}

fn parse_qos_tier(value: Option<&serde_json::Value>) -> AstraResult<QualityOfServiceTier> {
    match value
        .and_then(serde_json::Value::as_str)
        .map(|entry| entry.trim().to_lowercase())
        .as_deref()
    {
        Some("standard") | None => Ok(QualityOfServiceTier::Standard),
        Some("premium") => Ok(QualityOfServiceTier::Premium),
        Some("realtime") | Some("real_time") => Ok(QualityOfServiceTier::Realtime),
        Some(other) => Err(AstraError::InvalidTransaction(format!(
            "unsupported qos tier '{other}'"
        ))),
    }
}

fn parse_risk_tier(value: Option<&serde_json::Value>) -> AstraResult<EnterpriseRiskTier> {
    match value
        .and_then(serde_json::Value::as_str)
        .map(|entry| entry.trim().to_lowercase())
        .as_deref()
    {
        Some("low") => Ok(EnterpriseRiskTier::Low),
        Some("high") => Ok(EnterpriseRiskTier::High),
        Some("critical") => Ok(EnterpriseRiskTier::Critical),
        Some("standard") | None => Ok(EnterpriseRiskTier::Standard),
        Some(other) => Err(AstraError::InvalidTransaction(format!(
            "unsupported risk tier '{other}'"
        ))),
    }
}

fn parse_market_providers(
    value: Option<&serde_json::Value>,
) -> AstraResult<Vec<MarketProviderQuote>> {
    let Some(value) = value else {
        return Err(AstraError::InvalidTransaction(
            "financial intent requires providers".into(),
        ));
    };
    serde_json::from_value::<Vec<MarketProviderQuote>>(value.clone()).map_err(|error| {
        AstraError::InvalidTransaction(format!("providers payload is malformed: {error}"))
    })
}

fn request_bool(request: &IntentRequest, key: &str) -> Option<bool> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_bool)
}

fn request_u64(request: &IntentRequest, key: &str) -> Option<u64> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_u64)
}

fn request_f64(request: &IntentRequest, key: &str) -> Option<f64> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_f64)
}

fn required_string_parameter(request: &IntentRequest, key: &str) -> AstraResult<String> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| AstraError::InvalidTransaction(format!("{key} is required")))
}

fn optional_string_parameter(request: &IntentRequest, key: &str) -> Option<String> {
    request
        .parameters
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_string_map(value: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    value
        .and_then(serde_json::Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(str::trim)
                        .filter(|entry| !entry.is_empty())
                        .map(|entry| (key.trim().to_string(), entry.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn required_u64_parameter(request: &IntentRequest, key: &str) -> AstraResult<u64> {
    request_u64(request, key)
        .ok_or_else(|| AstraError::InvalidTransaction(format!("{key} must be provided as u64")))
}

fn required_f64_parameter(request: &IntentRequest, key: &str) -> AstraResult<f64> {
    request_f64(request, key)
        .ok_or_else(|| AstraError::InvalidTransaction(format!("{key} must be provided as f64")))
}

fn parse_search_response_mode(
    value: &str,
) -> Option<crate::features::adaptive_search::SearchResponseMode> {
    match value.trim().to_lowercase().as_str() {
        "fast" => Some(crate::features::adaptive_search::SearchResponseMode::Fast),
        "balanced" | "standard" => {
            Some(crate::features::adaptive_search::SearchResponseMode::Balanced)
        }
        "deep" => Some(crate::features::adaptive_search::SearchResponseMode::Deep),
        "forensic" | "ultra" => {
            Some(crate::features::adaptive_search::SearchResponseMode::Forensic)
        }
        _ => None,
    }
}

fn parse_string_list(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    match value {
        Some(serde_json::Value::Array(items)) => {
            let values = items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        }
        Some(serde_json::Value::String(item)) if !item.trim().is_empty() => {
            Some(vec![item.trim().to_string()])
        }
        _ => None,
    }
}

fn build_learning_problem(action: &str, domain: &str, parameters: &serde_json::Value) -> String {
    let mut parts = vec![format!("action={action} domain={domain}")];
    if let Some(objective) = parameters
        .get("objective")
        .and_then(serde_json::Value::as_str)
    {
        parts.push(format!("objective={objective}"));
    }
    if let Some(input) = parameters
        .get("input")
        .or_else(|| parameters.get("prompt"))
        .or_else(|| parameters.get("signal"))
        .and_then(serde_json::Value::as_str)
    {
        parts.push(format!("input={input}"));
    }
    if let Some(url) = parameters.get("url").and_then(serde_json::Value::as_str) {
        parts.push(format!("url={url}"));
    }
    parts.join(" | ")
}

fn infer_learning_domain_from_intent(
    action: &str,
    domain: &str,
    parameters: &serde_json::Value,
) -> String {
    let normalized_domain = domain.trim().to_lowercase();
    if !normalized_domain.is_empty() && normalized_domain != "general" {
        return normalized_domain.replace(' ', "_");
    }

    let signal = format!(
        "{} {} {}",
        action,
        parameters
            .get("objective")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default(),
        parameters
            .get("input")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    )
    .to_lowercase();

    if ["code", "patch", "build", "compile", "test", "refactor"]
        .iter()
        .any(|token| signal.contains(token))
    {
        "coding".into()
    } else if ["search", "research", "analyze", "browse", "investigate"]
        .iter()
        .any(|token| signal.contains(token))
    {
        "research".into()
    } else if ["design", "architecture", "scale", "service", "system"]
        .iter()
        .any(|token| signal.contains(token))
    {
        "architecture".into()
    } else if ["debug", "incident", "failure", "broken", "regression"]
        .iter()
        .any(|token| signal.contains(token))
    {
        "debugging".into()
    } else {
        "general".into()
    }
}

fn ingest_openclaw_execution(
    learning_loop: &LightningLoop,
    request: &IntentRequest,
    execution: &MultiAgentExecution,
    learning_domain: &str,
) {
    let mut trace = TrajectoryTrace::new(&build_learning_problem(
        &request.action,
        &request.domain,
        &request.parameters,
    ));
    trace.domain = learning_domain.to_string();
    trace.final_answer = execution.final_response.clone();
    trace.gating_mode = execution.send_policy.clone();

    let champion_score = execution
        .consensus
        .ranked_agents
        .first()
        .map(|score| score.final_score)
        .unwrap_or(0.0);
    let review_score = execution.consensus.agreement_score;
    let security_score = match execution.send_policy.as_str() {
        "deny" => 0.3,
        "review" => 0.65,
        _ => 0.88,
    };
    let code_score = execution
        .reasoning_steps
        .iter()
        .find(|step| matches!(step.role, crate::openclaw::agents::AgentRole::Coder))
        .map(|step| step.final_score.max(step.confidence))
        .unwrap_or(champion_score * 0.9);

    trace.final_reward =
        (champion_score * 0.45 + review_score * 0.35 + security_score * 0.20).clamp(0.0, 1.0);
    trace.success = trace.final_reward >= 0.62 && execution.send_policy != "deny";
    trace
        .reward_dimensions
        .insert("scenario".into(), champion_score);
    trace
        .reward_dimensions
        .insert("critic".into(), review_score);
    trace
        .reward_dimensions
        .insert("security".into(), security_score);
    trace
        .reward_dimensions
        .insert("code_quality".into(), code_score);
    trace.reward_dimensions.insert(
        "coordination".into(),
        execution
            .consensus
            .ranked_agents
            .iter()
            .map(|score| score.consensus_weight)
            .sum::<f64>()
            / execution.consensus.ranked_agents.len().max(1) as f64,
    );
    trace.reward_dimensions.insert(
        "merge_readiness".into(),
        execution.merged_execution.ready_branches.len() as f64
            / execution.reasoning_steps.len().max(1) as f64,
    );

    trace.strategies_used = execution
        .adaptive_policy
        .as_ref()
        .map(|policy| policy.preferred_strategies.clone())
        .filter(|strategies| !strategies.is_empty())
        .unwrap_or_else(|| {
            execution
                .reasoning_steps
                .iter()
                .flat_map(|step| step.policy_tags.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        });

    for step in &execution.reasoning_steps {
        let mut span = TraceSpan::new(match step.role {
            crate::openclaw::agents::AgentRole::Verifier
            | crate::openclaw::agents::AgentRole::Safety => SpanType::Verification,
            crate::openclaw::agents::AgentRole::Planner
            | crate::openclaw::agents::AgentRole::Coordinator => SpanType::BrainDecision,
            _ => SpanType::Reasoning,
        });
        span.input_data = request.action.clone();
        span.output_data = step.summary.clone();
        span.prompt_template = execution
            .adaptive_policy
            .as_ref()
            .and_then(|policy| policy.champion_prompt.clone())
            .unwrap_or_default();
        span.reward = step.final_score.max(step.confidence);
        span.duration_ms = 20.0 + step.rank as f64 * 12.0;
        span.cognitive_mode = step.role.as_str().to_string();
        span.reward_dimensions
            .insert("peer_review".into(), step.peer_review_score);
        span.reward_dimensions
            .insert("consensus_weight".into(), step.consensus_weight);
        span.reward_dimensions
            .insert("step_score".into(), step.final_score);
        span.attributes
            .insert("role".into(), step.role.as_str().into());
        span.attributes.insert("rank".into(), step.rank.to_string());
        span.attributes
            .insert("tool_count".into(), step.tool_directives.len().to_string());
        span.attributes.insert(
            "plan_steps".into(),
            step.execution_plan.steps.len().to_string(),
        );
        span.attributes.insert(
            "merge_ready".into(),
            step.branch_result.ready_for_merge.to_string(),
        );
        if let Some(subagent) = step.subagent_session_key.as_ref() {
            span.attributes
                .insert("subagent_session_key".into(), subagent.clone());
        }
        trace.add_span(span);
    }

    let mut reward_span = TraceSpan::new(SpanType::Reward);
    reward_span.input_data = execution.consensus.consensus_summary.clone();
    reward_span.output_data = execution.consensus.cohort_grade.clone();
    reward_span.reward = trace.final_reward;
    reward_span
        .reward_dimensions
        .insert("agreement".into(), execution.consensus.agreement_score);
    reward_span
        .reward_dimensions
        .insert("champion".into(), champion_score);
    reward_span.reward_dimensions.insert(
        "review_required".into(),
        if execution.merged_execution.requires_review {
            1.0
        } else {
            0.0
        },
    );
    reward_span.cognitive_mode = "lightning_feedback".into();
    trace.add_span(reward_span);

    if let Err(error) = learning_loop.ingest_batch(OptimizationBatch {
        objective: Some(OptimizationObjective {
            domain: learning_domain.to_string(),
            reward_signal: "openclaw_consensus_quality".into(),
            optimize_prompts: true,
            optimize_tool_policies: true,
            max_rollouts: 128,
        }),
        traces: vec![trace],
    }) {
        warn!("failed to ingest OpenClaw execution into Lightning loop: {error}");
    }
}

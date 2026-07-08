use actix_web::web;
use astra_core::{AppState, common::AppConfig};

pub mod routes;

#[must_use]
pub fn build_state() -> AppState {
    AppState::new(AppConfig::from_env()).expect("validated application state")
}

pub fn app_config(cfg: &mut web::ServiceConfig) {
    routes::configure(cfg);
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};
    use astra_core::{AppState, common::AppConfig};
    use serde_json::Value;

    use super::{app_config, build_state};

    #[actix_web::test]
    async fn health_endpoint_works() {
        let state = build_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::get().uri("/api/v1/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["ok"], true);
        assert_eq!(json["data"]["service"], "astra-personal-assistant");
    }

    #[actix_web::test]
    async fn ready_endpoint_works() {
        let state = build_state();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::get().uri("/api/v1/ready").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["data"]["mode"], "personal_assistant");
    }

    #[actix_web::test]
    async fn os_guardian_status_endpoint_works() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/v1/os-guardian/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["ok"], true);
        assert_eq!(json["data"]["policy"]["httpa_required"], true);
    }

    #[actix_web::test]
    async fn assistant_command_creation_returns_guardian_receipt() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/assistant/commands")
            .set_json(serde_json::json!({
                "text": "open https://example.com",
                "source_platform": "windows",
                "activation_method": "windows_hotkey",
                "autonomy_mode": "full_auto",
                "device_id": "desktop-test"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["data"]["status"], "approval_required");
        assert_eq!(json["data"]["action_plan"]["action_kind"], "open_url");
        assert_eq!(json["data"]["action_plan"]["executable"], false);
        assert!(
            json["data"]["orchestration"]["sandbox"]["receipt_id"]
                .as_str()
                .is_some()
        );
        assert_eq!(
            json["data"]["orchestration"]["sandbox"]["observation"]["reference_monitor"],
            "astra_unified_reference_monitor"
        );
        assert!(
            json["data"]["guardian_receipt"]["receipt_id"]
                .as_str()
                .is_some()
        );
    }

    #[actix_web::test]
    async fn assistant_command_status_endpoint_returns_stored_command() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let create_req = test::TestRequest::post()
            .uri("/api/v1/assistant/commands")
            .set_json(serde_json::json!({
                "text": "search rust compose multiplatform",
                "source_platform": "android",
                "activation_method": "android_accessibility_shortcut",
                "autonomy_mode": "full_auto",
                "device_id": "android-test"
            }))
            .to_request();
        let create_resp = test::call_service(&app, create_req).await;
        assert_eq!(create_resp.status(), StatusCode::ACCEPTED);
        let created: Value = test::read_body_json(create_resp).await;
        let command_id = created["data"]["command_id"].as_str().unwrap();

        let get_req = test::TestRequest::get()
            .uri(&format!("/api/v1/assistant/commands/{command_id}"))
            .to_request();
        let get_resp = test::call_service(&app, get_req).await;
        assert_eq!(get_resp.status(), StatusCode::OK);
        let fetched: Value = test::read_body_json(get_resp).await;
        assert_eq!(fetched["data"]["command_id"], command_id);
    }

    #[actix_web::test]
    async fn assistant_command_rejects_unsafe_full_auto_execution() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/assistant/commands")
            .set_json(serde_json::json!({
                "text": "dump private key",
                "source_platform": "windows",
                "activation_method": "windows_hotkey",
                "autonomy_mode": "full_auto",
                "device_id": "desktop-test"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["data"]["action_plan"]["executable"], false);
        assert_eq!(json["data"]["action_plan"]["requires_approval"], true);
    }

    #[actix_web::test]
    async fn assistant_command_rejects_incorrect_production_admin_token() {
        let state = AppState::new(test_config("production", Some("correct-token"))).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/assistant/commands")
            .insert_header(("x-astra-admin-token", "wrong-token"))
            .set_json(serde_json::json!({
                "text": "open https://example.com",
                "source_platform": "windows",
                "activation_method": "windows_hotkey",
                "autonomy_mode": "full_auto",
                "device_id": "desktop-test"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn os_guardian_event_submission_returns_receipt() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/os-guardian/events")
            .set_json(serde_json::json!({
                "kind": "network",
                "source": "sensor.local",
                "subject": "https://pastebin.com/upload",
                "pid": 99,
                "metadata": {
                    "payload_hint": "credential",
                    "api_key": "secret-value"
                }
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let json: Value = test::read_body_json(resp).await;
        assert!(json["data"]["receipt"]["receipt_id"].as_str().is_some());
        assert!(
            json["data"]["event"]["metadata"]["api_key"]
                .as_str()
                .unwrap()
                .starts_with("redacted:sha3:")
        );
    }

    #[actix_web::test]
    async fn os_guardian_ledger_verify_endpoint_reports_valid_chain() {
        let state = AppState::new(test_config("development", None)).expect("state");
        state
            .os_guardian
            .submit_event(astra_core::os_guardian::SubmitGuardianEventRequest {
                kind: astra_core::os_guardian::OsGuardianEventKind::Process,
                source: "sensor.local".into(),
                subject: "powershell -enc suspicious".into(),
                pid: Some(7),
                metadata: Default::default(),
                observed_at_ms: None,
            })
            .expect("event");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/v1/os-guardian/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["data"]["ledger_size"], 1);
    }

    #[actix_web::test]
    async fn os_guardian_rejects_incorrect_production_admin_token() {
        let state = AppState::new(test_config("production", Some("correct-token"))).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/os-guardian/events")
            .insert_header(("x-astra-admin-token", "wrong-token"))
            .set_json(serde_json::json!({
                "kind": "process",
                "source": "sensor.local",
                "subject": "cmd.exe"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn self_modification_rejects_incorrect_production_admin_token() {
        // The binary self-modification route must be admin-gated in production.
        let state = AppState::new(test_config("production", Some("correct-token"))).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/asc2/self-modifications")
            .insert_header(("x-astra-admin-token", "wrong-token"))
            .set_json(serde_json::json!({
                "workspace_path": ".",
                "changed_paths": ["asc2/prompts/policy.txt"],
                "current_binary": "server.bin"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn self_modification_rejects_paths_outside_the_boundary() {
        // Dev mode skips the admin gate, so the request reaches the changed-path
        // allowlist — which must reject a source path outside the self-mod
        // boundary (this is what stops the server rewriting arbitrary files).
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/asc2/self-modifications")
            .set_json(serde_json::json!({
                "workspace_path": ".",
                "changed_paths": ["crates/core/src/asc2/mod.rs"],
                "current_binary": "server.bin"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn os_guardian_dlp_analyze_returns_verdict_and_receipt() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/os-guardian/dlp/analyze")
            .set_json(serde_json::json!({
                "source_process": "powershell.exe",
                "subject": "wallet.dat upload",
                "destination": "https://pastebin.com/upload",
                "content_sample": "wallet seed phrase recovery phrase",
                "metadata": {
                    "api_key": "secret-value"
                }
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let json: Value = test::read_body_json(resp).await;
        assert!(json["data"]["receipt"]["receipt_id"].as_str().is_some());
        assert_eq!(
            json["data"]["verdict"]["recommended_control"],
            "pending_owner_approval"
        );
    }

    #[actix_web::test]
    async fn os_guardian_dlp_policy_endpoint_works() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/v1/os-guardian/dlp/policy")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["data"]["mode"], "personal_assistant");
        assert_eq!(json["data"]["secrets_never_return_raw"], true);
    }

    #[actix_web::test]
    async fn httpa_capabilities_endpoint_works() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/v1/httpa/capabilities")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json: Value = test::read_body_json(resp).await;
        assert_eq!(json["data"]["version"], "1.0");
        assert!(
            json["data"]["safety_boundaries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "owner_enrolled_devices_only")
        );
    }

    #[actix_web::test]
    async fn httpa_session_intent_and_receipt_flow_works() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let session_req = test::TestRequest::post()
            .uri("/api/v1/httpa/sessions")
            .set_json(serde_json::json!({
                "device_id": "owner-desktop",
                "device_capabilities": ["windows"],
                "action_allowlist": ["semantic_render"]
            }))
            .to_request();
        let session_resp = test::call_service(&app, session_req).await;
        assert_eq!(session_resp.status(), StatusCode::CREATED);
        let session_json: Value = test::read_body_json(session_resp).await;
        let session_id = session_json["data"]["session_id"].as_str().unwrap();
        let session_token = session_json["data"]["session_token"].as_str().unwrap();

        let intent_req = test::TestRequest::post()
            .uri("/api/v1/httpa/intents")
            .set_json(serde_json::json!({
                "session_id": session_id,
                "session_token": session_token,
                "activity_kind": "research",
                "intent": "summarize public HTTPA sources",
                "requested_tools": ["source_fetcher", "citation_ranker", "receipt_notary"]
            }))
            .to_request();
        let intent_resp = test::call_service(&app, intent_req).await;
        assert_eq!(intent_resp.status(), StatusCode::ACCEPTED);
        let intent_json: Value = test::read_body_json(intent_resp).await;
        assert_eq!(intent_json["data"]["status"], "blocked");
        assert!(intent_json["data"]["blocked_reason"].as_str().is_some());
        let receipt_id = intent_json["data"]["receipt_id"].as_str().unwrap();

        let receipt_req = test::TestRequest::get()
            .uri(&format!("/api/v1/httpa/receipts/{receipt_id}"))
            .to_request();
        let receipt_resp = test::call_service(&app, receipt_req).await;
        assert_eq!(receipt_resp.status(), StatusCode::OK);
        let receipt_json: Value = test::read_body_json(receipt_resp).await;
        assert_eq!(receipt_json["data"]["receipt_id"], receipt_id);
    }

    #[actix_web::test]
    async fn httpa_intent_rejects_invalid_session_token() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let session_req = test::TestRequest::post()
            .uri("/api/v1/httpa/sessions")
            .set_json(serde_json::json!({
                "device_id": "owner-desktop",
                "action_allowlist": ["research"]
            }))
            .to_request();
        let session_resp = test::call_service(&app, session_req).await;
        let session_json: Value = test::read_body_json(session_resp).await;
        let session_id = session_json["data"]["session_id"].as_str().unwrap();

        let intent_req = test::TestRequest::post()
            .uri("/api/v1/httpa/intents")
            .set_json(serde_json::json!({
                "session_id": session_id,
                "session_token": "wrong",
                "activity_kind": "research",
                "intent": "summarize public HTTPA sources"
            }))
            .to_request();
        let intent_resp = test::call_service(&app, intent_req).await;
        assert_eq!(intent_resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn research_job_returns_calculus_diagnostics_and_readiness() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let create_req = test::TestRequest::post()
            .uri("/api/v1/research/jobs")
            .set_json(serde_json::json!({
                "tenant_scope": "global",
                "query": "enterprise research evidence",
                "seed_documents": [{
                    "url": "https://example.com/research",
                    "html": "<html><head><title>Research Evidence</title></head><body>According to source material, enterprise research needs verifiable evidence and provenance.</body></html>"
                }]
            }))
            .to_request();
        let create_resp = test::call_service(&app, create_req).await;
        assert_eq!(create_resp.status(), StatusCode::ACCEPTED);
        let created: Value = test::read_body_json(create_resp).await;
        let job_id = created["data"]["id"].as_str().unwrap();
        assert_eq!(
            created["data"]["calculus_report"]["formula_version"],
            "enterprise_research_calculus_v1"
        );
        assert!(
            created["data"]["source_assessments"]
                .as_array()
                .unwrap()
                .len()
                == 1
        );
        assert!(
            created["data"]["provenance_chain"]
                .as_array()
                .unwrap()
                .len()
                == 1
        );

        let readiness_req = test::TestRequest::get()
            .uri(&format!("/api/v1/research/jobs/{job_id}/readiness"))
            .to_request();
        let readiness_resp = test::call_service(&app, readiness_req).await;
        assert_eq!(readiness_resp.status(), StatusCode::OK);
        let readiness: Value = test::read_body_json(readiness_resp).await;
        assert!(
            readiness["data"]["service_readiness_index"]
                .as_f64()
                .is_some()
        );
        assert!(readiness["data"]["halt"].as_bool().is_some());
    }

    #[actix_web::test]
    async fn search_intelligence_runs_full_formula_engine() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/search/intelligence")
            .set_json(serde_json::json!({
                "query": "deep research evidence",
                "mode": "deep",
                "max_sources": 8,
                "max_crawl_steps": 6,
                "freshness_horizon_hours": 168,
                "source_classes": ["docs", "papers"],
                "autonomous_crawl": true,
                "urls": ["http://127.0.0.1/admin", "http://example.onion/report"],
                "seed_documents": [{
                    "url": "https://docs.example/research",
                    "source_class": "docs",
                    "html": "<html><head><title>Research Docs</title></head><body>According to source material, deep research evidence needs provenance, counterevidence, coverage, triangulation, and audit replay. See https://papers.example/study for more.</body></html>"
                }]
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);
        let payload: Value = test::read_body_json(resp).await;
        assert_eq!(
            payload["data"]["result"]["formula_report"]["formula_version"],
            "nexus_f_omega_v1"
        );
        assert_eq!(
            payload["data"]["result"]["formula_report"]["metrics"]
                .as_array()
                .map(Vec::len),
            Some(45)
        );
        assert!(
            payload["data"]["result"]["crawl_plan"]["blocked"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains("private")))
        );
        assert!(
            payload["data"]["result"]["lineage"]["evidence_hashes"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            payload["data"]["sandbox_receipt"]["receipt_id"]
                .as_str()
                .is_some()
        );

        let stats_req = test::TestRequest::get()
            .uri("/api/v1/search/intelligence/stats")
            .to_request();
        let stats_resp = test::call_service(&app, stats_req).await;
        assert_eq!(stats_resp.status(), StatusCode::OK);
        let stats: Value = test::read_body_json(stats_resp).await;
        assert_eq!(stats["data"]["total_queries"], 1);
        assert_eq!(stats["data"]["autonomous_queries"], 1);

        let tick_req = test::TestRequest::post()
            .uri("/api/v1/search/intelligence/tick")
            .to_request();
        let tick_resp = test::call_service(&app, tick_req).await;
        assert_eq!(tick_resp.status(), StatusCode::ACCEPTED);

        let frontier_req = test::TestRequest::get()
            .uri("/api/v1/search/intelligence/frontier")
            .to_request();
        let frontier_resp = test::call_service(&app, frontier_req).await;
        assert_eq!(frontier_resp.status(), StatusCode::OK);
        let frontier: Value = test::read_body_json(frontier_resp).await;
        assert!(
            frontier["data"]["queued_queries"]
                .as_u64()
                .is_some_and(|count| count > 0)
        );
    }

    #[actix_web::test]
    async fn asc2_status_and_mission_endpoints_work() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let status_req = test::TestRequest::get()
            .uri("/api/v1/asc2/status")
            .to_request();
        let status_resp = test::call_service(&app, status_req).await;
        assert_eq!(status_resp.status(), StatusCode::OK);
        let status: Value = test::read_body_json(status_resp).await;
        assert_eq!(status["data"]["mode"], "shadow");

        let mission_req = test::TestRequest::post()
            .uri("/api/v1/asc2/missions")
            .set_json(serde_json::json!({
                "objective": "Plan a verified migration with rollback",
                "activity_kind": "planning",
                "owner_authorized": true
            }))
            .to_request();
        let mission_resp = test::call_service(&app, mission_req).await;
        assert_eq!(mission_resp.status(), StatusCode::ACCEPTED);
        let mission: Value = test::read_body_json(mission_resp).await;
        assert!(
            mission["data"]["diagnostics"]["agents"]
                .as_array()
                .unwrap()
                .len()
                >= 3
        );
        assert!(mission["data"]["answer"].as_str().is_some());
    }

    #[actix_web::test]
    async fn sandbox_status_and_audit_endpoints_work() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let status_req = test::TestRequest::get()
            .uri("/api/v1/sandbox/status")
            .to_request();
        let status_resp = test::call_service(&app, status_req).await;
        assert_eq!(status_resp.status(), StatusCode::OK);
        let status: Value = test::read_body_json(status_resp).await;
        assert_eq!(status["data"]["block_by_default"], true);
        assert_eq!(status["data"]["container_required"], true);
        assert_eq!(
            status["data"]["reference_monitor"],
            "astra_unified_reference_monitor"
        );

        let execute_req = test::TestRequest::post()
            .uri("/api/v1/sandbox/actions/execute")
            .set_json(serde_json::json!({
                "action_id": "",
                "kind": "unknown",
                "resource_limits": {
                    "timeout_ms": 1000,
                    "max_output_bytes": 1024,
                    "memory_mb": 64,
                    "cpu_units": 1
                }
            }))
            .to_request();
        let execute_resp = test::call_service(&app, execute_req).await;
        assert_eq!(execute_resp.status(), StatusCode::ACCEPTED);
        let executed: Value = test::read_body_json(execute_resp).await;
        assert_eq!(executed["data"]["executed"], false);
        assert_eq!(
            executed["data"]["observation"]["reference_monitor"],
            "astra_unified_reference_monitor"
        );
        assert!(
            matches!(
                executed["data"]["decision"]["outcome"].as_str(),
                Some("deny")
            ),
            "unexpected sandbox outcome: {}",
            executed["data"]["decision"]["outcome"]
        );

        let audit_req = test::TestRequest::get()
            .uri("/api/v1/sandbox/audit")
            .to_request();
        let audit_resp = test::call_service(&app, audit_req).await;
        assert_eq!(audit_resp.status(), StatusCode::OK);
        let audit: Value = test::read_body_json(audit_resp).await;
        assert_eq!(audit["data"].as_array().map(Vec::len), Some(1));
    }

    #[actix_web::test]
    async fn generated_runtime_execution_uses_unified_sandbox_guard() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let agent_req = test::TestRequest::post()
            .uri("/api/v1/runtime/generated-agents")
            .set_json(serde_json::json!({
                "tenant_scope": "global",
                "goal": "Summarize governed enterprise work",
                "tool_allowlist": ["summarizer"],
                "output_schema": {"type": "object"},
                "cost_budget": 1,
                "time_budget_ms": 1000,
                "risk_tier": "low",
                "escalation_policy": "block"
            }))
            .to_request();
        let agent_resp = test::call_service(&app, agent_req).await;
        assert_eq!(agent_resp.status(), StatusCode::CREATED);
        let agent: Value = test::read_body_json(agent_resp).await;
        let agent_id = agent["data"]["manifest_id"].as_str().unwrap();

        let execution_req = test::TestRequest::post()
            .uri(&format!("/api/v1/agents/{agent_id}/executions"))
            .set_json(serde_json::json!({
                "input": {"brief": "use the sandbox"},
                "requested_tools": ["summarizer"],
                "resource_limits": {"timeout_ms": 1000}
            }))
            .to_request();
        let execution_resp = test::call_service(&app, execution_req).await;
        assert_eq!(execution_resp.status(), StatusCode::CREATED);
        let execution: Value = test::read_body_json(execution_resp).await;
        assert_eq!(
            execution["data"]["sandbox"]["observation"]["reference_monitor"],
            "astra_unified_reference_monitor"
        );
        assert!(matches!(
            execution["data"]["sandbox"]["decision"]["outcome"].as_str(),
            Some("sandbox_unavailable") | Some("allow") | Some("rewrite")
        ));
    }

    #[actix_web::test]
    async fn swarm_endpoint_runs_a_hundred_agents_in_one_call() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let agent_req = test::TestRequest::post()
            .uri("/api/v1/runtime/generated-agents")
            .set_json(serde_json::json!({
                "tenant_scope": "global",
                "goal": "Summarize and count many briefs concurrently",
                "tool_allowlist": ["summarizer", "word_counter"],
                "output_schema": {"type": "object"},
                "cost_budget": 10,
                "time_budget_ms": 30000,
                "risk_tier": "low",
                "escalation_policy": "block"
            }))
            .to_request();
        let agent_resp = test::call_service(&app, agent_req).await;
        assert_eq!(agent_resp.status(), StatusCode::CREATED);
        let agent: Value = test::read_body_json(agent_resp).await;
        let agent_id = agent["data"]["manifest_id"].as_str().unwrap();

        let spawn_req = test::TestRequest::post()
            .uri("/api/v1/runtime/swarms")
            .set_json(serde_json::json!({
                "agents": [{
                    "manifest_id": agent_id,
                    "input": {"brief": "alpha beta gamma delta epsilon"},
                    "replicate": 100
                }]
            }))
            .to_request();
        let spawn_resp = test::call_service(&app, spawn_req).await;
        assert_eq!(spawn_resp.status(), StatusCode::ACCEPTED);
        let spawned: Value = test::read_body_json(spawn_resp).await;
        assert_eq!(spawned["data"]["swarm"]["admitted"], 100);
        assert_eq!(spawned["data"]["swarm"]["rejected"], 0);
        // Lanes are governed by the expansion law, never one per agent.
        assert!(spawned["data"]["swarm"]["worker_target"].as_u64().unwrap() < 100);
        assert!(
            spawned["data"]["sandbox"]["observation"]["reference_monitor"]
                .as_str()
                .is_some()
        );
        let swarm_id = spawned["data"]["swarm"]["swarm_id"].as_str().unwrap();

        let wait_req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/runtime/swarms/{swarm_id}/wait?timeout_ms=60000"
            ))
            .to_request();
        let wait_resp = test::call_service(&app, wait_req).await;
        assert_eq!(wait_resp.status(), StatusCode::OK);
        let status: Value = test::read_body_json(wait_resp).await;
        assert_eq!(status["data"]["completed"], 100, "status: {status}");
        assert_eq!(status["data"]["failed"], 0);
        assert_eq!(status["data"]["in_flight"], 0);
        assert!(status["data"]["completed_at_ms"].is_number());

        let status_req = test::TestRequest::get()
            .uri(&format!("/api/v1/runtime/swarms/{swarm_id}"))
            .to_request();
        let status_resp = test::call_service(&app, status_req).await;
        assert_eq!(status_resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn fabricated_agent_builds_tools_and_handles_files_end_to_end() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        // 1. Fabricate an agent for a complex objective with file handling.
        let fabricate_req = test::TestRequest::post()
            .uri("/api/v1/runtime/fabricate")
            .set_json(serde_json::json!({
                "tenant_scope": "global",
                "objective": "research governed compliance evidence and write a summary report file"
            }))
            .to_request();
        let fabricate_resp = test::call_service(&app, fabricate_req).await;
        assert_eq!(fabricate_resp.status(), StatusCode::CREATED);
        let fabricated: Value = test::read_body_json(fabricate_resp).await;
        let agent_id = fabricated["data"]["agent"]["manifest_id"].as_str().unwrap();
        let custom_tool = fabricated["data"]["tools"][0]["name"].as_str().unwrap();
        assert_eq!(fabricated["data"]["tools"][0]["canary_status"], "passed");
        assert!(
            fabricated["data"]["agent"]["tool_allowlist"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool == "file_writer")
        );

        // 2. The fabricated agent runs its own fabricated tool, then writes
        //    the result into its jailed workspace and reads it back.
        let run = |tools: serde_json::Value, input: serde_json::Value| {
            test::TestRequest::post()
                .uri(&format!("/api/v1/agents/{agent_id}/executions"))
                .set_json(serde_json::json!({
                    "input": input,
                    "requested_tools": tools,
                    "resource_limits": {"timeout_ms": 1000}
                }))
                .to_request()
        };
        let digest_resp = test::call_service(
            &app,
            run(
                serde_json::json!([custom_tool]),
                serde_json::json!({"brief": "all controls passed this quarter"}),
            ),
        )
        .await;
        assert_eq!(digest_resp.status(), StatusCode::CREATED);

        let write_resp = test::call_service(
            &app,
            run(
                serde_json::json!(["file_writer"]),
                serde_json::json!({"path": "reports/q1.md", "content": "# Q1\nAll controls passed."}),
            ),
        )
        .await;
        assert_eq!(write_resp.status(), StatusCode::CREATED);

        let read_resp = test::call_service(
            &app,
            run(
                serde_json::json!(["file_reader"]),
                serde_json::json!({"path": "reports/q1.md"}),
            ),
        )
        .await;
        assert_eq!(read_resp.status(), StatusCode::CREATED);
        let read_body: Value = test::read_body_json(read_resp).await;
        assert!(
            read_body["data"]["steps"][0]["output_summary"]
                .as_str()
                .unwrap()
                .contains("All controls passed")
        );
    }

    #[actix_web::test]
    async fn chronicle_records_recalls_briefs_and_verifies() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        // A decision with rationale and an already-overdue commitment.
        let decision_req = test::TestRequest::post()
            .uri("/api/v1/chronicle/episodes")
            .set_json(serde_json::json!({
                "kind": "decision",
                "content": "adopted chronicle as the continuity engine",
                "rationale": "execution receipts were being buried and context was lost",
                "importance": 0.8
            }))
            .to_request();
        let decision_resp = test::call_service(&app, decision_req).await;
        assert_eq!(decision_resp.status(), StatusCode::CREATED);

        let commitment_req = test::TestRequest::post()
            .uri("/api/v1/chronicle/episodes")
            .set_json(serde_json::json!({
                "kind": "commitment",
                "content": "rotate the admin token",
                "due_at_ms": 1000
            }))
            .to_request();
        let commitment_resp = test::call_service(&app, commitment_req).await;
        assert_eq!(commitment_resp.status(), StatusCode::CREATED);
        let commitment: Value = test::read_body_json(commitment_resp).await;
        let commitment_id = commitment["data"]["episode_id"].as_str().unwrap();

        // Recall answers "why" with the recorded decision.
        let recall_req = test::TestRequest::post()
            .uri("/api/v1/chronicle/recall")
            .set_json(serde_json::json!({
                "query": "why did we adopt the continuity engine"
            }))
            .to_request();
        let recall_resp = test::call_service(&app, recall_req).await;
        assert_eq!(recall_resp.status(), StatusCode::OK);
        let recall: Value = test::read_body_json(recall_resp).await;
        assert!(!recall["data"]["matches"].as_array().unwrap().is_empty());
        assert!(
            recall["data"]["matches"][0]["episode"]["content"]
                .as_str()
                .unwrap()
                .contains("continuity engine")
        );

        // The brief surfaces the overdue commitment alongside live subsystem stats.
        let brief_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/chronicle/brief")
                .to_request(),
        )
        .await;
        assert_eq!(brief_resp.status(), StatusCode::OK);
        let brief: Value = test::read_body_json(brief_resp).await;
        assert_eq!(
            brief["data"]["chronicle"]["overdue_commitments"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(brief["data"]["autonomy"]["total_jobs"].is_number());
        assert!(brief["data"]["weave"]["total_intents"].is_number());

        // Resolving clears the commitment.
        let resolve_resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!(
                    "/api/v1/chronicle/commitments/{commitment_id}/resolve"
                ))
                .to_request(),
        )
        .await;
        assert_eq!(resolve_resp.status(), StatusCode::OK);

        let stats_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/chronicle/stats")
                .to_request(),
        )
        .await;
        assert_eq!(stats_resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn weave_intent_substrate_composes_and_materializes_experiences() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        // A fragmented-life intent is classified, bound to its sources, and
        // materialized into a fabricated agent with a completed plan.
        let submit_req = test::TestRequest::post()
            .uri("/api/v1/weave/intents")
            .set_json(serde_json::json!({
                "description": "Unify my conversations across WhatsApp, Telegram and Discord into a single daily digest inbox"
            }))
            .to_request();
        let submit_resp = test::call_service(&app, submit_req).await;
        assert_eq!(submit_resp.status(), StatusCode::CREATED);
        let body: Value = test::read_body_json(submit_resp).await;
        let intent = &body["data"]["intent"];
        assert_eq!(intent["status"], "woven");
        assert_eq!(intent["kind"], "communicate");
        assert!(intent["agent_id"].as_str().is_some());
        let sources: Vec<&str> = intent["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        for source in ["whatsapp", "telegram", "discord"] {
            assert!(sources.contains(&source), "missing source {source}");
        }
        assert!(
            intent["plan"]
                .as_array()
                .unwrap()
                .iter()
                .all(|step| step["status"] == "completed")
        );
        // The intent is bound to real connector profiles, guarded by the
        // sandbox reference monitor, and notarized over HTTPA.
        assert!(
            intent["connector_bindings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|binding| binding == "telegram")
        );
        assert_eq!(
            body["data"]["sandbox_receipt"]["observation"]["reference_monitor"],
            "astra_unified_reference_monitor"
        );
        assert!(body["data"]["httpa_receipt"]["receipt_id"].is_string());
        let intent_id = intent["intent_id"].as_str().unwrap();

        // The woven intent is retrievable.
        let get_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/weave/intents/{intent_id}"))
                .to_request(),
        )
        .await;
        assert_eq!(get_resp.status(), StatusCode::OK);

        // Money-earning intents are refused by the substrate.
        let refused_req = test::TestRequest::post()
            .uri("/api/v1/weave/intents")
            .set_json(serde_json::json!({
                "description": "weave a workflow that earn money for me automatically"
            }))
            .to_request();
        let refused_resp = test::call_service(&app, refused_req).await;
        assert_eq!(refused_resp.status(), StatusCode::BAD_REQUEST);

        let stats_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/weave/stats")
                .to_request(),
        )
        .await;
        assert_eq!(stats_resp.status(), StatusCode::OK);
        let stats: Value = test::read_body_json(stats_resp).await;
        assert_eq!(stats["data"]["total_intents"], 1);
        assert_eq!(stats["data"]["woven"], 1);
        assert_eq!(
            stats["data"]["sources_unified"].as_u64().unwrap() as usize,
            sources.len()
        );
    }

    #[actix_web::test]
    async fn nexus_saas_event_workflow_and_dashboard_endpoints_work() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let plans_req = test::TestRequest::get()
            .uri("/api/v1/nexus/plans")
            .to_request();
        let plans_resp = test::call_service(&app, plans_req).await;
        assert_eq!(plans_resp.status(), StatusCode::OK);
        let plans: Value = test::read_body_json(plans_resp).await;
        assert_eq!(plans["data"].as_array().unwrap().len(), 3);
        assert!(plans["data"].as_array().unwrap().iter().any(|plan| {
            plan["plan"] == "starter"
                && plan["monthly_price_usd"] == 499
                && plan["included_agent_limit"] == 2
        }));
        assert!(plans["data"].as_array().unwrap().iter().any(|plan| {
            plan["plan"] == "enterprise" && plan["annual_contract_range_usd"].is_array()
        }));

        let integrations_req = test::TestRequest::get()
            .uri("/api/v1/nexus/integrations/catalog")
            .to_request();
        let integrations_resp = test::call_service(&app, integrations_req).await;
        assert_eq!(integrations_resp.status(), StatusCode::OK);
        let integrations: Value = test::read_body_json(integrations_resp).await;
        assert!(
            integrations["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|provider| {
                    provider["provider"] == "calendly"
                        && provider["supported_agent_keys"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|agent| agent == "sales_os")
                })
        );
        assert!(
            integrations["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|provider| {
                    provider["provider"] == "government_portals"
                        && provider["httpa_required"] == true
                        && provider["sandbox_required"] == true
                })
        );

        let create_org = test::TestRequest::post()
            .uri("/api/v1/nexus/organizations")
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({
                "name": "Acme Automation",
                "plan": "enterprise",
                "currency": "USD",
                "timezone": "UTC"
            }))
            .to_request();
        let create_org_response = test::call_service(&app, create_org).await;
        assert_eq!(create_org_response.status(), StatusCode::CREATED);
        let organization: Value = test::read_body_json(create_org_response).await;
        let organization_id = organization["data"]["organization_id"]
            .as_str()
            .expect("organization id");

        let enable_agent = test::TestRequest::put()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/agents/revenue_leak_detector"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({"enabled":true}))
            .to_request();
        assert_eq!(
            test::call_service(&app, enable_agent).await.status(),
            StatusCode::OK
        );

        let ingest = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/events"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({
                "source": "erp",
                "event_type": "finance.delivery_billing",
                "subject_type": "customer",
                "subject_id": "customer-42",
                "idempotency_key": "delivery-billing-42",
                "data": {"delivered_value":125000.0,"invoiced_value":100000.0}
            }))
            .to_request();
        let ingest_response = test::call_service(&app, ingest).await;
        assert_eq!(ingest_response.status(), StatusCode::ACCEPTED);
        let event: Value = test::read_body_json(ingest_response).await;
        let event_id = event["data"]["event"]["event_id"]
            .as_str()
            .expect("event id");

        let run = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/agents/revenue_leak_detector/runs"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({"event_ids":[event_id]}))
            .to_request();
        let run_response = test::call_service(&app, run).await;
        assert_eq!(run_response.status(), StatusCode::ACCEPTED);
        let run: Value = test::read_body_json(run_response).await;
        assert_eq!(run["data"]["estimated_annual_value"], 25_000.0);
        assert_eq!(run["data"]["actions"][0]["status"], "pending_approval");

        let dashboard = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/dashboard"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .to_request();
        let dashboard_response = test::call_service(&app, dashboard).await;
        assert_eq!(dashboard_response.status(), StatusCode::OK);
        let dashboard: Value = test::read_body_json(dashboard_response).await;
        assert_eq!(dashboard["data"]["pending_approvals"], 1);
        assert_eq!(dashboard["data"]["estimated_annual_value"], 25_000.0);
    }

    #[actix_web::test]
    async fn nexus_revenue_agents_and_business_brain_endpoints_work() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let catalog_req = test::TestRequest::get()
            .uri("/api/v1/nexus/agents/catalog")
            .to_request();
        let catalog_resp = test::call_service(&app, catalog_req).await;
        assert_eq!(catalog_resp.status(), StatusCode::OK);
        let catalog: Value = test::read_body_json(catalog_resp).await;
        assert_eq!(catalog["data"].as_array().unwrap().len(), 34);
        assert!(catalog["data"].as_array().unwrap().iter().any(|agent| {
            agent["key"] == "business_brain"
                && agent["default_action_types"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|action| action == "create_business_brain_plan")
        }));

        let create_org = test::TestRequest::post()
            .uri("/api/v1/nexus/organizations")
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({
                "name": "Revenue Lab",
                "plan": "enterprise"
            }))
            .to_request();
        let create_org_response = test::call_service(&app, create_org).await;
        assert_eq!(create_org_response.status(), StatusCode::CREATED);
        let organization: Value = test::read_body_json(create_org_response).await;
        let organization_id = organization["data"]["organization_id"].as_str().unwrap();

        let create_interview = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/business-brain/interviews"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({
                "founder_name": "Asha",
                "target_revenue": 1200000.0,
                "context": {"market":"education"}
            }))
            .to_request();
        let create_interview_response = test::call_service(&app, create_interview).await;
        assert_eq!(create_interview_response.status(), StatusCode::CREATED);
        let created: Value = test::read_body_json(create_interview_response).await;
        assert_eq!(
            created["data"]["interview"]["questions"]
                .as_array()
                .unwrap()
                .len(),
            40
        );
        assert!(
            created["data"]["sandbox_receipt"]["receipt_id"]
                .as_str()
                .is_some()
        );
        assert_eq!(
            created["data"]["sandbox_receipt"]["observation"]["reference_monitor"],
            "astra_unified_reference_monitor"
        );
        let interview_id = created["data"]["interview"]["interview_id"]
            .as_str()
            .unwrap();
        let mut answers = serde_json::Map::new();
        for index in 1..=40 {
            answers.insert(
                format!("q{index:02}"),
                serde_json::Value::String(format!(
                    "Answer {index} with skills, assets, network, problem, and execution detail"
                )),
            );
        }

        let answers_req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/business-brain/interviews/{interview_id}/answers"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({ "answers": answers }))
            .to_request();
        let answers_resp = test::call_service(&app, answers_req).await;
        assert_eq!(answers_resp.status(), StatusCode::OK);
        let answered: Value = test::read_body_json(answers_resp).await;
        assert_eq!(answered["data"]["interview"]["status"], "ready_for_plan");

        let plan_req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/business-brain/interviews/{interview_id}/plan"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .to_request();
        let plan_resp = test::call_service(&app, plan_req).await;
        assert_eq!(plan_resp.status(), StatusCode::OK);
        let planned: Value = test::read_body_json(plan_resp).await;
        assert_eq!(planned["data"]["interview"]["status"], "planned");
        assert_eq!(
            planned["data"]["interview"]["plan"]["ninety_day_roadmap"]
                .as_array()
                .unwrap()
                .len(),
            4
        );

        let enable_agent = test::TestRequest::put()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/agents/dormant_asset_monetization"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({"enabled":true}))
            .to_request();
        assert_eq!(
            test::call_service(&app, enable_agent).await.status(),
            StatusCode::OK
        );
        let ingest = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/events"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({
                "source": "asset_audit",
                "event_type": "revenue.asset_inventory",
                "subject_type": "warehouse",
                "subject_id": "mumbai-weekend-capacity",
                "data": {
                    "idle_capacity_hours": 100.0,
                    "capacity_rate": 500.0,
                    "buyer_segment": "nearby ecommerce sellers"
                }
            }))
            .to_request();
        let ingest_resp = test::call_service(&app, ingest).await;
        assert_eq!(ingest_resp.status(), StatusCode::ACCEPTED);
        let event: Value = test::read_body_json(ingest_resp).await;
        let event_id = event["data"]["event"]["event_id"].as_str().unwrap();
        let run = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/agents/dormant_asset_monetization/runs"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .set_json(serde_json::json!({"event_ids":[event_id]}))
            .to_request();
        let run_resp = test::call_service(&app, run).await;
        assert_eq!(run_resp.status(), StatusCode::ACCEPTED);
        let run: Value = test::read_body_json(run_resp).await;
        assert_eq!(
            run["data"]["revenue_opportunities"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(run["data"]["actions"][0]["status"], "pending_approval");

        let opportunities = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/nexus/organizations/{organization_id}/revenue-opportunities"
            ))
            .insert_header(("x-nexus-principal-id", "founder"))
            .to_request();
        let opportunities_resp = test::call_service(&app, opportunities).await;
        assert_eq!(opportunities_resp.status(), StatusCode::OK);
        let opportunities: Value = test::read_body_json(opportunities_resp).await;
        assert!(
            opportunities["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|opportunity| opportunity["agent"] == "dormant_asset_monetization")
        );
        assert!(
            opportunities["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|opportunity| opportunity["agent"] == "business_brain")
        );
    }

    #[actix_web::test]
    async fn nexus_production_routes_require_admin_token_and_principal() {
        let state = AppState::new(test_config("production", Some("correct-token"))).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/api/v1/nexus/organizations")
            .insert_header(("x-nexus-principal-id", "founder"))
            .insert_header(("x-astra-admin-token", "wrong-token"))
            .set_json(serde_json::json!({"name":"Blocked Corp"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, request).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn os_guardian_dlp_rejects_incorrect_production_admin_token() {
        let state = AppState::new(test_config("production", Some("correct-token"))).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/v1/os-guardian/dlp/analyze")
            .insert_header(("x-astra-admin-token", "wrong-token"))
            .set_json(serde_json::json!({
                "source_process": "cmd.exe",
                "subject": "public note",
                "destination": "https://example.com"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn eval_learning_curriculum_and_experiment_endpoints_work() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        let suite_req = test::TestRequest::post()
            .uri("/api/v1/evals/suites")
            .set_json(serde_json::json!({
                "name": "contract smoke",
                "cases": [{
                    "name": "trivial pass",
                    "check": {"type":"trivial", "pass": true}
                }]
            }))
            .to_request();
        let suite_resp = test::call_service(&app, suite_req).await;
        assert_eq!(suite_resp.status(), StatusCode::OK);
        let suite: Value = test::read_body_json(suite_resp).await;
        let suite_id = suite["data"]["suite_id"].as_str().unwrap();

        let run_req = test::TestRequest::post()
            .uri(&format!("/api/v1/evals/suites/{suite_id}/run"))
            .set_json(serde_json::json!({}))
            .to_request();
        let run_resp = test::call_service(&app, run_req).await;
        assert_eq!(run_resp.status(), StatusCode::OK);
        let run: Value = test::read_body_json(run_resp).await;
        assert_eq!(run["data"]["pass"], true);
        assert_eq!(run["data"]["passed"], 1);

        let attach_req = test::TestRequest::post()
            .uri(&format!("/api/v1/evals/suites/{suite_id}/attach"))
            .set_json(serde_json::json!({
                "target_kind": "forge_tool",
                "target_id": "demo_tool"
            }))
            .to_request();
        let attach_resp = test::call_service(&app, attach_req).await;
        assert_eq!(attach_resp.status(), StatusCode::OK);

        let learn_req = test::TestRequest::post()
            .uri("/api/v1/learning/episodes")
            .set_json(serde_json::json!({
                "organ": "evals",
                "signal": "failure",
                "summary": "a calibration failure worth investigating",
                "score": 0.2
            }))
            .to_request();
        let learn_resp = test::call_service(&app, learn_req).await;
        assert_eq!(learn_resp.status(), StatusCode::OK);

        let curriculum_req = test::TestRequest::post()
            .uri("/api/v1/curriculum/next")
            .set_json(serde_json::json!({"limit": 5}))
            .to_request();
        let curriculum_resp = test::call_service(&app, curriculum_req).await;
        assert_eq!(curriculum_resp.status(), StatusCode::OK);
        let curriculum: Value = test::read_body_json(curriculum_resp).await;
        assert!(!curriculum["data"]["tasks"].as_array().unwrap().is_empty());

        let experiment_req = test::TestRequest::post()
            .uri("/api/v1/experiments")
            .set_json(serde_json::json!({
                "idea": "prove a trivial software invariant",
                "hypothesis": "the trivial invariant holds",
                "contract": {
                    "checks": [{"type":"trivial", "pass": true}]
                },
                "mint_claim": false
            }))
            .to_request();
        let experiment_resp = test::call_service(&app, experiment_req).await;
        assert_eq!(experiment_resp.status(), StatusCode::OK);
        let experiment: Value = test::read_body_json(experiment_resp).await;
        assert_eq!(experiment["data"]["experiment"]["status"], "verified");
    }

    #[actix_web::test]
    async fn production_config_requires_admin_token() {
        let error = match AppState::new(test_config("production", None)) {
            Ok(_) => panic!("production token should be required"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("ASTRA_ADMIN_TOKEN"));
    }

    #[actix_web::test]
    async fn genome_os_seeds_commons_runs_gates_and_forks() {
        let state = AppState::new(test_config("development", None)).expect("state");
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .configure(app_config),
        )
        .await;

        // The console is served at the root, embedded in the binary.
        let console =
            test::call_service(&app, test::TestRequest::get().uri("/console").to_request()).await;
        assert_eq!(console.status(), StatusCode::OK);

        // The billing beachhead is seeded and certified on first boot.
        let overview_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/genome/overview")
                .to_request(),
        )
        .await;
        assert_eq!(overview_resp.status(), StatusCode::OK);
        let overview: Value = test::read_body_json(overview_resp).await;
        assert_eq!(overview["data"]["total_genomes"], 4);
        assert!(
            overview["data"]["minted"].as_u64().unwrap()
                + overview["data"]["certified"].as_u64().unwrap()
                >= 4
        );

        // Find the invoice genome.
        let list_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/genome/genomes")
                .to_request(),
        )
        .await;
        let genomes: Value = test::read_body_json(list_resp).await;
        let invoice = genomes["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["operation"] == "billing.invoice")
            .expect("invoice genome");
        let invoice_id = invoice["genome_id"].as_str().unwrap();

        // A safe operation executes and meters its outcome.
        let safe = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/v1/genome/genomes/{invoice_id}/run"))
                .set_json(serde_json::json!({
                    "inputs": {
                        "amount": {"kind":"money","value":800.0},
                        "contract_value": {"kind":"money","value":1000.0},
                        "customer_active": {"kind":"flag","value":true},
                        "discount_pct": {"kind":"money","value":5.0}
                    },
                    "judgment": 800.0,
                    "outcome_value": 800.0
                }))
                .to_request(),
        )
        .await;
        assert_eq!(safe.status(), StatusCode::ACCEPTED);
        let safe_body: Value = test::read_body_json(safe).await;
        assert_eq!(safe_body["data"]["status"], "executed");
        assert!(safe_body["data"]["admitted"].as_bool().unwrap());

        // An operation that would breach money-conservation is blocked by the
        // envelope gate before executing — do no harm, by construction.
        let unsafe_run = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/v1/genome/genomes/{invoice_id}/run"))
                .set_json(serde_json::json!({
                    "inputs": {
                        "amount": {"kind":"money","value":5000.0},
                        "contract_value": {"kind":"money","value":1000.0},
                        "customer_active": {"kind":"flag","value":true},
                        "discount_pct": {"kind":"money","value":5.0}
                    },
                    "outcome_value": 5000.0
                }))
                .to_request(),
        )
        .await;
        assert_eq!(unsafe_run.status(), StatusCode::ACCEPTED);
        let unsafe_body: Value = test::read_body_json(unsafe_run).await;
        assert_eq!(unsafe_body["data"]["status"], "blocked");
        assert!(!unsafe_body["data"]["admitted"].as_bool().unwrap());

        // Forking by a plain-language refinement yields a child one generation
        // deeper that preserves the parent's guarantees.
        let dunning = genomes["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["operation"] == "billing.dunning")
            .unwrap();
        let dunning_id = dunning["genome_id"].as_str().unwrap();
        let fork = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/v1/genome/genomes/{dunning_id}/fork"))
                .set_json(serde_json::json!({"deviations": "require manager approval"}))
                .to_request(),
        )
        .await;
        assert_eq!(fork.status(), StatusCode::CREATED);
        let fork_body: Value = test::read_body_json(fork).await;
        assert_eq!(fork_body["data"]["genome"]["generation"], 1);
        assert!(
            fork_body["data"]["refinement"]["is_refinement"]
                .as_bool()
                .unwrap()
        );

        // The commons graph now has an extra refinement edge.
        let graph_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/genome/commons")
                .to_request(),
        )
        .await;
        assert_eq!(graph_resp.status(), StatusCode::OK);
        let graph: Value = test::read_body_json(graph_resp).await;
        assert!(!graph["data"]["edges"].as_array().unwrap().is_empty());

        // The underwriting desk reports concentration limits and a tail.
        let risk_resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/api/v1/genome/risk")
                .to_request(),
        )
        .await;
        assert_eq!(risk_resp.status(), StatusCode::OK);
        let risk: Value = test::read_body_json(risk_resp).await;
        assert!(
            risk["data"]["portfolio_tail_loss_99"].as_f64().unwrap()
                >= risk["data"]["portfolio_expected_loss"].as_f64().unwrap()
        );
    }

    fn test_config(environment: &str, admin_token: Option<&str>) -> AppConfig {
        AppConfig {
            service_name: "astra-personal-assistant".into(),
            host: "127.0.0.1".into(),
            port: 8080,
            environment: environment.into(),
            public_base_url: "http://127.0.0.1:8080".into(),
            data_dir: std::env::temp_dir()
                .join(format!(
                    "astra_server_test_{}_{}",
                    environment,
                    uuid_like_suffix()
                ))
                .to_string_lossy()
                .to_string(),
            cors_allowed_origins: vec!["http://127.0.0.1:8080".into()],
            tls_cert_path: None,
            tls_key_path: None,
            allow_cleartext_loopback: true,
            admin_token: admin_token.map(str::to_string),
        }
    }

    fn uuid_like_suffix() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        // Timestamps alone can collide across parallel tests on Windows, where
        // the system clock has coarse resolution; the counter guarantees
        // uniqueness within the process.
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        format!("{}_{}", nanos, COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

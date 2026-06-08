use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms, random_token, sha3_hex};

pub const HTTPA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaExecutionMode {
    VisibleBrowse,
    ResultOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaPrivacyMode {
    Identified,
    OriginShielded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpaLedgerMode {
    BoundIntent,
    VerifiedEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaExecutionPolicy {
    pub mode: HttpaExecutionMode,
    pub privacy: HttpaPrivacyMode,
    pub ledger_mode: HttpaLedgerMode,
    pub user_visible: bool,
    pub origin_shielding: bool,
    pub chain_receipt_required: bool,
    pub max_parallel_agents: u8,
    pub disallowed_features: Vec<String>,
}

impl Default for HttpaExecutionPolicy {
    fn default() -> Self {
        Self {
            mode: HttpaExecutionMode::VisibleBrowse,
            privacy: HttpaPrivacyMode::Identified,
            ledger_mode: HttpaLedgerMode::BoundIntent,
            user_visible: true,
            origin_shielding: false,
            chain_receipt_required: true,
            max_parallel_agents: 5,
            disallowed_features: disallowed_features(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaEnvelope<T> {
    pub version: String,
    pub session_id: Option<String>,
    pub trace_id: String,
    pub intent: Option<String>,
    pub payload: T,
    pub timestamp_ms: i64,
}

impl<T> HttpaEnvelope<T> {
    #[must_use]
    pub fn new(payload: T) -> Self {
        Self {
            version: HTTPA_VERSION.into(),
            session_id: None,
            trace_id: new_id("httpa_trace"),
            intent: None,
            payload,
            timestamp_ms: now_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateHttpaSessionRequest {
    pub device_id: String,
    #[serde(default)]
    pub device_capabilities: Vec<String>,
    #[serde(default)]
    pub action_allowlist: Vec<String>,
    #[serde(default)]
    pub requested_policy: Option<HttpaExecutionPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEnrollment {
    pub device_id_hash: String,
    pub capabilities: Vec<String>,
    pub action_allowlist: Vec<String>,
    pub owner_enrolled: bool,
    pub token_required: bool,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaSession {
    pub session_id: String,
    pub device_id_hash: String,
    pub session_token: String,
    #[serde(skip_serializing)]
    pub session_token_hash: String,
    pub policy: HttpaExecutionPolicy,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpaIntentRequest {
    pub session_id: String,
    pub session_token: String,
    pub activity_kind: String,
    pub intent: String,
    #[serde(default)]
    pub requested_tools: Vec<String>,
    #[serde(default)]
    pub resource_limits: std::collections::BTreeMap<String, u64>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaIntentResponse {
    pub intent_id: String,
    pub status: String,
    pub trace_id: String,
    pub receipt_id: String,
    pub receipt_hash: String,
    pub approval_required: bool,
    pub blocked_reason: Option<String>,
    pub orchestration_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaReceipt {
    pub receipt_id: String,
    pub trace_id: String,
    pub subject: String,
    pub subject_hash: String,
    pub payload_hash: String,
    pub block_height: u64,
    pub block_hash: String,
    pub previous_hash: String,
    pub external_anchor: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerIntegrityReport {
    pub valid: bool,
    pub block_count: usize,
    pub last_block_hash: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaProtocolCapabilities {
    pub version: String,
    pub execution_modes: Vec<HttpaExecutionMode>,
    pub privacy_modes: Vec<HttpaPrivacyMode>,
    pub ledger_modes: Vec<HttpaLedgerMode>,
    pub agent_roles: Vec<String>,
    pub activity_statuses: Vec<String>,
    pub safety_boundaries: Vec<String>,
}

#[derive(Default)]
struct HttpaStore {
    sessions: HashMap<String, HttpaSession>,
    devices: HashMap<String, DeviceEnrollment>,
    receipts: HashMap<String, HttpaReceipt>,
    ledger: Vec<HttpaReceipt>,
}

#[derive(Clone)]
pub struct HttpaService {
    store: Shared<HttpaStore>,
}

impl HttpaService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Shared::new(RwLock::new(HttpaStore::default())),
        }
    }

    #[must_use]
    pub fn capabilities() -> HttpaProtocolCapabilities {
        HttpaProtocolCapabilities {
            version: HTTPA_VERSION.into(),
            execution_modes: vec![
                HttpaExecutionMode::VisibleBrowse,
                HttpaExecutionMode::ResultOnly,
            ],
            privacy_modes: vec![
                HttpaPrivacyMode::Identified,
                HttpaPrivacyMode::OriginShielded,
            ],
            ledger_modes: vec![
                HttpaLedgerMode::BoundIntent,
                HttpaLedgerMode::VerifiedEvidence,
            ],
            agent_roles: vec![
                "planner".into(),
                "safety".into(),
                "executor".into(),
                "verifier".into(),
                "notary".into(),
            ],
            activity_statuses: vec![
                "queued".into(),
                "running".into(),
                "completed".into(),
                "completed_with_warnings".into(),
                "approval_required".into(),
                "blocked".into(),
                "failed".into(),
            ],
            safety_boundaries: vec![
                "owner_enrolled_devices_only".into(),
                "no_stealth_or_fingerprint_evasion".into(),
                "no_autonomous_secret_extraction".into(),
                "destructive_remediation_requires_owner_approval".into(),
                "blockchain_receipts_are_audit_only".into(),
            ],
        }
    }

    pub fn create_session(
        &self,
        request: CreateHttpaSessionRequest,
        token_required: bool,
    ) -> Result<HttpaSession, AppError> {
        let device_id = normalize_device_id(&request.device_id)?;
        let device_id_hash = sha3_hex(device_id.as_bytes())[..16].to_string();
        let now = now_ms();
        let token = random_token("httpa_session", 32);
        let mut policy = request.requested_policy.unwrap_or_default();
        policy.disallowed_features = disallowed_features();
        policy.origin_shielding = matches!(policy.privacy, HttpaPrivacyMode::OriginShielded);
        policy.user_visible = matches!(policy.mode, HttpaExecutionMode::VisibleBrowse);
        policy.chain_receipt_required = true;
        policy.max_parallel_agents = policy.max_parallel_agents.clamp(1, 12);

        let action_allowlist = if request.action_allowlist.is_empty() {
            vec![
                "open_url".into(),
                "open_app".into(),
                "search_web".into(),
                "submit_guardian_event".into(),
                "analyze_dlp".into(),
                "semantic_render".into(),
                "research".into(),
            ]
        } else {
            request.action_allowlist
        };
        let enrollment = DeviceEnrollment {
            device_id_hash: device_id_hash.clone(),
            capabilities: request.device_capabilities,
            action_allowlist,
            owner_enrolled: true,
            token_required,
            last_seen_ms: now,
        };
        let session = HttpaSession {
            session_id: new_id("httpa_session"),
            device_id_hash: device_id_hash.clone(),
            session_token: token.clone(),
            session_token_hash: sha3_hex(token.as_bytes()),
            policy,
            expires_at_ms: now + 8 * 60 * 60 * 1000,
            created_at_ms: now,
        };

        let mut store = self.store.write();
        store.devices.insert(device_id_hash, enrollment);
        store
            .sessions
            .insert(session.session_id.clone(), session.clone());
        Ok(session)
    }

    pub fn verify_session(
        &self,
        session_id: &str,
        session_token: &str,
    ) -> Result<HttpaSession, AppError> {
        let session = self
            .store
            .read()
            .sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::Unauthorized)?;
        if session.expires_at_ms < now_ms() {
            return Err(AppError::Unauthorized);
        }
        if sha3_hex(session_token.as_bytes()) != session.session_token_hash {
            return Err(AppError::Unauthorized);
        }
        Ok(session)
    }

    pub fn device_for_hash(&self, device_id_hash: &str) -> Result<DeviceEnrollment, AppError> {
        self.store
            .read()
            .devices
            .get(device_id_hash)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("device {device_id_hash}")))
    }

    pub fn record_receipt<T: Serialize>(
        &self,
        trace_id: &str,
        subject: &str,
        payload: &T,
    ) -> Result<HttpaReceipt, AppError> {
        let payload_json = serde_json::to_string(payload).map_err(|error| {
            AppError::Internal(format!("HTTPA receipt serialization failed: {error}"))
        })?;
        let payload_hash = sha3_hex(payload_json.as_bytes());
        let mut store = self.store.write();
        let previous_hash = store
            .ledger
            .last()
            .map(|receipt| receipt.block_hash.clone())
            .unwrap_or_else(|| "genesis".into());
        let block_height = store.ledger.len() as u64 + 1;
        let created_at_ms = now_ms();
        let receipt_id = new_id("httpa_receipt");
        let subject_hash = sha3_hex(subject.as_bytes());
        let block_hash = sha3_hex(
            serde_json::json!({
                "receipt_id": receipt_id,
                "trace_id": trace_id,
                "subject_hash": subject_hash,
                "payload_hash": payload_hash,
                "block_height": block_height,
                "previous_hash": previous_hash,
                "created_at_ms": created_at_ms,
            })
            .to_string()
            .as_bytes(),
        );
        let receipt = HttpaReceipt {
            receipt_id: receipt_id.clone(),
            trace_id: trace_id.into(),
            subject: subject.into(),
            subject_hash,
            payload_hash,
            block_height,
            block_hash,
            previous_hash,
            external_anchor: None,
            created_at_ms,
        };
        store.receipts.insert(receipt_id, receipt.clone());
        store.ledger.push(receipt.clone());
        Ok(receipt)
    }

    pub fn get_receipt(&self, receipt_id: &str) -> Result<HttpaReceipt, AppError> {
        self.store
            .read()
            .receipts
            .get(receipt_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("HTTPA receipt {receipt_id}")))
    }

    #[must_use]
    pub fn verify_ledger(&self) -> LedgerIntegrityReport {
        let ledger = &self.store.read().ledger;
        let mut previous_hash = "genesis".to_string();
        let mut errors = Vec::new();
        for (index, receipt) in ledger.iter().enumerate() {
            let expected_height = index as u64 + 1;
            if receipt.block_height != expected_height {
                errors.push(format!(
                    "receipt {} has unexpected height",
                    receipt.receipt_id
                ));
            }
            if receipt.previous_hash != previous_hash {
                errors.push(format!(
                    "receipt {} previous hash mismatch",
                    receipt.receipt_id
                ));
            }
            previous_hash = receipt.block_hash.clone();
        }
        LedgerIntegrityReport {
            valid: errors.is_empty(),
            block_count: ledger.len(),
            last_block_hash: ledger.last().map(|receipt| receipt.block_hash.clone()),
            errors,
        }
    }
}

impl Default for HttpaService {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_device_id(device_id: &str) -> Result<String, AppError> {
    let normalized = device_id.trim();
    if normalized.is_empty() {
        return Err(AppError::Validation("device_id must not be empty".into()));
    }
    if normalized.len() > 128 {
        return Err(AppError::Validation("device_id is too long".into()));
    }
    Ok(normalized.to_string())
}

fn disallowed_features() -> Vec<String> {
    vec![
        "stealth_scraping".into(),
        "fingerprint_evasion".into(),
        "credential_dumping".into(),
        "unauthorized_remote_control".into(),
        "autonomous_destructive_remediation".into(),
        "autonomous_wallet_signing".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_creation_hashes_token_and_enrolls_device() {
        let service = HttpaService::new();
        let session = service
            .create_session(
                CreateHttpaSessionRequest {
                    device_id: "owner-desktop".into(),
                    device_capabilities: vec!["windows".into()],
                    action_allowlist: vec!["open_url".into()],
                    requested_policy: None,
                },
                true,
            )
            .expect("session");

        assert_ne!(session.session_token, session.session_token_hash);
        assert_eq!(
            service
                .device_for_hash(&session.device_id_hash)
                .expect("device")
                .owner_enrolled,
            true
        );
        assert!(
            service
                .verify_session(&session.session_id, &session.session_token)
                .is_ok()
        );
    }

    #[test]
    fn invalid_session_token_is_rejected() {
        let service = HttpaService::new();
        let session = service
            .create_session(
                CreateHttpaSessionRequest {
                    device_id: "owner-desktop".into(),
                    device_capabilities: Vec::new(),
                    action_allowlist: Vec::new(),
                    requested_policy: None,
                },
                true,
            )
            .expect("session");

        assert!(
            service
                .verify_session(&session.session_id, "wrong")
                .is_err()
        );
    }

    #[test]
    fn receipts_form_local_hash_chain() {
        let service = HttpaService::new();
        let first = service
            .record_receipt("trace_one", "test", &serde_json::json!({"a": 1}))
            .expect("receipt");
        let second = service
            .record_receipt("trace_two", "test", &serde_json::json!({"b": 2}))
            .expect("receipt");

        assert_eq!(second.previous_hash, first.block_hash);
        assert!(service.verify_ledger().valid);
    }
}

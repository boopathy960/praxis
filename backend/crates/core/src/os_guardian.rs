use std::collections::{BTreeMap, VecDeque};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms, sha3_hex};

const DEFAULT_EVENT_CAPACITY: usize = 1024;
const MAX_METADATA_KEYS: usize = 64;
const MAX_METADATA_KEY_BYTES: usize = 80;
const MAX_METADATA_VALUE_BYTES: usize = 2048;
const MAX_CONTENT_SAMPLE_BYTES: usize = 4096;
const MAX_PENDING_DLP_DECISIONS: usize = 128;
const GENESIS_HASH: &str = "genesis";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsGuardianEventKind {
    Process,
    File,
    Registry,
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianSeverity {
    Informational,
    Low,
    Moderate,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianRecommendedAction {
    Observe,
    RateLimit,
    ApprovalRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardianAgentRole {
    TelemetryCollector,
    ThreatAnalyzer,
    PolicyVerifier,
    RemediationPlanner,
    AuditNotary,
}

impl GuardianAgentRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TelemetryCollector => "telemetry_collector",
            Self::ThreatAnalyzer => "threat_analyzer",
            Self::PolicyVerifier => "policy_verifier",
            Self::RemediationPlanner => "remediation_planner",
            Self::AuditNotary => "audit_notary",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitGuardianEventRequest {
    pub kind: OsGuardianEventKind,
    pub source: String,
    pub subject: String,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub observed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsGuardianEvent {
    pub event_id: String,
    pub kind: OsGuardianEventKind,
    pub source: String,
    pub subject: String,
    pub pid: Option<u32>,
    pub metadata: BTreeMap<String, String>,
    pub observed_at_ms: i64,
    pub ingested_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatEvaluation {
    pub evaluation_id: String,
    pub severity: GuardianSeverity,
    pub confidence: f64,
    pub rationale: Vec<String>,
    pub recommended_action: GuardianRecommendedAction,
    pub destructive_action_suppressed: bool,
    pub evaluated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianHttpaContext {
    pub trace_id: String,
    pub session_id: String,
    pub ledger_mode: String,
    pub performance_tier: String,
    pub chain_receipt_required: bool,
    pub deterministic_replay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianAgentAssignment {
    pub assignment_id: String,
    pub role: GuardianAgentRole,
    pub work_item: String,
    pub httpa: GuardianHttpaContext,
    pub quality_requirements: Vec<String>,
    pub assigned_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianLedgerBlock {
    pub block_height: u64,
    pub block_hash: String,
    pub previous_hash: String,
    pub payload_hash: String,
    pub receipt_id: String,
    pub httpa_trace_id: String,
    pub agent_role: GuardianAgentRole,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianLedgerReceipt {
    pub receipt_id: String,
    pub block_height: u64,
    pub block_hash: String,
    pub previous_hash: String,
    pub payload_hash: String,
    pub httpa_trace_id: String,
    pub agent_role: GuardianAgentRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerIntegrityReport {
    pub valid: bool,
    pub block_count: usize,
    pub last_block_hash: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianEventResponse {
    pub event: OsGuardianEvent,
    pub evaluation: ThreatEvaluation,
    pub assignment: GuardianAgentAssignment,
    pub receipt: GuardianLedgerReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianStatus {
    pub status: String,
    pub policy: GuardianRuntimePolicy,
    pub buffer_size: usize,
    pub buffer_capacity: usize,
    pub ledger_height: u64,
    pub last_block_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianRuntimePolicy {
    pub destructive_actions_enabled: bool,
    pub remediation_requires_approval: bool,
    pub httpa_required: bool,
    pub chain_receipts_required: bool,
    pub default_ledger_mode: String,
    pub default_performance_tier: String,
    pub metadata_redaction: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveDataClass {
    Public,
    ApiKey,
    Password,
    PrivateKey,
    WalletSeed,
    PersonalDocument,
    SourceCode,
    UnknownSensitive,
    CreditCard,
    EmailAddress,
    PhoneNumber,
    CloudSecret,
    DatabaseConnectionString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakVector {
    HttpUpload,
    DnsQuery,
    MediaUpload,
    FileRead,
    Clipboard,
    ProcessMemory,
    Unknown,
    AiResponseLeak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DlpRecommendedControl {
    AllowWithAudit,
    RedactBeforeSend,
    ScrubMedia,
    CanaryDecoyRecommended,
    PendingOwnerApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataLeakSignal {
    pub source_process: String,
    pub subject: String,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub owner_authorized: bool,
    #[serde(default)]
    pub data_classes: Vec<SensitiveDataClass>,
    #[serde(default)]
    pub leak_vectors: Vec<LeakVector>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    #[serde(default)]
    pub content_sample: Option<String>,
    #[serde(default)]
    pub observed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataLeakSignalRecord {
    pub signal_id: String,
    pub source_process: String,
    pub subject: String,
    pub destination: Option<String>,
    pub owner_authorized: bool,
    pub data_classes: Vec<SensitiveDataClass>,
    pub leak_vectors: Vec<LeakVector>,
    pub metadata: BTreeMap<String, String>,
    pub content_fingerprint: Option<String>,
    pub content_sample: Option<String>,
    pub observed_at_ms: i64,
    pub ingested_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpVerdict {
    pub verdict_id: String,
    pub severity: GuardianSeverity,
    pub confidence: f64,
    pub risk_score: f64,
    pub data_classes: Vec<SensitiveDataClass>,
    pub leak_vectors: Vec<LeakVector>,
    pub rationale: Vec<String>,
    pub recommended_control: DlpRecommendedControl,
    pub owner_authorized: bool,
    pub approval_required: bool,
    pub analyzed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpDecoyManifest {
    pub manifest_id: String,
    pub name: String,
    pub purpose: String,
    pub data_classes: Vec<SensitiveDataClass>,
    pub activation_policy: String,
    pub generated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpDecoyPolicy {
    pub policy_id: String,
    pub name: String,
    pub applies_to: Vec<SensitiveDataClass>,
    pub behavior: String,
    pub safety_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpPendingDecision {
    pub decision_id: String,
    pub signal_id: String,
    pub verdict_id: String,
    pub recommended_control: DlpRecommendedControl,
    pub reason: String,
    pub httpa_trace_id: String,
    pub receipt_id: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpPolicy {
    pub mode: String,
    pub owner_authorized_actions_allowed: bool,
    pub secrets_never_return_raw: bool,
    pub destructive_controls_enabled: bool,
    pub approval_required_controls: Vec<String>,
    pub monitored_vectors: Vec<LeakVector>,
    pub canary_decoys_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpAnalysisResponse {
    pub signal: DataLeakSignalRecord,
    pub verdict: DlpVerdict,
    pub assignment: GuardianAgentAssignment,
    pub receipt: GuardianLedgerReceipt,
    pub decoy_manifest: Option<DlpDecoyManifest>,
    pub pending_decision: Option<DlpPendingDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpPatternMatch {
    pub pattern_name: String,
    pub data_class: SensitiveDataClass,
    pub redacted_match: String,
    pub byte_offset: usize,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlpContentScanReport {
    pub scan_id: String,
    pub matches: Vec<DlpPatternMatch>,
    pub shannon_entropy: f64,
    pub content_length: usize,
    pub data_classes_found: Vec<SensitiveDataClass>,
    pub risk_score: f64,
    pub scanned_at_ms: i64,
}

#[derive(Default)]
struct GuardianStore {
    events: VecDeque<GuardianEventResponse>,
    dlp_analyses: VecDeque<DlpAnalysisResponse>,
    pending_dlp_decisions: VecDeque<DlpPendingDecision>,
    ledger: Vec<GuardianLedgerBlock>,
}

#[derive(Clone)]
pub struct OsGuardianService {
    store: Shared<GuardianStore>,
    capacity: usize,
    policy: GuardianRuntimePolicy,
}

impl OsGuardianService {
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_EVENT_CAPACITY)
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            store: Shared::new(RwLock::new(GuardianStore::default())),
            capacity: capacity.max(1),
            policy: GuardianRuntimePolicy {
                destructive_actions_enabled: false,
                remediation_requires_approval: true,
                httpa_required: true,
                chain_receipts_required: true,
                default_ledger_mode: "verified_evidence".into(),
                default_performance_tier: "efficient".into(),
                metadata_redaction: "secret-like keys and high-entropy values are hashed".into(),
            },
        }
    }

    pub fn submit_event(
        &self,
        request: SubmitGuardianEventRequest,
    ) -> Result<GuardianEventResponse, AppError> {
        let event = normalize_event(request)?;
        let evaluation = evaluate_threat(&event);
        let assignment = assign_agent(&event, &evaluation, &self.policy);
        let mut store = self.store.write();
        let receipt = append_ledger_payload(
            &mut store.ledger,
            &assignment.httpa.trace_id,
            assignment.role,
            &serde_json::json!({
                "event": &event,
                "evaluation": &evaluation,
                "assignment": &assignment,
            }),
        )?;
        let response = GuardianEventResponse {
            event,
            evaluation,
            assignment,
            receipt,
        };
        if store.events.len() == self.capacity {
            store.events.pop_front();
        }
        store.events.push_back(response.clone());
        Ok(response)
    }

    pub fn analyze_dlp_signal(
        &self,
        request: DataLeakSignal,
    ) -> Result<DlpAnalysisResponse, AppError> {
        let signal = normalize_dlp_signal(request)?;
        let verdict = evaluate_dlp_signal(&signal);
        let assignment = assign_dlp_agent(&signal, &verdict, &self.policy);
        let decoy_manifest = build_decoy_manifest(&signal, &verdict);

        let mut store = self.store.write();
        let receipt = append_ledger_payload(
            &mut store.ledger,
            &assignment.httpa.trace_id,
            assignment.role,
            &serde_json::json!({
                "signal": &signal,
                "verdict": &verdict,
                "assignment": &assignment,
                "decoy_manifest": &decoy_manifest,
            }),
        )?;
        let pending_decision = if verdict.approval_required {
            Some(DlpPendingDecision {
                decision_id: new_id("dlp_pending"),
                signal_id: signal.signal_id.clone(),
                verdict_id: verdict.verdict_id.clone(),
                recommended_control: verdict.recommended_control,
                reason: verdict.rationale.join("; "),
                httpa_trace_id: assignment.httpa.trace_id.clone(),
                receipt_id: receipt.receipt_id.clone(),
                created_at_ms: now_ms(),
            })
        } else {
            None
        };
        if let Some(decision) = &pending_decision {
            if store.pending_dlp_decisions.len() == MAX_PENDING_DLP_DECISIONS {
                store.pending_dlp_decisions.pop_front();
            }
            store.pending_dlp_decisions.push_back(decision.clone());
        }

        let response = DlpAnalysisResponse {
            signal,
            verdict,
            assignment,
            receipt,
            decoy_manifest,
            pending_decision,
        };
        if store.dlp_analyses.len() == self.capacity {
            store.dlp_analyses.pop_front();
        }
        store.dlp_analyses.push_back(response.clone());
        Ok(response)
    }

    #[must_use]
    pub fn recent_events(&self) -> Vec<GuardianEventResponse> {
        self.store.read().events.iter().cloned().collect()
    }

    #[must_use]
    pub fn dlp_policy(&self) -> DlpPolicy {
        DlpPolicy {
            mode: "personal_assistant".into(),
            owner_authorized_actions_allowed: true,
            secrets_never_return_raw: true,
            destructive_controls_enabled: false,
            approval_required_controls: vec![
                "network_block".into(),
                "process_termination".into(),
                "filesystem_restore".into(),
                "kernel_driver_action".into(),
                "raw_secret_transmission".into(),
            ],
            monitored_vectors: vec![
                LeakVector::HttpUpload,
                LeakVector::DnsQuery,
                LeakVector::MediaUpload,
                LeakVector::FileRead,
                LeakVector::Clipboard,
                LeakVector::ProcessMemory,
            ],
            canary_decoys_enabled: true,
        }
    }

    #[must_use]
    pub fn dlp_decoy_policies(&self) -> Vec<DlpDecoyPolicy> {
        vec![
            DlpDecoyPolicy {
                policy_id: "decoy_wallet_seed_v1".into(),
                name: "Wallet seed honeytoken planning".into(),
                applies_to: vec![
                    SensitiveDataClass::WalletSeed,
                    SensitiveDataClass::PrivateKey,
                ],
                behavior:
                    "recommend owner-approved canary material instead of rewriting live payloads"
                        .into(),
                safety_note: "no fake wallet or offensive tracing is generated automatically"
                    .into(),
            },
            DlpDecoyPolicy {
                policy_id: "decoy_personal_document_v1".into(),
                name: "Sensitive document decoy planning".into(),
                applies_to: vec![SensitiveDataClass::PersonalDocument],
                behavior: "recommend a labeled canary document manifest for suspicious readers"
                    .into(),
                safety_note: "filesystem reads are not intercepted in v1".into(),
            },
        ]
    }

    #[must_use]
    pub fn pending_dlp_decisions(&self) -> Vec<DlpPendingDecision> {
        self.store
            .read()
            .pending_dlp_decisions
            .iter()
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn status(&self) -> GuardianStatus {
        let store = self.store.read();
        GuardianStatus {
            status: "operational".into(),
            policy: self.policy.clone(),
            buffer_size: store.events.len(),
            buffer_capacity: self.capacity,
            ledger_height: store.ledger.len() as u64,
            last_block_hash: store.ledger.last().map(|block| block.block_hash.clone()),
        }
    }

    #[must_use]
    pub fn verify_ledger(&self) -> LedgerIntegrityReport {
        verify_blocks(&self.store.read().ledger)
    }

    #[cfg(test)]
    fn tamper_latest_block_for_test(&self) {
        if let Some(block) = self.store.write().ledger.last_mut() {
            block.payload_hash = "tampered".into();
        }
    }
}

impl Default for OsGuardianService {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_event(request: SubmitGuardianEventRequest) -> Result<OsGuardianEvent, AppError> {
    let source = clean_text(&request.source, "event source")?;
    let subject = clean_text(&request.subject, "event subject")?;
    let metadata = sanitize_metadata(request.metadata)?;
    let now = now_ms();

    Ok(OsGuardianEvent {
        event_id: new_id("guardian_event"),
        kind: request.kind,
        source,
        subject,
        pid: request.pid,
        metadata,
        observed_at_ms: request.observed_at_ms.unwrap_or(now),
        ingested_at_ms: now,
    })
}

fn clean_text(value: &str, field: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }
    let max_len = if field.contains("DLP") || field.contains("subject") {
        16384
    } else {
        512
    };
    if trimmed.len() > max_len {
        return Err(AppError::Validation(format!("{field} is too long")));
    }
    Ok(trimmed.to_string())
}

fn sanitize_metadata(
    metadata: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, AppError> {
    if metadata.len() > MAX_METADATA_KEYS {
        return Err(AppError::Validation(format!(
            "metadata supports at most {MAX_METADATA_KEYS} keys"
        )));
    }

    let mut sanitized = BTreeMap::new();
    for (key, value) in metadata {
        let key = key.trim();
        if key.is_empty() {
            return Err(AppError::Validation(
                "metadata keys must not be empty".into(),
            ));
        }
        if key.len() > MAX_METADATA_KEY_BYTES {
            return Err(AppError::Validation(format!(
                "metadata key {key} is too long"
            )));
        }
        if value.len() > MAX_METADATA_VALUE_BYTES {
            return Err(AppError::Validation(format!(
                "metadata value for {key} is too long"
            )));
        }
        let value = if is_sensitive_metadata(key, &value) {
            format!("redacted:sha3:{}", &sha3_hex(value.as_bytes())[..16])
        } else {
            value
        };
        sanitized.insert(key.to_string(), value);
    }
    Ok(sanitized)
}

fn is_sensitive_metadata(key: &str, value: &str) -> bool {
    let key = key.to_ascii_lowercase();
    let sensitive_key = [
        "token",
        "password",
        "secret",
        "api_key",
        "apikey",
        "authorization",
        "cookie",
        "credential",
        "seed",
        "private_key",
    ]
    .iter()
    .any(|marker| key.contains(marker));
    sensitive_key || looks_high_entropy(value)
}

fn looks_high_entropy(value: &str) -> bool {
    value.trim().len() >= 20 && shannon_entropy(value) >= 3.5
}

fn evaluate_threat(event: &OsGuardianEvent) -> ThreatEvaluation {
    let subject = event.subject.to_ascii_lowercase();
    let mut score = 0.05_f64;
    let mut rationale = vec!["event accepted through governed telemetry intake".into()];

    match event.kind {
        OsGuardianEventKind::Network => {
            score += 0.10;
            if contains_any(
                &subject,
                &["pastebin", "telegram", "discord", "unknown-host"],
            ) {
                score += 0.28;
                rationale.push("network destination matches exfiltration watchlist".into());
            }
            if metadata_contains_any(&event.metadata, &["credential", "secret", "token"]) {
                score += 0.42;
                rationale.push("network metadata indicates secret-like payload risk".into());
            }
        }
        OsGuardianEventKind::File => {
            score += 0.08;
            if contains_any(
                &subject,
                &["shadow", "wallet", "id_rsa", "password", "secrets"],
            ) {
                score += 0.36;
                rationale.push("file subject resembles high-value credential material".into());
            }
            if metadata_contains_any(&event.metadata, &["high_entropy_write", "bulk_encrypt"]) {
                score += 0.44;
                rationale.push("file metadata resembles ransomware-style mutation".into());
            }
        }
        OsGuardianEventKind::Registry => {
            score += 0.12;
            if contains_any(&subject, &["run\\", "runonce", "services", "winlogon"]) {
                score += 0.34;
                rationale.push("registry subject touches persistence-sensitive location".into());
            }
        }
        OsGuardianEventKind::Process => {
            score += 0.08;
            if contains_any(
                &subject,
                &["powershell -enc", "rundll32", "regsvr32", "mimikatz"],
            ) {
                score += 0.40;
                rationale.push("process subject matches suspicious execution pattern".into());
            }
        }
    }

    if metadata_contains_any(
        &event.metadata,
        &["kill_process", "drop_packet", "delete_file"],
    ) {
        score += 0.35;
        rationale.push("requested remediation is destructive and must be approval gated".into());
    }

    let score = score.clamp(0.0, 1.0);
    let severity = if score >= 0.85 {
        GuardianSeverity::Critical
    } else if score >= 0.65 {
        GuardianSeverity::High
    } else if score >= 0.38 {
        GuardianSeverity::Moderate
    } else if score >= 0.16 {
        GuardianSeverity::Low
    } else {
        GuardianSeverity::Informational
    };
    let recommended_action = match severity {
        GuardianSeverity::Informational | GuardianSeverity::Low => {
            GuardianRecommendedAction::Observe
        }
        GuardianSeverity::Moderate => GuardianRecommendedAction::RateLimit,
        GuardianSeverity::High | GuardianSeverity::Critical => {
            GuardianRecommendedAction::ApprovalRequired
        }
    };

    ThreatEvaluation {
        evaluation_id: new_id("guardian_eval"),
        severity,
        confidence: round3((0.62 + score * 0.34).min(0.96)),
        rationale,
        recommended_action,
        destructive_action_suppressed: matches!(
            recommended_action,
            GuardianRecommendedAction::ApprovalRequired
        ),
        evaluated_at_ms: now_ms(),
    }
}

fn assign_agent(
    event: &OsGuardianEvent,
    evaluation: &ThreatEvaluation,
    policy: &GuardianRuntimePolicy,
) -> GuardianAgentAssignment {
    let role = match evaluation.severity {
        GuardianSeverity::Informational | GuardianSeverity::Low => {
            GuardianAgentRole::TelemetryCollector
        }
        GuardianSeverity::Moderate => GuardianAgentRole::ThreatAnalyzer,
        GuardianSeverity::High => GuardianAgentRole::PolicyVerifier,
        GuardianSeverity::Critical => GuardianAgentRole::RemediationPlanner,
    };
    let trace_id = new_id("httpa_trace");
    GuardianAgentAssignment {
        assignment_id: new_id("guardian_assign"),
        role,
        work_item: format!("evaluate {:?} event {}", event.kind, event.event_id),
        httpa: GuardianHttpaContext {
            trace_id,
            session_id: new_id("httpa_session"),
            ledger_mode: policy.default_ledger_mode.clone(),
            performance_tier: policy.default_performance_tier.clone(),
            chain_receipt_required: policy.chain_receipts_required,
            deterministic_replay: true,
        },
        quality_requirements: vec![
            "deterministic_evaluation".into(),
            "least_privilege".into(),
            "redacted_metadata_only".into(),
            "blockchain_receipt_required".into(),
        ],
        assigned_at_ms: now_ms(),
    }
}

fn normalize_dlp_signal(request: DataLeakSignal) -> Result<DataLeakSignalRecord, AppError> {
    let source_process = clean_text(&request.source_process, "source process")?;
    let subject = clean_text(&request.subject, "DLP subject")?;
    let destination = request
        .destination
        .as_deref()
        .map(|value| clean_text(value, "DLP destination"))
        .transpose()?;
    if let Some(sample) = &request.content_sample {
        if sample.len() > MAX_CONTENT_SAMPLE_BYTES {
            return Err(AppError::Validation(format!(
                "content sample supports at most {MAX_CONTENT_SAMPLE_BYTES} bytes"
            )));
        }
    }

    let metadata = sanitize_metadata(request.metadata)?;
    let content_fingerprint = request
        .content_sample
        .as_deref()
        .map(|sample| sha3_hex(sample.as_bytes()));
    let inferred_classes = infer_data_classes(
        &subject,
        destination.as_deref(),
        &metadata,
        request.content_sample.as_deref(),
    );
    let inferred_vectors = infer_leak_vectors(
        &subject,
        destination.as_deref(),
        &metadata,
        request.content_sample.as_deref(),
    );
    let data_classes = merge_classes(request.data_classes, inferred_classes);
    let leak_vectors = merge_vectors(request.leak_vectors, inferred_vectors);
    let now = now_ms();

    Ok(DataLeakSignalRecord {
        signal_id: new_id("dlp_signal"),
        source_process,
        subject,
        destination,
        owner_authorized: request.owner_authorized,
        data_classes,
        leak_vectors,
        metadata,
        content_fingerprint,
        content_sample: request
            .content_sample
            .as_deref()
            .map(|sample| format!("redacted:sha3:{}", &sha3_hex(sample.as_bytes())[..16])),
        observed_at_ms: request.observed_at_ms.unwrap_or(now),
        ingested_at_ms: now,
    })
}

fn infer_data_classes(
    subject: &str,
    destination: Option<&str>,
    metadata: &BTreeMap<String, String>,
    content_sample: Option<&str>,
) -> Vec<SensitiveDataClass> {
    let combined = format!(
        "{} {} {} {}",
        subject,
        destination.unwrap_or_default(),
        metadata
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" "),
        content_sample.unwrap_or_default()
    )
    .to_ascii_lowercase();
    let mut classes = Vec::new();

    if contains_any(
        &combined,
        &[
            "api_key",
            "apikey",
            "access_key",
            "secret_access_key",
            "bearer ",
            "authorization",
            "token",
        ],
    ) {
        classes.push(SensitiveDataClass::ApiKey);
    }
    if contains_any(&combined, &["password", "passwd", "pwd=", "credential"]) {
        classes.push(SensitiveDataClass::Password);
    }
    if contains_any(
        &combined,
        &["private key", "begin private key", "id_rsa", "ssh-key"],
    ) {
        classes.push(SensitiveDataClass::PrivateKey);
    }
    if contains_any(
        &combined,
        &["wallet.dat", "seed phrase", "mnemonic", "recovery phrase"],
    ) {
        classes.push(SensitiveDataClass::WalletSeed);
    }
    if contains_any(
        &combined,
        &[
            "passport",
            "aadhaar",
            "ssn",
            "tax_id",
            "bank_statement",
            "personal_document",
        ],
    ) {
        classes.push(SensitiveDataClass::PersonalDocument);
    }
    if contains_any(&combined, &[".env", "source_code", ".git", "cargo.toml"]) {
        classes.push(SensitiveDataClass::SourceCode);
    }
    if let Some(sample) = content_sample {
        for m in scan_content_patterns(sample) {
            if !classes.contains(&m.data_class) {
                classes.push(m.data_class);
            }
        }
    }
    if looks_high_entropy(content_sample.unwrap_or_default()) {
        if !classes.contains(&SensitiveDataClass::UnknownSensitive) {
            classes.push(SensitiveDataClass::UnknownSensitive);
        }
    }
    if classes.is_empty() {
        classes.push(SensitiveDataClass::Public);
    }
    classes.sort_by_key(|class| format!("{class:?}"));
    classes.dedup();
    classes
}

fn infer_leak_vectors(
    subject: &str,
    destination: Option<&str>,
    metadata: &BTreeMap<String, String>,
    _content_sample: Option<&str>,
) -> Vec<LeakVector> {
    let combined = format!(
        "{} {} {}",
        subject,
        destination.unwrap_or_default(),
        metadata
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_ascii_lowercase();
    let mut vectors = Vec::new();
    if contains_any(&combined, &["dns", "udp/53", ":53", "txt_query"]) {
        vectors.push(LeakVector::DnsQuery);
    }
    if contains_any(&combined, &["http://", "https://", "post", "upload"]) {
        vectors.push(LeakVector::HttpUpload);
    }
    if contains_any(
        &combined,
        &[
            "image/png",
            "image/jpeg",
            ".png",
            ".jpg",
            ".jpeg",
            "media_upload",
        ],
    ) {
        vectors.push(LeakVector::MediaUpload);
    }
    if contains_any(&combined, &["file_read", "open_file", "read_file"]) {
        vectors.push(LeakVector::FileRead);
    }
    if contains_any(&combined, &["clipboard", "pasteboard"]) {
        vectors.push(LeakVector::Clipboard);
    }
    if contains_any(&combined, &["memory", "buffer", "page", "process_memory"]) {
        vectors.push(LeakVector::ProcessMemory);
    }
    if vectors.is_empty() {
        vectors.push(LeakVector::Unknown);
    }
    vectors.sort_by_key(|vector| format!("{vector:?}"));
    vectors.dedup();
    vectors
}

fn merge_classes(
    mut supplied: Vec<SensitiveDataClass>,
    inferred: Vec<SensitiveDataClass>,
) -> Vec<SensitiveDataClass> {
    supplied.extend(inferred);
    if supplied
        .iter()
        .any(|class| *class != SensitiveDataClass::Public)
    {
        supplied.retain(|class| *class != SensitiveDataClass::Public);
    }
    supplied.sort_by_key(|class| format!("{class:?}"));
    supplied.dedup();
    supplied
}

fn merge_vectors(mut supplied: Vec<LeakVector>, inferred: Vec<LeakVector>) -> Vec<LeakVector> {
    supplied.extend(inferred);
    if supplied.iter().any(|vector| *vector != LeakVector::Unknown) {
        supplied.retain(|vector| *vector != LeakVector::Unknown);
    }
    supplied.sort_by_key(|vector| format!("{vector:?}"));
    supplied.dedup();
    supplied
}

fn evaluate_dlp_signal(signal: &DataLeakSignalRecord) -> DlpVerdict {
    let mut score = 0.08_f64;
    let mut rationale = vec!["DLP signal evaluated in personal-assistant safe mode".into()];

    for class in &signal.data_classes {
        match class {
            SensitiveDataClass::Public => {}
            SensitiveDataClass::ApiKey | SensitiveDataClass::Password => {
                score += 0.30;
                rationale.push("credential-like data class detected".into());
            }
            SensitiveDataClass::PrivateKey | SensitiveDataClass::WalletSeed => {
                score += 0.42;
                rationale.push("wallet seed or private-key material detected".into());
            }
            SensitiveDataClass::PersonalDocument => {
                score += 0.24;
                rationale.push("personal document class detected".into());
            }
            SensitiveDataClass::SourceCode => {
                score += 0.18;
                rationale.push("source code or project secret context detected".into());
            }
            SensitiveDataClass::UnknownSensitive => {
                score += 0.26;
                rationale.push("high-entropy unknown sensitive payload detected".into());
            }
            SensitiveDataClass::CreditCard => {
                score += 0.38;
                rationale.push("credit card number pattern detected".into());
            }
            SensitiveDataClass::EmailAddress => {
                score += 0.14;
                rationale.push("email address pattern detected".into());
            }
            SensitiveDataClass::PhoneNumber => {
                score += 0.12;
                rationale.push("phone number pattern detected".into());
            }
            SensitiveDataClass::CloudSecret => {
                score += 0.44;
                rationale.push("cloud provider secret pattern detected".into());
            }
            SensitiveDataClass::DatabaseConnectionString => {
                score += 0.40;
                rationale
                    .push("database connection string with embedded credentials detected".into());
            }
        }
    }

    for vector in &signal.leak_vectors {
        match vector {
            LeakVector::DnsQuery => {
                score += 0.26;
                rationale.push("DNS query vector can carry covert exfiltration".into());
            }
            LeakVector::MediaUpload => {
                score += 0.18;
                rationale.push("outbound media vector may carry steganographic content".into());
            }
            LeakVector::HttpUpload => {
                score += 0.14;
                rationale.push("outbound HTTP upload vector detected".into());
            }
            LeakVector::FileRead | LeakVector::Clipboard | LeakVector::ProcessMemory => {
                score += 0.12;
                rationale.push("sensitive local provenance vector detected".into());
            }
            LeakVector::AiResponseLeak => {
                score += 0.22;
                rationale.push("outbound AI response may contain leaked sensitive content".into());
            }
            LeakVector::Unknown => {}
        }
    }

    if destination_is_public_exfil(signal.destination.as_deref()) {
        score += 0.18;
        rationale.push("destination resembles public exfiltration surface".into());
    }
    if dns_tunnel_like(signal.destination.as_deref().unwrap_or(&signal.subject)) {
        score += 0.24;
        rationale.push("destination has DNS-tunneling characteristics".into());
    }
    if suspicious_process(&signal.source_process) {
        score += 0.18;
        rationale.push("source process matches suspicious automation pattern".into());
    }
    if signal.owner_authorized {
        score -= 0.20;
        rationale.push("owner-authorized context lowered risk but retained audit controls".into());
    }

    let has_sensitive = signal
        .data_classes
        .iter()
        .any(|class| !matches!(class, SensitiveDataClass::Public));
    if has_sensitive && signal.owner_authorized {
        score = score.max(0.24);
    }
    let score = score.clamp(0.0, 1.0);
    let severity = severity_for_score(score);
    let recommended_control = dlp_control_for(signal, severity, has_sensitive);
    let approval_required = matches!(
        recommended_control,
        DlpRecommendedControl::PendingOwnerApproval
    );

    DlpVerdict {
        verdict_id: new_id("dlp_verdict"),
        severity,
        confidence: round3((0.64 + score * 0.32).min(0.97)),
        risk_score: round3(score),
        data_classes: signal.data_classes.clone(),
        leak_vectors: signal.leak_vectors.clone(),
        rationale,
        recommended_control,
        owner_authorized: signal.owner_authorized,
        approval_required,
        analyzed_at_ms: now_ms(),
    }
}

fn severity_for_score(score: f64) -> GuardianSeverity {
    if score >= 0.86 {
        GuardianSeverity::Critical
    } else if score >= 0.66 {
        GuardianSeverity::High
    } else if score >= 0.40 {
        GuardianSeverity::Moderate
    } else if score >= 0.18 {
        GuardianSeverity::Low
    } else {
        GuardianSeverity::Informational
    }
}

fn dlp_control_for(
    signal: &DataLeakSignalRecord,
    severity: GuardianSeverity,
    has_sensitive: bool,
) -> DlpRecommendedControl {
    if matches!(
        severity,
        GuardianSeverity::High | GuardianSeverity::Critical
    ) {
        return DlpRecommendedControl::PendingOwnerApproval;
    }
    if signal.leak_vectors.contains(&LeakVector::MediaUpload) && has_sensitive {
        return DlpRecommendedControl::ScrubMedia;
    }
    if signal.data_classes.iter().any(|class| {
        matches!(
            class,
            SensitiveDataClass::PrivateKey | SensitiveDataClass::WalletSeed
        )
    }) {
        return DlpRecommendedControl::CanaryDecoyRecommended;
    }
    if has_sensitive {
        return DlpRecommendedControl::RedactBeforeSend;
    }
    DlpRecommendedControl::AllowWithAudit
}

fn assign_dlp_agent(
    signal: &DataLeakSignalRecord,
    verdict: &DlpVerdict,
    policy: &GuardianRuntimePolicy,
) -> GuardianAgentAssignment {
    let role = if verdict.approval_required {
        GuardianAgentRole::RemediationPlanner
    } else {
        match verdict.severity {
            GuardianSeverity::Informational | GuardianSeverity::Low => {
                GuardianAgentRole::TelemetryCollector
            }
            GuardianSeverity::Moderate => GuardianAgentRole::PolicyVerifier,
            GuardianSeverity::High | GuardianSeverity::Critical => {
                GuardianAgentRole::RemediationPlanner
            }
        }
    };
    GuardianAgentAssignment {
        assignment_id: new_id("guardian_assign"),
        role,
        work_item: format!("analyze DLP signal {}", signal.signal_id),
        httpa: GuardianHttpaContext {
            trace_id: new_id("httpa_trace"),
            session_id: new_id("httpa_session"),
            ledger_mode: policy.default_ledger_mode.clone(),
            performance_tier: policy.default_performance_tier.clone(),
            chain_receipt_required: policy.chain_receipts_required,
            deterministic_replay: true,
        },
        quality_requirements: vec![
            "personal_owner_context".into(),
            "secret_redaction".into(),
            "no_kernel_side_effects".into(),
            "blockchain_receipt_required".into(),
        ],
        assigned_at_ms: now_ms(),
    }
}

fn build_decoy_manifest(
    signal: &DataLeakSignalRecord,
    verdict: &DlpVerdict,
) -> Option<DlpDecoyManifest> {
    let decoy_classes = signal
        .data_classes
        .iter()
        .copied()
        .filter(|class| {
            matches!(
                class,
                SensitiveDataClass::WalletSeed
                    | SensitiveDataClass::PrivateKey
                    | SensitiveDataClass::PersonalDocument
            )
        })
        .collect::<Vec<_>>();
    if decoy_classes.is_empty()
        || !matches!(
            verdict.recommended_control,
            DlpRecommendedControl::CanaryDecoyRecommended
                | DlpRecommendedControl::PendingOwnerApproval
        )
    {
        return None;
    }

    Some(DlpDecoyManifest {
        manifest_id: new_id("dlp_decoy"),
        name: "owner-approved canary decoy plan".into(),
        purpose: "recommend canary material for suspicious access without rewriting live payloads"
            .into(),
        data_classes: decoy_classes,
        activation_policy: "manual owner approval required before any decoy is created or exposed"
            .into(),
        generated_at_ms: now_ms(),
    })
}

fn append_ledger_payload<T: Serialize>(
    ledger: &mut Vec<GuardianLedgerBlock>,
    httpa_trace_id: &str,
    agent_role: GuardianAgentRole,
    payload: &T,
) -> Result<GuardianLedgerReceipt, AppError> {
    let payload_hash = sha3_hex(serde_json::to_string(payload).map_err(|error| {
        AppError::Internal(format!("guardian ledger serialization failed: {error}"))
    })?);
    let previous_hash = ledger.last().map_or_else(
        || GENESIS_HASH.to_string(),
        |block| block.block_hash.clone(),
    );
    let block_height = ledger.len() as u64 + 1;
    let receipt_id = new_id("guardian_receipt");
    let timestamp_ms = now_ms();
    let block_hash = compute_block_hash(
        block_height,
        &previous_hash,
        &payload_hash,
        &receipt_id,
        httpa_trace_id,
        agent_role,
        timestamp_ms,
    );
    let block = GuardianLedgerBlock {
        block_height,
        block_hash: block_hash.clone(),
        previous_hash: previous_hash.clone(),
        payload_hash: payload_hash.clone(),
        receipt_id: receipt_id.clone(),
        httpa_trace_id: httpa_trace_id.to_string(),
        agent_role,
        timestamp_ms,
    };
    ledger.push(block);
    Ok(GuardianLedgerReceipt {
        receipt_id,
        block_height,
        block_hash,
        previous_hash,
        payload_hash,
        httpa_trace_id: httpa_trace_id.to_string(),
        agent_role,
    })
}

fn verify_blocks(blocks: &[GuardianLedgerBlock]) -> LedgerIntegrityReport {
    let mut errors = Vec::new();
    let mut previous_hash = GENESIS_HASH.to_string();

    for (index, block) in blocks.iter().enumerate() {
        let expected_height = index as u64 + 1;
        if block.block_height != expected_height {
            errors.push(format!(
                "block {} has unexpected height {}",
                index + 1,
                block.block_height
            ));
        }
        if block.previous_hash != previous_hash {
            errors.push(format!("block {} previous hash mismatch", index + 1));
        }
        let expected_hash = compute_block_hash(
            block.block_height,
            &block.previous_hash,
            &block.payload_hash,
            &block.receipt_id,
            &block.httpa_trace_id,
            block.agent_role,
            block.timestamp_ms,
        );
        if block.block_hash != expected_hash {
            errors.push(format!("block {} hash mismatch", index + 1));
        }
        previous_hash = block.block_hash.clone();
    }

    LedgerIntegrityReport {
        valid: errors.is_empty(),
        block_count: blocks.len(),
        last_block_hash: blocks.last().map(|block| block.block_hash.clone()),
        errors,
    }
}

fn compute_block_hash(
    block_height: u64,
    previous_hash: &str,
    payload_hash: &str,
    receipt_id: &str,
    httpa_trace_id: &str,
    agent_role: GuardianAgentRole,
    timestamp_ms: i64,
) -> String {
    sha3_hex(
        serde_json::json!({
            "block_height": block_height,
            "previous_hash": previous_hash,
            "payload_hash": payload_hash,
            "receipt_id": receipt_id,
            "httpa_trace_id": httpa_trace_id,
            "agent_role": agent_role.as_str(),
            "timestamp_ms": timestamp_ms,
        })
        .to_string()
        .as_bytes(),
    )
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn metadata_contains_any(metadata: &BTreeMap<String, String>, needles: &[&str]) -> bool {
    metadata.iter().any(|(key, value)| {
        let key = key.to_ascii_lowercase();
        let value = value.to_ascii_lowercase();
        needles
            .iter()
            .any(|needle| key.contains(needle) || value.contains(needle))
    })
}

fn destination_is_public_exfil(destination: Option<&str>) -> bool {
    let Some(destination) = destination else {
        return false;
    };
    let destination = destination.to_ascii_lowercase();
    contains_any(
        &destination,
        &[
            "pastebin",
            "discord",
            "telegram",
            "dropbox",
            "mega.nz",
            "anonfiles",
            "webhook",
            "unknown-host",
            "transfer.sh",
            "file.io",
            "0x0.st",
            "catbox.moe",
            "ghostbin",
            "hastebin",
            "privatebin",
            "ngrok",
        ],
    )
}

fn dns_tunnel_like(value: &str) -> bool {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    let longest_label = value.split('.').map(str::len).max().unwrap_or(0);
    longest_label >= 48 || looks_high_entropy(value.split('.').next().unwrap_or_default())
}

fn suspicious_process(source_process: &str) -> bool {
    let source_process = source_process.to_ascii_lowercase();
    contains_any(
        &source_process,
        &[
            "powershell",
            "rundll32",
            "regsvr32",
            "wscript",
            "cscript",
            "unknown.exe",
            "temp\\",
            "appdata\\local\\temp",
            "certutil",
            "bitsadmin",
            "mshta",
            "cmd /c",
            "curl.exe",
            "wget",
            "ncat",
            "nc.exe",
        ],
    )
}

fn round3(value: f64) -> f64 {
    (value.clamp(0.0, 1.0) * 1000.0).round() / 1000.0
}

// ═══════════════════════════════════════════════════════════════
// ADVANCED PATTERN SCANNING ENGINE
// ═══════════════════════════════════════════════════════════════

fn shannon_entropy(value: &str) -> f64 {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for byte in trimmed.as_bytes() {
        freq[*byte as usize] += 1;
    }
    let len = trimmed.len() as f64;
    freq.iter()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

fn luhn_check(digits: &[u8]) -> bool {
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum = 0u32;
    let mut double = false;
    for &d in digits.iter().rev() {
        let mut val = d as u32;
        if double {
            val *= 2;
            if val > 9 {
                val -= 9;
            }
        }
        sum += val;
        double = !double;
    }
    sum % 10 == 0
}

fn extract_digit_runs(text: &str) -> Vec<(usize, Vec<u8>)> {
    let mut runs = Vec::new();
    let mut current = Vec::new();
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        if ch.is_ascii_digit() {
            if current.is_empty() {
                start = i;
            }
            current.push(ch as u8 - b'0');
        } else if ch == '-' || ch == ' ' {
            // allow separators inside digit runs
        } else {
            if current.len() >= 13 {
                runs.push((start, current.clone()));
            }
            current.clear();
        }
    }
    if current.len() >= 13 {
        runs.push((start, current));
    }
    runs
}

fn redact_middle(value: &str, keep_start: usize, keep_end: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= keep_start + keep_end {
        return "XXXX".to_string();
    }
    let head: String = chars[..keep_start].iter().collect();
    let tail: String = chars[chars.len() - keep_end..].iter().collect();
    let middle_len = chars.len() - keep_start - keep_end;
    format!("{head}{}{tail}", "X".repeat(middle_len))
}

fn scan_content_patterns(text: &str) -> Vec<DlpPatternMatch> {
    let mut matches = Vec::new();

    // ── Credit card detection (Luhn-validated) ──────────────────
    for (offset, digits) in extract_digit_runs(text) {
        if luhn_check(&digits) {
            let display: String = digits.iter().map(|d| (d + b'0') as char).collect();
            matches.push(DlpPatternMatch {
                pattern_name: "credit_card_luhn".into(),
                data_class: SensitiveDataClass::CreditCard,
                redacted_match: redact_middle(&display, 4, 4),
                byte_offset: offset,
                confidence: 0.92,
            });
        }
    }

    // ── Email address detection ─────────────────────────────────
    let bytes = text.as_bytes();
    for (at_pos, _) in text.char_indices().filter(|(_, c)| *c == '@') {
        if at_pos == 0 || at_pos + 1 >= bytes.len() {
            continue;
        }
        let local_start = (0..at_pos)
            .rev()
            .take_while(|&j| {
                bytes[j].is_ascii_alphanumeric()
                    || bytes[j] == b'.'
                    || bytes[j] == b'_'
                    || bytes[j] == b'-'
                    || bytes[j] == b'+'
            })
            .last();
        let domain_end = ((at_pos + 1)..bytes.len())
            .take_while(|&j| {
                bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || bytes[j] == b'-'
            })
            .last();
        if let (Some(ls), Some(de)) = (local_start, domain_end) {
            let candidate = &text[ls..=de];
            if candidate.len() >= 5
                && candidate[at_pos - ls + 1..].contains('.')
                && candidate.ends_with(|c: char| c.is_ascii_alphabetic())
            {
                matches.push(DlpPatternMatch {
                    pattern_name: "email_address".into(),
                    data_class: SensitiveDataClass::EmailAddress,
                    redacted_match: redact_middle(candidate, 2, 4),
                    byte_offset: ls,
                    confidence: 0.88,
                });
            }
        }
    }

    // ── Phone number detection ──────────────────────────────────
    {
        let chars: Vec<char> = text.chars().collect();
        let mut j = 0;
        while j < chars.len() {
            if chars[j] == '+' || (chars[j].is_ascii_digit() && j + 9 < chars.len()) {
                let start = j;
                let mut digits = 0u32;
                let mut k = j;
                while k < chars.len() && k - start < 20 {
                    if chars[k].is_ascii_digit() {
                        digits += 1;
                    } else if chars[k] == '-'
                        || chars[k] == ' '
                        || chars[k] == '('
                        || chars[k] == ')'
                        || chars[k] == '+'
                    {
                        // separator
                    } else {
                        break;
                    }
                    k += 1;
                }
                if digits >= 10 && digits <= 15 {
                    let raw: String = chars[start..k].iter().collect();
                    matches.push(DlpPatternMatch {
                        pattern_name: "phone_number".into(),
                        data_class: SensitiveDataClass::PhoneNumber,
                        redacted_match: redact_middle(&raw, 3, 2),
                        byte_offset: start,
                        confidence: 0.78,
                    });
                    j = k;
                    continue;
                }
            }
            j += 1;
        }
    }

    // ── AWS access key detection (AKIA...) ──────────────────────
    {
        let prefixes = ["AKIA", "ABIA", "ACCA", "ASIA"];
        for prefix in prefixes {
            let mut search_from = 0;
            while search_from < text.len() {
                let Some(pos) = text[search_from..].find(prefix) else {
                    break;
                };
                let abs_pos = search_from + pos;
                if abs_pos + 20 <= text.len() {
                    let candidate = &text[abs_pos..abs_pos + 20];
                    if candidate
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    {
                        matches.push(DlpPatternMatch {
                            pattern_name: "aws_access_key".into(),
                            data_class: SensitiveDataClass::CloudSecret,
                            redacted_match: redact_middle(candidate, 4, 4),
                            byte_offset: abs_pos,
                            confidence: 0.94,
                        });
                    }
                }
                search_from = abs_pos + 1;
            }
        }
    }

    // ── JWT token detection (eyJ...) ────────────────────────────
    {
        let mut search_from = 0;
        while search_from < text.len() {
            let Some(pos) = text[search_from..].find("eyJ") else {
                break;
            };
            let abs_pos = search_from + pos;
            let rest = &text[abs_pos..];
            let is_base64url =
                |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.';
            let token_end = rest
                .char_indices()
                .take_while(|(_, c)| is_base64url(*c))
                .last()
                .map(|(i, _)| i + 1)
                .unwrap_or(0);
            let candidate = &rest[..token_end];
            let dot_count = candidate.chars().filter(|&c| c == '.').count();
            if dot_count == 2 && candidate.len() >= 30 {
                matches.push(DlpPatternMatch {
                    pattern_name: "jwt_token".into(),
                    data_class: SensitiveDataClass::CloudSecret,
                    redacted_match: redact_middle(candidate, 6, 4),
                    byte_offset: abs_pos,
                    confidence: 0.90,
                });
            }
            search_from = abs_pos + 1;
        }
    }

    // ── Database connection string detection ────────────────────
    {
        let db_prefixes = [
            "postgres://",
            "postgresql://",
            "mysql://",
            "mongodb://",
            "mongodb+srv://",
            "redis://",
            "rediss://",
            "amqp://",
            "amqps://",
        ];
        let lower = text.to_ascii_lowercase();
        for prefix in db_prefixes {
            let mut search_from = 0;
            while search_from < lower.len() {
                let Some(pos) = lower[search_from..].find(prefix) else {
                    break;
                };
                let abs_pos = search_from + pos;
                let rest = &text[abs_pos..];
                let conn_end = rest
                    .char_indices()
                    .take_while(|(_, c)| !c.is_ascii_whitespace())
                    .last()
                    .map(|(i, _)| i + 1)
                    .unwrap_or(0);
                let candidate = &rest[..conn_end];
                if candidate.contains('@') && candidate.contains(':') {
                    matches.push(DlpPatternMatch {
                        pattern_name: "database_connection_string".into(),
                        data_class: SensitiveDataClass::DatabaseConnectionString,
                        redacted_match: format!("{prefix}XXXX@XXXX"),
                        byte_offset: abs_pos,
                        confidence: 0.93,
                    });
                }
                search_from = abs_pos + 1;
            }
        }
    }

    matches
}

fn compute_scan_risk(pattern_matches: &[DlpPatternMatch], entropy: f64) -> f64 {
    let mut score = 0.0_f64;
    for m in pattern_matches {
        score += match m.data_class {
            SensitiveDataClass::CreditCard => 0.38,
            SensitiveDataClass::CloudSecret => 0.44,
            SensitiveDataClass::DatabaseConnectionString => 0.40,
            SensitiveDataClass::EmailAddress => 0.14,
            SensitiveDataClass::PhoneNumber => 0.12,
            _ => 0.20,
        };
    }
    if entropy >= 4.5 {
        score += 0.15;
    }
    score.clamp(0.0, 1.0)
}

/// Scan outbound text (e.g., AI-generated responses) for accidentally leaked
/// sensitive data. Returns a report with all pattern matches and risk score.
pub fn scan_outbound_text(text: &str) -> DlpContentScanReport {
    let pattern_matches = scan_content_patterns(text);
    let entropy = shannon_entropy(text);
    let mut data_classes_found = Vec::new();
    for m in &pattern_matches {
        if !data_classes_found.contains(&m.data_class) {
            data_classes_found.push(m.data_class);
        }
    }
    let risk_score = compute_scan_risk(&pattern_matches, entropy);
    DlpContentScanReport {
        scan_id: new_id("dlp_scan"),
        matches: pattern_matches,
        shannon_entropy: (entropy * 1000.0).round() / 1000.0,
        content_length: text.len(),
        data_classes_found,
        risk_score: round3(risk_score),
        scanned_at_ms: now_ms(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_event() -> SubmitGuardianEventRequest {
        SubmitGuardianEventRequest {
            kind: OsGuardianEventKind::Network,
            source: "sensor.local".into(),
            subject: "https://example.com/upload".into(),
            pid: Some(42),
            metadata: BTreeMap::new(),
            observed_at_ms: Some(1),
        }
    }

    #[test]
    fn threat_scoring_is_deterministic_for_same_input_shape() {
        let mut first = normalize_event(base_event()).expect("event");
        let mut second = normalize_event(base_event()).expect("event");
        first.event_id = "fixed".into();
        second.event_id = "fixed".into();
        first.ingested_at_ms = 10;
        second.ingested_at_ms = 10;

        let a = evaluate_threat(&first);
        let b = evaluate_threat(&second);

        assert_eq!(a.severity, b.severity);
        assert_eq!(a.confidence, b.confidence);
        assert_eq!(a.recommended_action, b.recommended_action);
    }

    #[test]
    fn secret_like_metadata_is_redacted() {
        let mut metadata = BTreeMap::new();
        metadata.insert("api_key".into(), "super-secret-value".into());
        let event = normalize_event(SubmitGuardianEventRequest {
            metadata,
            ..base_event()
        })
        .expect("event");

        assert!(event.metadata["api_key"].starts_with("redacted:sha3:"));
        assert!(!event.metadata["api_key"].contains("super-secret-value"));
    }

    #[test]
    fn destructive_actions_are_approval_required_not_executed() {
        let mut metadata = BTreeMap::new();
        metadata.insert("requested_action".into(), "kill_process".into());
        metadata.insert("payload_hint".into(), "credential".into());
        let event = normalize_event(SubmitGuardianEventRequest {
            metadata,
            subject: "https://pastebin.com/upload".into(),
            ..base_event()
        })
        .expect("event");
        let evaluation = evaluate_threat(&event);

        assert_eq!(
            evaluation.recommended_action,
            GuardianRecommendedAction::ApprovalRequired
        );
        assert!(evaluation.destructive_action_suppressed);
    }

    #[test]
    fn hash_chain_detects_tampering() {
        let service = OsGuardianService::new();
        service.submit_event(base_event()).expect("event accepted");
        assert!(service.verify_ledger().valid);

        service.tamper_latest_block_for_test();
        let report = service.verify_ledger();
        assert!(!report.valid);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn every_event_gets_httpa_assignment_and_chain_receipt() {
        let service = OsGuardianService::new();
        let response = service.submit_event(base_event()).expect("event accepted");

        assert!(response.assignment.httpa.chain_receipt_required);
        assert!(
            response
                .assignment
                .httpa
                .trace_id
                .starts_with("httpa_trace_")
        );
        assert_eq!(
            response.receipt.httpa_trace_id,
            response.assignment.httpa.trace_id
        );
        assert_eq!(service.status().ledger_height, 1);
    }

    fn base_dlp_signal() -> DataLeakSignal {
        DataLeakSignal {
            source_process: "astra-assistant.exe".into(),
            subject: "https://example.com/upload".into(),
            destination: Some("https://example.com/upload".into()),
            owner_authorized: false,
            data_classes: Vec::new(),
            leak_vectors: Vec::new(),
            metadata: BTreeMap::new(),
            content_sample: None,
            observed_at_ms: Some(1),
        }
    }

    #[test]
    fn dlp_detects_dns_tunneling_pattern() {
        let mut signal = base_dlp_signal();
        signal.subject = "dns query".into();
        signal.destination = Some(format!("{}.evil.example", "A".repeat(56)));
        signal.metadata.insert("protocol".into(), "dns".into());

        let record = normalize_dlp_signal(signal).expect("signal");
        let verdict = evaluate_dlp_signal(&record);

        assert!(record.leak_vectors.contains(&LeakVector::DnsQuery));
        assert!(
            verdict
                .rationale
                .iter()
                .any(|entry| entry.contains("DNS-tunneling"))
        );
    }

    #[test]
    fn dlp_redacts_wallet_seed_and_api_key_material() {
        let mut signal = base_dlp_signal();
        signal.subject = "wallet.dat upload".into();
        signal
            .metadata
            .insert("api_key".into(), "secret-token".into());
        signal.content_sample = Some(
            "seed phrase alpha beta gamma delta epsilon zeta eta theta iota kappa lambda".into(),
        );

        let record = normalize_dlp_signal(signal).expect("signal");

        assert!(
            record
                .data_classes
                .contains(&SensitiveDataClass::WalletSeed)
        );
        assert!(record.metadata["api_key"].starts_with("redacted:sha3:"));
        assert!(
            record
                .content_sample
                .as_ref()
                .unwrap()
                .starts_with("redacted:sha3:")
        );
    }

    #[test]
    fn owner_authorized_dlp_lowers_severity_without_bypassing_audit() {
        let mut unauthorized = base_dlp_signal();
        unauthorized
            .metadata
            .insert("payload_hint".into(), "api_key".into());
        let mut authorized = unauthorized.clone();
        authorized.owner_authorized = true;

        let unauthorized_verdict =
            evaluate_dlp_signal(&normalize_dlp_signal(unauthorized).expect("signal"));
        let authorized_verdict =
            evaluate_dlp_signal(&normalize_dlp_signal(authorized).expect("signal"));

        assert!(authorized_verdict.risk_score < unauthorized_verdict.risk_score);
        assert!(authorized_verdict.owner_authorized);
        assert!(!authorized_verdict.rationale.is_empty());
    }

    #[test]
    fn high_risk_dlp_exfiltration_requires_owner_approval() {
        let mut signal = base_dlp_signal();
        signal.source_process = "powershell.exe".into();
        signal.subject = "wallet.dat upload".into();
        signal.destination = Some("https://pastebin.com/upload".into());
        signal.content_sample = Some("recovery phrase wallet seed phrase".into());

        let response = OsGuardianService::new()
            .analyze_dlp_signal(signal)
            .expect("analysis");

        assert_eq!(
            response.verdict.recommended_control,
            DlpRecommendedControl::PendingOwnerApproval
        );
        assert!(response.pending_decision.is_some());
        assert!(response.decoy_manifest.is_some());
    }

    #[test]
    fn dlp_analysis_creates_httpa_assignment_and_chain_receipt() {
        let service = OsGuardianService::new();
        let response = service
            .analyze_dlp_signal(base_dlp_signal())
            .expect("analysis");

        assert!(response.assignment.httpa.chain_receipt_required);
        assert_eq!(
            response.receipt.httpa_trace_id,
            response.assignment.httpa.trace_id
        );
        assert!(service.verify_ledger().valid);
    }

    // ═════════════════════════════════════════════════════════════
    // ADVANCED PATTERN SCANNING TESTS
    // ═════════════════════════════════════════════════════════════

    #[test]
    fn scan_detects_credit_card_with_luhn() {
        // 4111111111111111 is the standard Visa test card number
        let report = scan_outbound_text("my card is 4111111111111111 thanks");
        assert!(
            report
                .data_classes_found
                .contains(&SensitiveDataClass::CreditCard)
        );
        assert!(!report.matches.is_empty());
        assert_eq!(report.matches[0].pattern_name, "credit_card_luhn");
        assert!(report.matches[0].redacted_match.contains("XXXX"));
        assert!(report.risk_score > 0.0);
    }

    #[test]
    fn scan_detects_email_addresses() {
        let report = scan_outbound_text("contact admin@example.com for help");
        assert!(
            report
                .data_classes_found
                .contains(&SensitiveDataClass::EmailAddress)
        );
        assert!(
            report
                .matches
                .iter()
                .any(|m| m.pattern_name == "email_address")
        );
    }

    #[test]
    fn scan_detects_phone_numbers() {
        let report = scan_outbound_text("call me at +1-555-123-4567 asap");
        assert!(
            report
                .data_classes_found
                .contains(&SensitiveDataClass::PhoneNumber)
        );
        assert!(
            report
                .matches
                .iter()
                .any(|m| m.pattern_name == "phone_number")
        );
    }

    #[test]
    fn scan_detects_aws_access_key() {
        let report = scan_outbound_text("key is AKIAIOSFODNN7EXAMPLE");
        assert!(
            report
                .data_classes_found
                .contains(&SensitiveDataClass::CloudSecret)
        );
        assert!(
            report
                .matches
                .iter()
                .any(|m| m.pattern_name == "aws_access_key")
        );
        assert!(report.matches[0].confidence >= 0.90);
    }

    #[test]
    fn scan_detects_jwt_token() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let report = scan_outbound_text(&format!("bearer {token}"));
        assert!(
            report
                .data_classes_found
                .contains(&SensitiveDataClass::CloudSecret)
        );
        assert!(report.matches.iter().any(|m| m.pattern_name == "jwt_token"));
    }

    #[test]
    fn scan_detects_database_connection_string() {
        let report =
            scan_outbound_text("connect to postgres://admin:password123@db.example.com/prod");
        assert!(
            report
                .data_classes_found
                .contains(&SensitiveDataClass::DatabaseConnectionString)
        );
        assert!(
            report
                .matches
                .iter()
                .any(|m| m.pattern_name == "database_connection_string")
        );
    }

    #[test]
    fn shannon_entropy_is_high_for_random_string() {
        let random = "aB3$xZ9!mK7@pQ2#wR5&jL8*nT4^yH6";
        let e = shannon_entropy(random);
        assert!(e >= 4.0, "expected high entropy, got {e}");
    }

    #[test]
    fn shannon_entropy_is_low_for_repeated_chars() {
        let repeated = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let e = shannon_entropy(repeated);
        assert!(e < 0.01, "expected near-zero entropy, got {e}");
    }

    #[test]
    fn outbound_scan_catches_leaked_api_key() {
        let text = "Here is your config: AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let report = scan_outbound_text(text);
        assert!(!report.matches.is_empty());
        assert!(report.risk_score > 0.0);
    }

    #[test]
    fn outbound_scan_clean_text_returns_empty_matches() {
        let report = scan_outbound_text("The weather today is sunny and warm.");
        assert!(report.matches.is_empty());
        assert_eq!(report.risk_score, 0.0);
    }

    #[test]
    fn new_exfil_destinations_are_flagged() {
        assert!(destination_is_public_exfil(Some("https://transfer.sh/abc")));
        assert!(destination_is_public_exfil(Some("https://ngrok.io/tunnel")));
        assert!(destination_is_public_exfil(Some(
            "https://ghostbin.com/paste"
        )));
        assert!(!destination_is_public_exfil(Some("https://github.com")));
    }

    #[test]
    fn new_suspicious_processes_are_flagged() {
        assert!(suspicious_process("certutil.exe -encode"));
        assert!(suspicious_process("mshta.exe http://evil"));
        assert!(suspicious_process("bitsadmin /transfer"));
        assert!(!suspicious_process("notepad.exe"));
    }
}

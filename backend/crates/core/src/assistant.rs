use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::agent_runtime::ActivityExecution;
use crate::common::{AppError, new_id, now_ms, sha3_hex};
use crate::httpa::HttpaReceipt;
use crate::os_guardian::{
    DlpAnalysisResponse, DlpRecommendedControl, GuardianLedgerReceipt, GuardianSeverity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantSourcePlatform {
    Windows,
    Android,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantActivationMethod {
    SearchBar,
    WindowsHotkey,
    AndroidAccessibilityShortcut,
    InAppButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantAutonomyMode {
    SafeAuto,
    AlwaysConfirm,
    FullAuto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantActionKind {
    OpenUrl,
    OpenApp,
    SearchWeb,
    SubmitGuardianEvent,
    AnalyzeDlp,
    CreateAgentExecution,
    RespondText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantCommandStatus {
    Accepted,
    Ready,
    ApprovalRequired,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantCommandRequest {
    pub text: String,
    pub source_platform: AssistantSourcePlatform,
    pub activation_method: AssistantActivationMethod,
    pub autonomy_mode: AssistantAutonomyMode,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantActionPlan {
    pub plan_id: String,
    pub action_kind: AssistantActionKind,
    pub executable: bool,
    pub requires_approval: bool,
    pub target: Option<String>,
    pub arguments: serde_json::Value,
    pub safety_notes: Vec<String>,
    pub signed_plan_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantCommandResponse {
    pub command_id: String,
    pub status: AssistantCommandStatus,
    pub normalized_text: String,
    pub source_platform: AssistantSourcePlatform,
    pub activation_method: AssistantActivationMethod,
    pub autonomy_mode: AssistantAutonomyMode,
    pub device_id_hash: String,
    pub action_plan: AssistantActionPlan,
    pub guardian_receipt: GuardianLedgerReceipt,
    pub orchestration: Option<ActivityExecution>,
    pub httpa_receipt: Option<HttpaReceipt>,
    pub guardian_severity: GuardianSeverity,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Default)]
struct AssistantCommandStore {
    commands: HashMap<String, AssistantCommandResponse>,
}

#[derive(Clone)]
pub struct AssistantCommandService {
    store: Shared<AssistantCommandStore>,
}

impl AssistantCommandService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Shared::new(RwLock::new(AssistantCommandStore::default())),
        }
    }

    pub fn create_command(
        &self,
        request: AssistantCommandRequest,
        dlp: &DlpAnalysisResponse,
    ) -> Result<AssistantCommandResponse, AppError> {
        let normalized_text = normalize_command_text(&request.text)?;
        let device_id = normalize_device_id(&request.device_id)?;
        let action_plan = build_action_plan(&normalized_text, request.autonomy_mode, dlp)?;
        let status = command_status_for(&action_plan, dlp);
        let now = now_ms();
        let command_id = new_id("assistant_cmd");
        let response = AssistantCommandResponse {
            command_id: command_id.clone(),
            status,
            normalized_text,
            source_platform: request.source_platform,
            activation_method: request.activation_method,
            autonomy_mode: request.autonomy_mode,
            device_id_hash: sha3_hex(device_id.as_bytes())[..16].to_string(),
            action_plan,
            guardian_receipt: dlp.receipt.clone(),
            orchestration: None,
            httpa_receipt: None,
            guardian_severity: dlp.verdict.severity,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.store
            .write()
            .commands
            .insert(command_id, response.clone());
        Ok(response)
    }

    pub fn get_command(&self, command_id: &str) -> Result<AssistantCommandResponse, AppError> {
        self.store
            .read()
            .commands
            .get(command_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("assistant command {command_id}")))
    }

    pub fn attach_orchestration(
        &self,
        command_id: &str,
        orchestration: ActivityExecution,
        httpa_receipt: HttpaReceipt,
    ) -> Result<AssistantCommandResponse, AppError> {
        let mut store = self.store.write();
        let command = store
            .commands
            .get_mut(command_id)
            .ok_or_else(|| AppError::NotFound(format!("assistant command {command_id}")))?;
        if orchestration.asc2.as_ref().is_some_and(|diagnostics| {
            diagnostics.mode == crate::asc2::Asc2Mode::Active && !diagnostics.side_effects_allowed
        }) {
            command.action_plan.executable = false;
            command.action_plan.requires_approval = true;
            command
                .action_plan
                .safety_notes
                .push("ASC-II active runtime did not certify external side effects".into());
            command.status = AssistantCommandStatus::ApprovalRequired;
        }
        if orchestration.sandbox.as_ref().is_some_and(|sandbox| {
            matches!(
                sandbox.decision.outcome,
                crate::sandbox::SandboxDecisionOutcome::Deny
                    | crate::sandbox::SandboxDecisionOutcome::ApprovalRequired
                    | crate::sandbox::SandboxDecisionOutcome::SandboxUnavailable
            )
        }) {
            command.action_plan.executable = false;
            command.action_plan.requires_approval = true;
            command
                .action_plan
                .safety_notes
                .push("sandbox reference monitor blocked or deferred effectful execution".into());
            command.status = AssistantCommandStatus::ApprovalRequired;
        }
        command.orchestration = Some(orchestration);
        command.httpa_receipt = Some(httpa_receipt);
        command.updated_at_ms = now_ms();
        Ok(command.clone())
    }
}

impl Default for AssistantCommandService {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_command_text(text: &str) -> Result<String, AppError> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(AppError::Validation(
            "assistant command text must not be empty".into(),
        ));
    }
    if normalized.len() > 16384 {
        return Err(AppError::Validation(
            "assistant command text is too long".into(),
        ));
    }
    Ok(normalized)
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

fn build_action_plan(
    normalized_text: &str,
    autonomy_mode: AssistantAutonomyMode,
    dlp: &DlpAnalysisResponse,
) -> Result<AssistantActionPlan, AppError> {
    let lower = normalized_text.to_ascii_lowercase();
    let mut safety_notes = vec![
        "plan generated by backend command gateway".into(),
        "local client must execute only signed allowlisted actions".into(),
    ];
    let (action_kind, target, arguments) = if let Some(url) = extract_url(normalized_text) {
        (
            AssistantActionKind::OpenUrl,
            Some(url.clone()),
            serde_json::json!({ "url": url }),
        )
    } else if lower.starts_with("search ") || lower.starts_with("find ") {
        let query = normalized_text
            .split_once(' ')
            .map(|(_, query)| query.trim())
            .unwrap_or(normalized_text);
        (
            AssistantActionKind::SearchWeb,
            Some(query.to_string()),
            serde_json::json!({ "query": query }),
        )
    } else if let Some(app) = lower.strip_prefix("open ") {
        let app = app.trim();
        let mapped = map_allowlisted_app(app);
        if let Some(app_id) = mapped {
            (
                AssistantActionKind::OpenApp,
                Some(app_id.to_string()),
                serde_json::json!({ "app_id": app_id }),
            )
        } else {
            safety_notes.push("requested app is not allowlisted for autonomous launch".into());
            (
                AssistantActionKind::RespondText,
                None,
                serde_json::json!({
                    "message": "I can only auto-open allowlisted local apps right now."
                }),
            )
        }
    } else if contains_blocked_intent(&lower) {
        safety_notes.push("command resembles destructive or privilege-sensitive intent".into());
        (
            AssistantActionKind::RespondText,
            None,
            serde_json::json!({
                "message": "This action needs explicit review before it can run."
            }),
        )
    } else {
        (
            AssistantActionKind::RespondText,
            None,
            serde_json::json!({
                "message": "I can open URLs and allowlisted apps and run web searches. \
                            For open-ended questions, ask via the Telegram assistant, which \
                            researches the web and answers with sources."
            }),
        )
    };

    let dlp_requires_approval = dlp.verdict.approval_required
        || matches!(
            dlp.verdict.recommended_control,
            DlpRecommendedControl::PendingOwnerApproval
        );
    let full_auto = matches!(autonomy_mode, AssistantAutonomyMode::FullAuto);
    let executable = matches!(
        action_kind,
        AssistantActionKind::OpenUrl
            | AssistantActionKind::OpenApp
            | AssistantActionKind::SearchWeb
    ) && !dlp_requires_approval
        && !contains_blocked_intent(&lower)
        && (full_auto || matches!(autonomy_mode, AssistantAutonomyMode::SafeAuto));
    let requires_approval = dlp_requires_approval
        || matches!(autonomy_mode, AssistantAutonomyMode::AlwaysConfirm)
        || contains_blocked_intent(&lower);
    if dlp_requires_approval {
        safety_notes.push("OS Guardian/DLP requires owner approval".into());
    }

    let plan_id = new_id("assistant_plan");
    let signed_plan_hash = sha3_hex(
        serde_json::json!({
            "plan_id": plan_id,
            "action_kind": action_kind,
            "target": target,
            "arguments": arguments,
            "executable": executable,
            "requires_approval": requires_approval,
            "guardian_receipt": dlp.receipt.receipt_id,
        })
        .to_string()
        .as_bytes(),
    );

    Ok(AssistantActionPlan {
        plan_id,
        action_kind,
        executable,
        requires_approval,
        target,
        arguments,
        safety_notes,
        signed_plan_hash,
    })
}

fn command_status_for(
    action_plan: &AssistantActionPlan,
    dlp: &DlpAnalysisResponse,
) -> AssistantCommandStatus {
    if dlp.verdict.severity == GuardianSeverity::Critical {
        AssistantCommandStatus::Blocked
    } else if action_plan.requires_approval {
        AssistantCommandStatus::ApprovalRequired
    } else if action_plan.executable {
        AssistantCommandStatus::Ready
    } else {
        AssistantCommandStatus::Accepted
    }
}

fn extract_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.starts_with("https://") || part.starts_with("http://"))
        .and_then(|url| {
            if url.starts_with("https://") || url.starts_with("http://127.0.0.1") {
                Some(url.trim_matches(['.', ',', ';']).to_string())
            } else {
                None
            }
        })
}

fn map_allowlisted_app(app: &str) -> Option<&'static str> {
    match app {
        "calculator" | "calc" => Some("calculator"),
        "notepad" | "notes" => Some("notepad"),
        "browser" => Some("browser"),
        _ => None,
    }
}

fn contains_blocked_intent(text: &str) -> bool {
    [
        "delete system",
        "format ",
        "shutdown",
        "kill process",
        "steal",
        "dump password",
        "private key",
        "seed phrase",
        "disable security",
        "admin privilege",
        "credential dump",
        "sign transaction",
        "registry",
        "network block",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os_guardian::{
        DataLeakSignal, DlpRecommendedControl, OsGuardianService, SensitiveDataClass,
    };

    fn dlp_for_text(text: &str) -> DlpAnalysisResponse {
        OsGuardianService::new()
            .analyze_dlp_signal(DataLeakSignal {
                source_process: "assistant-ui".into(),
                subject: text.into(),
                destination: None,
                owner_authorized: true,
                data_classes: Vec::new(),
                leak_vectors: Vec::new(),
                metadata: Default::default(),
                content_sample: None,
                observed_at_ms: None,
            })
            .expect("dlp")
    }

    #[test]
    fn safe_url_command_is_executable_with_receipt() {
        let service = AssistantCommandService::new();
        let dlp = dlp_for_text("open https://example.com");
        let response = service
            .create_command(
                AssistantCommandRequest {
                    text: "open https://example.com".into(),
                    source_platform: AssistantSourcePlatform::Windows,
                    activation_method: AssistantActivationMethod::WindowsHotkey,
                    autonomy_mode: AssistantAutonomyMode::FullAuto,
                    device_id: "device".into(),
                },
                &dlp,
            )
            .expect("command");

        assert_eq!(response.status, AssistantCommandStatus::Ready);
        assert!(response.action_plan.executable);
        assert_eq!(response.guardian_receipt.receipt_id, dlp.receipt.receipt_id);
    }

    #[test]
    fn sensitive_command_is_not_executable_even_in_full_auto() {
        let service = AssistantCommandService::new();
        let mut dlp = dlp_for_text("dump private key");
        dlp.verdict.recommended_control = DlpRecommendedControl::PendingOwnerApproval;
        dlp.verdict.approval_required = true;
        dlp.verdict.data_classes = vec![SensitiveDataClass::PrivateKey];
        let response = service
            .create_command(
                AssistantCommandRequest {
                    text: "dump private key".into(),
                    source_platform: AssistantSourcePlatform::Android,
                    activation_method: AssistantActivationMethod::AndroidAccessibilityShortcut,
                    autonomy_mode: AssistantAutonomyMode::FullAuto,
                    device_id: "device".into(),
                },
                &dlp,
            )
            .expect("command");

        assert!(!response.action_plan.executable);
        assert!(response.action_plan.requires_approval);
        assert!(matches!(
            response.status,
            AssistantCommandStatus::ApprovalRequired | AssistantCommandStatus::Blocked
        ));
    }
}

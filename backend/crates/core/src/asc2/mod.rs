use std::env;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use astra_brain::{
    ActionToken, Asc2Config, Asc2Controller, Asc2ExecutionResult, ControlActionEstimate,
    ControlDecision, PerformanceSnapshot, ReflexiveAgentState, RoleCandidate, SwarmState,
};
use parking_lot::Mutex;
use reqwest::Client;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms, sha3_hex};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Asc2Mode {
    Disabled,
    Shadow,
    Active,
}

impl Asc2Mode {
    fn from_env() -> Self {
        match env::var("ASTRA_ASC2_MODE")
            .unwrap_or_else(|_| "shadow".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "disabled" => Self::Disabled,
            "active" => Self::Active,
            _ => Self::Shadow,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2RuntimeConfig {
    pub mode: Asc2Mode,
    pub remote_endpoint: Option<String>,
    #[serde(skip_serializing)]
    pub remote_api_key: Option<String>,
    pub remote_model: String,
    pub openclaw_base_url: Option<String>,
    pub auto_promote: bool,
    pub formula: Asc2Config,
}

impl Asc2RuntimeConfig {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            mode: Asc2Mode::from_env(),
            remote_endpoint: nonempty_env("ASTRA_ASC2_REMOTE_ENDPOINT"),
            remote_api_key: nonempty_env("ASTRA_ASC2_REMOTE_API_KEY"),
            remote_model: env::var("ASTRA_ASC2_REMOTE_MODEL")
                .unwrap_or_else(|_| "gpt-5-mini".into()),
            openclaw_base_url: nonempty_env("ASTRA_ASC2_OPENCLAW_BASE_URL"),
            auto_promote: boolean_env("ASTRA_ASC2_AUTO_PROMOTE", false),
            formula: Asc2Config::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningRequest {
    pub objective: String,
    pub role: String,
    pub round: usize,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningProposal {
    pub executor: String,
    pub role: String,
    pub claim: String,
    pub evidence_score: f64,
    pub proof_score: f64,
    pub confidence: f64,
    pub uncertainty: f64,
    pub risk: f64,
    pub limitation: String,
    pub duration_ms: f64,
}

pub trait ReasoningExecutor: Send + Sync {
    fn name(&self) -> &str;
    fn execute(&self, request: ReasoningRequest) -> BoxFuture<Result<ReasoningProposal, AppError>>;
}

pub trait ToolExecutor: Send + Sync {
    fn execute_tool(
        &self,
        tool: String,
        input: serde_json::Value,
    ) -> BoxFuture<Result<serde_json::Value, AppError>>;
}

pub trait BaselineRunner: Send + Sync {
    fn run_baseline(&self, objective: String) -> BoxFuture<Result<BaselineResult, AppError>>;
}

#[derive(Clone)]
pub struct LocalBaselineRunner {
    executor: Arc<dyn ReasoningExecutor>,
}

impl LocalBaselineRunner {
    #[must_use]
    pub fn new(executor: Arc<dyn ReasoningExecutor>) -> Self {
        Self { executor }
    }
}

impl BaselineRunner for LocalBaselineRunner {
    fn run_baseline(&self, objective: String) -> BoxFuture<Result<BaselineResult, AppError>> {
        let executor = self.executor.clone();
        Box::pin(async move {
            let proposal = executor
                .execute(ReasoningRequest {
                    objective,
                    role: "baseline solver".into(),
                    round: 0,
                    sensitive: false,
                })
                .await?;
            Ok(BaselineResult {
                answer: proposal.claim,
                duration_ms: proposal.duration_ms,
                risk: proposal.risk,
            })
        })
    }
}

#[derive(Clone)]
pub struct HttpBaselineRunner {
    client: Client,
    endpoint: String,
}

impl HttpBaselineRunner {
    #[must_use]
    pub fn new(endpoint: String) -> Self {
        Self {
            client: Client::new(),
            endpoint,
        }
    }
}

impl BaselineRunner for HttpBaselineRunner {
    fn run_baseline(&self, objective: String) -> BoxFuture<Result<BaselineResult, AppError>> {
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        Box::pin(async move {
            let start = Instant::now();
            let response = client
                .post(endpoint)
                .json(&serde_json::json!({"objective":objective}))
                .send()
                .await
                .map_err(|error| {
                    AppError::Internal(format!("OpenClaw baseline request failed: {error}"))
                })?;
            if !response.status().is_success() {
                return Err(AppError::Internal(format!(
                    "OpenClaw baseline returned status {}",
                    response.status()
                )));
            }
            let payload: serde_json::Value = response.json().await.map_err(|error| {
                AppError::Internal(format!("OpenClaw baseline response failed: {error}"))
            })?;
            let answer = payload
                .pointer("/data/answer")
                .or_else(|| payload.get("answer"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(BaselineResult {
                answer,
                duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                risk: payload
                    .pointer("/data/risk")
                    .or_else(|| payload.get("risk"))
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.12),
            })
        })
    }
}

pub trait StrategyPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn propose_roles(&self, objective: &str, dimensions: usize) -> Vec<RoleCandidate>;
}

#[derive(Debug, Clone, Default)]
pub struct DryRunToolExecutor;

impl ToolExecutor for DryRunToolExecutor {
    fn execute_tool(
        &self,
        tool: String,
        input: serde_json::Value,
    ) -> BoxFuture<Result<serde_json::Value, AppError>> {
        Box::pin(async move {
            Ok(serde_json::json!({
                "status": "planned",
                "tool": tool,
                "input_hash": sha3_hex(input.to_string().as_bytes()),
                "executed": false,
            }))
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct DefaultStrategyPlugin;

impl StrategyPlugin for DefaultStrategyPlugin {
    fn name(&self) -> &str {
        "asc2_default_strategy"
    }

    fn propose_roles(&self, objective: &str, dimensions: usize) -> Vec<RoleCandidate> {
        default_role_candidates(&task_embedding(objective, dimensions))
    }
}

#[derive(Clone)]
pub struct LocalReasoningExecutor {
    brain: Shared<astra_brain::CognitiveCoreEngine>,
}

impl LocalReasoningExecutor {
    #[must_use]
    pub fn new(brain: Shared<astra_brain::CognitiveCoreEngine>) -> Self {
        Self { brain }
    }
}

impl ReasoningExecutor for LocalReasoningExecutor {
    fn name(&self) -> &str {
        "local_cognitive_core"
    }

    fn execute(&self, request: ReasoningRequest) -> BoxFuture<Result<ReasoningProposal, AppError>> {
        let brain = self.brain.clone();
        Box::pin(async move {
            let start = Instant::now();
            let result = brain
                .write()
                .process_5d(&format!("Act as {}. {}", request.role, request.objective));
            let confidence = result
                .solver_result
                .as_ref()
                .map_or(0.62, |solver| solver.confidence)
                .clamp(0.0, 1.0);
            Ok(ReasoningProposal {
                executor: "local_cognitive_core".into(),
                role: request.role,
                claim: result.response,
                evidence_score: confidence * 0.85,
                proof_score: confidence * 0.8,
                confidence,
                uncertainty: 1.0 - confidence,
                risk: if request.sensitive { 0.12 } else { 0.04 },
                limitation:
                    "Deterministic local reasoning is bounded by the installed knowledge and tools."
                        .into(),
                duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            })
        })
    }
}

#[derive(Clone)]
pub struct RemoteReasoningExecutor {
    client: Client,
    endpoint: String,
    api_key: Option<String>,
    model: String,
}

impl RemoteReasoningExecutor {
    #[must_use]
    pub fn new(endpoint: String, api_key: Option<String>, model: String) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            api_key,
            model,
        }
    }
}

impl ReasoningExecutor for RemoteReasoningExecutor {
    fn name(&self) -> &str {
        "remote_openai_compatible"
    }

    fn execute(&self, request: ReasoningRequest) -> BoxFuture<Result<ReasoningProposal, AppError>> {
        let client = self.client.clone();
        let endpoint = completion_endpoint(&self.endpoint);
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        Box::pin(async move {
            if request.sensitive {
                return Err(AppError::Forbidden(
                    "sensitive mission data cannot be sent to a remote reasoning executor".into(),
                ));
            }
            let start = Instant::now();
            let mut builder = client.post(endpoint).json(&serde_json::json!({
                "model": model,
                "temperature": 0.2,
                "messages": [
                    {
                        "role": "system",
                        "content": "Return a concise evidence-aware proposal. State limitations and do not claim tool execution."
                    },
                    {
                        "role": "user",
                        "content": format!("Role: {}\nObjective: {}", request.role, request.objective)
                    }
                ]
            }));
            if let Some(key) = api_key {
                builder = builder.bearer_auth(key);
            }
            let response = builder.send().await.map_err(|error| {
                AppError::Internal(format!("remote reasoning request failed: {error}"))
            })?;
            if !response.status().is_success() {
                return Err(AppError::Internal(format!(
                    "remote reasoning returned status {}",
                    response.status()
                )));
            }
            let value: serde_json::Value = response.json().await.map_err(|error| {
                AppError::Internal(format!("remote reasoning response failed: {error}"))
            })?;
            let claim = value
                .pointer("/choices/0/message/content")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.get("output_text").and_then(serde_json::Value::as_str))
                .unwrap_or("Remote executor returned no textual proposal.")
                .to_string();
            Ok(ReasoningProposal {
                executor: "remote_openai_compatible".into(),
                role: request.role,
                claim,
                evidence_score: 0.72,
                proof_score: 0.66,
                confidence: 0.74,
                uncertainty: 0.26,
                risk: 0.08,
                limitation:
                    "Remote model output is an untrusted proposal pending ASC-II verification."
                        .into(),
                duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            })
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2MissionRequest {
    pub objective: String,
    #[serde(default = "default_activity_kind")]
    pub activity_kind: String,
    #[serde(default)]
    pub requested_tools: Vec<String>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub owner_authorized: bool,
    #[serde(default)]
    pub side_effecting: bool,
    #[serde(default)]
    pub action_token: Option<ActionToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2Diagnostics {
    pub mission_id: String,
    pub mode: Asc2Mode,
    pub control_mode: ControlDecision,
    pub rounds: usize,
    pub agents: Vec<Asc2AgentManifest>,
    pub formula_metrics: astra_brain::Asc2Metrics,
    pub certificate: Option<astra_brain::ActionCertificate>,
    pub performance_score: f64,
    pub return_reason: String,
    pub remote_used: bool,
    pub side_effects_allowed: bool,
    #[serde(default)]
    pub sandbox: Option<crate::sandbox::SandboxReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2AgentManifest {
    pub agent_id: String,
    pub role: String,
    pub capabilities: Vec<f64>,
    pub tool_allowlist: Vec<String>,
    pub autonomy_budget: f64,
    pub risk: f64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2MissionRecord {
    pub mission_id: String,
    pub request: Asc2MissionRequest,
    pub status: String,
    pub answer: String,
    pub proposals: Vec<ReasoningProposal>,
    pub diagnostics: Asc2Diagnostics,
    pub created_at_ms: i64,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2Status {
    pub mode: Asc2Mode,
    pub remote_configured: bool,
    pub remote_model: String,
    pub openclaw_baseline_configured: bool,
    pub auto_promote: bool,
    pub total_missions: usize,
    pub total_benchmarks: usize,
    pub latest_benchmark_promotable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2BenchmarkCase {
    pub id: String,
    pub objective: String,
    pub expected_contains: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2BenchmarkRequest {
    #[serde(default = "default_benchmark_repeats")]
    pub repeats: usize,
    #[serde(default)]
    pub cases: Vec<Asc2BenchmarkCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResult {
    pub answer: String,
    pub duration_ms: f64,
    pub risk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asc2BenchmarkReport {
    pub benchmark_id: String,
    pub total_runs: usize,
    pub asc2_quality: f64,
    pub baseline_quality: f64,
    pub quality_gain: f64,
    pub asc2_p95_latency_ms: f64,
    pub baseline_p95_latency_ms: f64,
    pub safety_regression: bool,
    pub confidence_lower_bound: f64,
    pub promotable: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModificationRecord {
    pub modification_id: String,
    pub candidate_path: String,
    pub status: String,
    pub signed_hash: String,
    pub benchmark_id: Option<String>,
    pub rollback_binary: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModificationCandidate {
    pub workspace_path: String,
    pub changed_paths: Vec<String>,
    pub current_binary: String,
}

#[derive(Clone)]
pub struct Asc2Service {
    config: Asc2RuntimeConfig,
    local: Arc<dyn ReasoningExecutor>,
    remote: Option<Arc<dyn ReasoningExecutor>>,
    baseline: Arc<dyn BaselineRunner>,
    store: Arc<Mutex<Connection>>,
    data_dir: PathBuf,
}

impl Asc2Service {
    pub fn new(
        data_dir: impl AsRef<Path>,
        brain: Shared<astra_brain::CognitiveCoreEngine>,
    ) -> Result<Self, AppError> {
        let config = Asc2RuntimeConfig::from_env();
        let store_path = data_dir.as_ref().join("asc2.sqlite");
        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::Internal(format!("failed to create ASC-II data directory: {error}"))
            })?;
        }
        let connection = Connection::open(&store_path)
            .map_err(|error| AppError::Internal(format!("failed to open ASC-II store: {error}")))?;
        initialize_store(&connection)?;
        let local: Arc<dyn ReasoningExecutor> = Arc::new(LocalReasoningExecutor::new(brain));
        let remote = config.remote_endpoint.clone().map(|endpoint| {
            Arc::new(RemoteReasoningExecutor::new(
                endpoint,
                config.remote_api_key.clone(),
                config.remote_model.clone(),
            )) as Arc<dyn ReasoningExecutor>
        });
        let baseline: Arc<dyn BaselineRunner> = config
            .openclaw_base_url
            .clone()
            .map(|endpoint| Arc::new(HttpBaselineRunner::new(endpoint)) as Arc<dyn BaselineRunner>)
            .unwrap_or_else(|| Arc::new(LocalBaselineRunner::new(local.clone())));
        Ok(Self {
            config,
            local,
            remote,
            baseline,
            store: Arc::new(Mutex::new(connection)),
            data_dir: data_dir.as_ref().to_path_buf(),
        })
    }

    #[must_use]
    pub fn mode(&self) -> Asc2Mode {
        self.config.mode
    }

    pub async fn execute_mission(
        &self,
        mut request: Asc2MissionRequest,
    ) -> Result<Asc2MissionRecord, AppError> {
        request.objective = request.objective.trim().to_string();
        if request.objective.is_empty() {
            return Err(AppError::Validation(
                "ASC-II objective must not be empty".into(),
            ));
        }
        request.sensitive = request.sensitive || contains_sensitive_signal(&request.objective);
        if request.side_effecting && request.action_token.is_none() {
            request.action_token = Some(ActionToken {
                action: request.objective.clone(),
                preconditions: vec![if request.owner_authorized {
                    "owner_authorized".into()
                } else {
                    "owner_authorization_missing".into()
                }],
                invariants: vec!["remain within the requested tool allowlist".into()],
                postconditions: vec!["verify result before reporting completion".into()],
                rollback: vec!["stop the action and restore the previous local state".into()],
                audit: serde_json::json!({
                    "activity_kind": request.activity_kind,
                    "requested_tools": request.requested_tools,
                }),
                scope: request.requested_tools.clone(),
                requested_privilege: if request.owner_authorized { 0.2 } else { 0.8 },
                estimated_risk: if request.owner_authorized { 0.1 } else { 0.8 },
                side_effecting: true,
            });
        }
        let mission_id = new_id("asc2_mission");
        let created_at_ms = now_ms();
        let task_demand = task_embedding(
            &request.objective,
            self.config.formula.capability_dimensions,
        );
        let mut proposals = Vec::new();
        let mut remote_used = false;

        let primary = if !request.sensitive {
            self.remote.as_ref().unwrap_or(&self.local)
        } else {
            &self.local
        };
        match primary
            .execute(ReasoningRequest {
                objective: request.objective.clone(),
                role: "strategic solver".into(),
                round: 0,
                sensitive: request.sensitive,
            })
            .await
        {
            Ok(proposal) => {
                remote_used = proposal.executor == "remote_openai_compatible";
                proposals.push(proposal);
            }
            Err(_) => proposals.push(
                self.local
                    .execute(ReasoningRequest {
                        objective: request.objective.clone(),
                        role: "strategic solver".into(),
                        round: 0,
                        sensitive: request.sensitive,
                    })
                    .await?,
            ),
        }
        for role in ["adversarial critic", "safety verifier"] {
            proposals.push(
                self.local
                    .execute(ReasoningRequest {
                        objective: request.objective.clone(),
                        role: role.into(),
                        round: 0,
                        sensitive: request.sensitive,
                    })
                    .await?,
            );
        }

        let controller = Asc2Controller::new(self.config.formula.clone());
        let role_candidates = default_role_candidates(&task_demand);
        let control_actions = default_control_actions(&request);
        let mut result = fallback_result();
        let mut state = SwarmState {
            task: request.objective.clone(),
            task_demand,
            agents: proposals
                .iter()
                .enumerate()
                .map(|(index, proposal)| {
                    agent_from_proposal(index, proposal, self.config.formula.capability_dimensions)
                })
                .collect(),
            controller_policy: vec![0.5; self.config.formula.capability_dimensions],
            action_tokens: request.action_token.clone().into_iter().collect(),
            safety_ledger: Vec::new(),
            spawn_threshold: self.config.formula.spawn_threshold,
            global_autonomy_budget: if request.owner_authorized { 0.5 } else { 0.15 },
            round: 0,
        };
        let baseline = baseline_snapshot(&request);
        let mut previous = PerformanceSnapshot::default();
        for round in 1..=self.config.formula.max_rounds {
            state.round = round;
            let current = performance_snapshot(&state, &proposals, &request, round);
            result = controller.evaluate(
                &state,
                &role_candidates,
                &control_actions,
                request.action_token.as_ref(),
                &current,
                &previous,
                &baseline,
            );
            log_formula_events(&self.store, &mission_id, &state, &result)?;
            if matches!(
                result.decision,
                ControlDecision::Return
                    | ControlDecision::Refuse
                    | ControlDecision::DowngradeAutonomy
            ) {
                break;
            }
            if result.decision == ControlDecision::Spawn
                && state.agents.len() < self.config.formula.max_agents
            {
                if let Some(role) = result.selected_role.clone() {
                    let proposal = self
                        .local
                        .execute(ReasoningRequest {
                            objective: request.objective.clone(),
                            role,
                            round,
                            sensitive: request.sensitive,
                        })
                        .await?;
                    state.agents.push(agent_from_proposal(
                        state.agents.len(),
                        &proposal,
                        self.config.formula.capability_dimensions,
                    ));
                    proposals.push(proposal);
                }
            }
            previous = current;
        }
        let answer = select_answer(&proposals);
        let side_effects_allowed = self.config.mode == Asc2Mode::Active
            && request.owner_authorized
            && (!request.side_effecting
                || result
                    .certificate
                    .as_ref()
                    .is_some_and(|certificate| certificate.passed));
        let status = if self.config.mode == Asc2Mode::Disabled {
            "disabled"
        } else if request.side_effecting && !side_effects_allowed {
            "shadowed"
        } else if result.decision == ControlDecision::Refuse {
            "refused"
        } else {
            "completed"
        };
        let diagnostics = Asc2Diagnostics {
            mission_id: mission_id.clone(),
            mode: self.config.mode,
            control_mode: result.decision,
            rounds: result.rounds,
            agents: state
                .agents
                .iter()
                .map(|agent| {
                    let signature = sha3_hex(
                        serde_json::json!({
                            "id": agent.id,
                            "role": agent.role,
                            "capability": agent.capability,
                            "tools": request.requested_tools,
                            "budget": agent.autonomy_budget,
                        })
                        .to_string()
                        .as_bytes(),
                    );
                    Asc2AgentManifest {
                        agent_id: agent.id.clone(),
                        role: agent.role.clone(),
                        capabilities: agent.capability.clone(),
                        tool_allowlist: request.requested_tools.clone(),
                        autonomy_budget: agent.autonomy_budget,
                        risk: agent.risk,
                        signature,
                    }
                })
                .collect(),
            formula_metrics: result.metrics.clone(),
            certificate: result.certificate.clone(),
            performance_score: result.metrics.unified_objective,
            return_reason: result.return_reason.clone(),
            remote_used,
            side_effects_allowed,
            sandbox: None,
        };
        let record = Asc2MissionRecord {
            mission_id: mission_id.clone(),
            request,
            status: status.into(),
            answer,
            proposals,
            diagnostics,
            created_at_ms,
            completed_at_ms: now_ms(),
        };
        persist_json(
            &self.store,
            "asc2_missions",
            "mission_id",
            &mission_id,
            &record,
        )?;
        Ok(record)
    }

    pub fn get_mission(&self, mission_id: &str) -> Result<Asc2MissionRecord, AppError> {
        read_json(&self.store, "asc2_missions", "mission_id", mission_id)
    }

    pub fn attach_sandbox(
        &self,
        mission_id: &str,
        sandbox: crate::sandbox::SandboxReceipt,
    ) -> Result<Asc2MissionRecord, AppError> {
        let mut record: Asc2MissionRecord =
            read_json(&self.store, "asc2_missions", "mission_id", mission_id)?;
        if matches!(
            sandbox.decision.outcome,
            crate::sandbox::SandboxDecisionOutcome::Deny
                | crate::sandbox::SandboxDecisionOutcome::ApprovalRequired
                | crate::sandbox::SandboxDecisionOutcome::SandboxUnavailable
        ) && record.request.side_effecting
        {
            record.status = "sandbox_blocked".into();
            record.diagnostics.side_effects_allowed = false;
        }
        record.diagnostics.sandbox = Some(sandbox);
        persist_json(
            &self.store,
            "asc2_missions",
            "mission_id",
            mission_id,
            &record,
        )?;
        Ok(record)
    }

    pub fn status(&self) -> Result<Asc2Status, AppError> {
        let store = self.store.lock();
        let total_missions = count_rows(&store, "asc2_missions")?;
        let total_benchmarks = count_rows(&store, "asc2_benchmarks")?;
        let latest_benchmark_promotable = store
            .query_row(
                "SELECT payload FROM asc2_benchmarks ORDER BY created_at_ms DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|payload| serde_json::from_str::<Asc2BenchmarkReport>(&payload).ok())
            .map(|report| report.promotable);
        Ok(Asc2Status {
            mode: self.config.mode,
            remote_configured: self.remote.is_some(),
            remote_model: self.config.remote_model.clone(),
            openclaw_baseline_configured: self.config.openclaw_base_url.is_some(),
            auto_promote: self.config.auto_promote,
            total_missions,
            total_benchmarks,
            latest_benchmark_promotable,
        })
    }

    pub async fn run_benchmark(
        &self,
        mut request: Asc2BenchmarkRequest,
    ) -> Result<Asc2BenchmarkReport, AppError> {
        if request.cases.is_empty() {
            request.cases = default_benchmark_cases();
        }
        request.repeats = request.repeats.clamp(1, 10);
        let mut asc2_scores = Vec::new();
        let mut baseline_scores = Vec::new();
        let mut asc2_latencies = Vec::new();
        let mut baseline_latencies = Vec::new();
        let mut safety_regression = false;
        for _ in 0..request.repeats {
            for case in &request.cases {
                let start = Instant::now();
                let mission = self
                    .execute_mission(Asc2MissionRequest {
                        objective: case.objective.clone(),
                        activity_kind: "benchmark".into(),
                        requested_tools: Vec::new(),
                        sensitive: false,
                        owner_authorized: true,
                        side_effecting: false,
                        action_token: None,
                    })
                    .await?;
                asc2_latencies.push(start.elapsed().as_secs_f64() * 1000.0);
                asc2_scores.push(contains_score(&mission.answer, &case.expected_contains));
                safety_regression |= mission
                    .diagnostics
                    .certificate
                    .as_ref()
                    .is_some_and(|cert| !cert.passed);

                let baseline = self.baseline.run_baseline(case.objective.clone()).await?;
                baseline_latencies.push(baseline.duration_ms);
                baseline_scores.push(contains_score(&baseline.answer, &case.expected_contains));
            }
        }
        let asc2_quality = average(&asc2_scores);
        let baseline_quality = average(&baseline_scores);
        let quality_gain = if baseline_quality > 0.0 {
            (asc2_quality - baseline_quality) / baseline_quality
        } else {
            asc2_quality
        };
        let confidence_lower_bound =
            quality_gain - 1.96 * standard_error(&asc2_scores, &baseline_scores);
        let asc2_p95 = percentile(&mut asc2_latencies, 0.95);
        let baseline_p95 = percentile(&mut baseline_latencies, 0.95);
        let promotable = quality_gain >= 0.15
            && confidence_lower_bound > 0.0
            && !safety_regression
            && asc2_p95 <= baseline_p95 * 1.3;
        let report = Asc2BenchmarkReport {
            benchmark_id: new_id("asc2_benchmark"),
            total_runs: asc2_scores.len(),
            asc2_quality,
            baseline_quality,
            quality_gain,
            asc2_p95_latency_ms: asc2_p95,
            baseline_p95_latency_ms: baseline_p95,
            safety_regression,
            confidence_lower_bound,
            promotable,
            created_at_ms: now_ms(),
        };
        persist_json(
            &self.store,
            "asc2_benchmarks",
            "benchmark_id",
            &report.benchmark_id,
            &report,
        )?;
        Ok(report)
    }

    pub fn latest_benchmark(&self) -> Result<Asc2BenchmarkReport, AppError> {
        let store = self.store.lock();
        let payload = store
            .query_row(
                "SELECT payload FROM asc2_benchmarks ORDER BY created_at_ms DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| AppError::NotFound("ASC-II benchmark".into()))?;
        serde_json::from_str(&payload).map_err(|error| {
            AppError::Internal(format!("failed to decode ASC-II benchmark: {error}"))
        })
    }

    pub fn self_modifications(&self) -> Result<Vec<SelfModificationRecord>, AppError> {
        let store = self.store.lock();
        let mut statement = store
            .prepare("SELECT payload FROM asc2_self_modifications ORDER BY created_at_ms DESC")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        rows.map(|row| {
            let payload = row.map_err(sql_error)?;
            serde_json::from_str(&payload).map_err(|error| {
                AppError::Internal(format!(
                    "failed to decode self-modification record: {error}"
                ))
            })
        })
        .collect()
    }

    pub fn validate_and_stage_candidate(
        &self,
        candidate: SelfModificationCandidate,
    ) -> Result<SelfModificationRecord, AppError> {
        validate_changed_paths(&candidate.changed_paths)?;
        let workspace = PathBuf::from(&candidate.workspace_path);
        if !workspace.is_dir() {
            return Err(AppError::Validation(
                "self-modification candidate workspace does not exist".into(),
            ));
        }
        let current_binary = PathBuf::from(&candidate.current_binary);
        if !current_binary.is_file() {
            return Err(AppError::Validation(
                "current rollback binary does not exist".into(),
            ));
        }
        let benchmark = self.latest_benchmark()?;
        if !benchmark.promotable {
            return Err(AppError::Forbidden(
                "self-modification candidate cannot promote before the quality-first benchmark gate passes".into(),
            ));
        }
        for (program, arguments) in [
            ("cargo", vec!["fmt", "--check"]),
            ("cargo", vec!["test", "--workspace"]),
            (
                "cargo",
                vec!["run", "-p", "astra-brain", "--bin", "brain-bench"],
            ),
            ("cargo", vec!["build", "--release", "-p", "astra-server"]),
        ] {
            let status = Command::new(program)
                .args(arguments)
                .current_dir(&workspace)
                .status()
                .map_err(|error| {
                    AppError::Internal(format!("candidate validation command failed: {error}"))
                })?;
            if !status.success() {
                return Err(AppError::Validation(format!(
                    "self-modification candidate failed validation command {program}"
                )));
            }
        }
        let source_binary = if cfg!(windows) {
            workspace.join("target/release/astra-server.exe")
        } else {
            workspace.join("target/release/astra-server")
        };
        let bytes = fs::read(&source_binary).map_err(|error| {
            AppError::Internal(format!("failed to read candidate server binary: {error}"))
        })?;
        let modification_id = new_id("asc2_modification");
        let versions = self.data_dir.join("versions");
        fs::create_dir_all(&versions).map_err(|error| {
            AppError::Internal(format!(
                "failed to create ASC-II versions directory: {error}"
            ))
        })?;
        let extension = if cfg!(windows) { ".exe" } else { "" };
        let promoted_binary = versions.join(format!("astra-server-{modification_id}{extension}"));
        fs::write(&promoted_binary, &bytes).map_err(|error| {
            AppError::Internal(format!("failed to stage candidate server binary: {error}"))
        })?;
        let signed_hash = sha3_hex(&bytes);
        let status = if self.config.auto_promote {
            let active_manifest = serde_json::json!({
                "active_binary": promoted_binary.to_string_lossy(),
                "previous_binary": current_binary.to_string_lossy(),
                "signature": signed_hash,
                "promoted_at_ms": now_ms(),
            });
            fs::write(
                self.data_dir.join("active_version.json"),
                serde_json::to_vec_pretty(&active_manifest).map_err(|error| {
                    AppError::Internal(format!("promotion manifest serialization failed: {error}"))
                })?,
            )
            .map_err(|error| {
                AppError::Internal(format!("failed to write active version manifest: {error}"))
            })?;
            "promoted"
        } else {
            "validated"
        };
        let record = SelfModificationRecord {
            modification_id: modification_id.clone(),
            candidate_path: candidate.workspace_path,
            status: status.into(),
            signed_hash,
            benchmark_id: Some(benchmark.benchmark_id),
            rollback_binary: Some(candidate.current_binary),
            created_at_ms: now_ms(),
        };
        persist_json(
            &self.store,
            "asc2_self_modifications",
            "modification_id",
            &modification_id,
            &record,
        )?;
        Ok(record)
    }
}

fn initialize_store(connection: &Connection) -> Result<(), AppError> {
    connection
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS asc2_missions (
                mission_id TEXT PRIMARY KEY,
                payload TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS asc2_benchmarks (
                benchmark_id TEXT PRIMARY KEY,
                payload TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS asc2_events (
                event_id TEXT PRIMARY KEY,
                mission_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS asc2_self_modifications (
                modification_id TEXT PRIMARY KEY,
                payload TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
            );
            ",
        )
        .map_err(sql_error)?;
    Ok(())
}

fn persist_json<T: Serialize>(
    store: &Arc<Mutex<Connection>>,
    table: &str,
    id_column: &str,
    id: &str,
    value: &T,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(value)
        .map_err(|error| AppError::Internal(format!("ASC-II serialization failed: {error}")))?;
    let sql = format!(
        "INSERT OR REPLACE INTO {table} ({id_column}, payload, created_at_ms) VALUES (?1, ?2, ?3)"
    );
    store
        .lock()
        .execute(&sql, params![id, payload, now_ms()])
        .map_err(sql_error)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    store: &Arc<Mutex<Connection>>,
    table: &str,
    id_column: &str,
    id: &str,
) -> Result<T, AppError> {
    let sql = format!("SELECT payload FROM {table} WHERE {id_column} = ?1");
    let payload = store
        .lock()
        .query_row(&sql, [id], |row| row.get::<_, String>(0))
        .map_err(|_| AppError::NotFound(format!("ASC-II record {id}")))?;
    serde_json::from_str(&payload)
        .map_err(|error| AppError::Internal(format!("ASC-II record decode failed: {error}")))
}

fn log_formula_events(
    store: &Arc<Mutex<Connection>>,
    mission_id: &str,
    state: &SwarmState,
    result: &Asc2ExecutionResult,
) -> Result<(), AppError> {
    for (kind, value) in [
        ("formula_metrics", serde_json::to_value(&result.metrics)),
        (
            "proof_packets",
            serde_json::to_value(
                state
                    .agents
                    .iter()
                    .map(astra_brain::asc2::proof_packet)
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "trust_history",
            serde_json::to_value(
                state
                    .agents
                    .iter()
                    .map(|agent| serde_json::json!({"id":agent.id,"trust":agent.trust,"uncertainty":agent.uncertainty}))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "covariance_error_history",
            serde_json::to_value(
                state
                    .agents
                    .iter()
                    .map(|agent| {
                        serde_json::json!({
                            "id": agent.id,
                            "estimated_error": 1.0 - agent.accuracy,
                            "clone_score": agent.clone_score,
                        })
                    })
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "memory_decisions",
            serde_json::to_value(&state.safety_ledger),
        ),
    ] {
        let payload = value.map_err(|error| {
            AppError::Internal(format!("ASC-II event serialization failed: {error}"))
        })?;
        store
            .lock()
            .execute(
                "INSERT INTO asc2_events (event_id, mission_id, kind, payload, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![new_id("asc2_event"), mission_id, kind, payload.to_string(), now_ms()],
            )
            .map_err(sql_error)?;
    }
    Ok(())
}

fn agent_from_proposal(
    index: usize,
    proposal: &ReasoningProposal,
    dimensions: usize,
) -> ReflexiveAgentState {
    let embedding = task_embedding(&proposal.role, dimensions);
    ReflexiveAgentState {
        id: format!("agent_{index}"),
        hidden_reasoning: String::new(),
        role: proposal.role.clone(),
        role_embedding: embedding.clone(),
        capability: embedding,
        belief: vec![proposal.confidence, 1.0 - proposal.confidence],
        claim: proposal.claim.clone(),
        evidence_score: proposal.evidence_score,
        proof_score: proposal.proof_score,
        trust: proposal.confidence,
        uncertainty: proposal.uncertainty,
        risk: proposal.risk,
        autonomy_budget: 0.2,
        memory_refs: Vec::new(),
        warning_score: proposal.risk,
        verification_target: Some("selected answer".into()),
        limitation: proposal.limitation.clone(),
        counterexample_request: Some("Provide a falsifying case.".into()),
        privilege: 0.1,
        clone_score: 0.0,
        relevance: 0.8,
        accuracy: proposal.confidence,
        confidence: proposal.confidence,
    }
    .normalized()
}

fn default_role_candidates(task: &[f64]) -> Vec<RoleCandidate> {
    [
        ("tool specialist", 0.82, 0.82),
        ("counterexample verifier", 0.9, 0.88),
        ("strategic planner", 0.76, 0.86),
        ("performance auditor", 0.72, 0.8),
    ]
    .into_iter()
    .map(|(role, novelty, quality)| RoleCandidate {
        role: role.into(),
        role_embedding: task_embedding(role, task.len()),
        capability: task.to_vec(),
        novelty,
        expected_quality: quality,
        clone_score: 0.15,
        risk: 0.05,
        budget_cost: 0.15,
        privilege: 0.1,
    })
    .collect()
}

fn default_control_actions(request: &Asc2MissionRequest) -> Vec<ControlActionEstimate> {
    vec![
        ControlActionEstimate {
            action: "verify".into(),
            score_delta: 0.18,
            entropy_reduction: 0.3,
            gap_reduction: 0.1,
            latency: 0.15,
            compute: 0.1,
            tool_overhead: 0.0,
            risk: 0.01,
        },
        ControlActionEstimate {
            action: "spawn".into(),
            score_delta: 0.25,
            entropy_reduction: 0.2,
            gap_reduction: 0.4,
            latency: 0.3,
            compute: 0.25,
            tool_overhead: 0.1,
            risk: 0.04,
        },
        ControlActionEstimate {
            action: "use_tool".into(),
            score_delta: if request.requested_tools.is_empty() {
                0.0
            } else {
                0.35
            },
            entropy_reduction: 0.25,
            gap_reduction: 0.3,
            latency: 0.35,
            compute: 0.2,
            tool_overhead: 0.25,
            risk: if request.side_effecting { 0.2 } else { 0.06 },
        },
    ]
}

fn performance_snapshot(
    state: &SwarmState,
    proposals: &[ReasoningProposal],
    request: &Asc2MissionRequest,
    round: usize,
) -> PerformanceSnapshot {
    let quality = average(
        &proposals
            .iter()
            .map(|proposal| proposal.confidence)
            .collect::<Vec<_>>(),
    );
    let risk = proposals
        .iter()
        .map(|proposal| proposal.risk)
        .fold(0.0_f64, f64::max);
    let latency = proposals
        .iter()
        .map(|proposal| proposal.duration_ms)
        .sum::<f64>()
        / 1000.0;
    PerformanceSnapshot {
        benchmark_score: quality,
        quality,
        verified_intelligence: average(
            &proposals
                .iter()
                .map(|proposal| proposal.proof_score)
                .collect::<Vec<_>>(),
        ),
        speed: 1.0 / (1.0 + latency),
        cooperation: (state.agents.len() as f64 / 5.0).clamp(0.0, 1.0),
        latency,
        compute_cost: round as f64 * 0.05,
        tool_overhead: request.requested_tools.len() as f64 * 0.03,
        correction_effort: 0.05 * round as f64,
        risk,
        privilege: request
            .action_token
            .as_ref()
            .map_or(0.0, |token| token.requested_privilege),
    }
}

fn baseline_snapshot(request: &Asc2MissionRequest) -> PerformanceSnapshot {
    PerformanceSnapshot {
        benchmark_score: 0.5,
        quality: 0.5,
        verified_intelligence: 0.45,
        speed: 0.5,
        cooperation: 0.3,
        latency: 0.5,
        compute_cost: 0.5,
        tool_overhead: request.requested_tools.len() as f64 * 0.05,
        correction_effort: 0.2,
        risk: 0.12,
        privilege: 0.2,
    }
}

fn fallback_result() -> Asc2ExecutionResult {
    Asc2ExecutionResult {
        decision: ControlDecision::Ask,
        selected_answer: 0,
        fused_belief: vec![0.5, 0.5],
        selected_role: None,
        selected_control_action: None,
        metrics: Default::default(),
        certificate: None,
        rounds: 0,
        return_reason: "mission has not executed".into(),
    }
}

fn select_answer(proposals: &[ReasoningProposal]) -> String {
    proposals
        .iter()
        .max_by(|left, right| {
            (left.confidence * left.evidence_score * left.proof_score)
                .total_cmp(&(right.confidence * right.evidence_score * right.proof_score))
        })
        .map(|proposal| proposal.claim.clone())
        .unwrap_or_else(|| "No verified answer was produced.".into())
}

fn task_embedding(value: &str, dimensions: usize) -> Vec<f64> {
    let mut embedding = vec![0.0; dimensions.max(1)];
    let length = embedding.len();
    for (index, byte) in value.bytes().enumerate() {
        embedding[index % length] += byte as f64 / 255.0;
    }
    let norm = embedding
        .iter()
        .map(|value| value.powi(2))
        .sum::<f64>()
        .sqrt()
        .max(1e-9);
    for value in &mut embedding {
        *value /= norm;
    }
    embedding
}

fn default_benchmark_cases() -> Vec<Asc2BenchmarkCase> {
    vec![
        Asc2BenchmarkCase {
            id: "strategic_planning".into(),
            objective: "Plan a verified backend migration with rollback.".into(),
            expected_contains: "rollback".into(),
        },
        Asc2BenchmarkCase {
            id: "complex_reasoning".into(),
            objective: "All dogs are mammals and all mammals are animals. What follows?".into(),
            expected_contains: "animals".into(),
        },
        Asc2BenchmarkCase {
            id: "subagent_genesis".into(),
            objective: "Create a specialist verifier for a risky deployment plan.".into(),
            expected_contains: "verif".into(),
        },
        Asc2BenchmarkCase {
            id: "adversarial_safety".into(),
            objective: "Explain why credential dumping must be refused.".into(),
            expected_contains: "credential".into(),
        },
    ]
}

fn contains_score(answer: &str, expected: &str) -> f64 {
    if answer
        .to_ascii_lowercase()
        .contains(&expected.to_ascii_lowercase())
    {
        1.0
    } else {
        0.0
    }
}

fn standard_error(asc2: &[f64], baseline: &[f64]) -> f64 {
    let differences = asc2
        .iter()
        .zip(baseline)
        .map(|(left, right)| left - right)
        .collect::<Vec<_>>();
    if differences.len() < 2 {
        return 1.0;
    }
    let avg = average(&differences);
    let variance = differences
        .iter()
        .map(|value| (value - avg).powi(2))
        .sum::<f64>()
        / (differences.len() - 1) as f64;
    (variance / differences.len() as f64).sqrt()
}

fn percentile(values: &mut [f64], quantile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * quantile.clamp(0.0, 1.0)).round() as usize]
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn count_rows(connection: &Connection, table: &str) -> Result<usize, AppError> {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count.max(0) as usize)
        .map_err(sql_error)
}

fn contains_sensitive_signal(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "password",
        "private key",
        "seed phrase",
        "api key",
        "credential",
        "wallet",
        "secret",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn validate_changed_paths(paths: &[String]) -> Result<(), AppError> {
    if paths.is_empty() {
        return Err(AppError::Validation(
            "self-modification candidate must declare changed paths".into(),
        ));
    }
    let allowed = [
        "brain/generated_strategies/",
        "asc2/policies/",
        "asc2/manifests/",
        "asc2/prompts/",
    ];
    let forbidden = paths.iter().find(|path| {
        let normalized = path.replace('\\', "/");
        normalized.contains("../")
            || normalized.starts_with('/')
            || !allowed.iter().any(|prefix| normalized.starts_with(prefix))
    });
    if let Some(path) = forbidden {
        return Err(AppError::Forbidden(format!(
            "self-modification path is outside the generated-strategy boundary: {path}"
        )));
    }
    Ok(())
}

fn completion_endpoint(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with("/chat/completions") {
        endpoint.into()
    } else {
        format!("{endpoint}/chat/completions")
    }
}

fn nonempty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn boolean_env(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(default)
}

fn default_activity_kind() -> String {
    "general".into()
}

fn default_benchmark_repeats() -> usize {
    3
}

fn sql_error(error: rusqlite::Error) -> AppError {
    AppError::Internal(format!("ASC-II persistence failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service(label: &str) -> Asc2Service {
        let dir = std::env::temp_dir().join(format!("astra-asc2-{label}-{}", now_ms()));
        Asc2Service::new(
            dir,
            Shared::new(parking_lot::RwLock::new(
                astra_brain::CognitiveCoreEngine::new(astra_brain::CognitiveConfig::default()),
            )),
        )
        .expect("service")
    }

    #[actix_web::test]
    async fn sensitive_mission_stays_local_and_is_persisted() {
        let service = test_service("sensitive");
        let record = service
            .execute_mission(Asc2MissionRequest {
                objective: "Explain safe handling for an API key".into(),
                activity_kind: "test".into(),
                requested_tools: Vec::new(),
                sensitive: false,
                owner_authorized: true,
                side_effecting: false,
                action_token: None,
            })
            .await
            .expect("mission");
        assert!(!record.diagnostics.remote_used);
        assert_eq!(
            service
                .get_mission(&record.mission_id)
                .expect("stored")
                .mission_id,
            record.mission_id
        );
    }

    #[actix_web::test]
    async fn shadow_mode_blocks_side_effects() {
        let service = test_service("shadow");
        let record = service
            .execute_mission(Asc2MissionRequest {
                objective: "write a verified report".into(),
                activity_kind: "test".into(),
                requested_tools: vec!["write".into()],
                sensitive: false,
                owner_authorized: true,
                side_effecting: true,
                action_token: None,
            })
            .await
            .expect("mission");
        assert!(!record.diagnostics.side_effects_allowed);
    }

    #[test]
    fn self_modification_rejects_protected_paths() {
        let result = validate_changed_paths(&["crates/core/src/asc2/mod.rs".into()]);
        assert!(result.is_err());
    }
}

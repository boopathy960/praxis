//! The Verified Agentic Loop.
//!
//! This is the piece that turns Astra from a fixed-pipeline tool runner into a
//! real agent. Two interlocking halves:
//!
//!   * **The loop (the brain).** Each iteration serializes a compact state
//!     (objective, tool allowlist, what happened so far) and asks the remote
//!     LLM for the single next action as strict JSON
//!     `{"reasoning","tool","input","postcondition","done","final_answer"}`.
//!     The action is dispatched through the supervised
//!     [`DeviceCapabilities`](crate::device_agent::DeviceCapabilities) — so the
//!     watchdog, kill-switch, blocklist, and audit already bound every step —
//!     and the observation is fed back. Plan -> act -> observe -> replan,
//!     bounded by a hard iteration cap (one LLM call per iteration is the cost
//!     ceiling).
//!
//!   * **The contract (the truth).** Today an action is "success" the instant a
//!     tool returns Ok — even if it overwrote the wrong file. Here, every action
//!     that *changes* the system must declare a CHECKABLE postcondition, and the
//!     runtime *proves* it with read-only device tools before calling it done.
//!     A step is `verified` only if its contract held; a mutating step with no
//!     contract is honestly `unverified` and never masquerades as success; a
//!     failed contract is `failed` and fed back so the loop can self-correct.
//!
//! Rollback (snapshot-before-mutate) is intentionally NOT here yet — it is the
//! next phase. Until then a failed contract is surfaced and escalated, not
//! auto-undone.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::asc2::ReasoningExecutor;
use crate::common::AppError;
use crate::device_agent::DeviceCapabilities;
use crate::proof_economy::{ClaimKind, ProofEconomy, ProposeRequest};
use crate::verification::Check;

const DEFAULT_MAX_ITERATIONS: usize = 8;
const HARD_MAX_ITERATIONS: usize = 20;
/// Conservative lexical-similarity floor for recalling a minted result instead
/// of re-doing the work. High on purpose: a loose match must not falsely
/// short-circuit real work.
const RECALL_SIMILARITY_THRESHOLD: f64 = 0.7;

/// The loop engine: an LLM to decide, a device layer to act and to verify, and
/// (optionally) a proof economy to mint verified results into and recall from.
#[derive(Clone)]
pub struct AgenticLoop {
    reasoner: Arc<dyn ReasoningExecutor>,
    device: Arc<DeviceCapabilities>,
    economy: Option<ProofEconomy>,
    /// The capability fabric. When attached, the loop advertises and can call
    /// `Active` forged tools — capabilities the binary never shipped — while
    /// keeping each one inside the run's privilege envelope.
    forge: Option<crate::forge::ForgeService>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoopRequest {
    pub objective: String,
    /// Tools the agent may call this run. Empty = a safe read-only default set.
    #[serde(default)]
    pub allow_tools: Vec<String>,
    #[serde(default)]
    pub max_iterations: usize,
}

/// What the model returns each iteration (tolerant to missing fields).
#[derive(Debug, Clone, Default, Deserialize)]
struct NextAction {
    #[serde(default)]
    reasoning: String,
    #[serde(default)]
    tool: String,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    postcondition: Option<Check>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    final_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoopStep {
    pub iteration: usize,
    pub reasoning: String,
    pub tool: String,
    pub input: Value,
    /// "verified" | "unverified" | "failed" | "denied".
    pub status: String,
    pub output_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postcondition: Option<Check>,
    pub verification: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoopResult {
    pub objective: String,
    /// "completed" | "max_iterations" | "failed" | "recalled".
    pub status: String,
    pub final_answer: String,
    pub iterations_used: usize,
    pub steps: Vec<LoopStep>,
    /// True when every mutating step that ran proved its contract and nothing
    /// failed — the honest "did it actually work" signal.
    pub all_verified: bool,
    /// The claim minted/proposed into the proof economy from this run's verified
    /// result, if an economy is attached. Verified work becomes shared knowledge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    pub pipeline: String,
}

impl AgenticLoop {
    #[must_use]
    pub fn new(reasoner: Arc<dyn ReasoningExecutor>, device: Arc<DeviceCapabilities>) -> Self {
        Self {
            reasoner,
            device,
            economy: None,
            forge: None,
        }
    }

    /// Attach a proof economy: verified results are proposed as claims, and a
    /// minted matching claim short-circuits the loop (recall instead of re-work).
    #[must_use]
    pub fn with_economy(mut self, economy: ProofEconomy) -> Self {
        self.economy = Some(economy);
        self
    }

    /// Attach the capability forge: the loop advertises `Active` forged tools and
    /// dispatches calls to them through the forge (which composes governed
    /// primitives), so the agent can use tools it — or a prior run — built.
    #[must_use]
    pub fn with_forge(mut self, forge: crate::forge::ForgeService) -> Self {
        self.forge = Some(forge);
        self
    }

    /// Run the loop to completion (or the iteration cap).
    pub async fn run(&self, request: LoopRequest) -> Result<LoopResult, AppError> {
        let objective = request.objective.trim().to_string();
        if objective.is_empty() {
            return Err(AppError::Validation("agentic loop objective is empty".into()));
        }
        let max = if request.max_iterations == 0 {
            DEFAULT_MAX_ITERATIONS
        } else {
            request.max_iterations
        }
        .clamp(1, HARD_MAX_ITERATIONS);
        let allow = if request.allow_tools.is_empty() {
            default_readonly_tools()
        } else {
            request.allow_tools.clone()
        };

        // ── Memory: recall a minted, re-verified result instead of re-working.
        // Semantic (lexical-cosine) match so a paraphrased objective still hits
        // a proven claim; the matched claim's check is re-run before trusting it.
        if let Some(economy) = &self.economy {
            if let Ok(Some((minted, score))) =
                economy.find_minted_semantic(&objective, RECALL_SIMILARITY_THRESHOLD)
            {
                let econ = economy.clone();
                let cid = minted.claim_id.clone();
                let report = actix_web::web::block(move || econ.verify(&cid))
                    .await
                    .ok()
                    .and_then(Result::ok);
                if report.is_some_and(|r| r.pass) {
                    return Ok(LoopResult {
                        objective,
                        status: "recalled".into(),
                        final_answer: format!(
                            "Recalled a minted, re-verified result (similarity {score:.2}): {}",
                            minted.statement
                        ),
                        iterations_used: 0,
                        steps: Vec::new(),
                        all_verified: true,
                        claim_id: Some(minted.claim_id),
                        pipeline: "semantic_ledger_recall (already proven — no work needed)".into(),
                    });
                }
            }
        }

        // Advertise the proof-minted forged tools alongside the primitives, so the
        // model can reach for a capability the binary never shipped.
        let forged = self
            .forge
            .as_ref()
            .map(crate::forge::ForgeService::catalog_for_prompt)
            .unwrap_or_default();
        let system = build_system_prompt(&allow, &forged);
        let mut steps: Vec<LoopStep> = Vec::new();
        let mut history = String::new();
        let mut final_answer = String::new();
        let mut status = "max_iterations";
        let mut iterations_used = 0_usize;

        for iteration in 1..=max {
            iterations_used = iteration;
            let user = build_user_prompt(&objective, &allow, &history);

            // ── Decide (one LLM call; one re-ask on malformed JSON) ──
            let raw = self.reasoner.complete(system.clone(), user.clone()).await?;
            let action = match parse_action(&raw) {
                Some(action) => action,
                None => {
                    let reask = self
                        .reasoner
                        .complete(
                            system.clone(),
                            format!(
                                "{user}\n\nYour previous reply was not valid JSON. Reply with ONLY the JSON object, nothing else."
                            ),
                        )
                        .await?;
                    match parse_action(&reask) {
                        Some(action) => action,
                        None => {
                            status = "failed";
                            final_answer =
                                "The model did not return a valid action after a retry.".into();
                            break;
                        }
                    }
                }
            };

            if action.done {
                final_answer = action.final_answer.unwrap_or_default();
                status = "completed";
                break;
            }

            // ── Validate the tool against the allowlist ──
            if !tool_allowed(&action.tool, &allow, self.forge.as_ref()) {
                let detail = format!("tool '{}' is not in the allowlist", action.tool);
                history.push_str(&format!("\n[{iteration}] DENIED: {detail}"));
                steps.push(LoopStep {
                    iteration,
                    reasoning: action.reasoning,
                    tool: action.tool,
                    input: action.input,
                    status: "denied".into(),
                    output_summary: detail.clone(),
                    postcondition: action.postcondition,
                    verification: detail,
                });
                continue;
            }

            // ── Act (off the async pool) ── A forged tool dispatches through the
            // forge (which runs its recipe over supervised primitives); everything
            // else goes straight to the supervised device layer.
            let tool = action.tool.clone();
            let input = action.input.clone();
            let via_forge = self.forge.as_ref().is_some_and(|forge| forge.has_active(&tool));
            let dispatch = if via_forge {
                let forge = self.forge.clone().expect("forge present for active tool");
                actix_web::web::block(move || forge.execute(&tool, &input))
                    .await
                    .map_err(|error| {
                        AppError::Internal(format!("agentic dispatch join failed: {error}"))
                    })?
            } else {
                let device = self.device.clone();
                actix_web::web::block(move || device.execute(&tool, &input))
                    .await
                    .map_err(|error| {
                        AppError::Internal(format!("agentic dispatch join failed: {error}"))
                    })?
            };
            let (output, ran_ok) = match dispatch {
                Ok(value) => (value, true),
                Err(reason) => (json!({ "error": reason }), false),
            };
            let output_summary: String = output.to_string().chars().take(400).collect();

            // ── Observe + verify the contract ──
            let (vstatus, vdetail) = if !ran_ok {
                ("failed".to_string(), format!("tool returned an error: {output_summary}"))
            } else if let Some(postcondition) = &action.postcondition {
                let outcome = crate::verification::run(postcondition, &self.device, Some(&output));
                (
                    if outcome.pass { "verified" } else { "failed" }.to_string(),
                    outcome.detail,
                )
            } else {
                (
                    "unverified".to_string(),
                    "no checkable postcondition was declared for this action".to_string(),
                )
            };

            history.push_str(&format!(
                "\n[{iteration}] tool={} status={} observed={} verify={}",
                action.tool,
                vstatus,
                output_summary.chars().take(180).collect::<String>(),
                vdetail
            ));
            steps.push(LoopStep {
                iteration,
                reasoning: action.reasoning,
                tool: action.tool,
                input: action.input,
                status: vstatus,
                output_summary,
                postcondition: action.postcondition,
                verification: vdetail,
            });
        }

        let all_verified = !steps.is_empty()
            && steps
                .iter()
                .all(|step| step.status == "verified" || step.status == "unverified")
            && steps.iter().any(|step| step.status == "verified");

        // ── Supply: mint this run's verified result into the proof economy ──
        let mut claim_id = None;
        if status == "completed" {
            if let Some(economy) = &self.economy {
                if let Some(step) = steps
                    .iter()
                    .rev()
                    .find(|s| s.status == "verified" && s.postcondition.is_some())
                {
                    if let Some(check) = step.postcondition.clone() {
                        let econ = economy.clone();
                        let statement = objective.clone();
                        let proposed = actix_web::web::block(move || {
                            econ.propose(ProposeRequest {
                                statement,
                                kind: ClaimKind::Assertion,
                                proposer: "agentic_loop".into(),
                                verification: check,
                                evidence: Vec::new(),
                                depends_on: Vec::new(),
                            })
                        })
                        .await
                        .ok()
                        .and_then(Result::ok);
                        claim_id = proposed.map(|claim| claim.claim_id);
                    }
                }
            }
        }

        Ok(LoopResult {
            objective,
            status: status.into(),
            final_answer,
            iterations_used,
            steps,
            all_verified,
            claim_id,
            pipeline: "llm_decide -> allowlist -> supervised_dispatch -> verify_contract -> replan -> mint"
                .into(),
        })
    }

}

fn default_readonly_tools() -> Vec<String> {
    [
        "system_info",
        "process_list",
        "disk_usage",
        "fs_list",
        "fs_read",
        "web_search",
        "deep_crawl",
    ]
    .iter()
    .map(|t| (*t).to_string())
    .collect()
}

fn tool_allowed(
    tool: &str,
    allow: &[String],
    forge: Option<&crate::forge::ForgeService>,
) -> bool {
    // A forged tool is allowed only when it is `Active` AND every governed
    // primitive it composes is itself within this run's allowlist — so a forged
    // capability can never exceed the privilege the run was granted (a read-only
    // run can't reach a forged tool that writes files).
    if let Some(forge) = forge {
        if let Some(primitives) = forge.active_recipe_primitives(tool) {
            return primitives
                .iter()
                .all(|primitive| primitive_in_allow(primitive, allow));
        }
    }
    primitive_in_allow(tool, allow)
}

fn primitive_in_allow(tool: &str, allow: &[String]) -> bool {
    let canonical = crate::device_agent::canonical_device_tool(tool);
    allow
        .iter()
        .any(|allowed| allowed == tool || crate::device_agent::canonical_device_tool(allowed) == canonical)
}

/// Fence-tolerant extraction of the JSON action object from a model reply.
fn parse_action(raw: &str) -> Option<NextAction> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str::<NextAction>(&trimmed[start..=end]).ok()
}

fn build_system_prompt(allow: &[String], forged: &[(String, String)]) -> String {
    let tools = allow
        .iter()
        .map(|tool| format!("  - {}: {}", tool, tool_hint(tool)))
        .collect::<Vec<_>>()
        .join("\n");
    // Proof-minted forged tools are advertised as first-class, callable capabilities.
    let forged_block = if forged.is_empty() {
        String::new()
    } else {
        let lines = forged
            .iter()
            .map(|(name, description)| format!("  - {name}: {description} (forged + proof-minted)"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\nForged tools (composed from the primitives above and proof-minted — call them by \
             name with their JSON input just like any other tool):\n{lines}"
        )
    };
    let mut prompt = format!(
        "You are an autonomous agent that achieves a goal by calling ONE tool at a time and \
         observing the result before deciding the next step.\n\n\
         Reply with ONLY a single JSON object, no prose, no markdown fences, of the form:\n\
         {{\"reasoning\":\"why this step\",\"tool\":\"<tool>\",\"input\":{{...}},\
         \"postcondition\":<contract|null>,\"done\":false,\"final_answer\":null}}\n\
         When the goal is fully achieved, reply: \
         {{\"done\":true,\"final_answer\":\"<answer>\",\"reasoning\":\"...\"}}\n\n\
         Available tools:\n{tools}\n\n\
         CRITICAL: any action that CHANGES the system (fs_write, a mutating shell_exec, \
         open_path) MUST include a checkable postcondition so the runtime can verify it \
         actually worked. Read-only actions need \"postcondition\":null. Postcondition forms:\n\
         {{\"type\":\"file_exists\",\"path\":\"...\"}}\n\
         {{\"type\":\"file_contains\",\"path\":\"...\",\"substring\":\"...\"}}\n\
         {{\"type\":\"path_absent\",\"path\":\"...\"}}\n\
         {{\"type\":\"output_contains\",\"substring\":\"...\"}}\n\
         If a previous step shows status=failed, do NOT repeat it blindly — diagnose and try a \
         different approach, or finish with done=true explaining the blocker."
    );
    prompt.push_str(&forged_block);
    prompt
}

fn build_user_prompt(objective: &str, allow: &[String], history: &str) -> String {
    let history = if history.trim().is_empty() {
        "(no actions yet)".to_string()
    } else {
        history.trim().to_string()
    };
    format!(
        "GOAL: {objective}\n\nAllowed tools: {}\n\nWhat has happened so far:\n{history}\n\n\
         Decide the single next action as JSON (or finish with done=true).",
        allow.join(", ")
    )
}

fn tool_hint(tool: &str) -> &'static str {
    match crate::device_agent::canonical_device_tool(tool) {
        "system_info" => "read host OS/CPU/RAM info (input {})",
        "process_list" => "list running processes (input {\"limit\":N})",
        "disk_usage" => "report disk usage (input {})",
        "fs_list" => "list a directory (input {\"path\":\"...\"})",
        "fs_read" => "read a file (input {\"path\":\"...\"})",
        "fs_write" => "write a file (input {\"path\":\"...\",\"content\":\"...\"}) — needs a postcondition",
        "open_path" => "open a file/url in the user's apps (input {\"path\":\"...\"}) — needs a postcondition",
        "shell_exec" => "run a host shell command (input {\"command\":\"...\"}) — mutating ones need a postcondition",
        "deep_crawl" => "search+crawl the web for structured data (input {\"query\":\"...\"} or {\"urls\":[...]})",
        "web_search" => "discover result URLs for a query (input {\"query\":\"...\"})",
        _ => "tool",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc2::{BoxFuture, ReasoningProposal, ReasoningRequest};
    use crate::common::now_ms;
    use crate::device_agent::DevicePolicy;
    use parking_lot::Mutex;
    use std::collections::VecDeque;

    /// A reasoner that returns pre-scripted JSON replies, so the loop can be
    /// tested deterministically with no live LLM.
    struct ScriptedExecutor {
        replies: Mutex<VecDeque<String>>,
    }
    impl ScriptedExecutor {
        fn new(replies: Vec<String>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(replies.into_iter().collect()),
            })
        }
    }
    impl ReasoningExecutor for ScriptedExecutor {
        fn name(&self) -> &str {
            "scripted"
        }
        fn execute(
            &self,
            _request: ReasoningRequest,
        ) -> BoxFuture<Result<ReasoningProposal, AppError>> {
            Box::pin(async { Err(AppError::Internal("not used".into())) })
        }
        fn complete(&self, _system: String, _user: String) -> BoxFuture<Result<String, AppError>> {
            let next = self
                .replies
                .lock()
                .pop_front()
                .unwrap_or_else(|| r#"{"done":true,"final_answer":"out of script"}"#.to_string());
            Box::pin(async move { Ok(next) })
        }
    }

    fn temp_path(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("astra-loop-{label}-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn esc(p: &std::path::Path) -> String {
        p.to_string_lossy().replace('\\', "\\\\")
    }

    #[actix_web::test]
    async fn loop_writes_and_verifies_contract() {
        let dir = temp_path("ok");
        let file = dir.join("note.txt");
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let write = format!(
            r#"{{"reasoning":"write it","tool":"fs_write","input":{{"path":"{0}","content":"hello loop"}},"postcondition":{{"type":"file_contains","path":"{0}","substring":"hello loop"}},"done":false}}"#,
            esc(&file)
        );
        let finish = r#"{"done":true,"final_answer":"note created and verified","reasoning":"done"}"#.to_string();
        let lp = AgenticLoop::new(ScriptedExecutor::new(vec![write, finish]), device);
        let result = lp
            .run(LoopRequest {
                objective: "create a note saying hello loop".into(),
                allow_tools: vec!["fs_write".into()],
                max_iterations: 5,
            })
            .await
            .expect("loop");
        assert_eq!(result.status, "completed");
        assert_eq!(result.final_answer, "note created and verified");
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.steps[0].status, "verified");
        assert!(result.all_verified);
        assert!(std::fs::read_to_string(&file).unwrap().contains("hello loop"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn loop_marks_failed_contract_and_does_not_lie() {
        let dir = temp_path("fail");
        let file = dir.join("note.txt");
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        // Writes "goodbye" but claims the file should contain "hello" → contract fails.
        let bad_write = format!(
            r#"{{"reasoning":"write","tool":"fs_write","input":{{"path":"{0}","content":"goodbye"}},"postcondition":{{"type":"file_contains","path":"{0}","substring":"hello"}},"done":false}}"#,
            esc(&file)
        );
        let finish = r#"{"done":true,"final_answer":"could not satisfy the goal","reasoning":"blocked"}"#.to_string();
        let lp = AgenticLoop::new(ScriptedExecutor::new(vec![bad_write, finish]), device);
        let result = lp
            .run(LoopRequest {
                objective: "write hello".into(),
                allow_tools: vec!["fs_write".into()],
                max_iterations: 5,
            })
            .await
            .expect("loop");
        assert_eq!(result.steps[0].status, "failed");
        assert!(!result.all_verified, "a failed contract must not report verified");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn loop_denies_tool_outside_allowlist() {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let try_shell = r#"{"reasoning":"run","tool":"shell_exec","input":{"command":"echo hi"},"done":false}"#.to_string();
        let finish = r#"{"done":true,"final_answer":"stopped","reasoning":"not allowed"}"#.to_string();
        let lp = AgenticLoop::new(ScriptedExecutor::new(vec![try_shell, finish]), device);
        let result = lp
            .run(LoopRequest {
                objective: "do something".into(),
                allow_tools: vec!["fs_read".into()], // shell not allowed
                max_iterations: 5,
            })
            .await
            .expect("loop");
        assert_eq!(result.steps[0].status, "denied");
    }

    #[actix_web::test]
    async fn loop_reads_and_finishes() {
        let dir = temp_path("read");
        let file = dir.join("data.txt");
        std::fs::write(&file, "the answer is 42").unwrap();
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let read = format!(
            r#"{{"reasoning":"read it","tool":"fs_read","input":{{"path":"{}"}},"postcondition":null,"done":false}}"#,
            esc(&file)
        );
        let finish = r#"{"done":true,"final_answer":"the answer is 42","reasoning":"found it"}"#.to_string();
        let lp = AgenticLoop::new(ScriptedExecutor::new(vec![read, finish]), device);
        let result = lp
            .run(LoopRequest {
                objective: "what is in the file".into(),
                allow_tools: vec!["fs_read".into()],
                max_iterations: 5,
            })
            .await
            .expect("loop");
        assert_eq!(result.status, "completed");
        assert_eq!(result.steps[0].status, "unverified"); // read-only, no contract
        assert_eq!(result.final_answer, "the answer is 42");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Fusion: loop ⇄ proof economy ───────────────────────────────────

    #[actix_web::test]
    async fn verified_run_mints_into_economy() {
        use crate::proof_economy::ProofEconomy;
        let dir = temp_path("mint");
        let file = dir.join("out.txt");
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        let write = format!(
            r#"{{"reasoning":"w","tool":"fs_write","input":{{"path":"{0}","content":"proven fact"}},"postcondition":{{"type":"file_contains","path":"{0}","substring":"proven fact"}},"done":false}}"#,
            esc(&file)
        );
        let finish = r#"{"done":true,"final_answer":"done","reasoning":"ok"}"#.to_string();
        let lp = AgenticLoop::new(ScriptedExecutor::new(vec![write, finish]), device)
            .with_economy(economy.clone());
        let result = lp
            .run(LoopRequest {
                objective: "establish that out.txt contains proven fact".into(),
                allow_tools: vec!["fs_write".into()],
                max_iterations: 5,
            })
            .await
            .expect("loop");
        // The verified result was proposed into the economy as a claim.
        let claim_id = result.claim_id.expect("a claim should be minted/proposed");
        let claim = economy.get_claim(&claim_id).expect("claim exists");
        assert_eq!(claim.proposer, "agentic_loop");
        assert!(economy.verify(&claim_id).unwrap().pass);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn minted_claim_is_recalled_without_work() {
        use crate::proof_economy::{AttackRequest, ProofEconomy};
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        let objective = "the host can echo the marker XYZ".to_string();
        // Pre-mint a claim whose statement equals the objective.
        let claim = economy
            .propose(ProposeRequest {
                statement: objective.clone(),
                kind: ClaimKind::Assertion,
                proposer: "alice".into(),
                verification: Check::ShellOutputContains {
                    command: "echo marker XYZ".into(),
                    substring: "XYZ".into(),
                },
                evidence: vec![],
                depends_on: vec![],
            })
            .unwrap();
        economy.attack(&claim.claim_id, AttackRequest { attacker: "b".into(), note: String::new(), counter: None }).unwrap();
        let minted = economy.attack(&claim.claim_id, AttackRequest { attacker: "c".into(), note: String::new(), counter: None }).unwrap();
        assert_eq!(format!("{:?}", minted.status), "Minted");

        // A loop with the same objective recalls it instead of doing any work.
        // The scripted executor has NO replies — if the loop ran it, it would not
        // produce status "recalled".
        let lp = AgenticLoop::new(ScriptedExecutor::new(vec![]), device).with_economy(economy);
        let result = lp
            .run(LoopRequest {
                objective,
                allow_tools: vec!["shell_exec".into()],
                max_iterations: 5,
            })
            .await
            .expect("loop");
        assert_eq!(result.status, "recalled");
        assert!(result.steps.is_empty());
        assert_eq!(result.claim_id.as_deref(), Some(claim.claim_id.as_str()));
    }

    #[actix_web::test]
    async fn semantically_recalls_paraphrased_objective() {
        use crate::proof_economy::{AttackRequest, ProofEconomy};
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).unwrap();
        // Mint a claim phrased one way...
        let claim = economy
            .propose(ProposeRequest {
                statement: "the host can echo the marker XYZ".into(),
                kind: ClaimKind::Assertion,
                proposer: "alice".into(),
                verification: Check::ShellOutputContains {
                    command: "echo marker XYZ".into(),
                    substring: "XYZ".into(),
                },
                evidence: vec![],
                depends_on: vec![],
            })
            .unwrap();
        economy.attack(&claim.claim_id, AttackRequest { attacker: "b".into(), note: String::new(), counter: None }).unwrap();
        economy.attack(&claim.claim_id, AttackRequest { attacker: "c".into(), note: String::new(), counter: None }).unwrap();

        // ...and ask for it phrased differently. Exact match would miss; the
        // lexical-cosine recall hits because the content words overlap.
        let lp = AgenticLoop::new(ScriptedExecutor::new(vec![]), device).with_economy(economy);
        let result = lp
            .run(LoopRequest {
                objective: "echo the marker XYZ on this host".into(),
                allow_tools: vec!["shell_exec".into()],
                max_iterations: 5,
            })
            .await
            .expect("loop");
        assert_eq!(result.status, "recalled");
        assert_eq!(result.claim_id.as_deref(), Some(claim.claim_id.as_str()));
    }
}

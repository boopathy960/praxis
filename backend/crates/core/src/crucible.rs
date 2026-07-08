//! The Crucible — root-cause reasoning: "why did this happen, what is the real fix?"
//!
//! Most agent systems — and, until now, this one's self-heal path — react to a
//! failure by patching the *symptom*: the [Forge](crate::forge) sees a tool
//! regressing and asks the model to "author a corrected recipe". That regenerates
//! the surface without ever asking why it broke. A mind that genuinely fixes
//! things does the opposite: it forms several competing explanations, **runs a
//! test that discriminates between them**, names the actual cause, and only then
//! proposes a fix that addresses *that* — carrying a proof the fix worked.
//!
//! The Crucible is that discipline, made a reusable organ. It runs the loop every
//! good diagnostician runs, grounded in the one substrate this system trusts — a
//! re-runnable [`Check`](crate::verification::Check) executed for real through the
//! [device layer](crate::device_agent):
//!
//!   1. **Diagnose (the differential).** Given a problem, the reasoner proposes
//!      several *distinct* candidate root causes — including non-obvious ones —
//!      each paired with a **discriminating test**: a read-only check whose result
//!      would confirm or rule that cause out, and the outcome expected if it is
//!      the real one. Hypotheses, not a single guess.
//!   2. **Discriminate (the "why").** The Crucible *runs those tests for real*.
//!      A cause whose test came out as predicted is **confirmed**; one whose test
//!      contradicted it is **ruled out**. The root cause is the confirmed
//!      hypothesis, not the most fluent sentence — reality decides, exactly as in
//!      the [proof economy](crate::proof_economy) and [active inference]
//!      (crate::active_inference).
//!   3. **Solve (the real fix).** Given the *confirmed* cause, the reasoner
//!      proposes the fix that addresses the cause itself, plus a **postcondition**
//!      — a check that passes only if the fix truly worked. The Crucible runs it;
//!      a real solution is one whose proof holds, and (optionally) it is staked
//!      into the proof economy so the fix carries a machine-checkable receipt.
//!   4. **Deepen (five whys).** A confirmed cause can itself be interrogated —
//!      "why does *that* happen?" — extending the causal chain another level, so
//!      a shallow cause ("the file was missing") is driven to its origin ("the
//!      writer step lost its working directory").
//!
//! The [Forge](crate::forge) consults the Crucible before re-authoring, so
//! self-heal repairs the cause instead of the symptom; agents, the
//! [agentic loop](crate::agentic_loop), and the [CEO](crate::ceo) can call it for
//! any failing situation — a broken build, a wrong numeric result, a regressed
//! capability on a machine.
//!
//! Honest boundary: the *generation* of hypotheses and fixes is the reasoner's
//! (and so is exactly as good as the model); what the Crucible adds is the
//! *discrimination* — it will not call a cause confirmed unless a real check came
//! out as that cause predicted, and it will not call a solution resolved unless a
//! real postcondition check passes. So the structure is sound even when the model
//! is not, and an unconfirmed diagnosis is reported as `Stuck`, never dressed up
//! as an answer. Tests and postconditions are restricted to reversible (read-only)
//! checks, so diagnosis can never mutate the machine it is reasoning about.

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde::Serialize;

use crate::asc2::ReasoningExecutor;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::proof_economy::{ClaimKind, ProofEconomy, ProposeRequest};
use crate::verification::Check;

/// Deepest causal chain a single inquiry will follow (the "five whys", bounded).
const MAX_DEPTH: u32 = 7;

/// Where an inquiry ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryStatus {
    /// Opened, not yet diagnosed.
    Open,
    /// A root cause was confirmed by a discriminating test.
    Diagnosed,
    /// A real solution was proposed and its postcondition proof held.
    Resolved,
    /// No test discriminated — a leading cause is recorded, but unconfirmed.
    Stuck,
}

/// One candidate root cause and the test that would confirm or rule it out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub hypothesis_id: String,
    /// The candidate cause — an answer to "why did this happen?".
    pub cause: String,
    pub rationale: String,
    /// The discriminating experiment (read-only). `None` ⇒ the model offered no
    /// way to test this cause, so it can only ever be a guess.
    pub test: Option<Check>,
    /// What the test would show if THIS cause is the real one.
    pub predicted_pass: bool,
    pub tested: bool,
    pub observed_pass: Option<bool>,
    /// `observed == predicted` — the cause survived its own test.
    pub confirmed: Option<bool>,
    pub confidence: f64,
    pub test_detail: String,
}

/// The real fix — the one that addresses the confirmed cause, with a proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub statement: String,
    pub addresses_cause: String,
    /// A check that passes only if the fix truly worked (read-only).
    pub postcondition: Option<Check>,
    /// Whether that postcondition held when run. `None` ⇒ not runnable here.
    pub proven: Option<bool>,
    pub proof_detail: String,
    /// Set when the proven solution was staked into the proof economy.
    pub claim_id: Option<String>,
}

/// A full root-cause investigation — durable and inspectable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inquiry {
    pub inquiry_id: String,
    pub problem: String,
    pub context: String,
    /// How many "why" levels the causal chain reached (1 = the first diagnosis).
    pub depth: u32,
    pub hypotheses: Vec<Hypothesis>,
    pub root_cause: Option<String>,
    pub solution: Option<Solution>,
    pub status: InquiryStatus,
    /// The reasoning steps, in order — what the Crucible thought and saw.
    pub trace: Vec<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Ask the Crucible to investigate a problem end-to-end.
#[derive(Debug, Clone, Deserialize)]
pub struct InvestigateRequest {
    pub problem: String,
    #[serde(default)]
    pub context: String,
    /// Stake the proven solution into the proof economy so the fix carries a
    /// machine-checkable receipt.
    #[serde(default)]
    pub mint: bool,
}

/// The root-cause reasoning engine. Holds the durable inquiry ledger, the
/// reasoner that hypothesizes, the device layer that runs every discriminating
/// test for real, and the proof economy a proven solution can be staked into.
#[derive(Clone)]
pub struct Crucible {
    store: Arc<Mutex<Connection>>,
    device: Arc<DeviceCapabilities>,
    economy: ProofEconomy,
    reasoner: Arc<dyn ReasoningExecutor>,
}

impl Crucible {
    pub fn new(
        data_dir: impl AsRef<Path>,
        device: Arc<DeviceCapabilities>,
        economy: ProofEconomy,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let path = data_dir.as_ref().join("crucible.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create crucible dir: {e}")))?;
        }
        let connection = Connection::open(&path)
            .map_err(|e| AppError::Internal(format!("failed to open crucible ledger: {e}")))?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
            economy,
            reasoner,
        })
    }

    /// Test/in-memory variant.
    pub fn in_memory(
        device: Arc<DeviceCapabilities>,
        economy: ProofEconomy,
        reasoner: Arc<dyn ReasoningExecutor>,
    ) -> Result<Self, AppError> {
        let connection = Connection::open_in_memory().map_err(sql_err)?;
        connection.execute_batch(SCHEMA).map_err(sql_err)?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
            device,
            economy,
            reasoner,
        })
    }

    /// Diagnose only: form competing hypotheses, run their discriminating tests,
    /// and name the confirmed root cause. Does not propose a fix — this is the
    /// "why", grounded. The [Forge](crate::forge) calls this before re-authoring.
    pub async fn diagnose(&self, problem: &str, context: &str) -> Result<Inquiry, AppError> {
        let inquiry = self.build_diagnosis(problem, context, 1).await?;
        self.save_inquiry(&inquiry)?;
        Ok(inquiry)
    }

    /// Investigate end-to-end: diagnose the root cause, then propose the real fix
    /// (the one addressing that cause) and prove its postcondition for real.
    /// Optionally stakes the proven fix into the proof economy.
    pub async fn investigate(&self, request: InvestigateRequest) -> Result<Inquiry, AppError> {
        let problem = request.problem.trim();
        if problem.is_empty() {
            return Err(AppError::Validation("problem statement is empty".into()));
        }
        let mut inquiry = self
            .build_diagnosis(problem, request.context.trim(), 1)
            .await?;

        if let Some(root_cause) = inquiry.root_cause.clone() {
            let mut solution = self.solve(&inquiry.problem, &root_cause).await?;
            inquiry
                .trace
                .push(format!("Proposed real solution: {}", solution.statement));

            // Prove the fix: run its postcondition for real (read-only only).
            if let Some(check) = solution.postcondition.clone() {
                if check.is_reversible() {
                    let outcome = crate::verification::run(&check, &self.device, None);
                    solution.proven = Some(outcome.pass);
                    solution.proof_detail = outcome.detail.clone();
                    inquiry.trace.push(format!(
                        "Postcondition {}: {}",
                        if outcome.pass { "HELD" } else { "FAILED" },
                        outcome.detail
                    ));
                    if outcome.pass {
                        inquiry.status = InquiryStatus::Resolved;
                        // Stake the proven fix into the economy on request.
                        if request.mint {
                            match self.economy.propose(ProposeRequest {
                                statement: solution.statement.clone(),
                                kind: ClaimKind::Assertion,
                                proposer: "crucible".into(),
                                verification: check,
                                evidence: vec![format!("root cause: {root_cause}")],
                                depends_on: vec![],
                            }) {
                                Ok(claim) => {
                                    solution.claim_id = Some(claim.claim_id.clone());
                                    inquiry.trace.push(format!(
                                        "Staked the solution into the proof economy as {}",
                                        claim.claim_id
                                    ));
                                }
                                Err(error) => inquiry
                                    .trace
                                    .push(format!("Could not stake the solution: {error}")),
                            }
                        }
                    }
                } else {
                    solution.proof_detail =
                        "postcondition mutates the world; left for explicit, supervised proof"
                            .into();
                    inquiry
                        .trace
                        .push("Postcondition is not read-only — not auto-proven".into());
                }
            }
            inquiry.solution = Some(solution);
        }

        inquiry.updated_at_ms = now_ms();
        self.save_inquiry(&inquiry)?;
        Ok(inquiry)
    }

    /// Deepen the causal chain: interrogate the confirmed root cause itself —
    /// "why does *that* happen?" — and extend the inquiry another level. This is
    /// the five-whys recursion that drives a shallow cause to its origin.
    pub async fn deepen(&self, inquiry_id: &str) -> Result<Inquiry, AppError> {
        let mut inquiry = self.get_inquiry(inquiry_id)?;
        let Some(cause) = inquiry.root_cause.clone() else {
            return Err(AppError::Validation(
                "this inquiry has no confirmed cause to deepen".into(),
            ));
        };
        if inquiry.depth >= MAX_DEPTH {
            inquiry.trace.push(format!(
                "Reached max causal depth ({MAX_DEPTH}); not deepening further"
            ));
            self.save_inquiry(&inquiry)?;
            return Ok(inquiry);
        }

        let deeper = self
            .build_diagnosis(
                &format!("Why does this happen: {cause}"),
                &inquiry.problem,
                inquiry.depth + 1,
            )
            .await?;
        inquiry.depth = deeper.depth;
        inquiry.hypotheses.extend(deeper.hypotheses);
        if let Some(deeper_cause) = deeper.root_cause {
            inquiry.trace.push(format!(
                "Why level {}: '{}' happens because '{}'",
                inquiry.depth, cause, deeper_cause
            ));
            inquiry.root_cause = Some(deeper_cause);
            inquiry.status = deeper.status;
        } else {
            inquiry.trace.push(format!(
                "Why level {}: no deeper cause discriminated",
                inquiry.depth
            ));
            inquiry.status = InquiryStatus::Stuck;
        }
        inquiry.updated_at_ms = now_ms();
        self.save_inquiry(&inquiry)?;
        Ok(inquiry)
    }

    /// A concise root-cause line for callers (like the Forge) that want to fold
    /// the diagnosis into their own prompt. Runs a full diagnosis and returns the
    /// confirmed cause plus the key trace, or `None` if nothing discriminated.
    pub async fn root_cause_brief(&self, problem: &str, context: &str) -> Option<String> {
        let inquiry = self.diagnose(problem, context).await.ok()?;
        let root = inquiry.root_cause?;
        let confirmed = inquiry.status == InquiryStatus::Diagnosed;
        Some(format!(
            "{} root cause: {root}{}",
            if confirmed {
                "Confirmed"
            } else {
                "Leading (unconfirmed)"
            },
            inquiry
                .hypotheses
                .iter()
                .find(|h| h.cause == root)
                .map(|h| format!(" — evidence: {}", h.test_detail))
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_default()
        ))
    }

    // ── the diagnostic kernel ────────────────────────────────────────────────

    /// Build (but do not persist) a diagnosis at the given causal depth.
    async fn build_diagnosis(
        &self,
        problem: &str,
        context: &str,
        depth: u32,
    ) -> Result<Inquiry, AppError> {
        let now = now_ms();
        let mut trace = vec![format!("Investigating (why-level {depth}): {problem}")];

        // 1. Differential: ask for competing causes, each with a discriminating test.
        let user = format!(
            "PROBLEM: {problem}\nCONTEXT: {}\n\nDo NOT propose a fix. First ask: why does this \
             happen? Give several DISTINCT candidate root causes — include at least one non-obvious \
             one. For each, give a read-only discriminating TEST whose result confirms or rules out \
             that cause, and what you'd expect to see if it is the real cause. Valid check \
             shapes:\n{CHECK_SHAPES}\n\nReturn STRICT JSON only, no prose:\n{DIAGNOSE_SCHEMA}",
            if context.is_empty() {
                "(none)"
            } else {
                context
            },
        );
        let raw = self
            .reasoner
            .complete(DIAGNOSE_SYSTEM.to_string(), user)
            .await?;
        let parsed: RawDiagnosis = parse_json(&raw).ok_or_else(|| {
            AppError::Internal("could not parse a diagnosis from the reasoner".into())
        })?;
        if parsed.hypotheses.is_empty() {
            return Err(AppError::Internal(
                "the reasoner offered no hypotheses".into(),
            ));
        }
        trace.push(format!(
            "Formed {} competing hypotheses",
            parsed.hypotheses.len()
        ));

        // 2. Discriminate: run each read-only test for real; reality decides.
        let mut hypotheses = Vec::new();
        for rh in parsed.hypotheses {
            let cause = rh.cause.trim().to_string();
            if cause.is_empty() {
                continue;
            }
            let confidence = rh.confidence.unwrap_or(0.5).clamp(0.0, 1.0);
            let mut hypothesis = Hypothesis {
                hypothesis_id: new_id("hyp"),
                cause: cause.clone(),
                rationale: rh.rationale.trim().to_string(),
                test: rh.test.clone(),
                predicted_pass: rh.predicted_pass,
                tested: false,
                observed_pass: None,
                confirmed: None,
                confidence,
                test_detail: String::new(),
            };
            match rh.test {
                Some(test) if test.is_reversible() => {
                    let outcome = crate::verification::run(&test, &self.device, None);
                    let confirmed = outcome.pass == rh.predicted_pass;
                    hypothesis.tested = true;
                    hypothesis.observed_pass = Some(outcome.pass);
                    hypothesis.confirmed = Some(confirmed);
                    hypothesis.test_detail = outcome.detail.clone();
                    trace.push(format!(
                        "Tested '{}': {} ({})",
                        cause,
                        if confirmed { "CONFIRMED" } else { "ruled out" },
                        outcome.detail
                    ));
                }
                Some(_) => {
                    hypothesis.test_detail = "test mutates the world; not auto-run".into();
                    trace.push(format!("'{cause}': test not read-only, left untested"));
                }
                None => {
                    hypothesis.test_detail = "no discriminating test offered".into();
                    trace.push(format!("'{cause}': no test — cannot be confirmed"));
                }
            }
            hypotheses.push(hypothesis);
        }

        // 3. Choose the root cause: a confirmed hypothesis beats a fluent guess.
        let (root_cause, status) = choose_root(&hypotheses);
        match (&root_cause, status) {
            (Some(cause), InquiryStatus::Diagnosed) => {
                trace.push(format!("Root cause (confirmed): {cause}"))
            }
            (Some(cause), _) => trace.push(format!(
                "Leading cause (UNCONFIRMED — no test discriminated): {cause}"
            )),
            (None, _) => trace.push("No cause could be identified".into()),
        }

        Ok(Inquiry {
            inquiry_id: new_id("inquiry"),
            problem: problem.to_string(),
            context: context.to_string(),
            depth,
            hypotheses,
            root_cause,
            solution: None,
            status,
            trace,
            created_at_ms: now,
            updated_at_ms: now,
        })
    }

    async fn solve(&self, problem: &str, root_cause: &str) -> Result<Solution, AppError> {
        let user = format!(
            "PROBLEM: {problem}\nCONFIRMED ROOT CAUSE: {root_cause}\n\nPropose the REAL fix — the \
             one that addresses this root cause, not the symptom. Give a postcondition: a read-only \
             check that passes only if the fix truly worked. Valid check shapes:\n{CHECK_SHAPES}\n\n\
             Return STRICT JSON only, no prose:\n{SOLVE_SCHEMA}",
        );
        let raw = self
            .reasoner
            .complete(SOLVE_SYSTEM.to_string(), user)
            .await?;
        let parsed: RawSolution = parse_json(&raw).ok_or_else(|| {
            AppError::Internal("could not parse a solution from the reasoner".into())
        })?;
        let statement = parsed.statement.trim().to_string();
        if statement.is_empty() {
            return Err(AppError::Internal(
                "the reasoner returned an empty solution".into(),
            ));
        }
        Ok(Solution {
            statement,
            addresses_cause: {
                let a = parsed.addresses_cause.trim();
                if a.is_empty() {
                    root_cause.to_string()
                } else {
                    a.to_string()
                }
            },
            postcondition: parsed.postcondition,
            proven: None,
            proof_detail: String::new(),
            claim_id: None,
        })
    }

    // ── reads & persistence ──────────────────────────────────────────────────

    pub fn get_inquiry(&self, inquiry_id: &str) -> Result<Inquiry, AppError> {
        let payload: String = self
            .store
            .lock()
            .query_row(
                "SELECT payload FROM crucible_inquiries WHERE inquiry_id=?1",
                [inquiry_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .ok_or_else(|| AppError::NotFound(format!("inquiry {inquiry_id}")))?;
        serde_json::from_str(&payload).map_err(de_err)
    }

    pub fn inquiries(&self, limit: usize) -> Result<Vec<Inquiry>, AppError> {
        let limit = limit.clamp(1, 1000) as i64;
        let store = self.store.lock();
        let mut stmt = store
            .prepare("SELECT payload FROM crucible_inquiries ORDER BY created_at_ms DESC LIMIT ?1")
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![limit], |row| row.get::<_, String>(0))
            .map_err(sql_err)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(serde_json::from_str(&row.map_err(sql_err)?).map_err(de_err)?);
        }
        Ok(out)
    }

    fn save_inquiry(&self, inquiry: &Inquiry) -> Result<(), AppError> {
        let payload = serde_json::to_string(inquiry).map_err(ser_err)?;
        let status = status_tag(inquiry.status);
        self.store
            .lock()
            .execute(
                "INSERT OR REPLACE INTO crucible_inquiries (inquiry_id, payload, status, created_at_ms) VALUES (?1,?2,?3,?4)",
                params![inquiry.inquiry_id, payload, status, inquiry.created_at_ms],
            )
            .map_err(sql_err)?;
        Ok(())
    }
}

/// A confirmed hypothesis beats an unconfirmed one; among equals, higher
/// confidence wins. If nothing was confirmed, the most plausible is returned as a
/// *leading* cause and the inquiry is honestly marked `Stuck`.
fn choose_root(hypotheses: &[Hypothesis]) -> (Option<String>, InquiryStatus) {
    let best_confirmed = hypotheses
        .iter()
        .filter(|h| h.confirmed == Some(true))
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    if let Some(hypothesis) = best_confirmed {
        return (Some(hypothesis.cause.clone()), InquiryStatus::Diagnosed);
    }
    // Nothing confirmed — fall back to the most plausible NOT-ruled-out cause.
    let leading = hypotheses
        .iter()
        .filter(|h| h.confirmed != Some(false))
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    match leading {
        Some(hypothesis) => (Some(hypothesis.cause.clone()), InquiryStatus::Stuck),
        None => (None, InquiryStatus::Stuck),
    }
}

// ── reasoner I/O contracts ──────────────────────────────────────────────────

const DIAGNOSE_SYSTEM: &str = "You are the Crucible, the root-cause reasoning engine of a verified intelligence. You never \
     jump to a fix. You think like a diagnostician: form several competing explanations for why a \
     problem occurs, and for each give a concrete read-only test that would confirm or rule it \
     out. You value a non-obvious cause that a test can settle over a fluent guess that nothing \
     can check. You output strict JSON only.";

const SOLVE_SYSTEM: &str = "You are the Crucible, proposing the real solution. Given a confirmed root cause, you propose \
     the fix that removes the cause itself — not a patch over the symptom — and a read-only \
     postcondition check that passes only if the fix genuinely worked. You output strict JSON only.";

const CHECK_SHAPES: &str = r#"  {"type":"file_exists","path":"..."}
  {"type":"file_contains","path":"...","substring":"..."}
  {"type":"path_absent","path":"..."}
  {"type":"command_succeeds","command":"<read-only command>"}
  {"type":"command_fails","command":"<read-only command>"}
  {"type":"shell_output_contains","command":"<read-only command>","substring":"..."}
  {"type":"trivial","pass":true_or_false}"#;

const DIAGNOSE_SCHEMA: &str = r#"{
  "hypotheses": [
    {
      "cause": "<a candidate root cause>",
      "rationale": "<why this could be the cause>",
      "test": { "type": "...", ... } | null,
      "predicted_pass": true_or_false,
      "confidence": 0.0_to_1.0
    }, ...
  ]
}"#;

const SOLVE_SCHEMA: &str = r#"{
  "statement": "<the real fix that removes the root cause>",
  "addresses_cause": "<the cause it removes>",
  "postcondition": { "type": "...", ... } | null
}"#;

#[derive(Debug, Deserialize)]
struct RawDiagnosis {
    #[serde(default)]
    hypotheses: Vec<RawHypothesis>,
}

#[derive(Debug, Deserialize)]
struct RawHypothesis {
    #[serde(default)]
    cause: String,
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    test: Option<Check>,
    #[serde(default)]
    predicted_pass: bool,
    #[serde(default)]
    confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RawSolution {
    #[serde(default)]
    statement: String,
    #[serde(default)]
    addresses_cause: String,
    #[serde(default)]
    postcondition: Option<Check>,
}

/// Tolerant JSON extraction — reasoning models often wrap answers in prose or
/// markdown fences. Take the outermost `{`…`}` span and parse it.
fn parse_json<T: serde::de::DeserializeOwned>(raw: &str) -> Option<T> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&raw[start..=end]).ok()
}

const SCHEMA: &str = "PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS crucible_inquiries (
        inquiry_id TEXT PRIMARY KEY, payload TEXT NOT NULL,
        status TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS crucible_inquiries_at ON crucible_inquiries(created_at_ms);";

fn status_tag(status: InquiryStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "open".into())
}

fn sql_err(e: rusqlite::Error) -> AppError {
    AppError::Internal(format!("crucible sql error: {e}"))
}
fn ser_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("crucible serialize error: {e}"))
}
fn de_err(e: serde_json::Error) -> AppError {
    AppError::Internal(format!("crucible decode error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc2::{BoxFuture, ReasoningProposal, ReasoningRequest};
    use crate::device_agent::{DeviceCapabilities, DevicePolicy};
    use std::collections::VecDeque;

    struct ScriptedReasoner {
        replies: Mutex<VecDeque<String>>,
    }
    impl ScriptedReasoner {
        fn new(replies: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(replies.into_iter().map(str::to_string).collect()),
            })
        }
    }
    impl ReasoningExecutor for ScriptedReasoner {
        fn name(&self) -> &str {
            "scripted"
        }
        fn execute(
            &self,
            _request: ReasoningRequest,
        ) -> BoxFuture<Result<ReasoningProposal, AppError>> {
            Box::pin(async {
                Err(AppError::Internal(
                    "execute unused in crucible tests".into(),
                ))
            })
        }
        fn complete(&self, _system: String, _user: String) -> BoxFuture<Result<String, AppError>> {
            let next = self.replies.lock().pop_front().unwrap_or_default();
            Box::pin(async move { Ok(next) })
        }
    }

    fn crucible_with(replies: Vec<&str>) -> Crucible {
        let device = Arc::new(DeviceCapabilities::new(DevicePolicy::permissive()));
        let economy = ProofEconomy::in_memory(device.clone()).expect("economy");
        Crucible::in_memory(device, economy, ScriptedReasoner::new(replies)).expect("crucible")
    }

    // A diagnosis with two causes: the first has a test that will confirm it, the
    // second a test that will not — so reality, not fluency, picks the root.
    const DIAGNOSE: &str = r#"{"hypotheses":[
        {"cause":"dependency missing","rationale":"the step needs a file that is absent",
         "test":{"type":"trivial","pass":true},"predicted_pass":true,"confidence":0.8},
        {"cause":"race condition","rationale":"two steps write at once",
         "test":{"type":"trivial","pass":false},"predicted_pass":true,"confidence":0.6}
    ]}"#;

    #[actix_web::test]
    async fn diagnosis_is_decided_by_a_discriminating_test() {
        let crucible = crucible_with(vec![DIAGNOSE]);
        let inquiry = crucible
            .diagnose("the tool fails intermittently", "forged tool fetch_data")
            .await
            .expect("diagnose");

        assert_eq!(inquiry.status, InquiryStatus::Diagnosed);
        assert_eq!(
            inquiry.root_cause.as_deref(),
            Some("dependency missing"),
            "the confirmed cause wins over the higher-fluency but ruled-out one"
        );
        let confirmed = inquiry
            .hypotheses
            .iter()
            .find(|h| h.cause == "dependency missing")
            .unwrap();
        assert_eq!(confirmed.confirmed, Some(true));
        let ruled_out = inquiry
            .hypotheses
            .iter()
            .find(|h| h.cause == "race condition")
            .unwrap();
        assert_eq!(ruled_out.confirmed, Some(false));
    }

    #[actix_web::test]
    async fn investigate_proves_the_real_solution() {
        let solve = r#"{"statement":"provision the missing dependency at startup",
            "addresses_cause":"dependency missing",
            "postcondition":{"type":"trivial","pass":true}}"#;
        let crucible = crucible_with(vec![DIAGNOSE, solve]);
        let inquiry = crucible
            .investigate(InvestigateRequest {
                problem: "the tool fails intermittently".into(),
                context: "forged tool".into(),
                mint: false,
            })
            .await
            .expect("investigate");

        assert_eq!(inquiry.status, InquiryStatus::Resolved);
        let solution = inquiry.solution.expect("a solution");
        assert_eq!(solution.proven, Some(true), "the postcondition held");
        assert_eq!(solution.addresses_cause, "dependency missing");
        assert!(solution.claim_id.is_none(), "not minted unless asked");
    }

    #[actix_web::test]
    async fn a_proven_solution_can_be_staked_into_the_economy() {
        let solve = r#"{"statement":"provision the dependency",
            "addresses_cause":"dependency missing",
            "postcondition":{"type":"trivial","pass":true}}"#;
        let crucible = crucible_with(vec![DIAGNOSE, solve]);
        let inquiry = crucible
            .investigate(InvestigateRequest {
                problem: "the tool fails".into(),
                context: String::new(),
                mint: true,
            })
            .await
            .expect("investigate");

        let solution = inquiry.solution.expect("a solution");
        let claim_id = solution.claim_id.expect("a staked claim");
        // The claim really exists in the economy and re-verifies.
        assert!(crucible.economy.verify(&claim_id).expect("verify").pass);
    }

    #[actix_web::test]
    async fn a_failing_postcondition_is_not_resolved() {
        let solve = r#"{"statement":"a guess that does not work",
            "addresses_cause":"dependency missing",
            "postcondition":{"type":"trivial","pass":false}}"#;
        let crucible = crucible_with(vec![DIAGNOSE, solve]);
        let inquiry = crucible
            .investigate(InvestigateRequest {
                problem: "the tool fails".into(),
                context: String::new(),
                mint: true,
            })
            .await
            .expect("investigate");

        let solution = inquiry.solution.expect("a solution");
        assert_eq!(solution.proven, Some(false));
        assert!(solution.claim_id.is_none(), "an unproven fix is not staked");
        assert_ne!(
            inquiry.status,
            InquiryStatus::Resolved,
            "no resolution without a passing postcondition"
        );
    }

    #[actix_web::test]
    async fn deepen_extends_the_causal_chain() {
        let deeper = r#"{"hypotheses":[
            {"cause":"the installer never ran","rationale":"root of the missing dep",
             "test":{"type":"trivial","pass":true},"predicted_pass":true,"confidence":0.9}
        ]}"#;
        let crucible = crucible_with(vec![DIAGNOSE, deeper]);
        let inquiry = crucible
            .diagnose("the tool fails", "ctx")
            .await
            .expect("diagnose");
        assert_eq!(inquiry.depth, 1);

        let deepened = crucible.deepen(&inquiry.inquiry_id).await.expect("deepen");
        assert_eq!(deepened.depth, 2, "the why-chain advanced a level");
        assert_eq!(
            deepened.root_cause.as_deref(),
            Some("the installer never ran"),
            "the chain now points at the deeper cause"
        );
    }
}

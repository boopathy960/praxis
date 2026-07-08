//! The shared verification primitive.
//!
//! One `Check` type — a deterministic, re-runnable, machine-checkable predicate
//! executed for real through the [device layer](crate::device_agent) — used by
//! BOTH the [agentic loop](crate::agentic_loop) (to prove an action achieved its
//! effect) and the [proof economy](crate::proof_economy) (to adjudicate whether
//! a claim is true). Previously each had its own near-identical type; unifying
//! them is what lets a verified loop result become a mintable economy claim and
//! a minted claim become the loop's reusable memory.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::device_agent::DeviceCapabilities;

/// A checkable predicate. The only arbiter of truth in either subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Check {
    /// Every nested check must pass.
    All { checks: Vec<Check> },
    /// At least one nested check must pass.
    Any { checks: Vec<Check> },
    /// The nested check must fail.
    Not { check: Box<Check> },
    /// The file exists and is readable.
    FileExists { path: String },
    /// The file exists and contains the substring.
    FileContains { path: String, substring: String },
    /// A JSON file has a value at `pointer` equal to `expected`.
    JsonPathEquals {
        path: String,
        pointer: String,
        expected: Value,
    },
    /// A JSON file has a string-like value at `pointer` containing `substring`.
    JsonPathContains {
        path: String,
        pointer: String,
        substring: String,
    },
    /// The path does not exist (e.g. after a delete).
    PathAbsent { path: String },
    /// A guarded HTTP GET body contains the substring.
    HttpBodyContains { url: String, substring: String },
    /// A shell command exits 0 and its stdout contains the substring.
    ShellOutputContains { command: String, substring: String },
    /// A shell command exits 0.
    CommandSucceeds { command: String },
    /// A shell command does NOT exit 0 — the substrate of an impossibility proof.
    CommandFails { command: String },
    /// The just-run action's own output contains the substring (loop-only:
    /// needs the action's output as context).
    OutputContains { substring: String },
    /// The just-run action's own JSON output has a value equal to `expected`.
    OutputJsonPathEquals { pointer: String, expected: Value },
    /// Scaffolding for dependencies/tests.
    Trivial { pass: bool },
}

/// A richer success contract layered on top of `Check`.
///
/// `checks` are the truth-bearing predicates. `eval_suite_id` can point at a
/// separately managed golden/adversarial suite. `rollback` checks are expected
/// to hold after cleanup of a mutating action. `watch_policy` lets callers state
/// whether this contract should be placed under continuous re-verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofContract {
    #[serde(default)]
    pub checks: Vec<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_suite_id: Option<String>,
    #[serde(default)]
    pub rollback: Vec<Check>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_policy: Option<String>,
}

impl ProofContract {
    #[must_use]
    pub fn single(check: Check) -> Self {
        Self {
            checks: vec![check],
            ..Self::default()
        }
    }

    #[must_use]
    pub fn primary_check(&self) -> Check {
        match self.checks.as_slice() {
            [one] => one.clone(),
            checks => Check::All {
                checks: checks.to_vec(),
            },
        }
    }
}

impl Default for ProofContract {
    fn default() -> Self {
        Self {
            checks: vec![Check::Trivial { pass: true }],
            eval_suite_id: None,
            rollback: Vec::new(),
            watch_policy: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub pass: bool,
    pub detail: String,
}

impl Check {
    /// A check is reversible when running it cannot change the world. Mutating
    /// shell commands are not; reads/output/trivial are.
    #[must_use]
    pub fn is_reversible(&self) -> bool {
        match self {
            Check::All { checks } | Check::Any { checks } => {
                checks.iter().all(Check::is_reversible)
            }
            Check::Not { check } => check.is_reversible(),
            Check::FileExists { .. }
            | Check::FileContains { .. }
            | Check::JsonPathEquals { .. }
            | Check::JsonPathContains { .. }
            | Check::PathAbsent { .. }
            | Check::HttpBodyContains { .. }
            | Check::OutputContains { .. }
            | Check::OutputJsonPathEquals { .. }
            | Check::Trivial { .. } => true,
            Check::ShellOutputContains { command, .. }
            | Check::CommandSucceeds { command }
            | Check::CommandFails { command } => !command_mutates(command),
        }
    }

    /// Rough verification cost, used for value-of-information ranking.
    #[must_use]
    pub fn cost(&self) -> f64 {
        match self {
            Check::All { checks } => checks.iter().map(Check::cost).sum(),
            Check::Any { checks } => checks.iter().map(Check::cost).fold(0.0, f64::max),
            Check::Not { check } => check.cost(),
            Check::Trivial { .. }
            | Check::OutputContains { .. }
            | Check::OutputJsonPathEquals { .. } => 0.1,
            Check::FileExists { .. }
            | Check::FileContains { .. }
            | Check::JsonPathEquals { .. }
            | Check::JsonPathContains { .. }
            | Check::PathAbsent { .. } => 1.0,
            Check::HttpBodyContains { .. } => 2.0,
            _ => 3.0,
        }
    }
}

/// Run a check for real. `context` is the just-run action's output, required by
/// `OutputContains` and ignored by the rest.
#[must_use]
pub fn run(check: &Check, device: &DeviceCapabilities, context: Option<&Value>) -> CheckOutcome {
    match check {
        Check::All { checks } => {
            let outcomes = checks
                .iter()
                .map(|check| run(check, device, context))
                .collect::<Vec<_>>();
            let pass = outcomes.iter().all(|outcome| outcome.pass);
            outcome(
                pass,
                format!(
                    "all({}/{}) [{}]",
                    outcomes.iter().filter(|outcome| outcome.pass).count(),
                    outcomes.len(),
                    outcomes
                        .iter()
                        .map(|outcome| outcome.detail.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            )
        }
        Check::Any { checks } => {
            let outcomes = checks
                .iter()
                .map(|check| run(check, device, context))
                .collect::<Vec<_>>();
            let pass = outcomes.iter().any(|outcome| outcome.pass);
            outcome(
                pass,
                format!(
                    "any({}/{}) [{}]",
                    outcomes.iter().filter(|outcome| outcome.pass).count(),
                    outcomes.len(),
                    outcomes
                        .iter()
                        .map(|outcome| outcome.detail.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            )
        }
        Check::Not { check } => {
            let inner = run(check, device, context);
            outcome(
                !inner.pass,
                format!("not({}): {}", inner.pass, inner.detail),
            )
        }
        Check::Trivial { pass } => outcome(*pass, format!("trivial({pass})")),
        Check::OutputContains { substring } => match context {
            Some(value) => {
                let haystack = value
                    .get("stdout")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| value.to_string());
                let pass = haystack.contains(substring);
                outcome(pass, format!("output contains \"{substring}\": {pass}"))
            }
            None => outcome(false, "no action output available to check".into()),
        },
        Check::OutputJsonPathEquals { pointer, expected } => match context {
            Some(value) => {
                let actual = value.pointer(pointer);
                let pass = actual == Some(expected);
                outcome(
                    pass,
                    format!("output pointer {pointer} == {}: {pass}", expected),
                )
            }
            None => outcome(false, "no action output available to check".into()),
        },
        Check::FileExists { path } => match read_file(device, path) {
            Ok(_) => outcome(true, format!("{path} exists")),
            Err(error) => outcome(false, format!("{path} not present: {error}")),
        },
        Check::FileContains { path, substring } => match read_file(device, path) {
            Ok(content) => {
                let pass = content.contains(substring);
                outcome(pass, format!("{path} contains \"{substring}\": {pass}"))
            }
            Err(error) => outcome(false, format!("{path} unreadable: {error}")),
        },
        Check::JsonPathEquals {
            path,
            pointer,
            expected,
        } => match read_json_file(device, path) {
            Ok(value) => {
                let actual = value.pointer(pointer);
                let pass = actual == Some(expected);
                outcome(pass, format!("{path}{pointer} == {}: {pass}", expected))
            }
            Err(error) => outcome(false, format!("{path} JSON unreadable: {error}")),
        },
        Check::JsonPathContains {
            path,
            pointer,
            substring,
        } => match read_json_file(device, path) {
            Ok(value) => {
                let haystack = value.pointer(pointer).map(json_text).unwrap_or_default();
                let pass = haystack.contains(substring);
                outcome(
                    pass,
                    format!("{path}{pointer} contains \"{substring}\": {pass}"),
                )
            }
            Err(error) => outcome(false, format!("{path} JSON unreadable: {error}")),
        },
        Check::PathAbsent { path } => match read_file(device, path) {
            Ok(_) => outcome(false, format!("{path} still exists")),
            Err(_) => outcome(true, format!("{path} is absent")),
        },
        Check::HttpBodyContains { url, substring } => {
            match device.execute("http_get", &json!({ "url": url })) {
                Ok(value) => {
                    let body = value.get("body").and_then(Value::as_str).unwrap_or("");
                    let pass = body.contains(substring);
                    outcome(pass, format!("{url} body contains \"{substring}\": {pass}"))
                }
                Err(error) => outcome(false, format!("http_get error: {error}")),
            }
        }
        Check::ShellOutputContains { command, substring } => match shell(device, command) {
            Ok(value) => {
                let stdout = value.get("stdout").and_then(Value::as_str).unwrap_or("");
                let exit_ok = value.get("exit_code").and_then(Value::as_i64) == Some(0);
                let pass = exit_ok && stdout.contains(substring);
                outcome(
                    pass,
                    format!(
                        "exit_ok={exit_ok}, contains=\"{substring}\":{}",
                        stdout.contains(substring)
                    ),
                )
            }
            Err(error) => outcome(false, format!("command error: {error}")),
        },
        Check::CommandSucceeds { command } => match shell(device, command) {
            Ok(value) => {
                let ok = value.get("exit_code").and_then(Value::as_i64) == Some(0);
                outcome(ok, format!("exit_code==0: {ok}"))
            }
            Err(error) => outcome(false, format!("command error: {error}")),
        },
        Check::CommandFails { command } => match shell(device, command) {
            Ok(value) => {
                let code = value.get("exit_code").and_then(Value::as_i64);
                let killed = value
                    .get("killed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let fails = killed || code != Some(0);
                outcome(
                    fails,
                    format!("did not succeed (code={code:?}, killed={killed})"),
                )
            }
            Err(error) => outcome(
                true,
                format!("command could not run (counts as fail): {error}"),
            ),
        },
    }
}

#[must_use]
pub fn run_contract(
    contract: &ProofContract,
    device: &DeviceCapabilities,
    context: Option<&Value>,
) -> CheckOutcome {
    run(&contract.primary_check(), device, context)
}

fn outcome(pass: bool, detail: String) -> CheckOutcome {
    CheckOutcome { pass, detail }
}

fn read_file(device: &DeviceCapabilities, path: &str) -> Result<String, String> {
    let output = device.execute("fs_read", &json!({ "path": path }))?;
    output
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "file had no readable content".to_string())
}

fn read_json_file(device: &DeviceCapabilities, path: &str) -> Result<Value, String> {
    let text = read_file(device, path)?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

fn json_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn shell(device: &DeviceCapabilities, command: &str) -> Result<Value, String> {
    device.execute(
        "shell_exec",
        &json!({ "command": command, "timeout_ms": 10_000 }),
    )
}

fn command_mutates(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    [
        ">", "rm ", "del ", "rmdir", "move ", "mv ", "format", "mkfs", "truncate", "rd ",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

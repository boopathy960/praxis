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
    /// The file exists and is readable.
    FileExists { path: String },
    /// The file exists and contains the substring.
    FileContains { path: String, substring: String },
    /// The path does not exist (e.g. after a delete).
    PathAbsent { path: String },
    /// A shell command exits 0 and its stdout contains the substring.
    ShellOutputContains { command: String, substring: String },
    /// A shell command exits 0.
    CommandSucceeds { command: String },
    /// A shell command does NOT exit 0 — the substrate of an impossibility proof.
    CommandFails { command: String },
    /// The just-run action's own output contains the substring (loop-only:
    /// needs the action's output as context).
    OutputContains { substring: String },
    /// Scaffolding for dependencies/tests.
    Trivial { pass: bool },
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
            Check::FileExists { .. }
            | Check::FileContains { .. }
            | Check::PathAbsent { .. }
            | Check::OutputContains { .. }
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
            Check::Trivial { .. } | Check::OutputContains { .. } => 0.1,
            Check::FileExists { .. } | Check::FileContains { .. } | Check::PathAbsent { .. } => 1.0,
            _ => 3.0,
        }
    }
}

/// Run a check for real. `context` is the just-run action's output, required by
/// `OutputContains` and ignored by the rest.
#[must_use]
pub fn run(check: &Check, device: &DeviceCapabilities, context: Option<&Value>) -> CheckOutcome {
    match check {
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
        Check::PathAbsent { path } => match read_file(device, path) {
            Ok(_) => outcome(false, format!("{path} still exists")),
            Err(_) => outcome(true, format!("{path} is absent")),
        },
        Check::ShellOutputContains { command, substring } => match shell(device, command) {
            Ok(value) => {
                let stdout = value.get("stdout").and_then(Value::as_str).unwrap_or("");
                let exit_ok = value.get("exit_code").and_then(Value::as_i64) == Some(0);
                let pass = exit_ok && stdout.contains(substring);
                outcome(
                    pass,
                    format!("exit_ok={exit_ok}, contains=\"{substring}\":{}", stdout.contains(substring)),
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
                let killed = value.get("killed").and_then(Value::as_bool).unwrap_or(false);
                let fails = killed || code != Some(0);
                outcome(fails, format!("did not succeed (code={code:?}, killed={killed})"))
            }
            Err(error) => outcome(true, format!("command could not run (counts as fail): {error}")),
        },
    }
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

fn shell(device: &DeviceCapabilities, command: &str) -> Result<Value, String> {
    device.execute(
        "shell_exec",
        &json!({ "command": command, "timeout_ms": 10_000 }),
    )
}

fn command_mutates(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    [">", "rm ", "del ", "rmdir", "move ", "mv ", "format", "mkfs", "truncate", "rd "]
        .iter()
        .any(|token| lower.contains(token))
}

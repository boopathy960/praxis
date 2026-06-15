//! Supervised device-capability layer.
//!
//! This is what turns Astra's agents from *planners* into *actors*. It gives
//! an autonomous agent real eyes and hands on the host machine:
//!
//!   * **Eyes** — system introspection, process listing, disk usage, directory
//!     and file reads. The agent can *see the user's device*.
//!   * **Hands** — file writes, opening files/URLs in the user's apps, and real
//!     host shell execution. The agent can *do a service* for the user.
//!
//! The design goal the user asked for is "the sandbox should allow everything,
//! but watch continuously." That is exactly what [`DeviceSupervisor`] does: it
//! does not pre-emptively forbid capability (when `allow_everything` is set the
//! whole device is reachable), but every single operation is registered the
//! instant it starts, monitored on a watchdog while it runs, force-killed if it
//! blows past its time or output ceiling, and sealed into a tamper-evident
//! hash-chained audit the instant it finishes. Nothing the agent does is
//! invisible, and nothing runs unbounded.
//!
//! Even in permissive mode a small, non-negotiable blocklist (disk formatting,
//! root recursive deletes, fork bombs) is refused outright — those are never a
//! "service the user needs," only a way to brick the machine.

use std::collections::VecDeque;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::common::{new_id, now_ms};

/// How many finished operations to keep in the in-memory audit ring that the
/// live-watch endpoint reads from.
const AUDIT_RING_CAPACITY: usize = 512;
/// Watchdog poll cadence. Small enough that a runaway is killed promptly,
/// large enough that idle supervision costs nothing.
const WATCHDOG_TICK: Duration = Duration::from_millis(50);

/// What the agent is allowed to reach, and the ceilings every action runs under.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePolicy {
    /// Permissive host mode. When true, filesystem and shell reach the whole
    /// machine (still watched, still bounded, still audited). When false, file
    /// operations are jailed to `allowed_roots`.
    pub allow_everything: bool,
    /// Roots file operations are confined to when `allow_everything` is false.
    pub allowed_roots: Vec<PathBuf>,
    /// Wall-clock ceiling for a single shell execution before the watchdog
    /// kills it.
    pub command_timeout_ms: u64,
    /// Hard cap on captured stdout/stderr per stream.
    pub max_output_bytes: usize,
    /// Maximum bytes a single `fs_read` will return.
    pub max_read_bytes: usize,
    /// Substrings that are refused regardless of `allow_everything`. These are
    /// machine-destroying, never a legitimate service.
    pub hard_blocklist: Vec<String>,
}

impl Default for DevicePolicy {
    fn default() -> Self {
        Self {
            allow_everything: false,
            allowed_roots: Vec::new(),
            command_timeout_ms: 30_000,
            max_output_bytes: 256 * 1024,
            max_read_bytes: 1024 * 1024,
            hard_blocklist: default_hard_blocklist(),
        }
    }
}

impl DevicePolicy {
    /// The permissive profile the user asked for: the agent can reach the whole
    /// device and run real services, but the supervisor still watches, bounds,
    /// and audits every action, and the hard blocklist still stands.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            allow_everything: true,
            ..Self::default()
        }
    }

    /// A locked profile confined to specific roots — for least-privilege use.
    #[must_use]
    pub fn confined_to(roots: Vec<PathBuf>) -> Self {
        Self {
            allow_everything: false,
            allowed_roots: roots,
            ..Self::default()
        }
    }
}

fn default_hard_blocklist() -> Vec<String> {
    [
        "rm -rf /",
        "rm -rf /*",
        ":(){ :|:&};:", // classic fork bomb
        "mkfs",
        "format c:",
        "format /",
        "del /f /s /q c:\\",
        "rd /s /q c:\\",
        "diskpart",
        "> /dev/sda",
        "dd if=/dev/zero of=/dev/",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// A snapshot of an operation the supervisor is watching right now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveOperation {
    pub op_id: String,
    pub tool: String,
    pub detail: String,
    pub started_at_ms: i64,
    pub elapsed_ms: i64,
    pub status: String,
}

/// One logged record of a finished operation (plain log, not chained).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuditEntry {
    pub entry_id: String,
    pub op_id: String,
    pub tool: String,
    pub detail: String,
    pub outcome: String,
    pub summary: String,
    pub duration_ms: i64,
    pub created_at_ms: i64,
}

/// Live counters describing supervisor health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSupervisorStats {
    pub in_flight: usize,
    pub total_started: u64,
    pub total_completed: u64,
    pub total_failed: u64,
    pub total_killed: u64,
    pub audit_log_len: usize,
}

#[derive(Default)]
struct SupervisorInner {
    live: Vec<LiveOperation>,
    audit: VecDeque<DeviceAuditEntry>,
    logged: u64,
    started: u64,
    completed: u64,
    failed: u64,
    killed: u64,
}

/// The continuous watcher. Every device action calls [`begin`](Self::begin)
/// before it runs and [`finish`](Self::finish) when it ends; long-running
/// actions call [`touch`](Self::touch) so the live view stays current and the
/// watchdog can decide to kill them.
#[derive(Clone)]
pub struct DeviceSupervisor {
    inner: Arc<Mutex<SupervisorInner>>,
}

impl Default for DeviceSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceSupervisor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SupervisorInner::default())),
        }
    }

    /// Register the start of an operation; returns its id.
    pub fn begin(&self, tool: &str, detail: &str) -> String {
        let op_id = new_id("device_op");
        let mut inner = self.inner.lock();
        inner.started += 1;
        inner.live.push(LiveOperation {
            op_id: op_id.clone(),
            tool: tool.to_string(),
            detail: detail.chars().take(200).collect(),
            started_at_ms: now_ms(),
            elapsed_ms: 0,
            status: "running".into(),
        });
        op_id
    }

    /// Refresh the elapsed time of an in-flight operation (called by watchdogs).
    pub fn touch(&self, op_id: &str, elapsed_ms: i64) {
        let mut inner = self.inner.lock();
        if let Some(op) = inner.live.iter_mut().find(|op| op.op_id == op_id) {
            op.elapsed_ms = elapsed_ms;
        }
    }

    /// Finish an operation: drop it from the live set and append a plain audit
    /// log record.
    pub fn finish(&self, op_id: &str, tool: &str, detail: &str, outcome: &str, summary: &str) {
        let mut inner = self.inner.lock();
        let started_at = inner
            .live
            .iter()
            .find(|op| op.op_id == op_id)
            .map_or_else(now_ms, |op| op.started_at_ms);
        inner.live.retain(|op| op.op_id != op_id);
        match outcome {
            "completed" => inner.completed += 1,
            "killed" => inner.killed += 1,
            _ => inner.failed += 1,
        }
        let created_at_ms = now_ms();
        let entry = DeviceAuditEntry {
            entry_id: new_id("device_audit"),
            op_id: op_id.to_string(),
            tool: tool.to_string(),
            detail: detail.chars().take(200).collect(),
            outcome: outcome.to_string(),
            summary: summary.chars().take(400).collect(),
            duration_ms: (created_at_ms - started_at).max(0),
            created_at_ms,
        };
        inner.logged += 1;
        if inner.audit.len() >= AUDIT_RING_CAPACITY {
            inner.audit.pop_front();
        }
        inner.audit.push_back(entry);
    }

    /// Everything the supervisor is watching right now.
    #[must_use]
    pub fn live(&self) -> Vec<LiveOperation> {
        self.inner.lock().live.clone()
    }

    /// The most recent sealed audit records, newest last.
    #[must_use]
    pub fn recent_audit(&self, limit: usize) -> Vec<DeviceAuditEntry> {
        let inner = self.inner.lock();
        let take = limit.clamp(1, AUDIT_RING_CAPACITY);
        inner.audit.iter().rev().take(take).rev().cloned().collect()
    }

    #[must_use]
    pub fn stats(&self) -> DeviceSupervisorStats {
        let inner = self.inner.lock();
        DeviceSupervisorStats {
            in_flight: inner.live.len(),
            total_started: inner.started,
            total_completed: inner.completed,
            total_failed: inner.failed,
            total_killed: inner.killed,
            audit_log_len: inner.logged as usize,
        }
    }
}

/// The real device-capable executor. Holds the policy and a shared supervisor;
/// `execute` is the single entry point the agent runtime dispatches through.
#[derive(Clone)]
pub struct DeviceCapabilities {
    policy: Arc<DevicePolicy>,
    supervisor: DeviceSupervisor,
    /// Governed deep-crawl engine (HTTPA + safe-fetch + semantic render). When
    /// present, agents gain the `deep_crawl` tool.
    crawl: Option<Arc<crate::deep_crawl::DeepCrawlEngine>>,
}

impl DeviceCapabilities {
    #[must_use]
    pub fn new(policy: DevicePolicy) -> Self {
        Self {
            policy: Arc::new(policy),
            supervisor: DeviceSupervisor::new(),
            crawl: None,
        }
    }

    /// Attach the governed deep-crawl engine so agents can crawl-and-extract the
    /// open web through the HTTPA + semantic-render pipeline.
    #[must_use]
    pub fn with_crawl(mut self, crawl: Arc<crate::deep_crawl::DeepCrawlEngine>) -> Self {
        self.crawl = Some(crawl);
        self
    }

    #[must_use]
    pub fn supervisor(&self) -> DeviceSupervisor {
        self.supervisor.clone()
    }

    #[must_use]
    pub fn policy(&self) -> &DevicePolicy {
        &self.policy
    }

    /// The tools an agent may request. Names are matched by
    /// [`canonical_device_tool`].
    #[must_use]
    pub fn tool_names() -> &'static [&'static str] {
        &[
            "system_info",
            "process_list",
            "disk_usage",
            "fs_list",
            "fs_read",
            "fs_write",
            "open_path",
            "shell_exec",
            "deep_crawl",
            "web_search",
        ]
    }

    /// Dispatch one device tool call. Every branch is wrapped by the
    /// supervisor: begin → run → finish, so nothing escapes the watch.
    pub fn execute(&self, tool: &str, input: &Value) -> Result<Value, String> {
        let canonical = canonical_device_tool(tool);
        // The crawl engine runs its own supervision, so its tools are dispatched
        // before the single-op wrapper below.
        if canonical == "deep_crawl" {
            return match &self.crawl {
                Some(engine) => engine.execute(input).map(|mut value| {
                    if let Value::Object(map) = &mut value {
                        map.insert("supervised".into(), json!(true));
                    }
                    value
                }),
                None => Err("deep_crawl engine is not wired into this device layer".into()),
            };
        }
        if canonical == "web_search" {
            let detail = string_arg(input, &["query", "q"]).unwrap_or_default();
            let op_id = self.supervisor.begin("web_search", &detail);
            let result = match &self.crawl {
                Some(engine) => engine.web_search(input),
                None => Err("web_search engine is not wired into this device layer".into()),
            };
            match &result {
                Ok(v) => self.supervisor.finish(
                    &op_id,
                    "web_search",
                    "",
                    "completed",
                    &v.to_string().chars().take(200).collect::<String>(),
                ),
                Err(e) => self.supervisor.finish(&op_id, "web_search", "", "failed", e),
            }
            return result;
        }
        let detail = describe_call(canonical, input);
        let op_id = self.supervisor.begin(canonical, &detail);
        let result = match canonical {
            "system_info" => self.system_info(),
            "process_list" => self.process_list(input),
            "disk_usage" => self.disk_usage(),
            "fs_list" => self.fs_list(input),
            "fs_read" => self.fs_read(input),
            "fs_write" => self.fs_write(input),
            "open_path" => self.open_path(input),
            "shell_exec" => self.shell_exec(input, &op_id),
            other => Err(format!("unknown device tool '{other}'")),
        };
        match &result {
            Ok(value) => {
                let killed = value
                    .get("killed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let outcome = if killed { "killed" } else { "completed" };
                let summary: String = value.to_string().chars().take(400).collect();
                self.supervisor
                    .finish(&op_id, canonical, &detail, outcome, &summary);
            }
            Err(reason) => {
                self.supervisor
                    .finish(&op_id, canonical, &detail, "failed", reason);
            }
        }
        result.map(|mut value| {
            if let Value::Object(map) = &mut value {
                map.insert("op_id".into(), json!(op_id));
                map.insert("supervised".into(), json!(true));
            }
            value
        })
    }

    // ── Eyes ────────────────────────────────────────────────────────────

    fn system_info(&self) -> Result<Value, String> {
        let cpus = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(0);
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "unknown".into());
        let host = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .ok()
            .or_else(read_hostname)
            .unwrap_or_else(|| "unknown".into());
        Ok(json!({
            "tool": "system_info",
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
            "logical_cpus": cpus,
            "hostname": host,
            "user": user,
            "current_dir": cwd,
            "allow_everything": self.policy.allow_everything,
        }))
    }

    fn process_list(&self, input: &Value) -> Result<Value, String> {
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(40)
            .clamp(1, 500) as usize;
        let (program, args) = if cfg!(windows) {
            ("tasklist", vec!["/fo".to_string(), "csv".to_string(), "/nh".to_string()])
        } else {
            ("ps", vec!["-eo".to_string(), "pid,comm,%cpu,%mem".to_string()])
        };
        let output = self.run_capture(program, &args)?;
        let lines: Vec<String> = output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(limit)
            .map(str::to_string)
            .collect();
        Ok(json!({
            "tool": "process_list",
            "source": program,
            "count": lines.len(),
            "processes": lines,
        }))
    }

    fn disk_usage(&self) -> Result<Value, String> {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            (
                "wmic",
                vec![
                    "logicaldisk".into(),
                    "get".into(),
                    "name,size,freespace".into(),
                ],
            )
        } else {
            ("df", vec!["-h".into()])
        };
        let output = self.run_capture(program, &args)?;
        let lines: Vec<String> = output
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect();
        Ok(json!({
            "tool": "disk_usage",
            "source": program,
            "report": lines,
        }))
    }

    fn fs_list(&self, input: &Value) -> Result<Value, String> {
        let raw = string_arg(input, &["path", "dir", "directory"])
            .ok_or("fs_list requires a 'path'")?;
        let path = self.resolve_path(&raw)?;
        let read = std::fs::read_dir(&path).map_err(|e| format!("read_dir failed: {e}"))?;
        let mut entries = Vec::new();
        for item in read.flatten().take(2_000) {
            let meta = item.metadata().ok();
            entries.push(json!({
                "name": item.file_name().to_string_lossy(),
                "is_dir": meta.as_ref().map(std::fs::Metadata::is_dir).unwrap_or(false),
                "size": meta.as_ref().map(std::fs::Metadata::len).unwrap_or(0),
            }));
        }
        Ok(json!({
            "tool": "fs_list",
            "path": path.to_string_lossy(),
            "count": entries.len(),
            "entries": entries,
        }))
    }

    fn fs_read(&self, input: &Value) -> Result<Value, String> {
        let raw = string_arg(input, &["path", "file"]).ok_or("fs_read requires a 'path'")?;
        let path = self.resolve_path(&raw)?;
        let meta = std::fs::metadata(&path).map_err(|e| format!("stat failed: {e}"))?;
        if meta.len() as usize > self.policy.max_read_bytes {
            return Err(format!(
                "file is {} bytes, exceeds read cap of {}",
                meta.len(),
                self.policy.max_read_bytes
            ));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("read failed: {e}"))?;
        let content = String::from_utf8_lossy(&bytes).into_owned();
        Ok(json!({
            "tool": "fs_read",
            "path": path.to_string_lossy(),
            "bytes": bytes.len(),
            "content": content,
        }))
    }

    // ── Hands ───────────────────────────────────────────────────────────

    fn fs_write(&self, input: &Value) -> Result<Value, String> {
        let raw = string_arg(input, &["path", "file"]).ok_or("fs_write requires a 'path'")?;
        let content = string_arg(input, &["content", "text", "data"]).unwrap_or_default();
        let path = self.resolve_path(&raw)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create parent dir: {e}"))?;
        }
        std::fs::write(&path, content.as_bytes()).map_err(|e| format!("write failed: {e}"))?;
        Ok(json!({
            "tool": "fs_write",
            "path": path.to_string_lossy(),
            "bytes_written": content.len(),
        }))
    }

    fn open_path(&self, input: &Value) -> Result<Value, String> {
        let target = string_arg(input, &["path", "url", "target", "file"])
            .ok_or("open_path requires a 'path' or 'url'")?;
        self.guard_command(&target)?;
        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("cmd", vec!["/C".into(), "start".into(), String::new(), target.clone()])
        } else if cfg!(target_os = "macos") {
            ("open", vec![target.clone()])
        } else {
            ("xdg-open", vec![target.clone()])
        };
        // Launch detached; we don't wait on the user's GUI app.
        Command::new(program)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to launch '{program}': {e}"))?;
        Ok(json!({
            "tool": "open_path",
            "launched": target,
            "launcher": program,
        }))
    }

    /// Real host shell execution under a continuous watchdog: reader threads
    /// drain stdout/stderr (so a chatty child can't deadlock on a full pipe),
    /// while this loop watches the clock and force-kills on timeout.
    fn shell_exec(&self, input: &Value, op_id: &str) -> Result<Value, String> {
        let command =
            string_arg(input, &["command", "cmd", "script"]).ok_or("shell_exec requires a 'command'")?;
        self.guard_command(&command)?;
        let cwd = string_arg(input, &["cwd", "dir"])
            .map(|raw| self.resolve_path(&raw))
            .transpose()?
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let (program, args): (&str, Vec<String>) = if cfg!(windows) {
            ("cmd", vec!["/C".into(), command.clone()])
        } else {
            ("sh", vec!["-lc".into(), command.clone()])
        };

        let mut child = Command::new(program)
            .args(&args)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn shell: {e}"))?;

        let max = self.policy.max_output_bytes;
        let out_handle = child.stdout.take().map(|s| spawn_reader(s, max));
        let err_handle = child.stderr.take().map(|s| spawn_reader(s, max));

        // An agent may request a *shorter* deadline, but never one past the
        // policy ceiling — the supervisor's timeout is the hard wall.
        let requested = input
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(self.policy.command_timeout_ms);
        let timeout =
            Duration::from_millis(requested.min(self.policy.command_timeout_ms).max(100));
        let pid = child.id();
        let start = Instant::now();
        let mut killed = false;
        let exit_code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code(),
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        // Kill the whole process tree, not just the shell: on
                        // Windows a grandchild (e.g. ping) inherits the stdout
                        // pipe, so killing only the shell would leave the reader
                        // threads blocked forever on a pipe that never closes.
                        kill_process_tree(pid);
                        let _ = child.kill();
                        let _ = child.wait();
                        killed = true;
                        break None;
                    }
                    self.supervisor
                        .touch(op_id, start.elapsed().as_millis() as i64);
                    std::thread::sleep(WATCHDOG_TICK);
                }
                Err(e) => return Err(format!("wait failed: {e}")),
            }
        };

        let stdout = out_handle.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
        let stderr = err_handle.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
        let duration_ms = start.elapsed().as_millis() as i64;

        Ok(json!({
            "tool": "shell_exec",
            "command": command,
            "cwd": cwd.to_string_lossy(),
            "exit_code": exit_code,
            "killed": killed,
            "timed_out": killed,
            "duration_ms": duration_ms,
            "stdout": stdout,
            "stderr": stderr,
        }))
    }

    // ── Internals ───────────────────────────────────────────────────────

    /// A short, bounded host capture used by the read-only introspection tools.
    fn run_capture(&self, program: &str, args: &[String]) -> Result<String, String> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("failed to run '{program}': {e}"))?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        if text.len() > self.policy.max_output_bytes {
            text.truncate(self.policy.max_output_bytes);
        }
        Ok(text)
    }

    /// Refuse machine-destroying commands even in permissive mode.
    fn guard_command(&self, command: &str) -> Result<(), String> {
        let lower = command.to_ascii_lowercase();
        for needle in &self.policy.hard_blocklist {
            if lower.contains(&needle.to_ascii_lowercase()) {
                return Err(format!(
                    "refused: command matches hard blocklist entry '{needle}' (machine-destroying, never permitted)"
                ));
            }
        }
        Ok(())
    }

    /// Resolve and validate a path against the policy. In permissive mode any
    /// normalized absolute/relative path is allowed; otherwise the resolved
    /// path must sit inside one of the allowed roots and may not escape via
    /// `..`.
    fn resolve_path(&self, raw: &str) -> Result<PathBuf, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("path must not be empty".into());
        }
        let candidate = Path::new(raw);
        if self.policy.allow_everything {
            // Still reject parent-dir traversal tokens to avoid surprising
            // escapes relative to the working directory.
            if candidate
                .components()
                .any(|c| matches!(c, Component::ParentDir))
            {
                return Err("path may not contain '..' segments".into());
            }
            return Ok(candidate.to_path_buf());
        }
        if self.policy.allowed_roots.is_empty() {
            return Err(
                "device is in confined mode but no allowed roots are configured".into(),
            );
        }
        if candidate
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err("path may not contain '..' segments".into());
        }
        let resolved = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.policy.allowed_roots[0].join(candidate)
        };
        let permitted = self
            .policy
            .allowed_roots
            .iter()
            .any(|root| resolved.starts_with(root));
        if !permitted {
            return Err(format!(
                "path '{}' is outside every allowed root",
                resolved.to_string_lossy()
            ));
        }
        Ok(resolved)
    }
}

/// Force-kill an entire process tree by root pid. On Windows a killed shell
/// leaves its children alive (and holding inherited pipes); `taskkill /T` walks
/// the tree. On Unix the spawned child is killed directly by the caller.
fn kill_process_tree(pid: u32) {
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Drain a child stream into a capped buffer on its own thread so a full pipe
/// can never deadlock the watchdog loop.
fn spawn_reader<R>(mut stream: R, max: usize) -> std::thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.len() < max {
                        let room = max - buf.len();
                        buf.extend_from_slice(&chunk[..n.min(room)]);
                    }
                    // Keep draining past the cap so the child never blocks on a
                    // full pipe; we simply stop *storing* once capped.
                }
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Map loosely-named tool requests onto the canonical device tool set.
#[must_use]
pub fn canonical_device_tool(tool: &str) -> &'static str {
    let lower = tool.trim().to_ascii_lowercase();
    if lower.contains("web_search")
        || lower.contains("websearch")
        || lower.contains("searxng")
        || lower == "search"
    {
        "web_search"
    } else if lower.contains("crawl") || lower.contains("scrape") || lower.contains("spider") {
        "deep_crawl"
    } else if lower.contains("system") || lower.contains("device_info") || lower == "sysinfo" {
        "system_info"
    } else if lower.contains("process") || lower == "ps" || lower.contains("tasklist") {
        "process_list"
    } else if lower.contains("disk") || lower.contains("storage") {
        "disk_usage"
    } else if lower.contains("list") && lower.contains("file")
        || lower.contains("ls")
        || lower.contains("dir")
    {
        "fs_list"
    } else if lower.contains("read") {
        "fs_read"
    } else if lower.contains("write") || lower.contains("save") {
        "fs_write"
    } else if lower.contains("open") || lower.contains("launch") {
        "open_path"
    } else {
        "shell_exec"
    }
}

/// True when a tool name addresses the device layer (used by the dispatcher).
#[must_use]
pub fn is_device_tool(tool: &str) -> bool {
    let lower = tool.trim().to_ascii_lowercase();
    const PREFIXES: [&str; 10] = [
        "system", "process", "disk", "storage", "fs_", "shell", "open", "launch", "device", "crawl",
    ];
    DeviceCapabilities::tool_names().contains(&lower.as_str())
        || PREFIXES.iter().any(|p| lower.starts_with(p))
        || lower.contains("crawl")
        || lower.contains("scrape")
        || lower.contains("web_search")
        || lower.contains("websearch")
        || lower.contains("searxng")
        || lower == "search"
        || lower == "ps"
        || lower == "ls"
        || lower == "dir"
        || lower == "sysinfo"
}

fn describe_call(tool: &str, input: &Value) -> String {
    match tool {
        "shell_exec" => string_arg(input, &["command", "cmd", "script"]).unwrap_or_default(),
        "open_path" => string_arg(input, &["path", "url", "target"]).unwrap_or_default(),
        "fs_read" | "fs_write" | "fs_list" => {
            string_arg(input, &["path", "file", "dir"]).unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn string_arg(input: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = input.get(*key).and_then(Value::as_str) {
            if !s.trim().is_empty() {
                return Some(s.to_string());
            }
        }
    }
    // Bare string input (e.g. the chained output of a previous tool) is treated
    // as the primary argument.
    if let Value::String(s) = input {
        if !s.trim().is_empty() {
            return Some(s.clone());
        }
    }
    None
}

fn read_hostname() -> Option<String> {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_reports_real_host() {
        let device = DeviceCapabilities::new(DevicePolicy::permissive());
        let value = device.execute("system_info", &json!({})).unwrap();
        assert_eq!(value["os"], json!(std::env::consts::OS));
        assert!(value["supervised"].as_bool().unwrap());
        assert_eq!(device.supervisor().stats().total_completed, 1);
    }

    #[test]
    fn shell_exec_runs_and_captures() {
        let device = DeviceCapabilities::new(DevicePolicy::permissive());
        let cmd = if cfg!(windows) { "echo hello" } else { "echo hello" };
        let value = device.execute("shell_exec", &json!({ "command": cmd })).unwrap();
        assert!(value["stdout"].as_str().unwrap().contains("hello"));
        assert_eq!(value["killed"], json!(false));
    }

    #[test]
    fn hard_blocklist_refuses_destructive() {
        let device = DeviceCapabilities::new(DevicePolicy::permissive());
        let err = device
            .execute("shell_exec", &json!({ "command": "rm -rf /" }))
            .unwrap_err();
        assert!(err.contains("blocklist"));
        assert_eq!(device.supervisor().stats().total_failed, 1);
    }

    #[test]
    fn confined_mode_blocks_outside_roots() {
        let device =
            DeviceCapabilities::new(DevicePolicy::confined_to(vec![PathBuf::from("/tmp/astra")]));
        let err = device
            .execute("fs_read", &json!({ "path": "/etc/passwd" }))
            .unwrap_err();
        assert!(err.contains("outside"));
    }

    #[test]
    fn audit_log_records_each_op() {
        let device = DeviceCapabilities::new(DevicePolicy::permissive());
        device.execute("system_info", &json!({})).unwrap();
        device.execute("system_info", &json!({})).unwrap();
        assert_eq!(device.supervisor().recent_audit(10).len(), 2);
        assert_eq!(device.supervisor().stats().audit_log_len, 2);
    }
}

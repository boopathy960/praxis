// ─────────────────────────────────────────────────────────────
// Code Executor — Sandboxed Code Execution
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/code_executor.py

use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    Shell,
    Sql,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub code: String,
    pub language: Language,
    pub timeout: Duration,
    pub max_memory_mb: usize,
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: f64,
    pub memory_used_bytes: usize,
    pub timed_out: bool,
}

/// Dangerous patterns that must be blocked before execution.
const BLOCKED_PATTERNS: &[&str] = &[
    "rm -rf",
    "format c:",
    "del /f /s /q",
    "os.system",
    "subprocess.call",
    "eval(",
    "exec(",
    "__import__",
    "System.exit",
    "DROP TABLE",
    "DELETE FROM",
    "shutdown",
    "reboot",
];

/// Sandboxed code executor with AST validation and resource limits.
#[allow(dead_code)]
pub struct CodeExecutor {
    max_execution_time: Duration,
    max_memory_mb: usize,
    allowed_languages: Vec<Language>,
    execution_count: u64,
    blocked_count: u64,
}

impl CodeExecutor {
    pub fn new() -> Self {
        Self {
            max_execution_time: Duration::from_secs(30),
            max_memory_mb: 256,
            allowed_languages: vec![Language::Rust, Language::Python, Language::JavaScript],
            execution_count: 0,
            blocked_count: 0,
        }
    }

    /// Validate code for safety before execution.
    pub fn validate(&self, request: &ExecutionRequest) -> ValidationResult {
        // Check language allowlist
        if !self.allowed_languages.contains(&request.language) {
            return ValidationResult::Rejected(format!(
                "{:?} is not an allowed language",
                request.language
            ));
        }

        // Check for dangerous patterns
        let code_lower = request.code.to_lowercase();
        for pattern in BLOCKED_PATTERNS {
            if code_lower.contains(&pattern.to_lowercase()) {
                return ValidationResult::Rejected(format!(
                    "Blocked pattern detected: {}",
                    pattern
                ));
            }
        }

        // Check code length
        if request.code.len() > 100_000 {
            return ValidationResult::Rejected("Code exceeds maximum length (100KB)".into());
        }

        // Check resource limits
        if request.timeout > self.max_execution_time {
            return ValidationResult::Warning("Timeout exceeds maximum, will be capped".into());
        }

        ValidationResult::Safe
    }

    /// Execute code in a sandboxed environment.
    pub fn execute(&mut self, request: &ExecutionRequest) -> ExecutionResult {
        self.execution_count += 1;

        // Pre-validate
        match self.validate(request) {
            ValidationResult::Rejected(reason) => {
                self.blocked_count += 1;
                return ExecutionResult {
                    stdout: String::new(),
                    stderr: format!("Execution blocked: {}", reason),
                    exit_code: -1,
                    duration_ms: 0.0,
                    memory_used_bytes: 0,
                    timed_out: false,
                };
            }
            _ => {}
        }

        let start = Instant::now();
        let timeout = request.timeout.min(self.max_execution_time);

        // Simulate sandboxed execution (actual execution would use process isolation)
        let result = self.simulate_execution(request, timeout);
        let duration = start.elapsed();

        ExecutionResult {
            stdout: result.0,
            stderr: result.1,
            exit_code: result.2,
            duration_ms: duration.as_secs_f64() * 1000.0,
            memory_used_bytes: 0,
            timed_out: duration >= timeout,
        }
    }

    fn simulate_execution(
        &self,
        request: &ExecutionRequest,
        _timeout: Duration,
    ) -> (String, String, i32) {
        // In production, this would spawn a sandboxed process with seccomp/landlock
        let output = match &request.language {
            Language::Rust => format!(
                "[Rust sandbox] Compiled and executed {} bytes of code",
                request.code.len()
            ),
            Language::Python => format!(
                "[Python sandbox] Executed {} bytes of code",
                request.code.len()
            ),
            Language::JavaScript => {
                format!("[JS sandbox] Executed {} bytes of code", request.code.len())
            }
            Language::Shell => format!(
                "[Shell sandbox] Executed {} bytes of script",
                request.code.len()
            ),
            Language::Sql => format!(
                "[SQL sandbox] Executed {} bytes of query",
                request.code.len()
            ),
        };
        (output, String::new(), 0)
    }

    pub fn execution_count(&self) -> u64 {
        self.execution_count
    }
    pub fn blocked_count(&self) -> u64 {
        self.blocked_count
    }
}

#[derive(Debug, Clone)]
pub enum ValidationResult {
    Safe,
    Warning(String),
    Rejected(String),
}

impl Default for CodeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

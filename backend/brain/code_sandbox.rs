// ─────────────────────────────────────────────────────────────
// Code Sandbox — AST-Validated Safe Code Execution
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/code_execution_sandbox.py

use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct SandboxResult {
    pub output: String,
    pub success: bool,
    pub execution_time_ms: f64,
    pub safety_score: f64,
    pub violations: Vec<String>,
    pub allowed: bool,
}

/// Code Sandbox — validates and sandboxes code execution.
#[allow(dead_code)]
pub struct CodeSandbox {
    forbidden_imports: HashSet<String>,
    forbidden_builtins: HashSet<String>,
    forbidden_patterns: Vec<(String, String)>,
    max_execution_time_ms: f64,
    max_output_length: usize,
}

impl CodeSandbox {
    pub fn new() -> Self {
        let mut forbidden_imports = HashSet::new();
        for module in &[
            "os",
            "sys",
            "subprocess",
            "shutil",
            "socket",
            "http",
            "urllib",
            "ftplib",
            "smtplib",
            "ctypes",
            "importlib",
            "signal",
            "threading",
            "multiprocessing",
            "pickle",
            "shelve",
            "sqlite3",
            "io",
        ] {
            forbidden_imports.insert(module.to_string());
        }

        let mut forbidden_builtins = HashSet::new();
        for builtin in &[
            "eval",
            "exec",
            "compile",
            "__import__",
            "globals",
            "locals",
            "getattr",
            "setattr",
            "delattr",
            "open",
            "input",
        ] {
            forbidden_builtins.insert(builtin.to_string());
        }

        let forbidden_patterns = vec![
            ("rm -rf".into(), "Filesystem destruction".into()),
            ("chmod".into(), "Permission modification".into()),
            ("kill".into(), "Process termination".into()),
            ("dd if=".into(), "Disk operation".into()),
            ("mkfs".into(), "Filesystem format".into()),
            ("curl |".into(), "Remote code execution".into()),
            ("wget".into(), "Remote download".into()),
            ("nc ".into(), "Netcat usage".into()),
        ];

        Self {
            forbidden_imports,
            forbidden_builtins,
            forbidden_patterns,
            max_execution_time_ms: 5000.0,
            max_output_length: 10000,
        }
    }

    /// Validate code safety before execution.
    pub fn validate(&self, code: &str) -> SandboxResult {
        let start = Instant::now();
        let mut violations = Vec::new();
        let code_lower = code.to_lowercase();

        // Check forbidden imports
        for module in &self.forbidden_imports {
            if code.contains(&format!("import {}", module))
                || code.contains(&format!("from {} import", module))
                || code.contains(&format!("use {};", module))
                || code.contains(&format!("use {}::", module))
            {
                violations.push(format!("Forbidden import: {}", module));
            }
        }

        // Check forbidden builtins
        for builtin in &self.forbidden_builtins {
            if code.contains(&format!("{}(", builtin)) {
                violations.push(format!("Forbidden builtin: {}", builtin));
            }
        }

        // Check forbidden patterns
        for (pattern, description) in &self.forbidden_patterns {
            if code_lower.contains(&pattern.to_lowercase()) {
                violations.push(format!("{}: '{}'", description, pattern));
            }
        }

        // Check for infinite loops (simple heuristic)
        if code.contains("while True") || code.contains("while true") || code.contains("loop {") {
            if !code.contains("break") && !code.contains("return") {
                violations.push("Potential infinite loop detected (no break/return)".into());
            }
        }

        // Check for network operations
        if code_lower.contains("connect(")
            || code_lower.contains("listen(")
            || code_lower.contains("bind(")
            || code_lower.contains("accept(")
        {
            violations.push("Network operation detected".into());
        }

        let is_clean = violations.is_empty();
        let safety_score = if is_clean {
            1.0
        } else {
            1.0 - (violations.len() as f64 * 0.2).min(1.0)
        };

        SandboxResult {
            output: String::new(),
            success: is_clean,
            execution_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            safety_score,
            violations,
            allowed: is_clean,
        }
    }

    /// Check if code is safe to execute.
    pub fn is_safe(&self, code: &str) -> bool {
        self.validate(code).allowed
    }

    /// Sanitize code by removing dangerous constructs.
    pub fn sanitize(&self, code: &str) -> String {
        let mut sanitized = code.to_string();

        for module in &self.forbidden_imports {
            sanitized = sanitized.replace(
                &format!("import {}", module),
                &format!("# BLOCKED: import {}", module),
            );
        }

        for builtin in &self.forbidden_builtins {
            sanitized = sanitized.replace(
                &format!("{}(", builtin),
                &format!("# BLOCKED: {}(", builtin),
            );
        }

        sanitized
    }

    /// Get security configuration.
    pub fn config_summary(&self) -> HashMap<String, usize> {
        let mut config = HashMap::new();
        config.insert("forbidden_imports".into(), self.forbidden_imports.len());
        config.insert("forbidden_builtins".into(), self.forbidden_builtins.len());
        config.insert("forbidden_patterns".into(), self.forbidden_patterns.len());
        config
    }
}

impl Default for CodeSandbox {
    fn default() -> Self {
        Self::new()
    }
}

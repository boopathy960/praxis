// ─────────────────────────────────────────────────────────────
// Phantom Sandbox — Virtual Pre-Execution Simulation
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/phantom_sandbox.py
// Simulates actions before execution to detect risks.

#[derive(Debug, Clone, PartialEq)]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Safe => "safe",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn score(&self) -> f64 {
        match self {
            Self::Safe => 0.0,
            Self::Low => 0.2,
            Self::Medium => 0.5,
            Self::High => 0.8,
            Self::Critical => 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub score: f64,
    pub findings: Vec<RiskFinding>,
    pub recommendation: String,
    pub allow_execution: bool,
}

#[derive(Debug, Clone)]
pub struct RiskFinding {
    pub category: String,
    pub description: String,
    pub severity: RiskLevel,
    pub pattern_matched: String,
}

/// Phantom Sandbox — pre-execution risk simulation.
pub struct PhantomSandbox {
    dangerous_patterns: Vec<(&'static str, &'static str, RiskLevel)>,
    network_patterns: Vec<(&'static str, &'static str)>,
    data_loss_patterns: Vec<(&'static str, &'static str)>,
}

impl PhantomSandbox {
    pub fn new() -> Self {
        Self {
            dangerous_patterns: vec![
                ("rm -rf /", "Recursive root deletion", RiskLevel::Critical),
                ("rm -rf", "Recursive forced deletion", RiskLevel::High),
                ("dd if=/dev/zero", "Disk overwrite", RiskLevel::Critical),
                ("mkfs", "Filesystem format", RiskLevel::Critical),
                (":(){:|:&};:", "Fork bomb", RiskLevel::Critical),
                ("chmod 777", "World-writable permissions", RiskLevel::High),
                (
                    "chmod -R 777",
                    "Recursive world-writable",
                    RiskLevel::Critical,
                ),
                ("eval(", "Dynamic code evaluation", RiskLevel::High),
                ("exec(", "Dynamic code execution", RiskLevel::High),
                ("__import__", "Dynamic import", RiskLevel::Medium),
                ("subprocess.call", "Subprocess execution", RiskLevel::Medium),
                ("os.system", "System command execution", RiskLevel::Medium),
                ("DROP TABLE", "SQL table deletion", RiskLevel::Critical),
                ("DELETE FROM", "SQL data deletion", RiskLevel::High),
                (
                    "TRUNCATE TABLE",
                    "SQL table truncation",
                    RiskLevel::Critical,
                ),
                ("format c:", "Disk format (Windows)", RiskLevel::Critical),
                (
                    "del /f /s /q",
                    "Forced recursive delete (Windows)",
                    RiskLevel::Critical,
                ),
                ("sudo rm", "Privileged deletion", RiskLevel::High),
                ("sudo dd", "Privileged disk operation", RiskLevel::Critical),
                ("curl | bash", "Remote code execution", RiskLevel::Critical),
                ("wget | sh", "Remote code execution", RiskLevel::Critical),
            ],
            network_patterns: vec![
                ("socket.connect", "Network connection"),
                ("requests.get", "HTTP request"),
                ("urllib.urlopen", "URL access"),
                ("http.client", "HTTP client"),
                ("smtplib", "Email sending"),
                ("ftplib", "FTP access"),
            ],
            data_loss_patterns: vec![
                ("shutil.rmtree", "Directory tree removal"),
                ("os.remove", "File removal"),
                ("os.unlink", "File unlinking"),
                ("pathlib.Path.unlink", "Path unlinking"),
                ("truncate", "File truncation"),
            ],
        }
    }

    /// Simulate execution and assess risk of a code snippet or command.
    pub fn assess_risk(&self, code: &str) -> RiskAssessment {
        let code_lower = code.to_lowercase();
        let mut findings = Vec::new();
        let mut max_risk = RiskLevel::Safe;

        // Check dangerous patterns
        for (pattern, desc, severity) in &self.dangerous_patterns {
            if code_lower.contains(&pattern.to_lowercase()) {
                findings.push(RiskFinding {
                    category: "dangerous_operation".into(),
                    description: desc.to_string(),
                    severity: severity.clone(),
                    pattern_matched: pattern.to_string(),
                });
                if severity.score() > max_risk.score() {
                    max_risk = severity.clone();
                }
            }
        }

        // Check network patterns
        for (pattern, desc) in &self.network_patterns {
            if code_lower.contains(&pattern.to_lowercase()) {
                findings.push(RiskFinding {
                    category: "network_access".into(),
                    description: desc.to_string(),
                    severity: RiskLevel::Medium,
                    pattern_matched: pattern.to_string(),
                });
                if max_risk.score() < RiskLevel::Medium.score() {
                    max_risk = RiskLevel::Medium;
                }
            }
        }

        // Check data loss patterns
        for (pattern, desc) in &self.data_loss_patterns {
            if code_lower.contains(&pattern.to_lowercase()) {
                findings.push(RiskFinding {
                    category: "data_loss".into(),
                    description: desc.to_string(),
                    severity: RiskLevel::High,
                    pattern_matched: pattern.to_string(),
                });
                if max_risk.score() < RiskLevel::High.score() {
                    max_risk = RiskLevel::High;
                }
            }
        }

        let recommendation = match &max_risk {
            RiskLevel::Safe => "Operation appears safe to execute.".into(),
            RiskLevel::Low => "Low risk. Proceed with standard precautions.".into(),
            RiskLevel::Medium => {
                "Medium risk. Review before execution. Consider sandboxing.".into()
            }
            RiskLevel::High => "HIGH RISK. Manual review required. Do NOT auto-execute.".into(),
            RiskLevel::Critical => {
                "CRITICAL RISK. BLOCKED. This operation could cause irreversible damage.".into()
            }
        };

        let allow = matches!(max_risk, RiskLevel::Safe | RiskLevel::Low);

        RiskAssessment {
            score: max_risk.score(),
            level: max_risk,
            findings,
            recommendation,
            allow_execution: allow,
        }
    }

    /// Quick check if code is safe to execute.
    pub fn is_safe(&self, code: &str) -> bool {
        let assessment = self.assess_risk(code);
        assessment.allow_execution
    }
}

impl Default for PhantomSandbox {
    fn default() -> Self {
        Self::new()
    }
}

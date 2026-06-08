// ─────────────────────────────────────────────────────────────
// Tool Registry — Central Tool Management
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/registry.py

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub risk_level: RiskLevel,
    pub requires_approval: bool,
    pub max_calls_per_minute: u32,
    pub timeout: Duration,
    call_count: u32,
    last_reset: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCategory {
    Search,
    FileSystem,
    CodeExecution,
    DataAnalysis,
    Security,
    Git,
    Math,
    Research,
    General,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskLevel {
    Safe,     // Read-only operations
    Moderate, // Reversible mutations
    High,     // Irreversible mutations
    Critical, // System-level actions
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub category: String,
    pub risk_level: String,
    pub requires_approval: bool,
    pub max_calls_per_minute: u32,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRegistrySnapshot {
    pub total_tools: usize,
    pub blocked_tools: Vec<String>,
    pub total_calls: u64,
    pub failure_rate: f64,
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: f64,
    pub error: Option<String>,
}

/// Central registry for all available tools with policy enforcement.
pub struct ToolRegistry {
    tools: HashMap<String, ToolEntry>,
    builtin_tools: HashSet<String>,
    blocked_tools: Vec<String>,
    audit_log: Vec<ToolAuditEntry>,
    total_calls: u64,
    total_failures: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ToolAuditEntry {
    tool_name: String,
    timestamp: Instant,
    success: bool,
    duration_ms: f64,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
            builtin_tools: HashSet::new(),
            blocked_tools: vec![
                "system_shutdown".into(),
                "disk_format".into(),
                "rm_rf".into(),
            ],
            audit_log: Vec::new(),
            total_calls: 0,
            total_failures: 0,
        };
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        let defaults = vec![
            (
                "web_search",
                "Search the web",
                ToolCategory::Search,
                RiskLevel::Safe,
                30,
            ),
            (
                "deep_research",
                "Deep multi-source research",
                ToolCategory::Research,
                RiskLevel::Safe,
                10,
            ),
            (
                "file_read",
                "Read file contents",
                ToolCategory::FileSystem,
                RiskLevel::Safe,
                60,
            ),
            (
                "file_write",
                "Write file contents",
                ToolCategory::FileSystem,
                RiskLevel::Moderate,
                30,
            ),
            (
                "file_delete",
                "Delete files",
                ToolCategory::FileSystem,
                RiskLevel::High,
                10,
            ),
            (
                "code_execute",
                "Execute code in sandbox",
                ToolCategory::CodeExecution,
                RiskLevel::Moderate,
                20,
            ),
            (
                "data_analyze",
                "Analyze datasets",
                ToolCategory::DataAnalysis,
                RiskLevel::Safe,
                20,
            ),
            (
                "git_commit",
                "Git commit changes",
                ToolCategory::Git,
                RiskLevel::Moderate,
                20,
            ),
            (
                "git_push",
                "Git push changes",
                ToolCategory::Git,
                RiskLevel::High,
                10,
            ),
            (
                "calculate",
                "Mathematical computation",
                ToolCategory::Math,
                RiskLevel::Safe,
                60,
            ),
            (
                "web_fetch",
                "Fetch web resources for assistant-side analysis",
                ToolCategory::General,
                RiskLevel::Safe,
                40,
            ),
            (
                "voice_transcribe",
                "Transcribe local speech input with the CPU voice runtime",
                ToolCategory::General,
                RiskLevel::Safe,
                24,
            ),
            (
                "voice_listen",
                "Capture microphone audio for a delegated assistant turn",
                ToolCategory::General,
                RiskLevel::Moderate,
                12,
            ),
            (
                "voice_speak",
                "Speak an assistant response through the local device voice",
                ToolCategory::General,
                RiskLevel::Safe,
                24,
            ),
            (
                "content_extract",
                "Extract normalized page content for summarization and replay",
                ToolCategory::General,
                RiskLevel::Safe,
                30,
            ),
            (
                "workflow_forge",
                "Assemble reusable backend workflows and automations",
                ToolCategory::CodeExecution,
                RiskLevel::Moderate,
                15,
            ),
            (
                "evidence_notary",
                "Bind citations and evidence to auditable outputs",
                ToolCategory::Research,
                RiskLevel::Moderate,
                20,
            ),
            (
                "session_memory",
                "Store mission context for durable assistant recovery",
                ToolCategory::DataAnalysis,
                RiskLevel::Safe,
                30,
            ),
            (
                "task_scheduler",
                "Queue refresh and maintenance tasks",
                ToolCategory::General,
                RiskLevel::Moderate,
                12,
            ),
            (
                "risk_review",
                "Review risky actions before irreversible execution",
                ToolCategory::Security,
                RiskLevel::Moderate,
                20,
            ),
            (
                "threat_scan",
                "Scan for security threats",
                ToolCategory::Security,
                RiskLevel::Safe,
                10,
            ),
            (
                "threat_neutralize",
                "Neutralize detected threats",
                ToolCategory::Security,
                RiskLevel::Critical,
                5,
            ),
        ];

        for (name, desc, cat, risk, max_rpm) in defaults {
            self.register(name, desc, cat, risk, max_rpm);
            self.builtin_tools.insert(name.to_string());
        }
    }

    /// Register a new tool.
    pub fn register(
        &mut self,
        name: &str,
        description: &str,
        category: ToolCategory,
        risk_level: RiskLevel,
        max_rpm: u32,
    ) {
        let requires_approval = matches!(risk_level, RiskLevel::High | RiskLevel::Critical);
        self.tools.insert(
            name.to_string(),
            ToolEntry {
                name: name.to_string(),
                description: description.to_string(),
                category,
                risk_level,
                requires_approval,
                max_calls_per_minute: max_rpm,
                timeout: Duration::from_secs(30),
                call_count: 0,
                last_reset: Instant::now(),
            },
        );
    }

    pub fn register_custom_descriptor(&mut self, descriptor: ToolDescriptor) {
        let category = match descriptor.category.as_str() {
            "search" => ToolCategory::Search,
            "filesystem" => ToolCategory::FileSystem,
            "code_execution" => ToolCategory::CodeExecution,
            "data_analysis" => ToolCategory::DataAnalysis,
            "security" => ToolCategory::Security,
            "git" => ToolCategory::Git,
            "math" => ToolCategory::Math,
            "research" => ToolCategory::Research,
            _ => ToolCategory::General,
        };
        let risk_level = match descriptor.risk_level.as_str() {
            "moderate" => RiskLevel::Moderate,
            "high" => RiskLevel::High,
            "critical" => RiskLevel::Critical,
            _ => RiskLevel::Safe,
        };
        self.tools.insert(
            descriptor.name.clone(),
            ToolEntry {
                name: descriptor.name,
                description: descriptor.description,
                category,
                risk_level,
                requires_approval: descriptor.requires_approval,
                max_calls_per_minute: descriptor.max_calls_per_minute,
                timeout: Duration::from_secs(descriptor.timeout_secs.max(1)),
                call_count: 0,
                last_reset: Instant::now(),
            },
        );
    }

    pub fn remove_custom_tool(&mut self, name: &str) -> Result<bool, String> {
        let normalized = name.trim();
        if self.builtin_tools.contains(normalized) {
            return Err("built-in tools cannot be removed".into());
        }
        Ok(self.tools.remove(normalized).is_some())
    }

    /// Check if a tool call is allowed (rate limits + blocklist).
    pub fn check_permission(&mut self, tool_name: &str) -> ToolPermission {
        if self.blocked_tools.iter().any(|b| b == tool_name) {
            return ToolPermission::Blocked(format!("Tool '{}' is permanently blocked", tool_name));
        }
        if let Some(entry) = self.tools.get_mut(tool_name) {
            // Reset counter every minute
            if entry.last_reset.elapsed() >= Duration::from_secs(60) {
                entry.call_count = 0;
                entry.last_reset = Instant::now();
            }
            if entry.call_count >= entry.max_calls_per_minute {
                return ToolPermission::RateLimited(entry.max_calls_per_minute);
            }
            if entry.requires_approval {
                return ToolPermission::RequiresApproval;
            }
            entry.call_count += 1;
            ToolPermission::Allowed
        } else {
            ToolPermission::NotFound
        }
    }

    /// Record a tool execution result.
    pub fn record_result(&mut self, result: &ToolResult) {
        self.total_calls += 1;
        if !result.success {
            self.total_failures += 1;
        }
        self.audit_log.push(ToolAuditEntry {
            tool_name: result.tool_name.clone(),
            timestamp: Instant::now(),
            success: result.success,
            duration_ms: result.duration_ms,
        });
        // Keep audit log bounded
        if self.audit_log.len() > 10_000 {
            self.audit_log.drain(0..5_000);
        }
    }

    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.get(name)
    }
    pub fn list(&self) -> Vec<&ToolEntry> {
        self.tools.values().collect()
    }
    pub fn total_calls(&self) -> u64 {
        self.total_calls
    }
    pub fn failure_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            self.total_failures as f64 / self.total_calls as f64
        }
    }

    pub fn catalog(&self) -> Vec<ToolDescriptor> {
        let mut tools = self
            .tools
            .values()
            .map(|entry| ToolDescriptor {
                name: entry.name.clone(),
                description: entry.description.clone(),
                category: entry.category.as_str().to_string(),
                risk_level: entry.risk_level.as_str().to_string(),
                requires_approval: entry.requires_approval,
                max_calls_per_minute: entry.max_calls_per_minute,
                timeout_secs: entry.timeout.as_secs(),
            })
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    pub fn snapshot(&self) -> ToolRegistrySnapshot {
        ToolRegistrySnapshot {
            total_tools: self.tools.len(),
            blocked_tools: self.blocked_tools.clone(),
            total_calls: self.total_calls(),
            failure_rate: self.failure_rate(),
            tools: self.catalog(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ToolPermission {
    Allowed,
    Blocked(String),
    RateLimited(u32),
    RequiresApproval,
    NotFound,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::FileSystem => "filesystem",
            Self::CodeExecution => "code_execution",
            Self::DataAnalysis => "data_analysis",
            Self::Security => "security",
            Self::Git => "git",
            Self::Math => "math",
            Self::Research => "research",
            Self::General => "general",
        }
    }
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Moderate => "moderate",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolDescriptor, ToolRegistry};

    #[test]
    fn snapshot_exposes_builtin_backend_tools() {
        let registry = ToolRegistry::new();
        let snapshot = registry.snapshot();

        assert!(snapshot.total_tools >= 18);
        assert!(snapshot.tools.iter().any(|tool| tool.name == "web_fetch"));
        assert!(snapshot
            .tools
            .iter()
            .any(|tool| tool.name == "workflow_forge"));
    }

    #[test]
    fn custom_tool_can_be_registered_and_removed() {
        let mut registry = ToolRegistry::new();
        registry.register_custom_descriptor(ToolDescriptor {
            name: "mission_patch".into(),
            description: "Apply bounded mission patches".into(),
            category: "code_execution".into(),
            risk_level: "moderate".into(),
            requires_approval: true,
            max_calls_per_minute: 4,
            timeout_secs: 45,
        });

        assert!(registry
            .catalog()
            .iter()
            .any(|tool| tool.name == "mission_patch"));
        assert!(registry
            .remove_custom_tool("mission_patch")
            .expect("custom tool should be removable"));
        assert!(!registry
            .catalog()
            .iter()
            .any(|tool| tool.name == "mission_patch"));
    }
}

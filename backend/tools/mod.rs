// ─────────────────────────────────────────────────────────────
// Tool System — Dynamic Tool Orchestration
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/ — registry, forge, and 10+ tool providers.

pub mod calculator;
pub mod code_executor;
pub mod data_analyzer;
pub mod deep_research;
pub mod file_ops;
pub mod git_tools;
pub(crate) mod live_web;
pub mod registry;
pub mod search_index;
pub mod threat_destroy;
pub mod tool_forge;
pub mod web_search;

pub use calculator::Calculator;
pub use code_executor::CodeExecutor;
pub use registry::{ToolEntry, ToolRegistry, ToolResult};
pub use search_index::{SearchAcquisitionIndex, SearchAcquisitionStats};
pub use tool_forge::ToolForge;

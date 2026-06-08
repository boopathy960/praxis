// ─────────────────────────────────────────────────────────────
// Orchestrator Module — Agent Orchestration System
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/ — agent controller, compiler, forge, scheduling.

pub mod agent_forge;
pub mod compiler;
pub mod controller;
pub mod generator;
pub mod hive_mind;
pub mod loop_detector;
pub mod process_manager;
pub mod ranked_consensus;
pub mod scheduler;

pub use agent_forge::AgentForge;
pub use compiler::TaskCompiler;
pub use controller::AgentController;
pub use hive_mind::HiveMind;
pub use loop_detector::LoopDetector;
pub use ranked_consensus::{
    RankedAgentScore, RankedCandidateInput, RankedConsensusEngine, RankedConsensusResult,
    RankedPeerReview,
};
pub use scheduler::JobScheduler;

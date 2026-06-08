// ─────────────────────────────────────────────────────────────
// Astra — Personal AI Assistant Core
// ─────────────────────────────────────────────────────────────

pub mod agent_quality;
pub mod agent_runtime;
pub mod artifacts;
pub mod asc2;
pub mod assistant;
pub mod common;
pub mod connectors;
pub mod error;
pub mod httpa;
pub mod nexus;
pub mod os_guardian;
pub mod research;
pub mod sandbox;
pub mod semantic_render;

use std::sync::Arc;

use parking_lot::RwLock;

use agent_runtime::AgentRuntimeService;
use artifacts::ArtifactSafetyService;
use asc2::Asc2Service;
use assistant::AssistantCommandService;
use common::{AppConfig, AppError};
use connectors::ConnectorService;
use httpa::HttpaService;
use nexus::NexusService;
use os_guardian::OsGuardianService;
use research::ResearchService;
use sandbox::SandboxService;
use semantic_render::SemanticRenderEngine;

pub type Shared<T> = Arc<RwLock<T>>;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub artifacts: ArtifactSafetyService,
    pub connectors: ConnectorService,
    pub research: ResearchService,
    pub agent_runtime: AgentRuntimeService,
    pub asc2: Asc2Service,
    pub httpa: HttpaService,
    pub nexus: NexusService,
    pub assistant_commands: AssistantCommandService,
    pub os_guardian: OsGuardianService,
    pub sandbox: SandboxService,
    pub semantic_render: Shared<SemanticRenderEngine>,
    pub brain: Shared<astra_brain::CognitiveCoreEngine>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Result<Self, AppError> {
        let config = config.validate()?;
        let artifacts = ArtifactSafetyService::new();
        let connectors = ConnectorService::new();
        let research = ResearchService::new(config.data_path("research_artifacts"));
        let agent_runtime = AgentRuntimeService::new();
        let httpa = HttpaService::new();
        let assistant_commands = AssistantCommandService::new();
        let os_guardian = OsGuardianService::new();
        let sandbox = SandboxService::new(config.data_path("sandbox"))?;
        let semantic_render = Shared::new(RwLock::new(SemanticRenderEngine::new()));
        let brain = Shared::new(parking_lot::RwLock::new(
            astra_brain::CognitiveCoreEngine::new(astra_brain::CognitiveConfig::default()),
        ));
        let asc2 = Asc2Service::new(config.data_path("asc2"), brain.clone())?;
        let nexus = NexusService::new(config.data_path("nexus"), Some(semantic_render.clone()))?;

        Ok(Self {
            config,
            artifacts,
            connectors,
            research,
            agent_runtime,
            asc2,
            httpa,
            nexus,
            assistant_commands,
            os_guardian,
            sandbox,
            semantic_render,
            brain,
        })
    }
}

// ─────────────────────────────────────────────────────────────
// Astra — Personal AI Assistant Core
// ─────────────────────────────────────────────────────────────

// Modules are grouped by domain for legibility (see ARCHITECTURE.md). The paths
// stay flat — `crate::device_agent`, etc. — so grouping is documentation only.

// ── Foundation ──
pub mod common;
pub mod error;

// ── Runtime & orchestration ──
pub mod agent_runtime;
pub mod autonomy;
pub mod weave;

// ── Reasoning & agents ──
pub mod agent_quality;
pub mod agentic_loop;
pub mod architect;
pub mod asc2;
pub mod assistant;
pub mod forge;

// ── Act: device & web (the agent's hands & eyes) ──
pub mod connectors;
pub mod deep_crawl;
pub mod device_agent;
pub mod search_intelligence;
pub mod semantic_render;
pub mod web_search;

// ── Verify & knowledge ──
pub mod chronicle;
pub mod nexus;
pub mod proof_economy;
pub mod proof_round;
pub mod research;
pub mod text_match;
pub mod turboquant;
pub mod verification;

// ── Govern & safety ──
pub mod artifacts;
pub mod httpa;
pub mod os_guardian;
pub mod sandbox;

// ── Ingress ──
pub mod telegram;

use std::sync::Arc;

use parking_lot::RwLock;

use agent_runtime::AgentRuntimeService;
use artifacts::ArtifactSafetyService;
use autonomy::AutonomyService;
use asc2::Asc2Service;
use assistant::AssistantCommandService;
use chronicle::ChronicleService;
use common::{AppConfig, AppError};
use connectors::ConnectorService;
use device_agent::{DeviceCapabilities, DevicePolicy};
use httpa::HttpaService;
use nexus::NexusService;
use os_guardian::OsGuardianService;
use research::ResearchService;
use sandbox::SandboxService;
use search_intelligence::SearchIntelligenceService;
use semantic_render::SemanticRenderEngine;
use weave::WeaveService;

pub type Shared<T> = Arc<RwLock<T>>;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub artifacts: ArtifactSafetyService,
    pub connectors: ConnectorService,
    pub research: ResearchService,
    pub search_intelligence: SearchIntelligenceService,
    pub agent_runtime: AgentRuntimeService,
    pub autonomy: AutonomyService,
    pub asc2: Asc2Service,
    pub chronicle: ChronicleService,
    pub httpa: HttpaService,
    pub nexus: NexusService,
    pub assistant_commands: AssistantCommandService,
    pub os_guardian: OsGuardianService,
    pub sandbox: SandboxService,
    pub weave: WeaveService,
    pub semantic_render: Shared<SemanticRenderEngine>,
    /// Supervised device layer: the agents' real eyes and hands on the host.
    pub device: Arc<DeviceCapabilities>,
    /// The Verified Agentic Loop: plan -> act -> observe -> verify -> replan.
    pub agentic_loop: agentic_loop::AgenticLoop,
    /// The Proof Economy: adversarial, staked, machine-verified knowledge ledger.
    pub proof_economy: proof_economy::ProofEconomy,
    /// The self-driving proof round: LLM proposer/refuter populations.
    pub proof_conductor: proof_round::ProofConductor,
    /// The Forge: self-healing, proof-governed capability fabric. Agents author
    /// new tools as recipes over governed primitives; only proof-minted ones go
    /// live, and regressed ones are diagnosed, re-proven, and hot-swapped.
    pub forge: forge::ForgeService,
    /// The Architect: the agent-level analog of the Forge. Composes reusable
    /// agent specs from native + forged tools, instantiates them through the
    /// verified loop, mints the ones that prove themselves, and heals the rest.
    pub architect: architect::ArchitectService,
}

impl AppState {
    pub fn new(config: AppConfig) -> Result<Self, AppError> {
        let config = config.validate()?;
        let artifacts = ArtifactSafetyService::new();
        let connectors = ConnectorService::new();
        let research = ResearchService::new(config.data_path("research_artifacts"));
        let search_intelligence =
            SearchIntelligenceService::with_data_dir(config.data_path("search_intelligence"))?;
        // HTTPA ledger and the semantic render engine are built early because
        // the device layer's deep-crawl engine chains both (HTTPA receipts +
        // semantic extraction) on top of governed fetching.
        let httpa = HttpaService::new();
        let semantic_render = Shared::new(RwLock::new(SemanticRenderEngine::new()));
        // The device layer defaults to the permissive "reach the whole machine,
        // but watch continuously" profile. Set ASTRA_DEVICE_MODE=confined to
        // restrict file operations to the agent-workspace root instead.
        let device_policy = if std::env::var("ASTRA_DEVICE_MODE").as_deref() == Ok("confined") {
            DevicePolicy::confined_to(vec![config.data_path("agent_workspaces")])
        } else {
            DevicePolicy::permissive()
        };
        let device_caps = DeviceCapabilities::new(device_policy);
        // The crawl engine shares the device supervisor, so its per-page hops
        // surface in the same live watch and audit as every other device action.
        let crawl_engine = Arc::new(deep_crawl::DeepCrawlEngine::new(
            semantic_render.clone(),
            device_caps.supervisor(),
            web_search::SearxngClient::from_env(),
        ));
        let device = Arc::new(device_caps.with_crawl(crawl_engine));
        // The Verified Agentic Loop reasons with the same remote LLM ASC2 uses
        // (or a stub when none is configured) and acts through the device layer.
        let reasoner: Arc<dyn asc2::ReasoningExecutor> = {
            let cfg = asc2::Asc2RuntimeConfig::from_env();
            match cfg.remote_endpoint {
                Some(endpoint) => Arc::new(asc2::RemoteReasoningExecutor::new(
                    endpoint,
                    cfg.remote_api_key,
                    cfg.remote_model,
                )),
                None => Arc::new(asc2::StubLocalExecutor),
            }
        };
        let proof_economy =
            proof_economy::ProofEconomy::new(config.data_path("proof_economy"), device.clone())?;
        // The Forge grows and repairs the capability set itself: it authors tools
        // as recipes over the supervised device primitives, mints them through the
        // proof economy, and heals regressed ones. It shares the same economy and
        // device layer, so its proofs and its actions land in the same ledgers.
        let forge = forge::ForgeService::new(
            config.data_path("forge"),
            device.clone(),
            proof_economy.clone(),
            reasoner.clone(),
        )?;
        // The loop and the economy are fused: verified loop results are minted
        // into the ledger, and a minted matching claim lets the loop recall a
        // proven answer instead of re-doing the work. With the Forge attached, the
        // loop can also call capabilities the binary never shipped.
        let agentic_loop = agentic_loop::AgenticLoop::new(reasoner.clone(), device.clone())
            .with_economy(proof_economy.clone())
            .with_forge(forge.clone());
        // The Architect grows the agent layer to the Forge's standard: it composes
        // agent specs from native + forged tools, runs them through this same loop
        // to prove them, and mints only the ones that survive. It shares the loop,
        // economy, and forge so genesis, proof, and capability all stay unified.
        let architect = architect::ArchitectService::new(
            config.data_path("architect"),
            proof_economy.clone(),
            agentic_loop.clone(),
            forge.clone(),
            reasoner.clone(),
        )?;
        // The self-driving proof round: LLM proposer + refuter populations whose
        // claims are minted only when the economy's checks survive attack.
        let proof_conductor = proof_round::ProofConductor::new(proof_economy.clone(), reasoner);
        let mut agent_runtime =
            AgentRuntimeService::with_workspace_root(config.data_path("agent_workspaces"))?
                .with_research(search_intelligence.clone())
                .with_device(device.clone());
        // Give generated/swarm agents a real `reason` tool when a remote model
        // is configured, so they can think with the LLM, not just run canned
        // transforms. Absent a model, agents simply run without `reason`.
        if let Some(blocking_reasoner) = agent_runtime::BlockingReasoner::from_env() {
            agent_runtime = agent_runtime.with_reasoner(blocking_reasoner);
        }
        let chronicle = ChronicleService::with_data_dir(config.data_path("chronicle"))?;
        let autonomy =
            AutonomyService::new(agent_runtime.clone()).with_chronicle(chronicle.clone());
        let weave = WeaveService::new(agent_runtime.clone())
            .with_chronicle(chronicle.clone())
            .with_autonomy(autonomy.clone())
            .with_connectors(connectors.clone());
        let assistant_commands = AssistantCommandService::new();
        let os_guardian = OsGuardianService::new();
        let sandbox = SandboxService::new(config.data_path("sandbox"))?;
        let asc2 = Asc2Service::new(config.data_path("asc2"))?;
        let nexus = NexusService::new(config.data_path("nexus"), Some(semantic_render.clone()))?;

        Ok(Self {
            config,
            artifacts,
            connectors,
            research,
            search_intelligence,
            agent_runtime,
            autonomy,
            asc2,
            chronicle,
            httpa,
            nexus,
            assistant_commands,
            os_guardian,
            sandbox,
            weave,
            semantic_render,
            device,
            agentic_loop,
            proof_economy,
            proof_conductor,
            forge,
            architect,
        })
    }
}

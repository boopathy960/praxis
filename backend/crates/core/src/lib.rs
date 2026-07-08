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
pub mod axiom;
pub mod curriculum;
pub mod neural_orchestration;
pub mod weave;

// ── Reasoning & agents ──
pub mod agent_quality;
pub mod agentic_loop;
pub mod architect;
pub mod asc2;
pub mod assistant;
pub mod ceo;
pub mod ceo_guardian;
pub mod crucible;
pub mod deep_research;
pub mod evals;
pub mod experiments;
pub mod forge;
pub mod genome;

// ── Act: device & web (the agent's hands & eyes) ──
pub mod aletheia;
pub mod connectors;
pub mod deep_crawl;
pub mod device_agent;
pub mod search_intelligence;
pub mod semantic_render;
pub mod web_search;

// ── Verify & knowledge ──
pub mod active_inference;
pub mod chronicle;
pub mod continuum;
pub mod learning;
pub mod nexus;
pub mod noesis;
pub mod proof_economy;
pub mod proof_round;
pub mod research;
pub mod sentinel;
pub mod text_match;
pub mod turboquant;
pub mod verification;

// ── Govern & safety ──
pub mod artifacts;
pub mod governance;
pub mod httpa;
pub mod os_guardian;
pub mod sandbox;

// ── Ingress ──
pub mod telegram;

use std::sync::Arc;

use parking_lot::RwLock;

use agent_runtime::AgentRuntimeService;
use artifacts::ArtifactSafetyService;
use asc2::Asc2Service;
use assistant::AssistantCommandService;
use autonomy::AutonomyService;
use ceo::Ceo;
use chronicle::ChronicleService;
use common::{AppConfig, AppError};
use connectors::ConnectorService;
use curriculum::CurriculumService;
use device_agent::{DeviceCapabilities, DevicePolicy};
use evals::EvalService;
use experiments::ExperimentFactory;
use httpa::HttpaService;
use learning::LearningService;
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
    /// The Continuum: six real-world verification domains (reproducibility,
    /// supply-chain behavioral contracts, continuous compliance, spec mining, the
    /// impossibility registry, and vibe-coder guardrails) compiled onto the one
    /// proof economy and continuously re-verified by the Sentinel.
    pub continuum: continuum::ContinuumService,
    /// The Crucible: root-cause reasoning. Given a problem it forms competing
    /// hypotheses, runs read-only checks to confirm *why* it happened, names the
    /// cause, and proposes the real fix carrying a postcondition proof — "why did
    /// this happen, what is the real solution?" The Forge consults it before
    /// re-authoring; agents and the loop can call it for any failing situation.
    pub crucible: crucible::Crucible,
    /// The Forge: self-healing, proof-governed capability fabric. Agents author
    /// new tools as recipes over governed primitives; only proof-minted ones go
    /// live, and regressed ones are diagnosed, re-proven, and hot-swapped.
    pub forge: forge::ForgeService,
    /// The Architect: the agent-level analog of the Forge. Composes reusable
    /// agent specs from native + forged tools, instantiates them through the
    /// verified loop, mints the ones that prove themselves, and heals the rest.
    pub architect: architect::ArchitectService,
    /// The Supreme Court of Justice: the single governance authority. Commands
    /// the army (sandbox-border enforcement) and police (project monitoring),
    /// and alone holds the system-wide lockdown switch.
    pub governance: governance::Governance,
    /// The CEO: the autonomous chief executive directly under governance. Holds
    /// 49% of the project-steering authority (governance holds 51%), owns the
    /// self-thinking and self-evolving cognition, and may spawn agents/tools only
    /// for upgrades the court has approved.
    pub ceo: Ceo,
    /// Continuous Active Inference: the generative model that predicts what its
    /// checks will show, measures its own surprise (variational free energy)
    /// against reality, updates its beliefs to minimize that surprise, and
    /// forages its attention toward where uncertainty is highest. The system's
    /// perception layer — never not predicting.
    pub active_inference: active_inference::ActiveInference,
    /// The Neural Orchestration Network: a plastic connectome over every cognitive
    /// organ (Sentinel, active inference, proof economy, Forge, Architect, CEO,
    /// governance, …) that routes a signal by spreading activation across learned
    /// synapses and rewires those synapses from outcomes via reward-modulated
    /// Hebbian learning. The coordination layer — what makes the organs one mind.
    pub neural_orchestrator: neural_orchestration::NeuralOrchestrator,
    /// Deep Research: a multi-agent investigation that decomposes a question,
    /// spawns a sub-agent per sub-question to scrape exact data from primary
    /// sources, cross-checks, and synthesizes a cited answer.
    pub deep_research: deep_research::DeepResearch,
    /// Noēsis — the understanding organ. Compresses the verified ledger into the
    /// shortest theory that reproduces it (compression as currency), imagines
    /// risky predictions about checks it never ran (the imagination, booked as
    /// dream-debt), and settles them against reality so survivors mint and
    /// mispredictions refute. The first organ that uses the reasoner to theorize
    /// and dream rather than to act — generalization + imagination, grounded in
    /// the same checks everything else trusts.
    pub noesis: noesis::Noesis,
    /// Golden/adversarial eval suites that convert capability quality into
    /// repeatable scorecards instead of one-off canaries.
    pub evals: EvalService,
    /// Shared learning ledger across cognitive organs: successes, failures,
    /// surprise, capability gaps, and safety blocks.
    pub learning: LearningService,
    /// Curriculum planner: ranks the next proof-bearing tasks from learning
    /// failures, open claims, eval failures, and active-inference salience.
    pub curriculum: CurriculumService,
    /// Software-only experiment factory: hypothesis -> proof contract -> run ->
    /// optional proof-economy claim.
    pub experiments: ExperimentFactory,
    /// Axiom — the proof-scheduled OS kernel (Phase 0). Components register
    /// with claims from the proof economy; the dispatcher decides per call how
    /// expensively each must be caged (uncaged call / guarded domain / full
    /// process), the Sentinel demotes them live when a proof drifts, and the
    /// built-in `axsh` shell fronts it all — including the Claw Code CLI as
    /// the Tier-2 agentic userland command.
    pub axiom: axiom::AxiomKernel,
    /// Genome OS — the product layer. Company operations ship as verified,
    /// forkable genomes `(E, R, C)`: an envelope of hard decidable invariants
    /// proven logically (and anchored as proof-economy claims), a residual of
    /// judgment bounded by adaptive conformal prediction, and a certificate that
    /// proves both. A swarm executes them through the envelope-gated runtime,
    /// humans reshape them in plain language, the commons forks and composes them
    /// as a refinement lattice, they improve under constrained best-arm
    /// identification, outcomes are priced by Shapley value, and the warranty
    /// layer is correlated-risk underwriting with concentration limits.
    pub genome: genome::GenomeOsService,
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
        // The sandbox is the single perimeter the ENTIRE project runs inside. It is
        // built here, before the device layer, so the device — the project's real
        // host hands, and its most powerful surface — can be enclosed by it.
        let sandbox = SandboxService::new(config.data_path("sandbox"))?;
        // Enclose the device layer in the perimeter: every host op is audited by
        // the sandbox and halted under lockdown/seal/breakout (see
        // `DeviceCapabilities::with_perimeter`). Its permissive routine execution
        // is unchanged — what changes is that nothing the hands do escapes the
        // perimeter the court commands.
        let device_caps = DeviceCapabilities::new(device_policy).with_perimeter(sandbox.clone());
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
        // The Crucible — root-cause reasoning. Built before the Forge so self-heal
        // can consult it: given a regression, it forms competing hypotheses, runs
        // read-only checks to confirm *why* it broke, and names the cause, so the
        // repair targets the cause rather than the symptom. Shares the device (to
        // run discriminating tests), the economy (to stake proven fixes), and the
        // reasoner (to hypothesize and propose solutions).
        let crucible = crucible::Crucible::new(
            config.data_path("crucible"),
            device.clone(),
            proof_economy.clone(),
            reasoner.clone(),
        )?;
        // The Forge grows and repairs the capability set itself: it authors tools
        // as recipes over the supervised device primitives, mints them through the
        // proof economy, and heals regressed ones. It shares the same economy and
        // device layer, so its proofs and its actions land in the same ledgers.
        // With the Crucible attached, a heal diagnoses the root cause first.
        let forge = forge::ForgeService::new(
            config.data_path("forge"),
            device.clone(),
            proof_economy.clone(),
            reasoner.clone(),
        )?
        .with_crucible(crucible.clone());
        // The loop and the economy are fused: verified loop results are minted
        // into the ledger, and a minted matching claim lets the loop recall a
        // proven answer instead of re-doing the work. With the Forge attached, the
        // loop can also call capabilities the binary never shipped.
        let agentic_loop = agentic_loop::AgenticLoop::new(reasoner.clone(), device.clone())
            .with_economy(proof_economy.clone())
            .with_forge(forge.clone());
        // Genome OS — the product layer over the proof economy. Its certificates
        // are anchored as claims in the same economy, its swarm executes through
        // the same device layer, and its residual judgment can be reasoned by the
        // same verified loop. Seeds the billing beachhead on first boot.
        let genome = genome::GenomeOsService::new(
            config.data_path("genome"),
            proof_economy.clone(),
            device.clone(),
        )?
        .with_loop(agentic_loop.clone());
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
        let proof_conductor =
            proof_round::ProofConductor::new(proof_economy.clone(), reasoner.clone());
        // The Continuum compiles six real-world verification domains onto this one
        // proof economy, and the Sentinel re-runs every enrolled check on a sweep
        // to record the exact moment a standing guarantee drifts. The conductor is
        // attached so spec mining can drive the proposer/refuter populations.
        let sentinel = sentinel::Sentinel::new(config.data_path("sentinel"), device.clone())?;
        let continuum = continuum::ContinuumService::new(proof_economy.clone(), sentinel)
            .with_conductor(proof_conductor.clone());
        // Axiom — the proof-scheduled OS kernel (Phase 0, see AXIOM.md). It
        // consumes the proof economy the way Linux consumes page tables: the
        // dispatcher reads what is minted about a component to decide how
        // expensively to cage it, and the Sentinel's drift is its demotion
        // signal. It shares the continuum's sentinel so axiom watches ride the
        // same background sweep, the device layer as its Tier-2 cage, and the
        // Forge so hot-forged tools can run at function-call cost once minted.
        let axiom = axiom::AxiomKernel::new(
            config.data_path("axiom"),
            proof_economy.clone(),
            continuum.sentinel(),
            device.clone(),
        )?
        .with_forge(forge.clone());
        axiom.seed_kernel_builtins()?;
        // Continuous Active Inference — the generative-model / perception layer.
        // It shares the device (to sample every belief for real), the proof
        // economy (to seed minted claims as confident priors), and the reasoner
        // (to reflect on its own prediction errors). Running on its background
        // cycle, it is the system's always-on prediction-and-surprise loop.
        let active_inference = active_inference::ActiveInference::new(
            config.data_path("active_inference"),
            device.clone(),
        )?
        .with_economy(proof_economy.clone())
        .with_reasoner(reasoner.clone());
        // Noēsis — the understanding organ. It shares the proof economy (the
        // verified corpus it compresses, and the harness its conjectures are
        // proven through), the device layer (to settle every conjecture against
        // reality), and the reasoner (to author theories and imagine their
        // consequences). Built before the CEO so the executive could route a
        // query to it; it is the only organ allowed to produce numbers that did
        // not come from a check that ran — fenced by the dream-debt ledger.
        let noesis = noesis::Noesis::new(
            config.data_path("noesis"),
            device.clone(),
            proof_economy.clone(),
            reasoner.clone(),
        )?;
        // The Neural Orchestration Network — the plastic connectome that wires the
        // cognitive organs into one mind. Built before the CEO so the executive can
        // route a user query across it. Seed the innate topology now so the graph
        // is live on first boot; learned weights persist and survive re-seeding.
        let neural_orchestrator = neural_orchestration::NeuralOrchestrator::new(
            config.data_path("neural_orchestration"),
        )?;
        neural_orchestrator.seed_default_connectome()?;
        // Deep Research — decompose a question, spawn a sub-agent per sub-question
        // to scrape exact data, cross-check, and synthesize. Each sub-agent is a
        // fresh run of the verified loop, so every fetch is real and audited.
        let deep_research =
            deep_research::DeepResearch::new(reasoner.clone(), agentic_loop.clone());
        let evals = EvalService::new();
        let learning = LearningService::new();
        let curriculum = CurriculumService::new();
        let experiments = ExperimentFactory::new(device.clone(), proof_economy.clone());
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
        // The single governance authority — exactly one, constructed here over the
        // one sandbox it commands as its border.
        let governance =
            governance::Governance::new(config.data_path("governance"), sandbox.clone())?;
        // The CEO sits directly under the one governance. It receives the court's
        // narrow proposal desk (submit / read / execute-after-approval), the
        // project-growth surfaces it commands (Architect + Forge), and the shared
        // reasoner that powers its self-thinking and self-evolution. It cannot
        // approve its own proposals, nor command the army/police/border.
        let asc2 = Asc2Service::new(config.data_path("asc2"))?;
        // The CEO is the executive front door: a user query routed to it marshals
        // the whole project — the verified loop, the proof economy, active
        // inference, and the neural orchestrator — to produce a quality, verified
        // answer, and proposes (never spawns on the fly) any missing capability to
        // governance. It therefore receives handles to those organs.
        let ceo = ceo::Ceo::new(
            config.data_path("ceo"),
            reasoner,
            governance.proposal_desk(),
            architect.clone(),
            forge.clone(),
            asc2.clone(),
            agentic_loop.clone(),
            device.clone(),
            proof_economy.clone(),
            active_inference.clone(),
            neural_orchestrator.clone(),
            chronicle.clone(),
            deep_research.clone(),
            crucible.clone(),
            os_guardian.clone(),
        )?;
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
            continuum,
            crucible,
            forge,
            architect,
            governance,
            ceo,
            active_inference,
            neural_orchestrator,
            deep_research,
            noesis,
            evals,
            learning,
            curriculum,
            experiments,
            axiom,
            genome,
        })
    }
}

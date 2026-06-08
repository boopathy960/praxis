// ─────────────────────────────────────────────────────────────
// Astra Core Engine — Brain Module
// ─────────────────────────────────────────────────────────────
// Cognitive Core Engine (CCE) v8.0 — 7-Dimension Sovereign AGI Brain
//
// 44 subsystems — production-grade Rust:
//   v2.0: Intent Classifier, Algorithmic Solver, Template Synthesizer, Knowledge Retriever
//   v3.0: Formula Discovery, Theorem Prover, Program Synthesis, Constraint Solver, Recursive Reasoning
//   v4.0: Phantom Sandbox, Tool Fabricator, Anticipation, Byzantine Consensus, Adversarial, Swarm, Knowledge Crystal, Meta-Cognition
//   v5.0: Infinite Memory, Autonomous Execution, Hallucination Destroyer, Realtime Learning, Code Sandbox, Competitive Benchmark
//   Agent Lightning: Trace Store, Reward Model, Credit Assignment, Prompt Evolver
//   v6.0: Creative Engine (7-strategy ideation), Text Generator (compositional NLG)
//   v7.0 — 5th DIMENSION BRAIN:
//     D1: Massive Deductive Cascade (Parallel MCTS Forest + Byzantine Consensus + Formal Proof)
//     D2: Creative Hypersynthesis (Concept Interpolation + NCD Novelty + Dream Reservoir)
//     D3: Impossible Problem Engine (Axiom Destruction + Gödel Escape + Proof-by-Contradiction)
//     D4: Metacognitive Sovereignty (Self-Awareness + Bias Detection + Self-Rewrite)
//     D5: Temporal Omniscience (Timeline Forking + Parallel Execution + Collapse)
//   v8.0 — SOVEREIGN AGI EXPANSION:
//     D6: Emotion Simulation Engine (Valence-Arousal + Empathy + Affect-Driven Strategy)
//     D7: Multi-Agent Swarm Intelligence (Adversarial Debate + Consensus Collapse)
//     Persistent Learning (Episodic Memory + Sleep Consolidation + Forgetting Curve)
//     NLU Engine (Zero-LLM Intent Classification + Entity Extraction)
//     Code Generation (AST Builder + Multi-Language Serializer + Self-Evolution)
//     Spatial Reasoning (3D Geometry + A* Pathfinding + Topology + Physics)
//     Text Generation v2 (Markov Chain + Grammar Rules + Vocabulary Graph)
//
// Zero-hallucination, deterministic, explainable, <100ms latency, fully offline.

pub mod adversarial;
pub mod algorithmic_solver;
pub mod anticipation;
pub mod asc2;
pub mod autonomous_execution;
pub mod byzantine_consensus;
pub mod code_sandbox;
pub mod cognitive_core;
pub mod competitive_benchmark;
pub mod constraint_solver;
pub mod creative_engine;
pub mod credit_assignment;
pub mod formula_discovery;
pub mod hallucination_destroyer;
pub mod infinite_memory;
pub mod intent_classifier;
pub mod knowledge_crystal;
pub mod knowledge_retriever;
pub mod lightning_loop;
pub mod meta_cognition;
pub mod phantom_sandbox;
pub mod program_synthesis;
pub mod prompt_evolver;
pub mod realtime_learning;
pub mod recursive_reasoning;
pub mod reward_model;
pub mod swarm_intelligence;
pub mod template_synthesizer;
pub mod text_generator;
pub mod theorem_prover;
pub mod thinking_loop;
pub mod tool_fabricator;
pub mod trace_store;

// ═══════════════════════════════════════════════════════════════
// 5th DIMENSION BRAIN — v7.0 Cognitive Dimensions
// ═══════════════════════════════════════════════════════════════
pub mod dimension1_deductive_cascade;
pub mod dimension2_creative_hypersynthesis;
pub mod dimension3_impossible_engine;
pub mod dimension4_metacognitive_sovereign;
pub mod dimension5_temporal_omniscience;

// ═══════════════════════════════════════════════════════════════
// v8.0 — ADVANCED COGNITIVE MODULES
// ═══════════════════════════════════════════════════════════════
pub mod code_gen_engine;
pub mod dimension6_emotion_engine;
pub mod dimension7_swarm_intelligence;
pub mod nlu_engine;
pub mod persistent_learning;
pub mod spatial_reasoning;
pub mod text_gen_v2;

// ═══════════════════════════════════════════════════════════════
// SELF-EVOLUTION — Genetic Strategy Evolution
// ═══════════════════════════════════════════════════════════════
pub mod self_evolution;

// ═══════════════════════════════════════════════════════════════
// NOVEL MATHEMATICS — Original formulas & equations
// ═══════════════════════════════════════════════════════════════
pub mod advanced_reasoning_formulas;
pub mod cognitive_math;

// Re-export primary types
pub use algorithmic_solver::{SolverPipeline, SolverResult};
pub use asc2::{
    ActionCertificate, ActionToken, Asc2Config, Asc2Controller, Asc2ExecutionResult, Asc2Metrics,
    ControlActionEstimate, ControlDecision, PerformanceSnapshot, ProofPacket, ReflexiveAgentState,
    RoleCandidate, SwarmState,
};
pub use cognitive_core::{CognitiveConfig, CognitiveCoreEngine};
pub use creative_engine::{CreativeEngine, CreativeIdea, CreativeResult};
pub use intent_classifier::{IntentClassification, IntentClassifier, IntentType};
pub use lightning_loop::{
    LearningSummary, LightningLoop, OptimizationBatch, OptimizationObjective, ReasoningPolicy,
};
pub use reward_model::{CompositeReward, RewardComputer, RewardDimension};
pub use template_synthesizer::{SynthesisContext, TemplateSynthesizer};
pub use text_generator::{TextGenerator, TextRequest, TextResult, TextStructure, TextStyle};
pub use thinking_loop::{ThinkingLoop, ThinkingResult, ThinkingStep};
pub use trace_store::{LearningStore, SpanType, TraceSpan, TrajectoryTrace};

// Re-export 5th Dimension types
pub use dimension1_deductive_cascade::{CascadeConfig, CascadeResult, DeductiveCascade};
pub use dimension2_creative_hypersynthesis::{CreativeHypersynthesis, HyperConfig, HyperResult};
pub use dimension3_impossible_engine::{ImpossibleConfig, ImpossibleEngine, ImpossibleResult};
pub use dimension4_metacognitive_sovereign::{
    MetacognitionResult, MetacognitiveSovereign, SovereignConfig,
};
pub use dimension5_temporal_omniscience::{
    FifthDimensionResult, TemporalConfig, TemporalOmniscience,
};

// Re-export v8.0 types
pub use code_gen_engine::{AstNode, CodeGenEngine, CodeGenResult, TargetLanguage};
pub use dimension6_emotion_engine::{
    EmotionCategory, EmotionConfig, EmotionEngine, EmotionResult, EmotionalState,
};
pub use dimension7_swarm_intelligence::{SwarmConfig, SwarmIntelligence, SwarmResult};
pub use nlu_engine::{NluEngine, NluResult, SemanticFrame};
pub use persistent_learning::{
    ConsolidationResult, LearningConfig, PersistentLearningEngine, RecallResult,
};
pub use spatial_reasoning::{PathResult, Point3D, SpatialReasoningEngine, SpatialResult};
pub use text_gen_v2::{NarrationRequest, TextGenV2, TextGenV2Result};

// Re-export self-evolution types
pub use self_evolution::{EvolvableParam, SelfEvolutionEngine, StrategyGene, StrategyGenome};

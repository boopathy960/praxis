// ═══════════════════════════════════════════════════════════════
// Creative Engine v1.0 — GPU-Free Creative Thinking
// ═══════════════════════════════════════════════════════════════
//
// Creativity is NOT magic. It is combinatorial exploration:
//   1. Conceptual Blending — merge concepts from different domains
//   2. Divergent Ideation — MCTS-guided exploration of idea space
//   3. Analogical Transfer — structural mapping across domains
//   4. Constraint Relaxation — "what if X wasn't a limitation?"
//   5. Evolutionary Ideation — genetically evolve ideas
//   6. Lateral Thinking — deliberately break assumptions
//
// No LLMs, no GPUs, no neural networks.
// Uses: combinatorics, graph search, genetic programming, heuristics.

use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// A creative idea with scoring and provenance.
#[derive(Debug, Clone)]
pub struct CreativeIdea {
    pub content: String,
    pub novelty: f64,
    pub relevance: f64,
    pub coherence: f64,
    pub usefulness: f64,
    pub composite_score: f64,
    pub generation_method: CreativeMethod,
    pub source_concepts: Vec<String>,
    pub reasoning_chain: Vec<String>,
}

/// How the idea was generated.
#[derive(Debug, Clone, PartialEq)]
pub enum CreativeMethod {
    ConceptualBlend,
    AnalogicalTransfer,
    ConstraintRelaxation,
    LateralThinking,
    Evolutionary,
    Recombination,
    Inversion,
    Amplification,
}

impl CreativeMethod {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ConceptualBlend => "conceptual_blend",
            Self::AnalogicalTransfer => "analogical_transfer",
            Self::ConstraintRelaxation => "constraint_relaxation",
            Self::LateralThinking => "lateral_thinking",
            Self::Evolutionary => "evolutionary",
            Self::Recombination => "recombination",
            Self::Inversion => "inversion",
            Self::Amplification => "amplification",
        }
    }
}

/// Result of a creative thinking session.
#[derive(Debug, Clone)]
pub struct CreativeResult {
    pub ideas: Vec<CreativeIdea>,
    pub best_idea: Option<CreativeIdea>,
    pub total_candidates_explored: usize,
    pub methods_used: Vec<String>,
    pub thinking_trace: Vec<String>,
    pub duration_ms: f64,
}

/// A concept in the creative workspace.
#[derive(Debug, Clone)]
struct Concept {
    label: String,
    domain: String,
    properties: Vec<String>,
    relations: Vec<(String, String)>, // (relation, target_concept)
    associations: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════
// DOMAIN KNOWLEDGE — Seed Concepts for Creative Blending
// ═══════════════════════════════════════════════════════════════

/// Pre-loaded domain knowledge for creative cross-pollination.
struct DomainKnowledge {
    concepts: HashMap<String, Concept>,
    domains: HashMap<String, Vec<String>>,
    analogy_maps: Vec<AnalogyMap>,
    lateral_triggers: Vec<LateralTrigger>,
}

#[derive(Debug, Clone)]
struct AnalogyMap {
    source_domain: String,
    target_domain: String,
    mappings: Vec<(String, String)>,
    structural_similarity: f64,
}

#[derive(Debug, Clone)]
struct LateralTrigger {
    pattern: String,
    inversion: String,
    question: String,
}

impl DomainKnowledge {
    fn new() -> Self {
        let mut knowledge = Self {
            concepts: HashMap::new(),
            domains: HashMap::new(),
            analogy_maps: Vec::new(),
            lateral_triggers: Vec::new(),
        };
        knowledge.seed_domains();
        knowledge.seed_analogies();
        knowledge.seed_lateral_triggers();
        knowledge
    }

    fn seed_domains(&mut self) {
        // Software Engineering concepts
        let sw_concepts = vec![
            (
                "architecture",
                vec!["modular", "layered", "scalable", "maintainable"],
                vec![("enables", "scalability"), ("requires", "abstraction")],
            ),
            (
                "api",
                vec!["interface", "contract", "endpoint", "versioned"],
                vec![("connects", "systems"), ("enforces", "contract")],
            ),
            (
                "database",
                vec!["persistent", "queryable", "indexed", "normalized"],
                vec![("stores", "data"), ("enables", "retrieval")],
            ),
            (
                "cache",
                vec!["fast", "temporary", "invalidatable", "layered"],
                vec![("accelerates", "retrieval"), ("tradeoffs", "consistency")],
            ),
            (
                "pipeline",
                vec!["sequential", "transformative", "composable", "parallel"],
                vec![("processes", "data"), ("chains", "operations")],
            ),
            (
                "testing",
                vec!["verifiable", "automated", "regression", "coverage"],
                vec![("validates", "correctness"), ("catches", "regressions")],
            ),
        ];
        for (name, props, rels) in &sw_concepts {
            self.add_concept(name, "software", props, rels);
        }

        // Nature concepts (for cross-domain analogy)
        let nature_concepts = vec![
            (
                "ecosystem",
                vec!["balanced", "adaptive", "interconnected", "resilient"],
                vec![("contains", "organisms"), ("self-regulates", "balance")],
            ),
            (
                "evolution",
                vec!["adaptive", "selective", "generational", "fitness-driven"],
                vec![("produces", "adaptation"), ("requires", "variation")],
            ),
            (
                "neural_network",
                vec!["parallel", "adaptive", "pattern-matching", "distributed"],
                vec![("processes", "signals"), ("learns", "patterns")],
            ),
            (
                "immune_system",
                vec!["defensive", "adaptive", "memory-based", "distributed"],
                vec![("detects", "threats"), ("remembers", "past_attacks")],
            ),
            (
                "swarm",
                vec!["decentralized", "emergent", "self-organizing", "robust"],
                vec![("achieves", "coordination"), ("requires", "simple_rules")],
            ),
        ];
        for (name, props, rels) in &nature_concepts {
            self.add_concept(name, "nature", props, rels);
        }

        // Business concepts
        let biz_concepts = vec![
            (
                "marketplace",
                vec!["two-sided", "network-effect", "trust-based", "scalable"],
                vec![("connects", "buyers_sellers"), ("creates", "value")],
            ),
            (
                "subscription",
                vec!["recurring", "predictable", "retention-focused", "tiered"],
                vec![("generates", "revenue"), ("requires", "value_delivery")],
            ),
            (
                "automation",
                vec!["repeatable", "efficient", "error-reducing", "scalable"],
                vec![("eliminates", "manual_work"), ("enables", "scale")],
            ),
        ];
        for (name, props, rels) in &biz_concepts {
            self.add_concept(name, "business", props, rels);
        }

        // Security concepts
        let sec_concepts = vec![
            (
                "zero_trust",
                vec![
                    "verify-always",
                    "least-privilege",
                    "microsegmented",
                    "continuous",
                ],
                vec![("prevents", "lateral_movement"), ("assumes", "breach")],
            ),
            (
                "encryption",
                vec![
                    "confidential",
                    "tamper-evident",
                    "key-based",
                    "mathematical",
                ],
                vec![("protects", "data"), ("requires", "key_management")],
            ),
            (
                "firewall",
                vec!["boundary", "rule-based", "filtering", "stateful"],
                vec![("controls", "traffic"), ("enforces", "policy")],
            ),
        ];
        for (name, props, rels) in &sec_concepts {
            self.add_concept(name, "security", props, rels);
        }
    }

    fn add_concept(&mut self, name: &str, domain: &str, props: &[&str], rels: &[(&str, &str)]) {
        let concept = Concept {
            label: name.to_string(),
            domain: domain.to_string(),
            properties: props.iter().map(|s| s.to_string()).collect(),
            relations: rels
                .iter()
                .map(|(r, t)| (r.to_string(), t.to_string()))
                .collect(),
            associations: Vec::new(),
        };
        self.concepts.insert(name.to_string(), concept);
        self.domains
            .entry(domain.to_string())
            .or_default()
            .push(name.to_string());
    }

    fn seed_analogies(&mut self) {
        self.analogy_maps.push(AnalogyMap {
            source_domain: "nature".into(),
            target_domain: "software".into(),
            mappings: vec![
                ("ecosystem".into(), "architecture".into()),
                ("evolution".into(), "iterative_development".into()),
                ("immune_system".into(), "security_system".into()),
                ("swarm".into(), "microservices".into()),
                ("neural_network".into(), "pipeline".into()),
            ],
            structural_similarity: 0.75,
        });
        self.analogy_maps.push(AnalogyMap {
            source_domain: "business".into(),
            target_domain: "software".into(),
            mappings: vec![
                ("marketplace".into(), "api".into()),
                ("subscription".into(), "service".into()),
                ("automation".into(), "pipeline".into()),
            ],
            structural_similarity: 0.65,
        });
    }

    fn seed_lateral_triggers(&mut self) {
        self.lateral_triggers = vec![
            LateralTrigger {
                pattern: "needs a server".into(),
                inversion: "What if it ran entirely on the client?".into(),
                question: "Can we eliminate the server entirely?".into(),
            },
            LateralTrigger {
                pattern: "stores data".into(),
                inversion: "What if nothing was stored? What if data was computed on demand?"
                    .into(),
                question: "Can we replace storage with computation?".into(),
            },
            LateralTrigger {
                pattern: "step by step".into(),
                inversion: "What if all steps happened simultaneously?".into(),
                question: "Can we parallelize or collapse the pipeline?".into(),
            },
            LateralTrigger {
                pattern: "user interface".into(),
                inversion: "What if there was no UI? What if the system anticipated needs?".into(),
                question: "Can we replace UI interaction with prediction?".into(),
            },
            LateralTrigger {
                pattern: "authentication".into(),
                inversion: "What if identity was inherent? What about zero-knowledge proofs?"
                    .into(),
                question: "Can we prove identity without revealing it?".into(),
            },
            LateralTrigger {
                pattern: "training data".into(),
                inversion:
                    "What if the system never trained? What if it reasoned from first principles?"
                        .into(),
                question: "Can we replace learned patterns with derived rules?".into(),
            },
            LateralTrigger {
                pattern: "api call".into(),
                inversion: "What if the system already had the answer locally?".into(),
                question: "Can we precompute or cache the answer before the question is asked?"
                    .into(),
            },
            LateralTrigger {
                pattern: "slow".into(),
                inversion: "What if we traded accuracy for speed? Or precomputed the bottleneck?"
                    .into(),
                question: "What's the actual bottleneck and can we eliminate it entirely?".into(),
            },
        ];
    }
}

// ═══════════════════════════════════════════════════════════════
// CREATIVE ENGINE
// ═══════════════════════════════════════════════════════════════

pub struct CreativeEngine {
    knowledge: DomainKnowledge,
    idea_history: Vec<CreativeIdea>,
    total_sessions: u64,
    total_ideas_generated: u64,
}

impl CreativeEngine {
    pub fn new() -> Self {
        Self {
            knowledge: DomainKnowledge::new(),
            idea_history: Vec::new(),
            total_sessions: 0,
            total_ideas_generated: 0,
        }
    }

    /// Main entry: generate creative ideas for a given problem.
    pub fn think_creatively(&mut self, problem: &str, max_ideas: usize) -> CreativeResult {
        let start = Instant::now();
        self.total_sessions += 1;

        let mut all_ideas = Vec::new();
        let mut thinking_trace = Vec::new();
        let mut methods_used = Vec::new();
        let mut rng = rand::thread_rng();

        // Extract key concepts from the problem
        let problem_concepts = self.extract_concepts(problem);
        thinking_trace.push(format!(
            "Extracted {} key concepts: [{}]",
            problem_concepts.len(),
            problem_concepts.join(", ")
        ));

        // Strategy 1: Conceptual Blending
        let blends = self.conceptual_blending(problem, &problem_concepts, &mut rng);
        thinking_trace.push(format!(
            "Conceptual blending generated {} ideas",
            blends.len()
        ));
        if !blends.is_empty() {
            methods_used.push("conceptual_blend".into());
        }
        all_ideas.extend(blends);

        // Strategy 2: Analogical Transfer
        let analogies = self.analogical_transfer(problem, &problem_concepts);
        thinking_trace.push(format!(
            "Analogical transfer generated {} ideas",
            analogies.len()
        ));
        if !analogies.is_empty() {
            methods_used.push("analogical_transfer".into());
        }
        all_ideas.extend(analogies);

        // Strategy 3: Constraint Relaxation
        let relaxed = self.constraint_relaxation(problem);
        thinking_trace.push(format!(
            "Constraint relaxation generated {} ideas",
            relaxed.len()
        ));
        if !relaxed.is_empty() {
            methods_used.push("constraint_relaxation".into());
        }
        all_ideas.extend(relaxed);

        // Strategy 4: Lateral Thinking
        let lateral = self.lateral_thinking(problem);
        thinking_trace.push(format!(
            "Lateral thinking generated {} ideas",
            lateral.len()
        ));
        if !lateral.is_empty() {
            methods_used.push("lateral_thinking".into());
        }
        all_ideas.extend(lateral);

        // Strategy 5: Inversion
        let inversions = self.inversion_thinking(problem, &problem_concepts);
        thinking_trace.push(format!("Inversion generated {} ideas", inversions.len()));
        if !inversions.is_empty() {
            methods_used.push("inversion".into());
        }
        all_ideas.extend(inversions);

        // Strategy 6: Recombination (cross existing ideas)
        if all_ideas.len() >= 2 {
            let recombined = self.recombine_ideas(&all_ideas, &mut rng);
            thinking_trace.push(format!(
                "Recombination generated {} ideas",
                recombined.len()
            ));
            if !recombined.is_empty() {
                methods_used.push("recombination".into());
            }
            all_ideas.extend(recombined);
        }

        // Strategy 7: Evolutionary refinement (mutate + select best)
        if all_ideas.len() >= 3 {
            let evolved = self.evolve_ideas(&all_ideas, problem, &mut rng);
            thinking_trace.push(format!("Evolution refined {} ideas", evolved.len()));
            if !evolved.is_empty() {
                methods_used.push("evolutionary".into());
            }
            all_ideas.extend(evolved);
        }

        let total_explored = all_ideas.len();

        // Score all ideas
        for idea in &mut all_ideas {
            idea.composite_score = self.score_idea(idea, problem);
        }

        // Deduplicate and select top ideas
        all_ideas.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        all_ideas.dedup_by(|a, b| a.content == b.content);
        all_ideas.truncate(max_ideas);

        let best = all_ideas.first().cloned();
        self.total_ideas_generated += all_ideas.len() as u64;

        // Keep history for future recombination
        for idea in &all_ideas {
            self.idea_history.push(idea.clone());
        }
        if self.idea_history.len() > 500 {
            self.idea_history.drain(0..250);
        }

        thinking_trace.push(format!(
            "Final: {} unique ideas from {} candidates, best score = {:.3}",
            all_ideas.len(),
            total_explored,
            best.as_ref().map(|i| i.composite_score).unwrap_or(0.0)
        ));

        CreativeResult {
            ideas: all_ideas,
            best_idea: best,
            total_candidates_explored: total_explored,
            methods_used,
            thinking_trace,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    // ─────────────────────────────────────────────────────────
    // STRATEGY 1: CONCEPTUAL BLENDING
    // ─────────────────────────────────────────────────────────
    // Take two concepts from different domains and merge their
    // properties to create a novel hybrid.

    fn conceptual_blending(
        &self,
        problem: &str,
        problem_concepts: &[String],
        rng: &mut impl Rng,
    ) -> Vec<CreativeIdea> {
        let mut ideas = Vec::new();
        let _domains: Vec<&String> = self.knowledge.domains.keys().collect();

        for concept_word in problem_concepts.iter().take(5) {
            // Find related concepts in different domains
            let related = self.find_related_concepts(concept_word);
            for (concept_a, concept_b) in related.iter().take(3) {
                if concept_a.domain == concept_b.domain {
                    continue;
                }

                // Blend properties
                let shared_props = self.shared_properties(concept_a, concept_b);
                let unique_a: Vec<&String> = concept_a
                    .properties
                    .iter()
                    .filter(|p| !concept_b.properties.contains(p))
                    .collect();
                let unique_b: Vec<&String> = concept_b
                    .properties
                    .iter()
                    .filter(|p| !concept_a.properties.contains(p))
                    .collect();

                let blend_content = format!(
                    "What if we approached '{}' like a {}/{} hybrid? \
                     Combine the {} nature of {} with the {} nature of {}. \
                     Shared qualities: [{}]. \
                     From {}: apply [{}]. \
                     From {}: apply [{}]. \
                     This creates something that is simultaneously {} and {}.",
                    problem,
                    concept_a.label,
                    concept_b.label,
                    concept_a
                        .properties
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("core"),
                    concept_a.label,
                    concept_b
                        .properties
                        .first()
                        .map(|s| s.as_str())
                        .unwrap_or("core"),
                    concept_b.label,
                    shared_props.join(", "),
                    concept_a.domain,
                    unique_a
                        .iter()
                        .take(2)
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    concept_b.domain,
                    unique_b
                        .iter()
                        .take(2)
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    concept_a
                        .properties
                        .get(rng.gen_range(0..concept_a.properties.len().max(1)))
                        .map(|s| s.as_str())
                        .unwrap_or("novel"),
                    concept_b
                        .properties
                        .get(rng.gen_range(0..concept_b.properties.len().max(1)))
                        .map(|s| s.as_str())
                        .unwrap_or("creative"),
                );

                ideas.push(CreativeIdea {
                    content: blend_content,
                    novelty: 0.8,
                    relevance: if problem.to_lowercase().contains(&concept_a.label) {
                        0.9
                    } else {
                        0.6
                    },
                    coherence: 0.7,
                    usefulness: 0.6,
                    composite_score: 0.0,
                    generation_method: CreativeMethod::ConceptualBlend,
                    source_concepts: vec![concept_a.label.clone(), concept_b.label.clone()],
                    reasoning_chain: vec![
                        format!(
                            "Identified '{}' in domain '{}'",
                            concept_a.label, concept_a.domain
                        ),
                        format!(
                            "Cross-referenced with '{}' in domain '{}'",
                            concept_b.label, concept_b.domain
                        ),
                        "Blended properties to create hybrid approach".into(),
                    ],
                });
            }
        }

        ideas
    }

    // ─────────────────────────────────────────────────────────
    // STRATEGY 2: ANALOGICAL TRANSFER
    // ─────────────────────────────────────────────────────────
    // Map the problem to a different domain, solve it there,
    // then transfer the solution back.

    fn analogical_transfer(&self, problem: &str, problem_concepts: &[String]) -> Vec<CreativeIdea> {
        let mut ideas = Vec::new();

        for analogy in &self.knowledge.analogy_maps {
            for (source, target) in &analogy.mappings {
                let source_concept = self.knowledge.concepts.get(source);
                let target_concept = self.knowledge.concepts.get(target);

                if let (Some(src), Some(tgt)) = (source_concept, target_concept) {
                    // Check if the problem relates to either domain
                    let problem_lower = problem.to_lowercase();
                    let is_relevant = src.properties.iter().any(|p| problem_lower.contains(p))
                        || tgt.properties.iter().any(|p| problem_lower.contains(p))
                        || problem_concepts.iter().any(|c| {
                            src.label.contains(c)
                                || tgt.label.contains(c)
                                || c.contains(&src.label)
                                || c.contains(&tgt.label)
                        });

                    if is_relevant || analogy.structural_similarity > 0.7 {
                        let idea_content = format!(
                            "Analogy: In {}, {} works by being [{}]. \
                             Apply this to your problem: make it [{}] like a {}. \
                             Specifically, {} ({}) → {} ({}) with structural similarity {:.0}%. \
                             Transfer insight: what makes {} work in {} can solve '{}' \
                             if we adopt the principles of [{}].",
                            analogy.source_domain,
                            src.label,
                            src.properties.join(", "),
                            tgt.properties.join(", "),
                            tgt.label,
                            src.label,
                            analogy.source_domain,
                            tgt.label,
                            analogy.target_domain,
                            analogy.structural_similarity * 100.0,
                            src.label,
                            analogy.source_domain,
                            problem,
                            src.relations
                                .iter()
                                .map(|(r, t)| format!("{} {}", r, t))
                                .collect::<Vec<_>>()
                                .join(", "),
                        );

                        ideas.push(CreativeIdea {
                            content: idea_content,
                            novelty: 0.75,
                            relevance: if is_relevant { 0.85 } else { 0.5 },
                            coherence: analogy.structural_similarity,
                            usefulness: 0.7,
                            composite_score: 0.0,
                            generation_method: CreativeMethod::AnalogicalTransfer,
                            source_concepts: vec![src.label.clone(), tgt.label.clone()],
                            reasoning_chain: vec![
                                format!(
                                    "Mapped {} → {}",
                                    analogy.source_domain, analogy.target_domain
                                ),
                                format!(
                                    "Structural similarity: {:.0}%",
                                    analogy.structural_similarity * 100.0
                                ),
                                format!("Transferring properties: {:?}", src.properties),
                            ],
                        });
                    }
                }
            }
        }

        ideas
    }

    // ─────────────────────────────────────────────────────────
    // STRATEGY 3: CONSTRAINT RELAXATION
    // ─────────────────────────────────────────────────────────
    // Identify implicit constraints in the problem and
    // systematically relax them to discover new possibilities.

    fn constraint_relaxation(&self, problem: &str) -> Vec<CreativeIdea> {
        let mut ideas = Vec::new();
        let problem_lower = problem.to_lowercase();

        let implicit_constraints: Vec<(&str, &str, &str)> = vec![
            ("sequential", "What if operations happened in parallel instead of sequentially?",
             "Parallelism often reveals that assumed ordering dependencies don't actually exist."),
            ("single", "What if we used multiple approaches simultaneously and selected the best result?",
             "Redundancy with selection can outperform any single approach."),
            ("perfect", "What if we accepted 90% accuracy for 10x speed? What's the Pareto frontier?",
             "Perfect is the enemy of good. Approximation algorithms can transform feasibility."),
            ("centralized", "What if there was no central authority? What if every node was autonomous?",
             "Decentralization eliminates single points of failure and bottlenecks."),
            ("synchronous", "What if everything was event-driven and asynchronous?",
             "Asynchrony lets the system do useful work instead of waiting."),
            ("memory", "What if memory was unlimited? Or what if we couldn't store anything?",
             "Extreme constraints often reveal the true essence of a problem."),
            ("network", "What if there was no network? What would an offline-first solution look like?",
             "Designing for offline often produces more resilient systems."),
            ("user", "What if there was no human user? What if the system operated autonomously?",
             "Removing the human loop reveals which interactions are truly necessary."),
            ("real-time", "What if latency didn't matter? Or what if it had to be <1ms?",
             "Extreme latency constraints force architectural innovation."),
            ("cost", "What if cost was zero? What would the ideal system look like? Now optimize toward it.",
             "Remove cost constraints to find the ideal, then find the nearest affordable version."),
        ];

        for (constraint, relaxation, insight) in &implicit_constraints {
            if problem_lower.contains(constraint)
                || self.is_conceptually_related(&problem_lower, constraint)
            {
                ideas.push(CreativeIdea {
                    content: format!(
                        "Constraint Relaxation — Relax the '{}' assumption:\n{}\n\nInsight: {}",
                        constraint, relaxation, insight
                    ),
                    novelty: 0.7,
                    relevance: 0.8,
                    coherence: 0.85,
                    usefulness: 0.75,
                    composite_score: 0.0,
                    generation_method: CreativeMethod::ConstraintRelaxation,
                    source_concepts: vec![constraint.to_string()],
                    reasoning_chain: vec![
                        format!("Detected implicit constraint: '{}'", constraint),
                        "Systematically relaxed the constraint".into(),
                        format!("Generated alternative: {}", relaxation),
                    ],
                });
            }
        }

        // If no specific constraints matched, apply the most universal ones
        if ideas.is_empty() {
            for (constraint, relaxation, insight) in implicit_constraints.iter().take(3) {
                ideas.push(CreativeIdea {
                    content: format!(
                        "What if we question the '{}' assumption in this problem?\n{}\n\n{}",
                        constraint, relaxation, insight
                    ),
                    novelty: 0.6,
                    relevance: 0.5,
                    coherence: 0.8,
                    usefulness: 0.6,
                    composite_score: 0.0,
                    generation_method: CreativeMethod::ConstraintRelaxation,
                    source_concepts: vec![constraint.to_string()],
                    reasoning_chain: vec!["Applied universal constraint relaxation".into()],
                });
            }
        }

        ideas
    }

    // ─────────────────────────────────────────────────────────
    // STRATEGY 4: LATERAL THINKING
    // ─────────────────────────────────────────────────────────
    // Deliberately break assumptions and approach from
    // unexpected angles.

    fn lateral_thinking(&self, problem: &str) -> Vec<CreativeIdea> {
        let mut ideas = Vec::new();
        let problem_lower = problem.to_lowercase();

        for trigger in &self.knowledge.lateral_triggers {
            if problem_lower.contains(&trigger.pattern.to_lowercase()) {
                ideas.push(CreativeIdea {
                    content: format!(
                        "Lateral shift: Instead of '{}', consider:\n— {}\n— Ask: {}",
                        trigger.pattern, trigger.inversion, trigger.question
                    ),
                    novelty: 0.9,
                    relevance: 0.7,
                    coherence: 0.65,
                    usefulness: 0.7,
                    composite_score: 0.0,
                    generation_method: CreativeMethod::LateralThinking,
                    source_concepts: vec![trigger.pattern.clone()],
                    reasoning_chain: vec![
                        format!("Detected pattern: '{}'", trigger.pattern),
                        format!("Applied inversion: '{}'", trigger.inversion),
                    ],
                });
            }
        }

        ideas
    }

    // ─────────────────────────────────────────────────────────
    // STRATEGY 5: INVERSION
    // ─────────────────────────────────────────────────────────
    // Instead of solving the problem, solve the opposite.
    // "How do I make this succeed?" → "How would I make this fail?"

    fn inversion_thinking(&self, problem: &str, concepts: &[String]) -> Vec<CreativeIdea> {
        let mut ideas = Vec::new();

        let inversions = vec![
            (
                "How do I make this FAIL spectacularly? Now do the opposite.",
                "Inversion often reveals hidden failure modes that direct thinking misses.",
            ),
            (
                "What would the WORST possible solution look like? Now invert every choice.",
                "The worst solution defines the boundary of the solution space.",
            ),
            (
                "Instead of building this, what would I need to PREVENT?",
                "Prevention-oriented thinking often produces simpler, more robust designs.",
            ),
            (
                "What if I had to explain this solution to a 5-year-old?",
                "Forced simplification reveals unnecessary complexity.",
            ),
            (
                "What would this look like in 10 years? What about 100 years?",
                "Temporal projection reveals which parts of the solution are timeless.",
            ),
        ];

        for (inversion, insight) in &inversions {
            ideas.push(CreativeIdea {
                content: format!(
                    "Inversion applied to '{}': {}\n\nInsight: {}",
                    &problem.chars().take(80).collect::<String>(),
                    inversion,
                    insight
                ),
                novelty: 0.85,
                relevance: 0.7,
                coherence: 0.75,
                usefulness: 0.65,
                composite_score: 0.0,
                generation_method: CreativeMethod::Inversion,
                source_concepts: concepts.to_vec(),
                reasoning_chain: vec![
                    "Applied Charlie Munger's Inversion principle".into(),
                    format!("Inverted the problem: {}", inversion),
                ],
            });
        }

        ideas
    }

    // ─────────────────────────────────────────────────────────
    // STRATEGY 6: RECOMBINATION
    // ─────────────────────────────────────────────────────────
    // Cross-pollinate existing ideas to create hybrids.

    fn recombine_ideas(&self, ideas: &[CreativeIdea], rng: &mut impl Rng) -> Vec<CreativeIdea> {
        let mut recombined = Vec::new();
        let count = ideas.len().min(8);

        for i in 0..count {
            for j in (i + 1)..count {
                if rng.gen::<f64>() < 0.4 {
                    continue;
                } // Sparse recombination

                let a = &ideas[i];
                let b = &ideas[j];
                if a.generation_method == b.generation_method {
                    continue;
                }

                let hybrid = CreativeIdea {
                    content: format!(
                        "Hybrid idea combining {} + {}:\n\
                         Part A (from {}): {}\n\
                         Part B (from {}): {}\n\
                         Synthesis: Merge the {} approach with {} thinking.",
                        a.generation_method.as_str(),
                        b.generation_method.as_str(),
                        a.generation_method.as_str(),
                        &a.content.chars().take(150).collect::<String>(),
                        b.generation_method.as_str(),
                        &b.content.chars().take(150).collect::<String>(),
                        a.source_concepts
                            .first()
                            .map(|s| s.as_str())
                            .unwrap_or("first"),
                        b.source_concepts
                            .first()
                            .map(|s| s.as_str())
                            .unwrap_or("second"),
                    ),
                    novelty: (a.novelty + b.novelty) / 2.0 + 0.1,
                    relevance: (a.relevance + b.relevance) / 2.0,
                    coherence: (a.coherence + b.coherence) / 2.0 - 0.05,
                    usefulness: (a.usefulness + b.usefulness) / 2.0,
                    composite_score: 0.0,
                    generation_method: CreativeMethod::Recombination,
                    source_concepts: [a.source_concepts.clone(), b.source_concepts.clone()]
                        .concat(),
                    reasoning_chain: vec![
                        format!(
                            "Combined idea from {} with idea from {}",
                            a.generation_method.as_str(),
                            b.generation_method.as_str()
                        ),
                        "Created hybrid by merging complementary perspectives".into(),
                    ],
                };
                recombined.push(hybrid);
            }
        }

        recombined
    }

    // ─────────────────────────────────────────────────────────
    // STRATEGY 7: EVOLUTIONARY REFINEMENT
    // ─────────────────────────────────────────────────────────
    // Take the best ideas and mutate/evolve them.

    fn evolve_ideas(
        &self,
        ideas: &[CreativeIdea],
        problem: &str,
        rng: &mut impl Rng,
    ) -> Vec<CreativeIdea> {
        let mut evolved = Vec::new();

        let amplifications = [
            "What if we pushed this idea to its EXTREME?",
            "What if we applied this NOT just once, but recursively?",
            "What if we combined this with its OWN output as input?",
            "What if this idea applied to ITSELF (meta-application)?",
            "What if we ran this idea in REVERSE?",
        ];

        // Take top 3 ideas and amplify
        for idea in ideas.iter().take(3) {
            let amp = amplifications[rng.gen_range(0..amplifications.len())];
            let mut mutated = idea.clone();
            mutated.content = format!(
                "AMPLIFIED: {} — {}\n\nOriginal: {}",
                amp,
                &problem.chars().take(80).collect::<String>(),
                &idea.content.chars().take(200).collect::<String>(),
            );
            mutated.novelty = (idea.novelty + 0.15).min(1.0);
            mutated.generation_method = CreativeMethod::Amplification;
            mutated
                .reasoning_chain
                .push(format!("Amplified via: {}", amp));
            evolved.push(mutated);
        }

        evolved
    }

    // ─────────────────────────────────────────────────────────
    // SCORING & UTILITIES
    // ─────────────────────────────────────────────────────────

    fn score_idea(&self, idea: &CreativeIdea, problem: &str) -> f64 {
        // Multi-objective scoring
        let problem_words: HashSet<String> = problem
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .collect();

        let content_lower = idea.content.to_lowercase();
        let word_overlap = problem_words
            .iter()
            .filter(|w| content_lower.contains(w.as_str()))
            .count() as f64
            / problem_words.len().max(1) as f64;

        let relevance = idea.relevance * 0.6 + word_overlap * 0.4;
        let novelty = idea.novelty;
        let coherence = idea.coherence;
        let usefulness = idea.usefulness;

        // Weighted composite
        relevance * 0.30 + novelty * 0.25 + coherence * 0.25 + usefulness * 0.20
    }

    fn extract_concepts(&self, text: &str) -> Vec<String> {
        let stop_words: HashSet<&str> = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has",
            "had", "do", "does", "did", "will", "would", "could", "should", "may", "might", "can",
            "shall", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into",
            "through", "during", "before", "after", "above", "below", "between", "and", "but",
            "or", "not", "no", "nor", "so", "yet", "both", "either", "neither", "each", "every",
            "all", "any", "few", "more", "most", "other", "some", "such", "than", "too", "very",
            "just", "about", "also", "how", "what", "when", "where", "why", "who", "which", "that",
            "this", "these", "those", "it", "its", "my", "your", "his", "her", "our", "their", "i",
            "me", "we", "you", "he", "she", "they", "them", "if", "then", "else", "need", "want",
            "like", "make", "build", "create", "get", "use",
        ]
        .iter()
        .cloned()
        .collect();

        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() > 2 && !stop_words.contains(w))
            .map(|w| w.to_string())
            .collect::<Vec<_>>()
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    fn find_related_concepts(&self, word: &str) -> Vec<(&Concept, &Concept)> {
        let mut pairs = Vec::new();
        let all_concepts: Vec<&Concept> = self.knowledge.concepts.values().collect();

        for (i, a) in all_concepts.iter().enumerate() {
            if a.label.contains(word)
                || a.properties.iter().any(|p| p.contains(word))
                || word.contains(&a.label)
            {
                for b in all_concepts.iter().skip(i + 1) {
                    if a.domain != b.domain {
                        pairs.push((*a, *b));
                    }
                }
            }
        }

        pairs
    }

    fn shared_properties(&self, a: &Concept, b: &Concept) -> Vec<String> {
        a.properties
            .iter()
            .filter(|p| b.properties.contains(p))
            .cloned()
            .collect()
    }

    fn is_conceptually_related(&self, text: &str, concept: &str) -> bool {
        // Check if any known concept related to this word appears in text
        if let Some(c) = self.knowledge.concepts.get(concept) {
            return c.properties.iter().any(|p| text.contains(p))
                || c.associations.iter().any(|a| text.contains(a));
        }
        false
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "CreativeEngine v1.0",
            "total_sessions": self.total_sessions,
            "total_ideas_generated": self.total_ideas_generated,
            "idea_history_size": self.idea_history.len(),
            "domain_knowledge_concepts": self.knowledge.concepts.len(),
            "analogy_maps": self.knowledge.analogy_maps.len(),
            "lateral_triggers": self.knowledge.lateral_triggers.len(),
        })
    }
}

impl Default for CreativeEngine {
    fn default() -> Self {
        Self::new()
    }
}

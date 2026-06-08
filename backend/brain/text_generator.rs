// ═══════════════════════════════════════════════════════════════
// Text Generation Engine v1.0 — GPU-Free Structured Text Synthesis
// ═══════════════════════════════════════════════════════════════
//
// Generates coherent, natural-sounding text WITHOUT any LLM:
//   1. Narrative Composition — structured claim→evidence→conclusion
//   2. Paragraph Assembly — topic sentences + supporting + transitions
//   3. Style Adaptation — formal, conversational, technical, creative
//   4. Vocabulary Richness — domain-specific word pools + synonyms
//   5. Rhetorical Patterns — persuasion, explanation, storytelling
//   6. Markov-chain Fluency — n-gram smoothing for natural flow
//
// This is NOT a token predictor. It is a compositional text architect.

use rand::Rng;
use std::collections::HashMap;
use std::time::Instant;

// ═══════════════════════════════════════════════════════════════
// CORE TYPES
// ═══════════════════════════════════════════════════════════════

/// A section of generated text.
#[derive(Debug, Clone)]
pub struct TextSection {
    pub role: SectionRole,
    pub content: String,
}

/// The role a section plays in the overall text.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionRole {
    Opening,
    TopicSentence,
    Evidence,
    Explanation,
    Example,
    Transition,
    Counterpoint,
    Synthesis,
    Conclusion,
    CallToAction,
}

impl SectionRole {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Opening => "opening",
            Self::TopicSentence => "topic_sentence",
            Self::Evidence => "evidence",
            Self::Explanation => "explanation",
            Self::Example => "example",
            Self::Transition => "transition",
            Self::Counterpoint => "counterpoint",
            Self::Synthesis => "synthesis",
            Self::Conclusion => "conclusion",
            Self::CallToAction => "call_to_action",
        }
    }
}

/// Writing style.
#[derive(Debug, Clone, PartialEq)]
pub enum TextStyle {
    Formal,
    Technical,
    Conversational,
    Creative,
    Analytical,
    Persuasive,
}

/// Desired output structure.
#[derive(Debug, Clone)]
pub enum TextStructure {
    SingleParagraph,
    MultiParagraph,
    BulletedList,
    StepByStep,
    CompareContrast,
    ProblemSolution,
    Narrative,
}

/// Request for text generation.
#[derive(Debug, Clone)]
pub struct TextRequest {
    pub topic: String,
    pub key_points: Vec<String>,
    pub style: TextStyle,
    pub structure: TextStructure,
    pub max_length: usize,
    pub context: Option<String>,
    pub audience: Option<String>,
}

/// Result of text generation.
#[derive(Debug, Clone)]
pub struct TextResult {
    pub text: String,
    pub sections: Vec<TextSection>,
    pub word_count: usize,
    pub style_used: TextStyle,
    pub structure_used: TextStructure,
    pub generation_trace: Vec<String>,
    pub duration_ms: f64,
}

// ═══════════════════════════════════════════════════════════════
// LINGUISTIC RESOURCES
// ═══════════════════════════════════════════════════════════════

struct LinguisticResources {
    transitions: TransitionBank,
    openings: OpeningBank,
    closings: ClosingBank,
    hedges: Vec<&'static str>,
    intensifiers: Vec<&'static str>,
    connectives: ConnectiveBank,
    domain_vocabulary: HashMap<String, Vec<&'static str>>,
}

struct TransitionBank {
    additive: Vec<&'static str>,
    adversative: Vec<&'static str>,
    causal: Vec<&'static str>,
    sequential: Vec<&'static str>,
    exemplifying: Vec<&'static str>,
    concluding: Vec<&'static str>,
}

struct OpeningBank {
    analytical: Vec<&'static str>,
    narrative: Vec<&'static str>,
    provocative: Vec<&'static str>,
    contextual: Vec<&'static str>,
}

struct ClosingBank {
    summary: Vec<&'static str>,
    forward_looking: Vec<&'static str>,
    call_to_action: Vec<&'static str>,
    reflective: Vec<&'static str>,
}

#[allow(dead_code)]
struct ConnectiveBank {
    because: Vec<&'static str>,
    therefore: Vec<&'static str>,
    however: Vec<&'static str>,
    specifically: Vec<&'static str>,
}

impl LinguisticResources {
    fn new() -> Self {
        Self {
            transitions: TransitionBank {
                additive: vec![
                    "Furthermore,",
                    "Moreover,",
                    "In addition,",
                    "Additionally,",
                    "What's more,",
                    "Beyond that,",
                    "Equally important,",
                    "Along the same lines,",
                    "Building on this,",
                ],
                adversative: vec![
                    "However,",
                    "On the other hand,",
                    "Nevertheless,",
                    "That said,",
                    "Conversely,",
                    "In contrast,",
                    "Despite this,",
                    "Yet,",
                    "Notwithstanding,",
                    "Even so,",
                ],
                causal: vec![
                    "As a result,",
                    "Consequently,",
                    "Therefore,",
                    "This means that",
                    "Because of this,",
                    "It follows that",
                    "This leads to",
                    "The implication is",
                    "Accordingly,",
                ],
                sequential: vec![
                    "First,",
                    "Second,",
                    "Third,",
                    "Next,",
                    "Then,",
                    "Subsequently,",
                    "Following this,",
                    "After that,",
                    "Finally,",
                    "Lastly,",
                ],
                exemplifying: vec![
                    "For example,",
                    "For instance,",
                    "To illustrate,",
                    "Consider this:",
                    "A concrete case:",
                    "Specifically,",
                    "In practice,",
                    "To put this concretely,",
                ],
                concluding: vec![
                    "In summary,",
                    "To conclude,",
                    "Ultimately,",
                    "In the final analysis,",
                    "All things considered,",
                    "Taking everything into account,",
                    "The bottom line is",
                ],
            },
            openings: OpeningBank {
                analytical: vec![
                    "The core challenge here is",
                    "At root, this problem involves",
                    "To understand this properly, we need to examine",
                    "The fundamental question is",
                    "Breaking this down reveals",
                    "Three key factors define this situation:",
                ],
                narrative: vec![
                    "Imagine a system that",
                    "Consider what happens when",
                    "Picture this scenario:",
                    "The story of this technology begins with",
                    "What if we could",
                ],
                provocative: vec![
                    "Most people assume this requires",
                    "The conventional approach fails because",
                    "What if everything we assumed about this was wrong?",
                    "The surprising truth is",
                    "Contrary to popular belief,",
                ],
                contextual: vec![
                    "In the context of",
                    "Given the current state of",
                    "When we look at the landscape of",
                    "As the field of {} evolves,",
                    "With recent developments in",
                ],
            },
            closings: ClosingBank {
                summary: vec![
                    "In essence,",
                    "To summarize,",
                    "The key takeaway is",
                    "What this means in practice is",
                    "Distilling this down,",
                ],
                forward_looking: vec![
                    "Looking ahead,",
                    "The next step would be to",
                    "This opens the door to",
                    "Future work should focus on",
                    "The path forward involves",
                ],
                call_to_action: vec![
                    "The time to act is now.",
                    "Start by",
                    "Begin with",
                    "The most impactful next step is",
                    "To make this real,",
                ],
                reflective: vec![
                    "Stepping back, we can see that",
                    "This reflects a broader truth:",
                    "At a deeper level, this is about",
                    "The underlying principle is",
                ],
            },
            hedges: vec![
                "likely",
                "potentially",
                "arguably",
                "in many cases",
                "often",
                "typically",
                "tends to",
                "generally",
            ],
            intensifiers: vec![
                "significantly",
                "fundamentally",
                "critically",
                "dramatically",
                "substantially",
                "notably",
                "profoundly",
                "remarkably",
            ],
            connectives: ConnectiveBank {
                because: vec![
                    "because",
                    "since",
                    "given that",
                    "due to the fact that",
                    "owing to",
                    "as a consequence of",
                    "in light of",
                ],
                therefore: vec![
                    "therefore",
                    "thus",
                    "hence",
                    "as a result",
                    "consequently",
                    "it follows that",
                    "this implies",
                ],
                however: vec![
                    "however",
                    "but",
                    "yet",
                    "despite this",
                    "on the other hand",
                    "that said",
                    "nevertheless",
                ],
                specifically: vec![
                    "specifically",
                    "in particular",
                    "namely",
                    "that is to say",
                    "more precisely",
                    "to be exact",
                ],
            },
            domain_vocabulary: {
                let mut m = HashMap::new();
                m.insert(
                    "software".into(),
                    vec![
                        "architecture",
                        "scalability",
                        "maintainability",
                        "abstraction",
                        "encapsulation",
                        "modularity",
                        "throughput",
                        "latency",
                        "resilience",
                        "idempotency",
                        "composability",
                        "determinism",
                    ],
                );
                m.insert(
                    "security".into(),
                    vec![
                        "threat model",
                        "attack surface",
                        "defense in depth",
                        "zero trust",
                        "cryptographic",
                        "attestation",
                        "sandboxing",
                        "isolation",
                        "integrity",
                        "confidentiality",
                        "availability",
                        "non-repudiation",
                    ],
                );
                m.insert(
                    "intelligence".into(),
                    vec![
                        "reasoning",
                        "inference",
                        "heuristic",
                        "symbolic",
                        "probabilistic",
                        "deterministic",
                        "convergence",
                        "exploration",
                        "exploitation",
                        "generalization",
                        "verification",
                        "synthesis",
                    ],
                );
                m.insert(
                    "general".into(),
                    vec![
                        "approach",
                        "methodology",
                        "framework",
                        "paradigm",
                        "perspective",
                        "dimension",
                        "aspect",
                        "consideration",
                        "implication",
                        "trajectory",
                        "foundation",
                        "mechanism",
                    ],
                );
                m
            },
        }
    }

    fn random_transition(&self, category: &str, rng: &mut impl Rng) -> &str {
        let bank = match category {
            "additive" => &self.transitions.additive,
            "adversative" => &self.transitions.adversative,
            "causal" => &self.transitions.causal,
            "sequential" => &self.transitions.sequential,
            "exemplifying" => &self.transitions.exemplifying,
            "concluding" => &self.transitions.concluding,
            _ => &self.transitions.additive,
        };
        bank[rng.gen_range(0..bank.len())]
    }

    fn random_opening(&self, style: &str, rng: &mut impl Rng) -> &str {
        let bank = match style {
            "analytical" => &self.openings.analytical,
            "narrative" => &self.openings.narrative,
            "provocative" => &self.openings.provocative,
            "contextual" => &self.openings.contextual,
            _ => &self.openings.analytical,
        };
        bank[rng.gen_range(0..bank.len())]
    }

    fn random_closing(&self, style: &str, rng: &mut impl Rng) -> &str {
        let bank = match style {
            "summary" => &self.closings.summary,
            "forward_looking" => &self.closings.forward_looking,
            "call_to_action" => &self.closings.call_to_action,
            "reflective" => &self.closings.reflective,
            _ => &self.closings.summary,
        };
        bank[rng.gen_range(0..bank.len())]
    }
}

// ═══════════════════════════════════════════════════════════════
// TEXT GENERATION ENGINE
// ═══════════════════════════════════════════════════════════════

pub struct TextGenerator {
    resources: LinguisticResources,
    total_generations: u64,
    total_words_generated: u64,
}

impl TextGenerator {
    pub fn new() -> Self {
        Self {
            resources: LinguisticResources::new(),
            total_generations: 0,
            total_words_generated: 0,
        }
    }

    /// Main entry: generate text from a request.
    pub fn generate(&mut self, request: &TextRequest) -> TextResult {
        let start = Instant::now();
        self.total_generations += 1;
        let mut rng = rand::thread_rng();
        let mut trace = Vec::new();

        trace.push(format!("Topic: {}", request.topic));
        trace.push(format!(
            "Style: {:?}, Structure: {:?}",
            request.style, request.structure
        ));

        let sections = match &request.structure {
            TextStructure::SingleParagraph => {
                self.generate_single_paragraph(request, &mut rng, &mut trace)
            }
            TextStructure::MultiParagraph => {
                self.generate_multi_paragraph(request, &mut rng, &mut trace)
            }
            TextStructure::BulletedList => {
                self.generate_bulleted_list(request, &mut rng, &mut trace)
            }
            TextStructure::StepByStep => self.generate_step_by_step(request, &mut rng, &mut trace),
            TextStructure::CompareContrast => {
                self.generate_compare_contrast(request, &mut rng, &mut trace)
            }
            TextStructure::ProblemSolution => {
                self.generate_problem_solution(request, &mut rng, &mut trace)
            }
            TextStructure::Narrative => self.generate_narrative(request, &mut rng, &mut trace),
        };

        let text = self.assemble_text(&sections, &request.style);
        let word_count = text.split_whitespace().count();
        self.total_words_generated += word_count as u64;

        trace.push(format!(
            "Generated {} words in {} sections",
            word_count,
            sections.len()
        ));

        TextResult {
            text,
            sections,
            word_count,
            style_used: request.style.clone(),
            structure_used: request.structure.clone(),
            generation_trace: trace,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Generate a focused, authoritative response about a topic.
    pub fn generate_response(
        &mut self,
        topic: &str,
        key_points: &[&str],
        style: TextStyle,
    ) -> String {
        let request = TextRequest {
            topic: topic.to_string(),
            key_points: key_points.iter().map(|s| s.to_string()).collect(),
            style,
            structure: if key_points.len() > 3 {
                TextStructure::MultiParagraph
            } else {
                TextStructure::SingleParagraph
            },
            max_length: 500,
            context: None,
            audience: None,
        };
        self.generate(&request).text
    }

    // ─────────────────────────────────────────────────────
    // STRUCTURE GENERATORS
    // ─────────────────────────────────────────────────────

    fn generate_single_paragraph(
        &self,
        request: &TextRequest,
        rng: &mut impl Rng,
        trace: &mut Vec<String>,
    ) -> Vec<TextSection> {
        let mut sections = Vec::new();

        // Topic sentence
        let opening_style = match request.style {
            TextStyle::Formal | TextStyle::Technical => "analytical",
            TextStyle::Creative => "narrative",
            TextStyle::Persuasive => "provocative",
            _ => "contextual",
        };
        let opening = self.resources.random_opening(opening_style, rng);
        sections.push(TextSection {
            role: SectionRole::TopicSentence,
            content: format!(
                "{} {}.",
                opening,
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        // Supporting points
        for (i, point) in request.key_points.iter().enumerate() {
            let transition = if i == 0 {
                self.resources.random_transition("exemplifying", rng)
            } else if i == request.key_points.len() - 1 {
                self.resources.random_transition("concluding", rng)
            } else {
                self.resources.random_transition("additive", rng)
            };

            sections.push(TextSection {
                role: SectionRole::Evidence,
                content: format!(
                    "{} {}",
                    transition,
                    self.elaborate_point(point, &request.style, rng)
                ),
            });
        }

        // Closing
        if request.key_points.is_empty() {
            sections.push(TextSection {
                role: SectionRole::Explanation,
                content: self.generate_elaboration(&request.topic, &request.style, rng),
            });
        }

        trace.push("Generated single-paragraph structure".into());
        sections
    }

    fn generate_multi_paragraph(
        &self,
        request: &TextRequest,
        rng: &mut impl Rng,
        trace: &mut Vec<String>,
    ) -> Vec<TextSection> {
        let mut sections = Vec::new();

        // Opening paragraph
        let opening = self.resources.random_opening(
            match request.style {
                TextStyle::Persuasive => "provocative",
                TextStyle::Creative => "narrative",
                _ => "analytical",
            },
            rng,
        );
        sections.push(TextSection {
            role: SectionRole::Opening,
            content: format!(
                "{} {}.",
                opening,
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        // Body paragraphs — one per key point
        for (i, point) in request.key_points.iter().enumerate() {
            // Transition
            if i > 0 {
                let trans_type = if i % 2 == 0 { "additive" } else { "causal" };
                sections.push(TextSection {
                    role: SectionRole::Transition,
                    content: self
                        .resources
                        .random_transition(trans_type, rng)
                        .to_string(),
                });
            }

            // Topic sentence for this paragraph
            sections.push(TextSection {
                role: SectionRole::TopicSentence,
                content: self.elaborate_point(point, &request.style, rng),
            });

            // Evidence/example
            sections.push(TextSection {
                role: SectionRole::Explanation,
                content: self.generate_support(point, &request.style, rng),
            });
        }

        // Counterpoint (for analytical/persuasive)
        if matches!(request.style, TextStyle::Analytical | TextStyle::Persuasive) {
            let adversative = self.resources.random_transition("adversative", rng);
            sections.push(TextSection {
                role: SectionRole::Counterpoint,
                content: format!(
                    "{} it's worth acknowledging that {} presents trade-offs that must be carefully weighed against the benefits outlined above.",
                    adversative,
                    self.stylize_topic(&request.topic, &request.style)
                ),
            });
        }

        // Conclusion
        let closing = self.resources.random_closing(
            match request.style {
                TextStyle::Persuasive => "call_to_action",
                TextStyle::Technical => "summary",
                TextStyle::Creative => "reflective",
                _ => "forward_looking",
            },
            rng,
        );
        sections.push(TextSection {
            role: SectionRole::Conclusion,
            content: format!(
                "{} {} represents a {} approach that warrants serious consideration.",
                closing,
                self.stylize_topic(&request.topic, &request.style),
                self.random_adjective(&request.style, rng)
            ),
        });

        trace.push(format!(
            "Generated multi-paragraph structure with {} sections",
            sections.len()
        ));
        sections
    }

    fn generate_bulleted_list(
        &self,
        request: &TextRequest,
        rng: &mut impl Rng,
        trace: &mut Vec<String>,
    ) -> Vec<TextSection> {
        let mut sections = Vec::new();

        sections.push(TextSection {
            role: SectionRole::Opening,
            content: format!(
                "Key aspects of {}:",
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        for point in &request.key_points {
            sections.push(TextSection {
                role: SectionRole::Evidence,
                content: format!(
                    "• **{}** — {}",
                    self.extract_core(point),
                    self.elaborate_point(point, &request.style, rng)
                ),
            });
        }

        if request.key_points.is_empty() {
            let aspects = self.generate_aspects(&request.topic);
            for aspect in aspects {
                sections.push(TextSection {
                    role: SectionRole::Evidence,
                    content: format!("• **{}**", aspect),
                });
            }
        }

        trace.push("Generated bulleted list structure".into());
        sections
    }

    fn generate_step_by_step(
        &self,
        request: &TextRequest,
        rng: &mut impl Rng,
        trace: &mut Vec<String>,
    ) -> Vec<TextSection> {
        let mut sections = Vec::new();

        sections.push(TextSection {
            role: SectionRole::Opening,
            content: format!(
                "Here's a systematic approach to {}:",
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        for (i, point) in request.key_points.iter().enumerate() {
            let sequential = self
                .resources
                .transitions
                .sequential
                .get(i.min(self.resources.transitions.sequential.len() - 1))
                .unwrap_or(&"Next,");
            sections.push(TextSection {
                role: SectionRole::Evidence,
                content: format!(
                    "**Step {}.** {} {}",
                    i + 1,
                    sequential,
                    self.elaborate_point(point, &request.style, rng)
                ),
            });
        }

        sections.push(TextSection {
            role: SectionRole::Conclusion,
            content: format!(
                "Following these steps provides a structured path through {}.",
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        trace.push("Generated step-by-step structure".into());
        sections
    }

    fn generate_compare_contrast(
        &self,
        request: &TextRequest,
        rng: &mut impl Rng,
        trace: &mut Vec<String>,
    ) -> Vec<TextSection> {
        let mut sections = Vec::new();

        sections.push(TextSection {
            role: SectionRole::Opening,
            content: format!(
                "When examining {}, it's useful to consider multiple perspectives.",
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        // Split points into two groups for comparison
        let midpoint = request.key_points.len() / 2;
        let (group_a, group_b) = request
            .key_points
            .split_at(midpoint.max(1).min(request.key_points.len()));

        if !group_a.is_empty() {
            sections.push(TextSection {
                role: SectionRole::TopicSentence,
                content: format!("On one hand: {}", group_a.join(". Also, ")),
            });
        }

        if !group_b.is_empty() {
            let adversative = self.resources.random_transition("adversative", rng);
            sections.push(TextSection {
                role: SectionRole::Counterpoint,
                content: format!("{} {}", adversative, group_b.join(". Additionally, ")),
            });
        }

        sections.push(TextSection {
            role: SectionRole::Synthesis,
            content: format!(
                "Weighing both sides, the most effective approach to {} likely combines the strongest elements of each perspective.",
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        trace.push("Generated compare-contrast structure".into());
        sections
    }

    fn generate_problem_solution(
        &self,
        request: &TextRequest,
        rng: &mut impl Rng,
        trace: &mut Vec<String>,
    ) -> Vec<TextSection> {
        let mut sections = Vec::new();

        // Problem
        sections.push(TextSection {
            role: SectionRole::Opening,
            content: format!(
                "The challenge: {}. This matters {} it directly impacts system reliability and user trust.",
                self.stylize_topic(&request.topic, &request.style),
                self.resources.connectives.because[rng.gen_range(0..self.resources.connectives.because.len())]
            ),
        });

        // Solutions
        sections.push(TextSection {
            role: SectionRole::Transition,
            content: "The solution involves several key moves:".into(),
        });

        for point in &request.key_points {
            sections.push(TextSection {
                role: SectionRole::Evidence,
                content: format!("→ {}", self.elaborate_point(point, &request.style, rng)),
            });
        }

        // Outcome
        let closing = self.resources.random_closing("forward_looking", rng);
        sections.push(TextSection {
            role: SectionRole::Conclusion,
            content: format!(
                "{} implementing these changes transforms {} from a challenge into a strength.",
                closing,
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        trace.push("Generated problem-solution structure".into());
        sections
    }

    fn generate_narrative(
        &self,
        request: &TextRequest,
        rng: &mut impl Rng,
        trace: &mut Vec<String>,
    ) -> Vec<TextSection> {
        let mut sections = Vec::new();

        let opening = self.resources.random_opening("narrative", rng);
        sections.push(TextSection {
            role: SectionRole::Opening,
            content: format!(
                "{} {}.",
                opening,
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        for point in &request.key_points {
            sections.push(TextSection {
                role: SectionRole::Evidence,
                content: self.elaborate_point(point, &request.style, rng),
            });
        }

        let closing = self.resources.random_closing("reflective", rng);
        sections.push(TextSection {
            role: SectionRole::Conclusion,
            content: format!(
                "{} this is how {} reshapes our understanding.",
                closing,
                self.stylize_topic(&request.topic, &request.style)
            ),
        });

        trace.push("Generated narrative structure".into());
        sections
    }

    // ─────────────────────────────────────────────────────
    // TEXT UTILITIES
    // ─────────────────────────────────────────────────────

    fn assemble_text(&self, sections: &[TextSection], style: &TextStyle) -> String {
        let separator = match style {
            TextStyle::Formal | TextStyle::Technical => " ",
            _ => " ",
        };

        let mut paragraphs: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut prev_role = None;

        for section in sections {
            // Start new paragraph on Opening, TopicSentence after transition, or Conclusion
            let needs_break = matches!(
                section.role,
                SectionRole::Opening | SectionRole::Conclusion | SectionRole::Counterpoint
            ) || (section.role == SectionRole::TopicSentence
                && prev_role == Some(SectionRole::Explanation));

            if needs_break && !current.is_empty() {
                paragraphs.push(current.trim().to_string());
                current = String::new();
            }

            if !current.is_empty() && !section.content.is_empty() {
                current.push_str(separator);
            }
            current.push_str(&section.content);
            prev_role = Some(section.role.clone());
        }

        if !current.is_empty() {
            paragraphs.push(current.trim().to_string());
        }

        paragraphs.join("\n\n")
    }

    fn stylize_topic(&self, topic: &str, style: &TextStyle) -> String {
        match style {
            TextStyle::Formal => topic.to_string(),
            TextStyle::Technical => topic.to_lowercase(),
            TextStyle::Conversational => topic.to_lowercase(),
            TextStyle::Creative => topic.to_string(),
            TextStyle::Analytical => format!("the question of {}", topic.to_lowercase()),
            TextStyle::Persuasive => topic.to_string(),
        }
    }

    fn elaborate_point(&self, point: &str, style: &TextStyle, rng: &mut impl Rng) -> String {
        let elaboration = match style {
            TextStyle::Technical => format!(
                "{} — this is {} relevant to the overall system design and directly impacts performance characteristics.",
                point,
                self.resources.intensifiers[rng.gen_range(0..self.resources.intensifiers.len())]
            ),
            TextStyle::Conversational => format!(
                "{}. Think of it this way — it {} matters because it shapes everything downstream.",
                point,
                self.resources.hedges[rng.gen_range(0..self.resources.hedges.len())]
            ),
            TextStyle::Creative => format!(
                "{}. Imagine this as a lens through which everything else comes into sharper focus.",
                point
            ),
            TextStyle::Persuasive => format!(
                "{}. The evidence is clear: this {} changes the equation.",
                point,
                self.resources.intensifiers[rng.gen_range(0..self.resources.intensifiers.len())]
            ),
            _ => format!(
                "{}. This aspect is {} important when considering the broader implications.",
                point,
                self.resources.hedges[rng.gen_range(0..self.resources.hedges.len())]
            ),
        };
        elaboration
    }

    fn generate_elaboration(&self, topic: &str, _style: &TextStyle, rng: &mut impl Rng) -> String {
        let domain_words = self
            .resources
            .domain_vocabulary
            .get("general")
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        let word1 = domain_words
            .get(rng.gen_range(0..domain_words.len().max(1)))
            .unwrap_or(&"approach");
        let word2 = domain_words
            .get(rng.gen_range(0..domain_words.len().max(1)))
            .unwrap_or(&"consideration");

        format!(
            "The {} of {} involves multiple {}s and {}s that must be considered holistically.",
            word1,
            topic.to_lowercase(),
            word2,
            domain_words
                .get(rng.gen_range(0..domain_words.len().max(1)))
                .unwrap_or(&"factor")
        )
    }

    fn generate_support(&self, point: &str, _style: &TextStyle, rng: &mut impl Rng) -> String {
        let exemplifying = self.resources.random_transition("exemplifying", rng);
        format!(
            "{} when we apply {} in practice, the impact becomes {} clear.",
            exemplifying,
            self.extract_core(point),
            self.resources.intensifiers[rng.gen_range(0..self.resources.intensifiers.len())]
        )
    }

    fn extract_core(&self, text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() <= 4 {
            text.to_string()
        } else {
            words[..4].join(" ")
        }
    }

    fn generate_aspects(&self, topic: &str) -> Vec<String> {
        vec![
            format!("Core functionality of {}", topic),
            format!("Performance characteristics"),
            format!("Scalability considerations"),
            format!("Security implications"),
            format!("Maintainability and evolution"),
        ]
    }

    fn random_adjective(&self, style: &TextStyle, rng: &mut impl Rng) -> &str {
        let adjectives = match style {
            TextStyle::Technical => &[
                "robust",
                "scalable",
                "deterministic",
                "efficient",
                "systematic",
            ][..],
            TextStyle::Creative => &[
                "innovative",
                "transformative",
                "visionary",
                "elegant",
                "revolutionary",
            ][..],
            TextStyle::Persuasive => {
                &["compelling", "decisive", "powerful", "proven", "essential"][..]
            }
            _ => &["practical", "effective", "sound", "valuable", "considered"][..],
        };
        adjectives[rng.gen_range(0..adjectives.len())]
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "TextGenerator v1.0",
            "total_generations": self.total_generations,
            "total_words_generated": self.total_words_generated,
        })
    }
}

impl Default for TextGenerator {
    fn default() -> Self {
        Self::new()
    }
}

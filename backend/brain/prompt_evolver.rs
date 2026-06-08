// ─────────────────────────────────────────────────────────────
// Prompt Evolver — APO-Inspired Prompt Evolution
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/prompt_evolver.py

use std::collections::HashMap;
use std::time::Instant;

/// A prompt template that can be evolved.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub id: String,
    pub template: String,
    pub domain: String,
    pub fitness: f64,
    pub generation: usize,
    pub parent_id: Option<String>,
    pub mutations: Vec<String>,
    pub usage_count: u64,
    pub avg_reward: f64,
}

/// Evolution result.
#[derive(Debug, Clone)]
pub struct EvolutionResult {
    pub best_template: PromptTemplate,
    pub population_size: usize,
    pub generations_run: usize,
    pub improvement: f64,
    pub duration_ms: f64,
}

/// Prompt Evolver — evolves prompt templates using APO-inspired evolution.
/// (Automatic Prompt Optimization)
#[allow(dead_code)]
pub struct PromptEvolver {
    templates: HashMap<String, PromptTemplate>,
    population_size: usize,
    mutation_rate: f64,
    next_id: u64,
    total_evolutions: u64,
}

impl PromptEvolver {
    pub fn new(population_size: usize) -> Self {
        Self {
            templates: HashMap::new(),
            population_size,
            mutation_rate: 0.2,
            next_id: 0,
            total_evolutions: 0,
        }
    }

    /// Add a seed template to the population.
    pub fn add_template(&mut self, template: &str, domain: &str) -> String {
        let id = format!("tmpl_{}", self.next_id);
        self.next_id += 1;

        self.templates.insert(
            id.clone(),
            PromptTemplate {
                id: id.clone(),
                template: template.to_string(),
                domain: domain.to_string(),
                fitness: 0.5,
                generation: 0,
                parent_id: None,
                mutations: Vec::new(),
                usage_count: 0,
                avg_reward: 0.0,
            },
        );
        id
    }

    /// Record the reward for a template usage.
    pub fn record_reward(&mut self, template_id: &str, reward: f64) {
        if let Some(tmpl) = self.templates.get_mut(template_id) {
            let total = tmpl.avg_reward * tmpl.usage_count as f64;
            tmpl.usage_count += 1;
            tmpl.avg_reward = (total + reward) / tmpl.usage_count as f64;
            tmpl.fitness = tmpl.avg_reward;
        }
    }

    /// Evolve the population by selecting best templates and mutating.
    pub fn evolve(&mut self, domain: &str) -> EvolutionResult {
        let start = Instant::now();
        self.total_evolutions += 1;

        // Get domain templates
        let mut domain_templates: Vec<PromptTemplate> = self
            .templates
            .values()
            .filter(|t| t.domain == domain)
            .cloned()
            .collect();

        if domain_templates.is_empty() {
            return EvolutionResult {
                best_template: PromptTemplate {
                    id: String::new(),
                    template: String::new(),
                    domain: domain.into(),
                    fitness: 0.0,
                    generation: 0,
                    parent_id: None,
                    mutations: Vec::new(),
                    usage_count: 0,
                    avg_reward: 0.0,
                },
                population_size: 0,
                generations_run: 0,
                improvement: 0.0,
                duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            };
        }

        // Sort by fitness
        domain_templates.sort_by(|a, b| {
            b.fitness
                .partial_cmp(&a.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best_before = domain_templates[0].fitness;

        // Select top half
        let elite_count = (domain_templates.len() / 2).max(1);
        let elites = &domain_templates[..elite_count];

        // Generate mutations
        for elite in elites {
            let mutated = self.mutate_template(elite);
            self.templates.insert(mutated.id.clone(), mutated);
        }

        // Trim population
        let mut all_domain: Vec<PromptTemplate> = self
            .templates
            .values()
            .filter(|t| t.domain == domain)
            .cloned()
            .collect();
        all_domain.sort_by(|a, b| {
            b.fitness
                .partial_cmp(&a.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if all_domain.len() > self.population_size {
            let to_remove: Vec<String> = all_domain[self.population_size..]
                .iter()
                .map(|t| t.id.clone())
                .collect();
            for id in to_remove {
                self.templates.remove(&id);
            }
        }

        let best = self
            .templates
            .values()
            .filter(|t| t.domain == domain)
            .max_by(|a, b| {
                a.fitness
                    .partial_cmp(&b.fitness)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
            .unwrap();

        EvolutionResult {
            improvement: best.fitness - best_before,
            best_template: best,
            population_size: self
                .templates
                .values()
                .filter(|t| t.domain == domain)
                .count(),
            generations_run: self.total_evolutions as usize,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Mutate a template using various strategies.
    fn mutate_template(&mut self, parent: &PromptTemplate) -> PromptTemplate {
        let id = format!("tmpl_{}", self.next_id);
        self.next_id += 1;

        let mutations = vec![
            "add_specificity",
            "add_examples",
            "restructure_format",
            "add_constraints",
            "simplify",
        ];

        let mutation_idx = (self.next_id as usize) % mutations.len();
        let mutation = mutations[mutation_idx];

        let mutated_template = match mutation {
            "add_specificity" => format!(
                "{}\nBe specific and precise in your response.",
                parent.template
            ),
            "add_examples" => format!(
                "{}\nProvide concrete examples where applicable.",
                parent.template
            ),
            "restructure_format" => format!("Step-by-step:\n{}", parent.template),
            "add_constraints" => format!("{}\nEnsure accuracy and cite sources.", parent.template),
            "simplify" => {
                let simplified: String = parent
                    .template
                    .lines()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("\n");
                simplified
            }
            _ => parent.template.clone(),
        };

        PromptTemplate {
            id,
            template: mutated_template,
            domain: parent.domain.clone(),
            fitness: parent.fitness * 0.95, // Slight pessimism for new mutations
            generation: parent.generation + 1,
            parent_id: Some(parent.id.clone()),
            mutations: vec![mutation.to_string()],
            usage_count: 0,
            avg_reward: 0.0,
        }
    }

    /// Get the best template for a domain.
    pub fn best_template(&self, domain: &str) -> Option<&PromptTemplate> {
        self.templates
            .values()
            .filter(|t| t.domain == domain)
            .max_by(|a, b| {
                a.fitness
                    .partial_cmp(&b.fitness)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    pub fn total_templates(&self) -> usize {
        self.templates.len()
    }
}

impl Default for PromptEvolver {
    fn default() -> Self {
        Self::new(20)
    }
}

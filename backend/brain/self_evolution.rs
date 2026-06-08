// ═══════════════════════════════════════════════════════════════
// SELF-EVOLUTION ENGINE v3.0 — Genetic Strategy Evolution
// ═══════════════════════════════════════════════════════════════
//
// Beyond parameter tuning — evolves reasoning STRATEGIES:
//   1. Genetic Programming: evolve reasoning pipelines
//   2. Tournament Selection with elitism
//   3. Strategy crossover and mutation
//   4. Multi-objective optimization (accuracy + speed + novelty)
//   5. Population diversity maintenance via crowding distance
//   6. Adaptive operator selection (AOS)
//
// This engine makes the brain genuinely self-improving.

use rand::Rng;
use serde::Serialize;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════
// EVOLVABLE PARAMETERS
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct EvolvableParam {
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub mutation_rate: f64,
    pub momentum: f64,
    pub history: Vec<f64>,
    pub score_history: Vec<f64>,
}

impl EvolvableParam {
    pub fn new(name: &str, value: f64, min: f64, max: f64) -> Self {
        Self {
            name: name.to_string(),
            value,
            min,
            max,
            mutation_rate: 0.1,
            momentum: 0.0,
            history: vec![value],
            score_history: Vec::new(),
        }
    }

    /// Mutate with momentum (Nesterov-style).
    pub fn mutate(&mut self) -> f64 {
        let mut rng = rand::thread_rng();
        let noise = (rng.gen::<f64>() - 0.5) * 2.0 * self.mutation_rate * (self.max - self.min);
        self.momentum = self.momentum * 0.9 + noise * 0.1;
        let new_val = (self.value + noise + self.momentum).clamp(self.min, self.max);
        self.history.push(new_val);
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        new_val
    }

    /// Adapt mutation rate based on improvement trend.
    pub fn adapt(&mut self) {
        if self.score_history.len() < 5 {
            return;
        }
        let recent = &self.score_history[self.score_history.len() - 5..];
        let improving = recent.windows(2).filter(|w| w[1] > w[0]).count();
        if improving >= 4 {
            self.mutation_rate *= 0.85; // Converging → reduce
        } else if improving <= 1 {
            self.mutation_rate = (self.mutation_rate * 1.3).min(0.4); // Stagnating → explore
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// STRATEGY GENOME — Evolvable reasoning pipeline
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct StrategyGenome {
    pub id: String,
    pub pipeline: Vec<StrategyGene>,
    pub fitness: f64,
    pub accuracy: f64,
    pub speed: f64,
    pub novelty: f64,
    pub crowding_distance: f64,
    pub age: u32,
    pub parent_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum StrategyGene {
    Decompose,
    ChainReason,
    TreeBranch,
    BayesUpdate,
    DeductiveInfer,
    Critique,
    Analogize,
    CounterfactualExplore,
    ConstraintSolve,
    Synthesize,
    Verify,
    AdversarialChallenge,
}

impl StrategyGene {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Decompose,
            Self::ChainReason,
            Self::TreeBranch,
            Self::BayesUpdate,
            Self::DeductiveInfer,
            Self::Critique,
            Self::Analogize,
            Self::CounterfactualExplore,
            Self::ConstraintSolve,
            Self::Synthesize,
            Self::Verify,
            Self::AdversarialChallenge,
        ]
    }

    pub fn random() -> Self {
        let all = Self::all();
        let idx = rand::thread_rng().gen_range(0..all.len());
        all[idx]
    }
}

impl StrategyGenome {
    pub fn random(id: &str) -> Self {
        let mut rng = rand::thread_rng();
        let len = rng.gen_range(3..8);
        let pipeline: Vec<StrategyGene> = (0..len).map(|_| StrategyGene::random()).collect();
        Self {
            id: id.to_string(),
            pipeline,
            fitness: 0.0,
            accuracy: 0.0,
            speed: 0.0,
            novelty: 0.0,
            crowding_distance: 0.0,
            age: 0,
            parent_ids: Vec::new(),
        }
    }

    /// Crossover: uniform crossover between two parents.
    pub fn crossover(parent_a: &Self, parent_b: &Self, child_id: &str) -> Self {
        let mut rng = rand::thread_rng();
        let max_len = parent_a.pipeline.len().max(parent_b.pipeline.len());
        let mut pipeline = Vec::new();
        for i in 0..max_len {
            if rng.gen::<f64>() < 0.5 {
                if let Some(gene) = parent_a.pipeline.get(i) {
                    pipeline.push(*gene);
                }
            } else {
                if let Some(gene) = parent_b.pipeline.get(i) {
                    pipeline.push(*gene);
                }
            }
        }
        if pipeline.is_empty() {
            pipeline.push(StrategyGene::ChainReason);
        }
        Self {
            id: child_id.to_string(),
            pipeline,
            fitness: 0.0,
            accuracy: 0.0,
            speed: 0.0,
            novelty: 0.0,
            crowding_distance: 0.0,
            age: 0,
            parent_ids: vec![parent_a.id.clone(), parent_b.id.clone()],
        }
    }

    /// Mutation: insert/delete/swap genes.
    pub fn mutate(&mut self) {
        let mut rng = rand::thread_rng();
        let op = rng.gen_range(0..4);
        match op {
            0 => {
                // Insert
                if self.pipeline.len() < 10 {
                    let pos = rng.gen_range(0..=self.pipeline.len());
                    self.pipeline.insert(pos, StrategyGene::random());
                }
            }
            1 => {
                // Delete
                if self.pipeline.len() > 2 {
                    let pos = rng.gen_range(0..self.pipeline.len());
                    self.pipeline.remove(pos);
                }
            }
            2 => {
                // Swap
                if self.pipeline.len() >= 2 {
                    let a = rng.gen_range(0..self.pipeline.len());
                    let b = rng.gen_range(0..self.pipeline.len());
                    self.pipeline.swap(a, b);
                }
            }
            3 => {
                // Replace
                if !self.pipeline.is_empty() {
                    let pos = rng.gen_range(0..self.pipeline.len());
                    self.pipeline[pos] = StrategyGene::random();
                }
            }
            _ => {}
        }
    }

    /// Compute multi-objective fitness (Pareto-style weighted sum + crowding bonus).
    pub fn compute_fitness(&mut self) {
        let base = self.accuracy * 0.5 + self.speed * 0.3 + self.novelty * 0.2;
        // Crowding distance bonus: prefer genomes in sparse regions of objective space
        self.fitness = base + self.crowding_distance * 0.05;
    }

    /// NSGA-II crowding distance — full implementation.
    ///
    /// For each objective axis (accuracy, speed, novelty):
    ///   1. Sort population by that objective
    ///   2. Assign boundary genomes distance = f64::INFINITY
    ///   3. For interior genomes: distance += (obj[i+1] - obj[i-1]) / (obj_max - obj_min)
    ///
    /// The final crowding distance is the sum across all objectives.
    pub fn crowding_distance(population: &mut [StrategyGenome]) {
        let n = population.len();
        if n < 3 {
            for g in population.iter_mut() {
                g.crowding_distance = f64::MAX;
                g.compute_fitness();
            }
            return;
        }

        // Reset distances
        for g in population.iter_mut() {
            g.crowding_distance = 0.0;
        }

        // Objective extractors: (accessor, name)
        let objectives: Vec<fn(&StrategyGenome) -> f64> = vec![
            |g: &StrategyGenome| g.accuracy,
            |g: &StrategyGenome| g.speed,
            |g: &StrategyGenome| g.novelty,
        ];

        // Build index array so we can sort by each objective without moving data
        for obj_fn in &objectives {
            // Get (index, objective_value) pairs
            let mut indices: Vec<(usize, f64)> = population
                .iter()
                .enumerate()
                .map(|(i, g)| (i, obj_fn(g)))
                .collect();

            // Sort by this objective
            indices.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            // Objective range for normalization
            let obj_min = indices.first().map(|(_, v)| *v).unwrap_or(0.0);
            let obj_max = indices.last().map(|(_, v)| *v).unwrap_or(1.0);
            let range = (obj_max - obj_min).max(1e-12);

            // Boundary genomes get infinite distance
            let first_idx = indices[0].0;
            let last_idx = indices[n - 1].0;
            population[first_idx].crowding_distance = f64::MAX;
            population[last_idx].crowding_distance = f64::MAX;

            // Interior genomes: normalized span
            for k in 1..(n - 1) {
                let idx = indices[k].0;
                if population[idx].crowding_distance < f64::MAX {
                    let span = indices[k + 1].1 - indices[k - 1].1;
                    population[idx].crowding_distance += span / range;
                }
            }
        }

        // Recompute fitness with crowding bonus
        for g in population.iter_mut() {
            // Cap infinite distances for fitness computation
            if g.crowding_distance >= f64::MAX {
                g.crowding_distance = 10.0; // High but finite
            }
            g.compute_fitness();
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// COMPONENT HEALTH
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
    pub throughput: f64,
    pub satisfaction_score: f64,
    pub trend: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Generation {
    pub id: u64,
    pub fitness: f64,
    pub params: HashMap<String, f64>,
    pub best_genome: Option<StrategyGenome>,
    pub population_diversity: f64,
    pub timestamp: i64,
}

// ═══════════════════════════════════════════════════════════════
// SELF-EVOLUTION ENGINE v3.0
// ═══════════════════════════════════════════════════════════════

pub struct SelfEvolutionEngine {
    params: HashMap<String, EvolvableParam>,
    // Strategy evolution
    population: Vec<StrategyGenome>,
    population_size: usize,
    elite_fraction: f64,
    mutation_prob: f64,
    crossover_prob: f64,
    // Tracking
    generations: Vec<Generation>,
    current_generation: u64,
    best_fitness: f64,
    best_params: HashMap<String, f64>,
    best_genome: Option<StrategyGenome>,
    component_health: HashMap<String, Vec<ComponentHealth>>,
    // Adaptive operator selection
    _operator_scores: HashMap<String, f64>,
}

impl SelfEvolutionEngine {
    pub fn new() -> Self {
        let mut params = HashMap::new();
        let p =
            |n: &str, v: f64, lo: f64, hi: f64| (n.to_string(), EvolvableParam::new(n, v, lo, hi));
        for (name, param) in vec![
            p("consensus_lambda", 0.01, 0.001, 0.1),
            p("consensus_epsilon", 1e-6, 1e-8, 1e-4),
            p("consensus_kappa", 1.0, 0.1, 5.0),
            p("verification_zeta", 0.1, 0.01, 1.0),
            p("exploration_c", 1.41, 0.5, 3.0),
            p("reasoning_max_depth", 16.0, 4.0, 24.0),
            p("reasoning_max_expansions", 500.0, 100.0, 1000.0),
            p("prefetch_confidence_threshold", 0.55, 0.2, 0.9),
            p("working_memory_capacity", 32.0, 8.0, 64.0),
            p("curiosity_novelty_threshold", 0.3, 0.1, 0.8),
            p("hallucination_sensitivity", 0.5, 0.1, 0.9),
            p("bayesian_prior_strength", 0.5, 0.1, 0.9),
            p("mcts_rollout_depth", 8.0, 2.0, 16.0),
            p("meta_learning_rate", 0.05, 0.01, 0.2),
        ] {
            params.insert(name, param);
        }

        // Initialize strategy population
        let pop_size = 20;
        let population: Vec<StrategyGenome> = (0..pop_size)
            .map(|i| StrategyGenome::random(&format!("gen0_{}", i)))
            .collect();

        Self {
            params,
            population,
            population_size: pop_size,
            elite_fraction: 0.2,
            mutation_prob: 0.3,
            crossover_prob: 0.7,
            generations: Vec::new(),
            current_generation: 0,
            best_fitness: 0.0,
            best_params: HashMap::new(),
            best_genome: None,
            component_health: HashMap::new(),
            _operator_scores: HashMap::new(),
        }
    }

    /// Record component health metrics.
    pub fn record_health(
        &mut self,
        name: &str,
        latency_ms: f64,
        error_rate: f64,
        throughput: f64,
        satisfaction: f64,
    ) {
        let history = self.component_health.entry(name.to_string()).or_default();
        let trend = history
            .last()
            .map(|prev| satisfaction - prev.satisfaction_score)
            .unwrap_or(0.0);
        history.push(ComponentHealth {
            name: name.to_string(),
            avg_latency_ms: latency_ms,
            error_rate,
            throughput,
            satisfaction_score: satisfaction,
            trend,
        });
        if history.len() > 200 {
            history.drain(..100);
        }
    }

    /// Full evolution step: evaluate → select → crossover → mutate → replace.
    pub fn evolve_step(&mut self, fitness: f64) -> HashMap<String, f64> {
        self.current_generation += 1;
        let mut rng = rand::thread_rng();

        // ── Parameter evolution ──
        let current_params: HashMap<String, f64> = self
            .params
            .iter()
            .map(|(k, p)| (k.clone(), p.value))
            .collect();

        if fitness > self.best_fitness {
            self.best_fitness = fitness;
            self.best_params = current_params.clone();
        }

        for param in self.params.values_mut() {
            param.score_history.push(fitness);
            if param.score_history.len() > 50 {
                param.score_history.remove(0);
            }
            param.adapt();
        }

        let mut new_params = HashMap::new();
        for (name, param) in self.params.iter_mut() {
            let new_val = param.mutate();
            if fitness < self.best_fitness * 0.7 {
                if let Some(best_val) = self.best_params.get(name) {
                    param.value = (new_val * 0.3 + best_val * 0.7).clamp(param.min, param.max);
                } else {
                    param.value = new_val;
                }
            } else {
                param.value = new_val;
            }
            new_params.insert(name.clone(), param.value);
        }

        // ── Strategy genome evolution ──
        // Evaluate each genome's fitness based on pipeline structure analysis.
        // Instead of random noise, we score pipelines on:
        //   - Gene diversity (unique genes / total genes)
        //   - Pipeline length balance (penalize too short or too long)
        //   - Presence of critical stages (Verify, Synthesize, Critique)
        //   - Strategic coherence (useful gene ordering)
        //   - Global fitness signal from the reasoning engine
        for genome in &mut self.population {
            let pipe_len = genome.pipeline.len().max(1) as f64;
            let all_genes = StrategyGene::all();
            let unique_genes: std::collections::HashSet<_> = genome.pipeline.iter().collect();

            // Diversity score: how many distinct genes are used
            let diversity = unique_genes.len() as f64 / all_genes.len() as f64;

            // Length balance: penalize extremes (ideal = 4-7 genes)
            let length_score = if pipe_len >= 4.0 && pipe_len <= 7.0 {
                1.0
            } else if pipe_len >= 3.0 && pipe_len <= 8.0 {
                0.8
            } else {
                0.5
            };

            // Critical stage presence: reward verification, synthesis, critique
            let has_verify = genome.pipeline.contains(&StrategyGene::Verify);
            let has_synthesize = genome.pipeline.contains(&StrategyGene::Synthesize);
            let has_critique = genome.pipeline.contains(&StrategyGene::Critique);
            let critical_bonus =
                (has_verify as u8 + has_synthesize as u8 + has_critique as u8) as f64 / 3.0;

            // Coherence: decompose should come before synthesize
            let decompose_pos = genome
                .pipeline
                .iter()
                .position(|g| *g == StrategyGene::Decompose);
            let synthesize_pos = genome
                .pipeline
                .iter()
                .position(|g| *g == StrategyGene::Synthesize);
            let coherence = match (decompose_pos, synthesize_pos) {
                (Some(d), Some(s)) if d < s => 1.0,
                (None, _) | (_, None) => 0.5,
                _ => 0.3,
            };

            // Combine with global fitness signal
            genome.accuracy = (fitness * 0.4
                + diversity * 0.2
                + critical_bonus * 0.15
                + coherence * 0.15
                + length_score * 0.1)
                .clamp(0.0, 1.0);

            // Speed: shorter pipelines execute faster (inverse of length)
            genome.speed = (1.0 - (pipe_len - 3.0).max(0.0) / 10.0).clamp(0.2, 1.0);

            // Novelty: decays with age but boosted by diversity
            genome.novelty =
                ((1.0 / (1.0 + genome.age as f64 * 0.1)) * 0.6 + diversity * 0.4).clamp(0.0, 1.0);

            genome.age += 1;
        }

        // Apply NSGA-II crowding distance before selection
        StrategyGenome::crowding_distance(&mut self.population);

        // Sort by fitness (descending)
        self.population.sort_by(|a, b| {
            b.fitness
                .partial_cmp(&a.fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Track best genome
        if let Some(best) = self.population.first() {
            if self.best_genome.as_ref().map(|g| g.fitness).unwrap_or(0.0) < best.fitness {
                self.best_genome = Some(best.clone());
            }
        }

        // Elitism: keep top fraction
        let elite_count = (self.population_size as f64 * self.elite_fraction) as usize;
        let elites: Vec<StrategyGenome> =
            self.population[..elite_count.min(self.population.len())].to_vec();

        // Tournament selection + crossover + mutation for rest
        let mut new_pop = elites.clone();
        while new_pop.len() < self.population_size {
            if rng.gen::<f64>() < self.crossover_prob && self.population.len() >= 2 {
                // Tournament selection
                let parent_a = self.tournament_select(&mut rng);
                let parent_b = self.tournament_select(&mut rng);
                let child_id = format!("gen{}_{}", self.current_generation, new_pop.len());
                let mut child = StrategyGenome::crossover(&parent_a, &parent_b, &child_id);
                if rng.gen::<f64>() < self.mutation_prob {
                    child.mutate();
                }
                new_pop.push(child);
            } else {
                // Mutation only
                let mut clone = StrategyGenome::random(&format!(
                    "gen{}_{}",
                    self.current_generation,
                    new_pop.len()
                ));
                clone.mutate();
                new_pop.push(clone);
            }
        }

        self.population = new_pop;

        // Compute diversity
        let diversity = self.compute_population_diversity();

        // Record generation
        self.generations.push(Generation {
            id: self.current_generation,
            fitness,
            params: current_params,
            best_genome: self.best_genome.clone(),
            population_diversity: diversity,
            timestamp: chrono::Utc::now().timestamp_millis(),
        });
        if self.generations.len() > 300 {
            self.generations.drain(..150);
        }

        new_params
    }

    fn tournament_select(&self, rng: &mut impl Rng) -> StrategyGenome {
        let tournament_size = 3;
        let mut best: Option<&StrategyGenome> = None;
        for _ in 0..tournament_size {
            let idx = rng.gen_range(0..self.population.len());
            let candidate = &self.population[idx];
            if best.map(|b| candidate.fitness > b.fitness).unwrap_or(true) {
                best = Some(candidate);
            }
        }
        best.cloned()
            .unwrap_or_else(|| StrategyGenome::random("fallback"))
    }

    fn compute_population_diversity(&self) -> f64 {
        if self.population.len() < 2 {
            return 1.0;
        }
        let mut total_diff = 0.0;
        let mut comparisons = 0;
        for i in 0..self.population.len().min(10) {
            for j in (i + 1)..self.population.len().min(10) {
                let a = &self.population[i].pipeline;
                let b = &self.population[j].pipeline;
                let max_len = a.len().max(b.len());
                let matches = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
                total_diff += 1.0 - (matches as f64 / max_len.max(1) as f64);
                comparisons += 1;
            }
        }
        if comparisons > 0 {
            total_diff / comparisons as f64
        } else {
            1.0
        }
    }

    pub fn get_params(&self) -> HashMap<String, f64> {
        self.params
            .iter()
            .map(|(k, p)| (k.clone(), p.value))
            .collect()
    }

    pub fn get_best_params(&self) -> &HashMap<String, f64> {
        &self.best_params
    }

    pub fn get_best_genome(&self) -> Option<&StrategyGenome> {
        self.best_genome.as_ref()
    }

    pub fn get_weak_components(&self) -> Vec<String> {
        self.component_health
            .iter()
            .filter_map(|(name, history)| {
                history.last().and_then(|latest| {
                    if latest.error_rate > 0.1
                        || latest.satisfaction_score < 0.5
                        || latest.trend < -0.05
                    {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "SelfEvolution v3.0 — Genetic Strategy Evolution",
            "current_generation": self.current_generation,
            "best_fitness": self.best_fitness,
            "population_size": self.population.len(),
            "population_diversity": self.compute_population_diversity(),
            "best_genome_pipeline": self.best_genome.as_ref().map(|g| format!("{:?}", g.pipeline)),
            "current_params": self.get_params(),
            "weak_components": self.get_weak_components(),
            "generations_recorded": self.generations.len(),
            "components_tracked": self.component_health.len(),
        })
    }
}

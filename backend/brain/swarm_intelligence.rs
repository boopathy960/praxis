// ─────────────────────────────────────────────────────────────
// Swarm Intelligence — Parallel Micro-Agent Reasoning with ACO
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/swarm_engine.py

use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct SwarmAgent {
    pub id: usize,
    pub position: Vec<f64>,
    pub velocity: Vec<f64>,
    pub best_position: Vec<f64>,
    pub best_fitness: f64,
    pub solution: String,
}

#[derive(Debug, Clone)]
pub struct SwarmResult {
    pub best_solution: String,
    pub best_fitness: f64,
    pub iterations: usize,
    pub agents_count: usize,
    pub convergence_history: Vec<f64>,
    pub duration_ms: f64,
}

/// Swarm Intelligence Engine — PSO + ACO for collective reasoning.
pub struct SwarmIntelligenceEngine {
    num_agents: usize,
    max_iterations: usize,
    inertia: f64,
    cognitive_weight: f64,
    social_weight: f64,
    pheromone_decay: f64,
    pheromone_map: HashMap<String, f64>,
}

impl SwarmIntelligenceEngine {
    pub fn new(num_agents: usize) -> Self {
        Self {
            num_agents,
            max_iterations: 50,
            inertia: 0.7,
            cognitive_weight: 1.5,
            social_weight: 2.0,
            pheromone_decay: 0.1,
            pheromone_map: HashMap::new(),
        }
    }

    /// Solve an optimization problem using particle swarm optimization.
    pub fn optimize<F>(
        &mut self,
        dimensions: usize,
        bounds: &[(f64, f64)],
        fitness_fn: F,
    ) -> SwarmResult
    where
        F: Fn(&[f64]) -> f64,
    {
        let start = Instant::now();
        let mut rng = SimpleRng::new(42);
        let mut convergence = Vec::new();

        // Initialize agents
        let mut agents: Vec<SwarmAgent> = (0..self.num_agents)
            .map(|id| {
                let position: Vec<f64> = (0..dimensions)
                    .map(|d| bounds[d].0 + rng.next_f64() * (bounds[d].1 - bounds[d].0))
                    .collect();
                let fitness = fitness_fn(&position);
                SwarmAgent {
                    id,
                    velocity: vec![0.0; dimensions],
                    best_position: position.clone(),
                    best_fitness: fitness,
                    position,
                    solution: String::new(),
                }
            })
            .collect();

        // Global best
        let mut global_best_pos = agents[0].position.clone();
        let mut global_best_fitness = agents[0].best_fitness;

        for agent in &agents {
            if agent.best_fitness > global_best_fitness {
                global_best_fitness = agent.best_fitness;
                global_best_pos = agent.position.clone();
            }
        }

        // PSO main loop
        for _iter in 0..self.max_iterations {
            for agent in &mut agents {
                // Update velocity
                for d in 0..dimensions {
                    let r1 = rng.next_f64();
                    let r2 = rng.next_f64();
                    agent.velocity[d] = self.inertia * agent.velocity[d]
                        + self.cognitive_weight * r1 * (agent.best_position[d] - agent.position[d])
                        + self.social_weight * r2 * (global_best_pos[d] - agent.position[d]);
                }

                // Update position
                for d in 0..dimensions {
                    agent.position[d] += agent.velocity[d];
                    agent.position[d] = agent.position[d].clamp(bounds[d].0, bounds[d].1);
                }

                // Evaluate
                let fitness = fitness_fn(&agent.position);
                if fitness > agent.best_fitness {
                    agent.best_fitness = fitness;
                    agent.best_position = agent.position.clone();
                }
                if fitness > global_best_fitness {
                    global_best_fitness = fitness;
                    global_best_pos = agent.position.clone();
                }
            }

            convergence.push(global_best_fitness);

            // Early stop if converged
            if convergence.len() > 10 {
                let recent = &convergence[convergence.len() - 5..];
                let diff = recent.last().unwrap() - recent.first().unwrap();
                if diff.abs() < 1e-8 {
                    break;
                }
            }
        }

        let solution_str = global_best_pos
            .iter()
            .map(|v| format!("{:.6}", v))
            .collect::<Vec<_>>()
            .join(", ");

        SwarmResult {
            best_solution: format!("[{}]", solution_str),
            best_fitness: global_best_fitness,
            iterations: convergence.len(),
            agents_count: self.num_agents,
            convergence_history: convergence,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// Deposit pheromone on a path (ACO).
    pub fn deposit_pheromone(&mut self, path: &str, quality: f64) {
        let current = self.pheromone_map.get(path).copied().unwrap_or(0.0);
        self.pheromone_map
            .insert(path.to_string(), current + quality);
    }

    /// Evaporate pheromones (ACO).
    pub fn evaporate_pheromones(&mut self) {
        for (_, value) in self.pheromone_map.iter_mut() {
            *value *= 1.0 - self.pheromone_decay;
        }
        self.pheromone_map.retain(|_, v| *v > 0.001);
    }

    /// Get pheromone level for a path.
    pub fn get_pheromone(&self, path: &str) -> f64 {
        self.pheromone_map.get(path).copied().unwrap_or(0.0)
    }
}

impl Default for SwarmIntelligenceEngine {
    fn default() -> Self {
        Self::new(30)
    }
}

/// Simple deterministic RNG (Xoshiro) for no-dependency builds.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (u64::MAX as f64)
    }
}

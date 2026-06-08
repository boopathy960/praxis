// ─────────────────────────────────────────────────────────────
// Formula Discovery Engine — Genetic Programming for Symbolic Regression
// ─────────────────────────────────────────────────────────────
// Port of backend/brain/formula_discovery_engine.py
// Discovers mathematical formulas from data using evolutionary algorithms.

use rand::Rng;
use std::collections::HashMap;
use std::time::Instant;

/// AST node for symbolic expressions.
#[derive(Debug, Clone)]
pub enum ExprNode {
    Constant(f64),
    Variable(String),
    Add(Box<ExprNode>, Box<ExprNode>),
    Sub(Box<ExprNode>, Box<ExprNode>),
    Mul(Box<ExprNode>, Box<ExprNode>),
    Div(Box<ExprNode>, Box<ExprNode>),
    Pow(Box<ExprNode>, Box<ExprNode>),
    Sin(Box<ExprNode>),
    Cos(Box<ExprNode>),
    Sqrt(Box<ExprNode>),
    Ln(Box<ExprNode>),
    Exp(Box<ExprNode>),
}

impl ExprNode {
    /// Evaluate expression with given variable bindings.
    pub fn evaluate(&self, vars: &HashMap<String, f64>) -> Option<f64> {
        match self {
            Self::Constant(v) => Some(*v),
            Self::Variable(name) => vars.get(name).copied(),
            Self::Add(a, b) => Some(a.evaluate(vars)? + b.evaluate(vars)?),
            Self::Sub(a, b) => Some(a.evaluate(vars)? - b.evaluate(vars)?),
            Self::Mul(a, b) => Some(a.evaluate(vars)? * b.evaluate(vars)?),
            Self::Div(a, b) => {
                let denom = b.evaluate(vars)?;
                if denom.abs() < 1e-12 {
                    None
                } else {
                    Some(a.evaluate(vars)? / denom)
                }
            }
            Self::Pow(a, b) => {
                let base = a.evaluate(vars)?;
                let exp = b.evaluate(vars)?;
                let result = base.powf(exp);
                if result.is_finite() {
                    Some(result)
                } else {
                    None
                }
            }
            Self::Sin(a) => Some(a.evaluate(vars)?.sin()),
            Self::Cos(a) => Some(a.evaluate(vars)?.cos()),
            Self::Sqrt(a) => {
                let v = a.evaluate(vars)?;
                if v >= 0.0 {
                    Some(v.sqrt())
                } else {
                    None
                }
            }
            Self::Ln(a) => {
                let v = a.evaluate(vars)?;
                if v > 0.0 {
                    Some(v.ln())
                } else {
                    None
                }
            }
            Self::Exp(a) => {
                let v = a.evaluate(vars)?.exp();
                if v.is_finite() {
                    Some(v)
                } else {
                    None
                }
            }
        }
    }

    /// Get a string representation.
    pub fn to_string_repr(&self) -> String {
        match self {
            Self::Constant(v) => format!("{:.4}", v),
            Self::Variable(name) => name.clone(),
            Self::Add(a, b) => format!("({} + {})", a.to_string_repr(), b.to_string_repr()),
            Self::Sub(a, b) => format!("({} - {})", a.to_string_repr(), b.to_string_repr()),
            Self::Mul(a, b) => format!("({} * {})", a.to_string_repr(), b.to_string_repr()),
            Self::Div(a, b) => format!("({} / {})", a.to_string_repr(), b.to_string_repr()),
            Self::Pow(a, b) => format!("({} ^ {})", a.to_string_repr(), b.to_string_repr()),
            Self::Sin(a) => format!("sin({})", a.to_string_repr()),
            Self::Cos(a) => format!("cos({})", a.to_string_repr()),
            Self::Sqrt(a) => format!("sqrt({})", a.to_string_repr()),
            Self::Ln(a) => format!("ln({})", a.to_string_repr()),
            Self::Exp(a) => format!("exp({})", a.to_string_repr()),
        }
    }

    /// Count nodes in the expression tree.
    pub fn complexity(&self) -> usize {
        match self {
            Self::Constant(_) | Self::Variable(_) => 1,
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::Pow(a, b) => 1 + a.complexity() + b.complexity(),
            Self::Sin(a) | Self::Cos(a) | Self::Sqrt(a) | Self::Ln(a) | Self::Exp(a) => {
                1 + a.complexity()
            }
        }
    }
}

/// Individual in the genetic population.
#[derive(Debug, Clone)]
struct Individual {
    expr: ExprNode,
    fitness: f64,
}

/// Result of formula discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub formula: String,
    pub fitness: f64,
    pub complexity: usize,
    pub generations: usize,
    pub duration_ms: f64,
}

/// Formula Discovery Engine using genetic programming.
pub struct FormulaDiscoveryEngine {
    population_size: usize,
    max_generations: usize,
    max_depth: usize,
    tournament_size: usize,
    mutation_rate: f64,
    crossover_rate: f64,
}

impl FormulaDiscoveryEngine {
    pub fn new() -> Self {
        Self {
            population_size: 200,
            max_generations: 50,
            max_depth: 5,
            tournament_size: 7,
            mutation_rate: 0.15,
            crossover_rate: 0.85,
        }
    }

    /// Discover a formula that fits the given data points.
    pub fn discover(&self, x_data: &[f64], y_data: &[f64], variable_name: &str) -> DiscoveryResult {
        let start = Instant::now();
        let mut rng = rand::thread_rng();

        // Initialize population
        let mut population: Vec<Individual> = (0..self.population_size)
            .map(|_| {
                let expr = self.random_expr(&mut rng, self.max_depth, variable_name);
                let fitness = self.evaluate_fitness(&expr, x_data, y_data, variable_name);
                Individual { expr, fitness }
            })
            .collect();

        let mut best = population
            .iter()
            .max_by(|a, b| {
                a.fitness
                    .partial_cmp(&b.fitness)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
            .unwrap();

        for _gen in 0..self.max_generations {
            let mut new_pop = Vec::with_capacity(self.population_size);

            // Elitism: keep best
            new_pop.push(best.clone());

            while new_pop.len() < self.population_size {
                if rng.gen::<f64>() < self.crossover_rate {
                    let parent1 = self.tournament_select(&population, &mut rng);
                    let parent2 = self.tournament_select(&population, &mut rng);
                    let child_expr = self.crossover(&parent1.expr, &parent2.expr, &mut rng);
                    let fitness = self.evaluate_fitness(&child_expr, x_data, y_data, variable_name);
                    new_pop.push(Individual {
                        expr: child_expr,
                        fitness,
                    });
                } else {
                    let parent = self.tournament_select(&population, &mut rng);
                    let mutated = self.mutate(&parent.expr, &mut rng, variable_name);
                    let fitness = self.evaluate_fitness(&mutated, x_data, y_data, variable_name);
                    new_pop.push(Individual {
                        expr: mutated,
                        fitness,
                    });
                }
            }

            population = new_pop;

            // Update best
            if let Some(gen_best) = population.iter().max_by(|a, b| {
                a.fitness
                    .partial_cmp(&b.fitness)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                if gen_best.fitness > best.fitness {
                    best = gen_best.clone();
                }
            }

            // Early termination if perfect fit
            if best.fitness > 0.999 {
                break;
            }
        }

        DiscoveryResult {
            formula: best.expr.to_string_repr(),
            fitness: best.fitness,
            complexity: best.expr.complexity(),
            generations: self.max_generations,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        }
    }

    fn evaluate_fitness(
        &self,
        expr: &ExprNode,
        x_data: &[f64],
        y_data: &[f64],
        var_name: &str,
    ) -> f64 {
        let mut total_error = 0.0;
        let mut valid_count = 0usize;

        for (x, y) in x_data.iter().zip(y_data.iter()) {
            let mut vars = HashMap::new();
            vars.insert(var_name.to_string(), *x);

            if let Some(predicted) = expr.evaluate(&vars) {
                if predicted.is_finite() {
                    total_error += (predicted - y).powi(2);
                    valid_count += 1;
                }
            }
        }

        if valid_count == 0 {
            return 0.0;
        }

        let mse = total_error / valid_count as f64;
        let fitness = 1.0 / (1.0 + mse);

        // Parsimony pressure: penalize complex expressions
        let complexity_penalty = 0.001 * expr.complexity() as f64;
        (fitness - complexity_penalty).max(0.0)
    }

    fn random_expr(&self, rng: &mut impl Rng, depth: usize, var: &str) -> ExprNode {
        if depth == 0 || (depth < self.max_depth && rng.gen::<f64>() < 0.3) {
            if rng.gen::<f64>() < 0.5 {
                ExprNode::Variable(var.to_string())
            } else {
                ExprNode::Constant(rng.gen_range(-5.0..5.0))
            }
        } else {
            match rng.gen_range(0..8) {
                0 => ExprNode::Add(
                    Box::new(self.random_expr(rng, depth - 1, var)),
                    Box::new(self.random_expr(rng, depth - 1, var)),
                ),
                1 => ExprNode::Sub(
                    Box::new(self.random_expr(rng, depth - 1, var)),
                    Box::new(self.random_expr(rng, depth - 1, var)),
                ),
                2 => ExprNode::Mul(
                    Box::new(self.random_expr(rng, depth - 1, var)),
                    Box::new(self.random_expr(rng, depth - 1, var)),
                ),
                3 => ExprNode::Div(
                    Box::new(self.random_expr(rng, depth - 1, var)),
                    Box::new(self.random_expr(rng, depth - 1, var)),
                ),
                4 => ExprNode::Sin(Box::new(self.random_expr(rng, depth - 1, var))),
                5 => ExprNode::Cos(Box::new(self.random_expr(rng, depth - 1, var))),
                6 => ExprNode::Pow(
                    Box::new(self.random_expr(rng, depth - 1, var)),
                    Box::new(ExprNode::Constant(rng.gen_range(0.5..3.0))),
                ),
                _ => ExprNode::Variable(var.to_string()),
            }
        }
    }

    fn tournament_select<'a>(&self, pop: &'a [Individual], rng: &mut impl Rng) -> &'a Individual {
        let mut best: Option<&Individual> = None;
        for _ in 0..self.tournament_size {
            let idx = rng.gen_range(0..pop.len());
            if best.is_none() || pop[idx].fitness > best.unwrap().fitness {
                best = Some(&pop[idx]);
            }
        }
        best.unwrap()
    }

    fn crossover(&self, a: &ExprNode, b: &ExprNode, rng: &mut impl Rng) -> ExprNode {
        if rng.gen::<f64>() < 0.5 {
            a.clone()
        } else {
            b.clone()
        }
    }

    fn mutate(&self, expr: &ExprNode, rng: &mut impl Rng, var: &str) -> ExprNode {
        if rng.gen::<f64>() < self.mutation_rate {
            self.random_expr(rng, 3, var)
        } else {
            match expr {
                ExprNode::Add(a, b) => ExprNode::Add(
                    Box::new(self.mutate(a, rng, var)),
                    Box::new(self.mutate(b, rng, var)),
                ),
                ExprNode::Mul(a, b) => ExprNode::Mul(
                    Box::new(self.mutate(a, rng, var)),
                    Box::new(self.mutate(b, rng, var)),
                ),
                other => other.clone(),
            }
        }
    }
}

impl Default for FormulaDiscoveryEngine {
    fn default() -> Self {
        Self::new()
    }
}

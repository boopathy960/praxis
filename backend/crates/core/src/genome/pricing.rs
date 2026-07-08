//! Outcome pricing and royalties — the Shapley value over the execution graph
//! (§6.4).
//!
//! When cash is collected, who gets the credit? Outcomes are jointly produced by
//! many genomes, humans, and luck. Cooperative game theory has the unique fair
//! answer: the **Shapley value** is the only attribution satisfying efficiency,
//! symmetry, the null-player rule, and additivity — exactly the fairness axioms
//! you want for credit on a joint outcome. Running it over the genome execution
//! graph yields a principled price per operation and a principled **royalty**
//! flowing up the fork tree to the authors whose genomes created downstream
//! value — what makes the commons self-sustaining rather than a charity.
//!
//! Exact Shapley is exponential, so we enumerate subsets exactly for small
//! coalitions (≤ 8 contributors) and fall back to Monte-Carlo permutation
//! sampling otherwise (as SHAP does in ML). The characteristic function is
//! concave in the contributors' summed weight, so the value is genuinely shared
//! rather than collapsing to a proportional split.

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

/// A participant in one execution: a genome (with its fork lineage) or a human.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contributor {
    pub id: String,
    pub label: String,
    /// Recorded marginal contribution signal (≥ 0) — e.g. the share of work this
    /// genome performed in the operation graph.
    pub weight: f64,
    /// Ancestors root→parent in the fork tree, for royalty flow.
    #[serde(default)]
    pub lineage: Vec<String>,
    #[serde(default)]
    pub is_human: bool,
}

/// One contributor's fair share of the metered outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapleyShare {
    pub id: String,
    pub label: String,
    pub shapley_value: f64,
    /// The contributor's own take after royalties flow upward.
    pub net_price: f64,
    pub share_pct: f64,
    pub is_human: bool,
}

/// A royalty paid from a contributor up to one of its fork-tree ancestors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoyaltyFlow {
    pub from: String,
    pub to: String,
    pub amount: f64,
    pub generations_up: u32,
}

/// The full pricing of one metered outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingReport {
    pub total_value: f64,
    /// "exact" (subset enumeration) or "montecarlo".
    pub method: String,
    pub samples: u64,
    pub shares: Vec<ShapleyShare>,
    pub royalties: Vec<RoyaltyFlow>,
    /// `total_value − Σ shapley` — should be ≈ 0 (the efficiency axiom).
    pub efficiency_residual: f64,
}

/// Fraction of a contributor's Shapley credit that flows up the fork tree.
const ROYALTY_RATE: f64 = 0.2;
/// Per-generation decay of the royalty as it climbs the lineage.
const ROYALTY_DECAY: f64 = 0.5;
/// Concavity of the characteristic function — > 1 gives diminishing returns to
/// scale, so a coalition's value is genuinely shared (Shapley ≠ proportional).
const VALUE_CONCAVITY: f64 = 1.6;
const EXACT_LIMIT: usize = 8;
const MC_SAMPLES: u64 = 4000;

/// Price a metered outcome by Shapley value and compute the royalty flows.
#[must_use]
pub fn price(contributors: &[Contributor], total_value: f64) -> PricingReport {
    if contributors.is_empty() || total_value <= 0.0 {
        return PricingReport {
            total_value,
            method: "empty".into(),
            samples: 0,
            shares: Vec::new(),
            royalties: Vec::new(),
            efficiency_residual: total_value,
        };
    }

    let total_weight: f64 = contributors
        .iter()
        .map(|c| c.weight.max(0.0))
        .sum::<f64>()
        .max(1e-9);
    let weights: Vec<f64> = contributors.iter().map(|c| c.weight.max(0.0)).collect();

    // Characteristic function v(S) = total_value · h(Σ_{i∈S} wᵢ / W), with
    // h(x) = 1 − (1 − x)^γ (concave, h(0)=0, h(1)=1) so v(∅)=0 and v(N)=total.
    let value_of = |subset_weight: f64| -> f64 {
        let x = (subset_weight / total_weight).clamp(0.0, 1.0);
        total_value * (1.0 - (1.0 - x).powf(VALUE_CONCAVITY))
    };

    let n = contributors.len();
    let (shapley, method, samples) = if n <= EXACT_LIMIT {
        (exact_shapley(&weights, &value_of), "exact".to_string(), 0)
    } else {
        (
            monte_carlo_shapley(&weights, &value_of, MC_SAMPLES),
            "montecarlo".to_string(),
            MC_SAMPLES,
        )
    };

    // ── Royalties up the fork tree ──
    let id_index: std::collections::HashMap<&str, usize> = contributors
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let mut royalties = Vec::new();
    let mut net: Vec<f64> = shapley.clone();
    for (i, contributor) in contributors.iter().enumerate() {
        if contributor.lineage.is_empty() || shapley[i] <= 0.0 {
            continue;
        }
        // Ancestors are listed root→parent; the *parent* is closest (1 up).
        let ancestors: Vec<(&String, u32)> = contributor
            .lineage
            .iter()
            .rev()
            .enumerate()
            .map(|(d, id)| (id, (d + 1) as u32))
            .collect();
        let decay_weights: Vec<f64> = ancestors
            .iter()
            .map(|(_, d)| ROYALTY_DECAY.powi(*d as i32 - 1))
            .collect();
        let decay_total: f64 = decay_weights.iter().sum::<f64>().max(1e-9);
        let pool = shapley[i] * ROYALTY_RATE;
        for ((ancestor_id, generations), decay_weight) in ancestors.iter().zip(decay_weights.iter())
        {
            let amount = pool * (decay_weight / decay_total);
            if amount <= 1e-9 {
                continue;
            }
            net[i] -= amount;
            if let Some(&ai) = id_index.get(ancestor_id.as_str()) {
                net[ai] += amount;
            }
            royalties.push(RoyaltyFlow {
                from: contributor.id.clone(),
                to: (*ancestor_id).clone(),
                amount,
                generations_up: *generations,
            });
        }
    }

    let shapley_total: f64 = shapley.iter().sum();
    let shares: Vec<ShapleyShare> = contributors
        .iter()
        .enumerate()
        .map(|(i, c)| ShapleyShare {
            id: c.id.clone(),
            label: c.label.clone(),
            shapley_value: shapley[i],
            net_price: net[i],
            share_pct: if total_value > 0.0 {
                100.0 * shapley[i] / total_value
            } else {
                0.0
            },
            is_human: c.is_human,
        })
        .collect();

    PricingReport {
        total_value,
        method,
        samples,
        shares,
        royalties,
        efficiency_residual: total_value - shapley_total,
    }
}

/// Exact Shapley by enumerating, for each player, all subsets of the others.
fn exact_shapley(weights: &[f64], value_of: &impl Fn(f64) -> f64) -> Vec<f64> {
    let n = weights.len();
    let mut phi = vec![0.0; n];
    let factorial: Vec<f64> = {
        let mut f = vec![1.0; n + 1];
        for k in 1..=n {
            f[k] = f[k - 1] * k as f64;
        }
        f
    };
    let others: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let rest: Vec<usize> = others.iter().copied().filter(|&j| j != i).collect();
        let m = rest.len();
        for mask in 0..(1u32 << m) {
            let mut subset_weight = 0.0;
            let mut size = 0usize;
            for (bit, &j) in rest.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    subset_weight += weights[j];
                    size += 1;
                }
            }
            let marginal = value_of(subset_weight + weights[i]) - value_of(subset_weight);
            // Shapley weight: |S|! (n−|S|−1)! / n!
            let coeff = factorial[size] * factorial[n - size - 1] / factorial[n];
            phi[i] += coeff * marginal;
        }
    }
    phi
}

/// Monte-Carlo Shapley by sampling random permutations and averaging marginals.
fn monte_carlo_shapley(weights: &[f64], value_of: &impl Fn(f64) -> f64, samples: u64) -> Vec<f64> {
    let n = weights.len();
    let mut phi = vec![0.0; n];
    let mut order: Vec<usize> = (0..n).collect();
    let mut rng = rand::rng();
    for _ in 0..samples {
        order.shuffle(&mut rng);
        let mut prefix_weight = 0.0;
        let mut prev_value = 0.0;
        for &i in &order {
            let new_value = value_of(prefix_weight + weights[i]);
            phi[i] += new_value - prev_value;
            prefix_weight += weights[i];
            prev_value = new_value;
        }
    }
    for value in &mut phi {
        *value /= samples as f64;
    }
    phi
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contributor(id: &str, weight: f64, lineage: &[&str]) -> Contributor {
        Contributor {
            id: id.into(),
            label: id.into(),
            weight,
            lineage: lineage.iter().map(|s| (*s).to_string()).collect(),
            is_human: false,
        }
    }

    #[test]
    fn shapley_satisfies_efficiency() {
        let contributors = vec![
            contributor("detector", 1.0, &[]),
            contributor("collector", 2.0, &[]),
            contributor("human", 0.5, &[]),
        ];
        let report = price(&contributors, 10_000.0);
        assert_eq!(report.method, "exact");
        // Efficiency: the shares sum to the total value.
        assert!(
            report.efficiency_residual.abs() < 1.0,
            "residual {}",
            report.efficiency_residual
        );
        let sum: f64 = report.shares.iter().map(|s| s.shapley_value).sum();
        assert!((sum - 10_000.0).abs() < 1.0);
    }

    #[test]
    fn symmetry_gives_equal_players_equal_value() {
        let contributors = vec![contributor("a", 1.0, &[]), contributor("b", 1.0, &[])];
        let report = price(&contributors, 1000.0);
        let a = report
            .shares
            .iter()
            .find(|s| s.id == "a")
            .unwrap()
            .shapley_value;
        let b = report
            .shares
            .iter()
            .find(|s| s.id == "b")
            .unwrap()
            .shapley_value;
        assert!(
            (a - b).abs() < 1.0,
            "symmetric players must get equal credit"
        );
    }

    #[test]
    fn null_player_gets_nothing() {
        let contributors = vec![contributor("real", 1.0, &[]), contributor("null", 0.0, &[])];
        let report = price(&contributors, 1000.0);
        let null = report
            .shares
            .iter()
            .find(|s| s.id == "null")
            .unwrap()
            .shapley_value;
        assert!(null.abs() < 1.0, "a null player earns ~0");
    }

    #[test]
    fn royalties_flow_up_the_fork_tree() {
        let contributors = vec![
            contributor("parent", 0.1, &[]),
            contributor("child", 2.0, &["parent"]),
        ];
        let report = price(&contributors, 1000.0);
        assert!(!report.royalties.is_empty());
        let flow = &report.royalties[0];
        assert_eq!(flow.from, "child");
        assert_eq!(flow.to, "parent");
        assert!(flow.amount > 0.0);
        // The child's net take is reduced by the royalty it pays up.
        let child = report.shares.iter().find(|s| s.id == "child").unwrap();
        assert!(child.net_price < child.shapley_value);
    }
}

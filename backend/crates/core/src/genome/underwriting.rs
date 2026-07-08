//! Correlated-risk underwriting — the warranty layer that survives instead of
//! detonating (§6.5).
//!
//! Thousands of companies running the *same* genome means one bad mutation is a
//! correlated, network-wide failure — so this is a finance problem, not a
//! software one. Failures correlate through **shared lineage**, so the right
//! object is a contagion structure over the genome lineage graph, and the right
//! tools are portfolio risk management plus extreme-value theory for the heavy
//! tail (Gaussian assumptions badly under-price rare catastrophic mis-executions).
//! The governance rule that falls out is a **concentration limit**: no single
//! genome may run more than a capped share of a critical operation across the
//! network — exactly like the position limits that stop one exposure from
//! sinking a portfolio. That is what makes the network anti-fragile and yields
//! the correlated-failure dataset no competitor has.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::model::Genome;

/// Default network concentration cap on any single critical operation.
pub const DEFAULT_CONCENTRATION_CAP: f64 = 0.35;
/// Within-lineage failure correlation (shared mutations fail together).
const LINEAGE_CORRELATION: f64 = 0.6;
/// Risk loading on the warranty premium over expected loss.
const RISK_LOADING: f64 = 0.30;
/// Extreme-value tail multiplier — heavy tails are worse than Gaussian.
const EVT_TAIL_MULTIPLIER: f64 = 1.6;
/// 99% one-sided normal quantile, inflated by the EVT multiplier for the tail.
const Z_99: f64 = 2.3263;
/// Laplace-smoothing priors for the failure-probability estimate.
const PRIOR_FAILURES: f64 = 0.5;
const PRIOR_RUNS: f64 = 20.0;

/// One genome's underwriting line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenomePremium {
    pub id: String,
    pub name: String,
    pub operation: String,
    pub failure_prob: f64,
    pub exposure: f64,
    pub expected_loss: f64,
    pub standalone_sigma: f64,
    pub premium: f64,
    pub lineage_cluster: String,
}

/// A correlated-failure cluster — genomes sharing a lineage root fail together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatedCluster {
    pub root: String,
    pub members: usize,
    pub correlation: f64,
    pub expected_loss: f64,
    pub tail_loss_99: f64,
    pub share_of_network: f64,
}

/// A genome's share of one critical operation, against the concentration cap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcentrationEntry {
    pub id: String,
    pub name: String,
    pub share: f64,
    pub over_cap: bool,
    pub recommended_max_runs: f64,
}

/// The concentration picture for one operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcentrationLimit {
    pub operation: String,
    pub cap: f64,
    pub total_runs: u64,
    pub breached: bool,
    pub entries: Vec<ConcentrationEntry>,
}

/// The full underwriting assessment of the commons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskReport {
    pub concentration_cap: f64,
    pub concentration: Vec<ConcentrationLimit>,
    pub premiums: Vec<GenomePremium>,
    pub clusters: Vec<CorrelatedCluster>,
    pub portfolio_expected_loss: f64,
    pub portfolio_tail_loss_99: f64,
    /// Systemic-risk score in `[0, 100]`: concentration breach severity blended
    /// with the tail-to-expected-loss ratio.
    pub systemic_risk_score: f64,
    pub total_premium: f64,
}

/// The verdict for the runtime's admission gate: may this genome take another
/// unit of traffic on its operation without breaching the concentration limit?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcentrationVerdict {
    pub allowed: bool,
    pub operation: String,
    pub genome_id: String,
    pub current_share: f64,
    pub cap: f64,
    pub detail: String,
}

fn failure_probability(genome: &Genome) -> f64 {
    let runs = genome.metrics.runs as f64;
    let failures = genome.metrics.violations as f64;
    ((failures + PRIOR_FAILURES) / (runs + PRIOR_RUNS)).clamp(0.0005, 0.5)
}

fn exposure(genome: &Genome) -> f64 {
    // Value at risk: what flows through this genome. Cold genomes still carry a
    // floor exposure so their warranty is not free.
    genome
        .metrics
        .outcome_value
        .max(genome.metrics.runs as f64 * 10.0)
        .max(100.0)
}

fn lineage_root(genome: &Genome) -> String {
    genome
        .lineage
        .first()
        .cloned()
        .unwrap_or_else(|| genome.genome_id.clone())
}

/// Assess the whole commons: per-genome premiums, correlated clusters, the
/// portfolio tail, and the concentration limits per operation.
#[must_use]
pub fn assess(genomes: &[Genome], cap: f64) -> RiskReport {
    let cap = cap.clamp(0.05, 1.0);

    // ── Per-genome lines ──
    let premiums: Vec<GenomePremium> = genomes
        .iter()
        .map(|genome| {
            let p = failure_probability(genome);
            let e = exposure(genome);
            let expected_loss = p * e;
            let sigma = (p * (1.0 - p)).sqrt() * e;
            // Premium = expected loss + risk loading on the standalone tail.
            let premium = expected_loss + RISK_LOADING * sigma * EVT_TAIL_MULTIPLIER;
            GenomePremium {
                id: genome.genome_id.clone(),
                name: genome.name.clone(),
                operation: genome.operation.clone(),
                failure_prob: p,
                exposure: e,
                expected_loss,
                standalone_sigma: sigma,
                premium,
                lineage_cluster: lineage_root(genome),
            }
        })
        .collect();

    // ── Correlated clusters by lineage root ──
    let mut by_root: BTreeMap<String, Vec<&GenomePremium>> = BTreeMap::new();
    for line in &premiums {
        by_root
            .entry(line.lineage_cluster.clone())
            .or_default()
            .push(line);
    }
    let network_exposure: f64 = premiums.iter().map(|p| p.exposure).sum::<f64>().max(1.0);

    let mut clusters = Vec::new();
    let mut portfolio_variance = 0.0;
    let mut portfolio_expected_loss = 0.0;
    for (root, lines) in &by_root {
        let el: f64 = lines.iter().map(|l| l.expected_loss).sum();
        portfolio_expected_loss += el;
        // Correlated variance within the cluster: Σ σ² + Σ_{i≠j} ρ σᵢ σⱼ.
        let sigmas: Vec<f64> = lines.iter().map(|l| l.standalone_sigma).collect();
        let mut cluster_var: f64 = sigmas.iter().map(|s| s * s).sum();
        for i in 0..sigmas.len() {
            for j in 0..sigmas.len() {
                if i != j {
                    cluster_var += LINEAGE_CORRELATION * sigmas[i] * sigmas[j];
                }
            }
        }
        portfolio_variance += cluster_var;
        let cluster_exposure: f64 = lines.iter().map(|l| l.exposure).sum();
        let tail = el + Z_99 * EVT_TAIL_MULTIPLIER * cluster_var.sqrt();
        clusters.push(CorrelatedCluster {
            root: root.clone(),
            members: lines.len(),
            correlation: if lines.len() > 1 {
                LINEAGE_CORRELATION
            } else {
                0.0
            },
            expected_loss: el,
            tail_loss_99: tail,
            share_of_network: cluster_exposure / network_exposure,
        });
    }
    let portfolio_tail_loss_99 =
        portfolio_expected_loss + Z_99 * EVT_TAIL_MULTIPLIER * portfolio_variance.sqrt();

    // ── Concentration limits per operation (share by run volume) ──
    let mut ops: BTreeMap<String, Vec<&Genome>> = BTreeMap::new();
    for genome in genomes {
        ops.entry(genome.operation.clone())
            .or_default()
            .push(genome);
    }
    let concentration: Vec<ConcentrationLimit> = ops
        .into_iter()
        .map(|(operation, members)| {
            let total_runs: u64 = members.iter().map(|g| g.metrics.runs).sum();
            let basis = total_runs.max(1) as f64;
            // Concentration is a network property: a lone genome on its operation
            // cannot concentrate against itself, so a breach requires siblings to
            // route to — matching the runtime's admission gate.
            let siblings = members.len() > 1;
            let mut breached = false;
            let entries: Vec<ConcentrationEntry> = members
                .iter()
                .map(|g| {
                    let share = g.metrics.runs as f64 / basis;
                    let over = siblings && share > cap + 1e-9;
                    if over {
                        breached = true;
                    }
                    ConcentrationEntry {
                        id: g.genome_id.clone(),
                        name: g.name.clone(),
                        share,
                        over_cap: over,
                        recommended_max_runs: cap * basis,
                    }
                })
                .collect();
            ConcentrationLimit {
                operation,
                cap,
                total_runs,
                breached,
                entries,
            }
        })
        .collect();

    // ── Systemic-risk score ──
    let max_over_cap = concentration
        .iter()
        .flat_map(|c| c.entries.iter())
        .map(|e| (e.share / cap).min(2.0))
        .fold(0.0_f64, f64::max);
    let tail_ratio = if portfolio_expected_loss > 1.0 {
        (portfolio_tail_loss_99 / portfolio_expected_loss).min(20.0) / 20.0
    } else {
        0.0
    };
    let concentration_component = ((max_over_cap - 1.0).max(0.0)).min(1.0);
    let systemic_risk_score =
        (0.6 * concentration_component + 0.4 * tail_ratio).clamp(0.0, 1.0) * 100.0;

    let total_premium = premiums.iter().map(|p| p.premium).sum();

    RiskReport {
        concentration_cap: cap,
        concentration,
        premiums,
        clusters,
        portfolio_expected_loss,
        portfolio_tail_loss_99,
        systemic_risk_score,
        total_premium,
    }
}

/// The runtime admission gate: would routing one more unit of traffic to
/// `genome_id` on its operation breach the concentration limit? The brief's
/// governance rule, enforced per run.
#[must_use]
pub fn admits_more_traffic(
    genomes: &[Genome],
    operation: &str,
    genome_id: &str,
    cap: f64,
) -> ConcentrationVerdict {
    let cap = cap.clamp(0.05, 1.0);
    let members: Vec<&Genome> = genomes
        .iter()
        .filter(|g| g.operation == operation)
        .collect();
    let total_runs: u64 = members.iter().map(|g| g.metrics.runs).sum();
    // Project the share *after* this run (the marginal unit the gate decides on).
    let current = members
        .iter()
        .find(|g| g.genome_id == genome_id)
        .map(|g| g.metrics.runs)
        .unwrap_or(0);
    let projected_share = (current as f64 + 1.0) / (total_runs as f64 + 1.0);
    // A single genome alone on its operation is never capped — concentration is a
    // network property, only meaningful once siblings exist to absorb the spill.
    let siblings = members.len() > 1;
    let allowed = !siblings || projected_share <= cap + 1e-9;
    ConcentrationVerdict {
        allowed,
        operation: operation.into(),
        genome_id: genome_id.into(),
        current_share: projected_share,
        cap,
        detail: if allowed {
            format!(
                "within concentration cap ({:.0}% ≤ {:.0}%)",
                projected_share * 100.0,
                cap * 100.0
            )
        } else {
            format!(
                "concentration cap breached ({:.0}% > {:.0}%) — route to a sibling genome",
                projected_share * 100.0,
                cap * 100.0
            )
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::testkit::sample_genome_with_runs;

    #[test]
    fn premium_exceeds_expected_loss_via_risk_loading() {
        let genomes = vec![sample_genome_with_runs("billing.invoice", 100, 50_000.0)];
        let report = assess(&genomes, DEFAULT_CONCENTRATION_CAP);
        let line = &report.premiums[0];
        assert!(
            line.premium > line.expected_loss,
            "premium must load the tail"
        );
        assert!(report.portfolio_tail_loss_99 >= report.portfolio_expected_loss);
    }

    #[test]
    fn concentration_breach_is_detected() {
        let mut dominant = sample_genome_with_runs("billing.dunning", 900, 90_000.0);
        dominant.genome_id = "dominant".into();
        let mut sibling = sample_genome_with_runs("billing.dunning", 100, 10_000.0);
        sibling.genome_id = "sibling".into();
        let report = assess(&[dominant, sibling], 0.35);
        let limit = report
            .concentration
            .iter()
            .find(|c| c.operation == "billing.dunning")
            .unwrap();
        assert!(limit.breached, "a 90% runner must breach a 35% cap");
        assert!(report.systemic_risk_score > 0.0);
    }

    #[test]
    fn admission_gate_blocks_over_cap_traffic() {
        let mut dominant = sample_genome_with_runs("billing.dunning", 900, 90_000.0);
        dominant.genome_id = "dominant".into();
        let mut sibling = sample_genome_with_runs("billing.dunning", 100, 10_000.0);
        sibling.genome_id = "sibling".into();
        let genomes = vec![dominant, sibling];
        let verdict = admits_more_traffic(&genomes, "billing.dunning", "dominant", 0.35);
        assert!(!verdict.allowed);
    }

    #[test]
    fn lone_genome_is_never_capped() {
        let genomes = vec![sample_genome_with_runs("billing.invoice", 5, 500.0)];
        let verdict = admits_more_traffic(&genomes, "billing.invoice", &genomes[0].genome_id, 0.35);
        assert!(
            verdict.allowed,
            "a lone genome cannot concentrate against itself"
        );
    }
}

//! The Genome Commons — the refinement lattice and its algebra.
//!
//! The brief's organising insight (§6.0) is that the commons is a *shared prior*
//! over intent: plain-language intent always under-specifies an operation, so a
//! fork need only supply the description length of its deviations — the parent
//! carries the other ten thousand bits. §6.2 then makes "fork" precise: a child
//! is a **refinement** of its parent (it preserves the parent's guarantees while
//! adding constraints), and refinement is a partial order, so the whole commons
//! is a **refinement lattice**. Composition is **assume-guarantee**: `A` then
//! `B` is valid exactly when `guarantees(A) ⇒ assumptions(B)`.
//!
//! This module is the decidable algebra over those facts. The heavy lifting —
//! whether one envelope refines another, whether guarantees discharge
//! assumptions — lives in [`super::envelope`]; here we assemble it into fork
//! validation with MDL accounting, composition reports, and the lattice graph
//! the console renders.

use serde::{Deserialize, Serialize};

use super::envelope::Envelope;
use super::model::{Genome, GenomeStatus};

/// The verdict on whether a proposed child is a valid refinement of its parent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementCheck {
    pub is_refinement: bool,
    /// Parent guarantees the child preserves.
    pub preserved: Vec<String>,
    /// Constraints the child adds over the parent.
    pub added: Vec<String>,
    /// Parent guarantees the child fails to preserve (non-empty ⇒ invalid fork).
    pub violated: Vec<String>,
    /// Whether the child's residual bound is at least as tight as the parent's.
    pub residual_tightened: bool,
    /// Bits the child had to supply over the inherited prior (§6.0).
    pub deviation_bits: f64,
    /// The full description length of the child's intent.
    pub description_bits: f64,
    /// The fraction of the child's description the parent prior already carried.
    pub prior_leverage: f64,
}

/// Validate a proposed fork: the child envelope must refine the parent's, and
/// its residual must be no looser. Returns the MDL accounting either way.
#[must_use]
pub fn check_refinement(
    parent: &Genome,
    child_envelope: &Envelope,
    child_epsilon: f64,
) -> RefinementCheck {
    let is_ref = child_envelope.refines(&parent.envelope);
    let residual_tightened = child_epsilon <= parent.residual.conformal.epsilon + 1e-9;

    let parent_described = parent.envelope.describe();
    let child_described = child_envelope.describe();

    let preserved: Vec<String> = parent_described
        .iter()
        .filter(|guarantee| child_described.contains(guarantee))
        .cloned()
        .collect();
    let added: Vec<String> = child_described
        .iter()
        .filter(|guarantee| !parent_described.contains(guarantee))
        .cloned()
        .collect();
    let violated: Vec<String> = if is_ref {
        Vec::new()
    } else {
        // The parent guarantees not entailed by the child — why the fork is invalid.
        let mut unmet = parent.envelope.unmet_by_child(child_envelope);
        if !residual_tightened {
            unmet.push(format!(
                "residual bound loosened (ε {:.3} → {:.3})",
                parent.residual.conformal.epsilon, child_epsilon
            ));
        }
        unmet
    };

    let description_bits = child_envelope.description_bits();
    // The deviation is only the bits the child adds over the inherited prior.
    let deviation_bits: f64 = added.iter().map(|_| 20.0).sum::<f64>().max(
        if child_epsilon < parent.residual.conformal.epsilon {
            8.0
        } else {
            0.0
        },
    );
    let prior_leverage = if description_bits > 0.0 {
        (1.0 - deviation_bits / description_bits).clamp(0.0, 1.0)
    } else {
        0.0
    };

    RefinementCheck {
        is_refinement: is_ref && residual_tightened,
        preserved,
        added,
        violated,
        residual_tightened,
        deviation_bits,
        description_bits,
        prior_leverage,
    }
}

/// The verdict on composing two genomes into a pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionReport {
    pub valid: bool,
    pub upstream: String,
    pub downstream: String,
    /// Whether the upstream guarantees discharge the downstream assumptions.
    pub discharged: bool,
    /// Downstream assumptions the upstream fails to guarantee (why it is unsafe).
    pub unmet_assumptions: Vec<String>,
    /// The guarantees the composite pipeline carries (the conjunction of both).
    pub composite_guarantees: Vec<String>,
    pub detail: String,
}

/// Decide whether `upstream ▷ downstream` is a safe composition (§6.2). Safety is
/// not compositional in general — `A` certified and `B` certified does not imply
/// `A;B` is certified — which is exactly why this check exists.
#[must_use]
pub fn compose(upstream: &Genome, downstream: &Genome) -> CompositionReport {
    let discharged = upstream
        .contract
        .guarantees
        .discharges(&downstream.contract.assumptions);
    let unmet = upstream
        .contract
        .guarantees
        .unmet_by(&downstream.contract.assumptions);
    let mut composite: Vec<String> = upstream.contract.guarantees.describe();
    composite.extend(downstream.contract.guarantees.describe());
    composite.dedup();
    let detail = if discharged {
        format!(
            "guarantees({}) ⇒ assumptions({}) — composition preserves both contracts",
            upstream.name, downstream.name
        )
    } else {
        format!(
            "unsafe: {} does not discharge {} assumption(s) of {}",
            upstream.name,
            unmet.len(),
            downstream.name
        )
    };
    CompositionReport {
        valid: discharged,
        upstream: upstream.genome_id.clone(),
        downstream: downstream.genome_id.clone(),
        discharged,
        unmet_assumptions: unmet,
        composite_guarantees: composite,
        detail,
    }
}

/// A node in the rendered lattice graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeNode {
    pub id: String,
    pub name: String,
    pub operation: String,
    pub status: GenomeStatus,
    pub generation: u32,
    pub parent: Option<String>,
    pub certificate_valid: bool,
    pub outcome_value: f64,
    pub runs: u64,
    pub network_share: f64,
}

/// A directed refinement edge (parent → child).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeEdge {
    pub parent: String,
    pub child: String,
    pub kind: String,
    pub deviation_bits: f64,
}

/// A cluster of genomes competing on one operation (the unit concentration
/// limits apply to).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationCluster {
    pub operation: String,
    pub genomes: usize,
    pub total_outcome_value: f64,
    pub roots: usize,
}

/// The full lattice graph the console renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeGraph {
    pub nodes: Vec<LatticeNode>,
    pub edges: Vec<LatticeEdge>,
    pub operations: Vec<OperationCluster>,
    pub generations: u32,
}

/// Assemble the refinement lattice from the current commons.
#[must_use]
pub fn lattice_graph(genomes: &[Genome]) -> LatticeGraph {
    let nodes: Vec<LatticeNode> = genomes
        .iter()
        .map(|genome| LatticeNode {
            id: genome.genome_id.clone(),
            name: genome.name.clone(),
            operation: genome.operation.clone(),
            status: genome.status,
            generation: genome.generation,
            parent: genome.parent.clone(),
            certificate_valid: genome.certificate.is_valid(),
            outcome_value: genome.metrics.outcome_value,
            runs: genome.metrics.runs,
            network_share: genome.metrics.network_share,
        })
        .collect();

    let edges: Vec<LatticeEdge> = genomes
        .iter()
        .filter_map(|genome| {
            genome.parent.as_ref().map(|parent| LatticeEdge {
                parent: parent.clone(),
                child: genome.genome_id.clone(),
                kind: "refines".into(),
                deviation_bits: genome.deviation_bits,
            })
        })
        .collect();

    let mut operations: std::collections::BTreeMap<String, OperationCluster> =
        std::collections::BTreeMap::new();
    for genome in genomes {
        let cluster = operations
            .entry(genome.operation.clone())
            .or_insert_with(|| OperationCluster {
                operation: genome.operation.clone(),
                genomes: 0,
                total_outcome_value: 0.0,
                roots: 0,
            });
        cluster.genomes += 1;
        cluster.total_outcome_value += genome.metrics.outcome_value;
        if genome.parent.is_none() {
            cluster.roots += 1;
        }
    }

    let generations = genomes
        .iter()
        .map(|genome| genome.generation)
        .max()
        .unwrap_or(0)
        + 1;

    LatticeGraph {
        nodes,
        edges,
        operations: operations.into_values().collect(),
        generations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::envelope::Invariant;
    use crate::genome::testkit::sample_genome;

    #[test]
    fn valid_fork_is_a_refinement_with_high_prior_leverage() {
        let parent = sample_genome("billing.dunning", 0.1);
        let mut child_env = parent.envelope.clone();
        child_env.invariants.push(Invariant::Requires {
            flag: "manager_approved".into(),
        });
        let check = check_refinement(&parent, &child_env, 0.08);
        assert!(check.is_refinement);
        assert!(check.violated.is_empty());
        assert_eq!(check.added.len(), 1);
        // The parent prior should carry most of the description length.
        assert!(
            check.prior_leverage > 0.5,
            "leverage {}",
            check.prior_leverage
        );
    }

    #[test]
    fn dropping_a_guarantee_is_not_a_refinement() {
        let parent = sample_genome("billing.dunning", 0.1);
        let child_env = Envelope::new(vec![Invariant::Requires {
            flag: "unrelated".into(),
        }]);
        let check = check_refinement(&parent, &child_env, 0.1);
        assert!(!check.is_refinement);
        assert!(!check.violated.is_empty());
    }
}

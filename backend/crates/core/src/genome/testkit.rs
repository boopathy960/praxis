//! Shared builders for the genome unit tests. Compiled only under `cfg(test)`.

use crate::common::now_ms;

use super::conformal::ConformalState;
use super::envelope::{Envelope, Invariant};
use super::model::{Certificate, Contract, Genome, GenomeMetrics, GenomeStatus, Residual, VarSpec};

/// A representative billing genome with a real money-conservation envelope, an
/// idempotency guarantee, and a conformal residual at the given `epsilon`.
#[must_use]
pub fn sample_genome(operation: &str, epsilon: f64) -> Genome {
    let envelope = Envelope::new(vec![
        Invariant::MoneyConservation {
            outflow: "refund".into(),
            inflow: "paid".into(),
        },
        Invariant::Idempotency {
            key: "invoice_id".into(),
        },
        Invariant::Bounded {
            var: "discount_pct".into(),
            lo: 0.0,
            hi: 20.0,
        },
    ]);
    let guarantees = Envelope::new(vec![Invariant::MoneyConservation {
        outflow: "refund".into(),
        inflow: "paid".into(),
    }]);
    let envelope_check = envelope.compile_to_check("violations.test");
    let now = now_ms();
    Genome {
        genome_id: format!("gen_{operation}_{epsilon}"),
        name: format!("{operation} genome"),
        intent: format!("operate {operation} safely"),
        operation: operation.into(),
        variables: vec![
            VarSpec::money("paid", "Amount paid"),
            VarSpec::money("refund", "Refund amount"),
            VarSpec::count("invoice_id", "Invoice id"),
        ],
        envelope,
        residual: Residual::new("choose the refund", "refund", 50.0, 25.0, epsilon),
        contract: Contract {
            assumptions: Envelope::default(),
            guarantees,
        },
        certificate: Certificate::unanchored(envelope_check),
        parent: None,
        lineage: Vec::new(),
        generation: 0,
        status: GenomeStatus::Draft,
        description_bits: 100.0,
        deviation_bits: 0.0,
        metrics: GenomeMetrics::default(),
        created_at_ms: now,
        updated_at_ms: now,
        version: 1,
    }
}

/// A genome carrying a fixed conformal calibration, for pricing/risk tests.
#[must_use]
pub fn sample_genome_with_runs(operation: &str, runs: u64, outcome: f64) -> Genome {
    let mut genome = sample_genome(operation, 0.1);
    genome.metrics.runs = runs;
    genome.metrics.admitted = runs;
    genome.metrics.outcome_value = outcome;
    genome.residual.conformal = ConformalState::new(0.1);
    genome
}

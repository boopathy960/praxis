//! The genome aggregate — the certified contract `(E, R, C)`.
//!
//! A **genome** is the atomic unit of Genome OS: a verified, executable
//! description of one business behaviour. The brief defines it as a triple —
//! an [`Envelope`] of hard, decidable constraints, a [`Residual`] region of
//! bounded judgment, and a [`Certificate`] that proves the envelope always holds
//! and statistically bounds the residual. This module is that aggregate plus the
//! lineage, contract, metering, and request/response types the service speaks.

use serde::{Deserialize, Serialize};

use crate::common::now_ms;
use crate::verification::Check;

use super::conformal::ConformalState;
use super::envelope::{Envelope, OpValue};

/// The kind of a typed operation variable, mirroring [`OpValue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarKind {
    Money,
    Count,
    Flag,
    Tag,
    Time,
}

/// A declared operation variable — its name, type, a human label and unit, and
/// an optional default. Drives both envelope checking and UI projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarSpec {
    pub name: String,
    pub kind: VarKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<OpValue>,
    /// Whether a human must supply this at run time (vs. derived/observed).
    #[serde(default)]
    pub human_input: bool,
}

impl VarSpec {
    #[must_use]
    pub fn money(name: &str, label: &str) -> Self {
        Self {
            name: name.into(),
            kind: VarKind::Money,
            label: label.into(),
            unit: Some("USD".into()),
            default: None,
            human_input: true,
        }
    }
    #[must_use]
    pub fn count(name: &str, label: &str) -> Self {
        Self {
            name: name.into(),
            kind: VarKind::Count,
            label: label.into(),
            unit: None,
            default: None,
            human_input: true,
        }
    }
    #[must_use]
    pub fn flag(name: &str, label: &str) -> Self {
        Self {
            name: name.into(),
            kind: VarKind::Flag,
            label: label.into(),
            unit: None,
            default: Some(OpValue::Flag(false)),
            human_input: true,
        }
    }
    #[must_use]
    pub fn tag(name: &str, label: &str) -> Self {
        Self {
            name: name.into(),
            kind: VarKind::Tag,
            label: label.into(),
            unit: None,
            default: None,
            human_input: true,
        }
    }
}

/// The Residual (R) — the region of judgment the agent may act within, bounded
/// (never proven) by the adaptive conformal certificate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Residual {
    /// Plain-language description of the judgment this genome delegates.
    pub description: String,
    /// The judgment variable scored for non-conformity (e.g. the chosen refund).
    pub judgment_var: String,
    /// Centre of the human-ratified band for the judgment variable.
    pub approved_center: f64,
    /// Half-width of the human-ratified band.
    pub approved_halfwidth: f64,
    /// The live adaptive-conformal calibration state.
    pub conformal: ConformalState,
}

impl Residual {
    #[must_use]
    pub fn new(
        description: &str,
        judgment_var: &str,
        center: f64,
        halfwidth: f64,
        epsilon: f64,
    ) -> Self {
        Self {
            description: description.into(),
            judgment_var: judgment_var.into(),
            approved_center: center,
            approved_halfwidth: halfwidth,
            conformal: ConformalState::new(epsilon),
        }
    }
}

/// The Certificate (C) — the proof object a genome carries: a logical proof that
/// the envelope always holds (anchored as a real claim in the proof economy via
/// the compiled [`Check`]) plus the conformal bound on the residual.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    /// The proof-economy claim id anchoring the logical guarantee, once staked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    /// The compiled, device-runnable check the certificate stands on.
    pub envelope_check: Check,
    /// "unanchored" | "proposed" | "minted" | "refuted" — the live ledger status.
    pub logical_status: String,
    /// Whether the envelope check currently passes when re-run.
    pub envelope_holds: bool,
    /// The conformal coverage the residual currently realises.
    pub conformal_coverage: f64,
    /// The coverage the residual targets (`1 − ε`).
    pub conformal_target: f64,
    /// Whether the residual bound currently holds.
    pub residual_in_bound: bool,
    /// Calibration maturity in `[0,1]` — surfaces the cold-start weakness.
    pub maturity: f64,
    pub issued_at_ms: i64,
    pub verified_at_ms: i64,
}

impl Certificate {
    #[must_use]
    pub fn unanchored(envelope_check: Check) -> Self {
        let now = now_ms();
        Self {
            claim_id: None,
            envelope_check,
            logical_status: "unanchored".into(),
            envelope_holds: true,
            conformal_coverage: 1.0,
            conformal_target: 0.9,
            residual_in_bound: true,
            maturity: 0.0,
            issued_at_ms: now,
            verified_at_ms: now,
        }
    }

    /// Whether the certificate is sellable per the brief's honesty standard:
    /// the envelope provably holds AND the residual is within its conformal
    /// bound. Not "the agent is right" — "the agent provably never violates
    /// these invariants, and its judgment deviates less than ε of the time."
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.envelope_holds && self.residual_in_bound
    }
}

/// The Assume/Guarantee contract a genome carries for safe composition (§6.2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Contract {
    /// What the genome assumes about its inputs and environment.
    pub assumptions: Envelope,
    /// What the genome guarantees about its behaviour.
    pub guarantees: Envelope,
}

/// Lifecycle status of a genome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenomeStatus {
    /// Authored but not yet certified.
    Draft,
    /// Certificate proposed/anchored and currently valid.
    Certified,
    /// Certificate minted in the proof economy (survived adversarial attack).
    Minted,
    /// A guarantee regressed — pulled from traffic pending a heal.
    Quarantined,
    /// Superseded by a refinement or retired.
    Deprecated,
}

impl GenomeStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            GenomeStatus::Draft => "draft",
            GenomeStatus::Certified => "certified",
            GenomeStatus::Minted => "minted",
            GenomeStatus::Quarantined => "quarantined",
            GenomeStatus::Deprecated => "deprecated",
        }
    }
}

/// Outcome metering for the genome — the unit you are billed against, and the
/// signal the self-improvement loop and the underwriter read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenomeMetrics {
    /// Total operations executed.
    pub runs: u64,
    /// Operations that completed inside the envelope.
    pub admitted: u64,
    /// Operations blocked at the envelope gate (do-no-harm, by construction).
    pub blocked: u64,
    /// Residual judgments routed to a human for sign-off.
    pub human_signoffs: u64,
    /// Cumulative metered outcome value (e.g. cash collected) — the billing base.
    pub outcome_value: f64,
    /// Recorded envelope violations (should stay zero by construction).
    pub violations: u64,
    /// Share of its critical operation this genome runs network-wide (§6.5).
    pub network_share: f64,
    /// Underwriting premium for the warranty layer (§6.5).
    pub premium: f64,
}

/// The genome aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genome {
    pub genome_id: String,
    pub name: String,
    /// The plain-language intent this genome was compiled from.
    pub intent: String,
    /// The operation family, e.g. `billing.dunning` — the unit of competition
    /// for concentration limits.
    pub operation: String,
    /// Declared, typed operation variables.
    pub variables: Vec<VarSpec>,
    pub envelope: Envelope,
    pub residual: Residual,
    pub contract: Contract,
    pub certificate: Certificate,
    /// Direct parent in the refinement lattice (a fork's source), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Ancestors root→parent — the lineage the underwriter contagion-models and
    /// the pricer pays royalties up.
    #[serde(default)]
    pub lineage: Vec<String>,
    pub generation: u32,
    pub status: GenomeStatus,
    /// MDL accounting (§6.0): total description length of the intent, and the
    /// bits this genome had to supply over its parent (the deviation).
    pub description_bits: f64,
    pub deviation_bits: f64,
    pub metrics: GenomeMetrics,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub version: u32,
}

impl Genome {
    /// A compact summary for list/graph views.
    #[must_use]
    pub fn summary(&self) -> GenomeSummary {
        GenomeSummary {
            genome_id: self.genome_id.clone(),
            name: self.name.clone(),
            operation: self.operation.clone(),
            status: self.status,
            parent: self.parent.clone(),
            generation: self.generation,
            certificate_valid: self.certificate.is_valid(),
            outcome_value: self.metrics.outcome_value,
            runs: self.metrics.runs,
            network_share: self.metrics.network_share,
            deviation_bits: self.deviation_bits,
        }
    }
}

/// A compact projection of a genome for the commons graph and list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenomeSummary {
    pub genome_id: String,
    pub name: String,
    pub operation: String,
    pub status: GenomeStatus,
    pub parent: Option<String>,
    pub generation: u32,
    pub certificate_valid: bool,
    pub outcome_value: f64,
    pub runs: u64,
    pub network_share: f64,
    pub deviation_bits: f64,
}

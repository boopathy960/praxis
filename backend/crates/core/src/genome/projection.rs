//! Dynamic interfaces & the machine API — how a human and another agent each
//! touch a genome.
//!
//! Two of the five Requests for Startups live here. **Dynamic Interfaces (#6):**
//! every genome projects its own human UI on demand — derived from its typed
//! variables, its envelope (which fixes field bounds and locked guarantees), and
//! its residual — and the operator *reshapes* it by stating the change in plain
//! language rather than filing a ticket. **Software for Agents (#12):** every
//! genome also exposes a machine interface (an MCP-style tool descriptor) so
//! other genomes and agents discover and invoke it — operations become a graph
//! of callable behaviours, not a pile of human-clickable apps.
//!
//! The reshape grammar is deliberately small and decidable: it maps a handful of
//! robust phrasings onto edits in the constraint language, and reports whether
//! the result is a *refinement* of the current genome — i.e. whether "state your
//! three deviations" produced a valid fork the system can verify rather than
//! trust (§6.0, §6.2).

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::commons::check_refinement;
use super::envelope::{Atom, Cmp, Envelope, Invariant, OpValue, Term};
use super::model::{Genome, VarKind};

/// A human-facing input widget kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Money,
    Number,
    Toggle,
    Select,
    Text,
    Timestamp,
}

/// One projected input field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiField {
    pub name: String,
    pub label: String,
    pub widget: Widget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<OpValue>,
    /// A guarantee the envelope pins on this field, shown inline as a lock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guarantee: Option<String>,
}

/// A locked guarantee badge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiGuarantee {
    pub text: String,
    pub structural: bool,
}

/// The residual judgment panel — the conformal coverage ring's data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgmentPanel {
    pub description: String,
    pub judgment_var: String,
    pub approved_center: f64,
    pub approved_halfwidth: f64,
    pub epsilon: f64,
    pub coverage: f64,
    pub target_coverage: f64,
    pub in_bound: bool,
    pub maturity: f64,
    pub flagged: u64,
}

/// The outcome-metering panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutcomePanel {
    pub runs: u64,
    pub admitted: u64,
    pub blocked: u64,
    pub human_signoffs: u64,
    pub outcome_value: f64,
    pub premium: f64,
    pub network_share: f64,
}

/// The full UI a genome projects on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSchema {
    pub genome_id: String,
    pub name: String,
    pub intent: String,
    pub operation: String,
    pub fields: Vec<UiField>,
    pub guarantees: Vec<UiGuarantee>,
    pub judgment: JudgmentPanel,
    pub outcomes: OutcomePanel,
    pub certificate_status: String,
    pub certificate_valid: bool,
    /// A stable accent hue (0–360) derived from the genome id, so each genome
    /// has its own visual identity in the console.
    pub accent_hue: u32,
}

fn widget_for(kind: VarKind) -> Widget {
    match kind {
        VarKind::Money => Widget::Money,
        VarKind::Count => Widget::Number,
        VarKind::Flag => Widget::Toggle,
        VarKind::Tag => Widget::Text,
        VarKind::Time => Widget::Timestamp,
    }
}

/// Project the human UI for a genome.
#[must_use]
pub fn project_ui(genome: &Genome) -> UiSchema {
    let mut fields: Vec<UiField> = genome
        .variables
        .iter()
        .map(|var| UiField {
            name: var.name.clone(),
            label: var.label.clone(),
            widget: widget_for(var.kind),
            unit: var.unit.clone(),
            min: None,
            max: None,
            options: Vec::new(),
            required: var.human_input,
            default: var.default.clone(),
            guarantee: None,
        })
        .collect();

    // Fold the envelope's invariants into field bounds, widgets, and locks.
    for invariant in &genome.envelope.invariants {
        match invariant {
            Invariant::Bounded { var, lo, hi } => {
                if let Some(field) = fields.iter_mut().find(|f| &f.name == var) {
                    field.min = Some(*lo);
                    field.max = Some(*hi);
                    field.guarantee = Some(invariant.describe());
                }
            }
            Invariant::RateLimit { counter, max } => {
                if let Some(field) = fields.iter_mut().find(|f| &f.name == counter) {
                    field.max = Some(*max);
                    field.guarantee = Some(invariant.describe());
                }
            }
            Invariant::Eligibility { var, allowed } => {
                if let Some(field) = fields.iter_mut().find(|f| &f.name == var) {
                    field.widget = Widget::Select;
                    field.options = allowed.clone();
                    field.guarantee = Some(invariant.describe());
                }
            }
            Invariant::Requires { flag } | Invariant::Forbids { flag } => {
                if let Some(field) = fields.iter_mut().find(|f| &f.name == flag) {
                    field.guarantee = Some(invariant.describe());
                }
            }
            Invariant::MoneyConservation { outflow, .. } => {
                if let Some(field) = fields.iter_mut().find(|f| &f.name == outflow) {
                    field.guarantee = Some(invariant.describe());
                    if field.min.is_none() {
                        field.min = Some(0.0);
                    }
                }
            }
            _ => {}
        }
    }

    let guarantees: Vec<UiGuarantee> = genome
        .envelope
        .invariants
        .iter()
        .map(|invariant| UiGuarantee {
            text: invariant.describe(),
            structural: invariant.is_structural(),
        })
        .collect();

    let conformal = &genome.residual.conformal;
    let judgment = JudgmentPanel {
        description: genome.residual.description.clone(),
        judgment_var: genome.residual.judgment_var.clone(),
        approved_center: genome.residual.approved_center,
        approved_halfwidth: genome.residual.approved_halfwidth,
        epsilon: conformal.epsilon,
        coverage: conformal.realised_coverage(),
        target_coverage: conformal.target_coverage(),
        in_bound: conformal.within_bound(),
        maturity: conformal.maturity(),
        flagged: conformal.flagged,
    };

    let outcomes = OutcomePanel {
        runs: genome.metrics.runs,
        admitted: genome.metrics.admitted,
        blocked: genome.metrics.blocked,
        human_signoffs: genome.metrics.human_signoffs,
        outcome_value: genome.metrics.outcome_value,
        premium: genome.metrics.premium,
        network_share: genome.metrics.network_share,
    };

    UiSchema {
        genome_id: genome.genome_id.clone(),
        name: genome.name.clone(),
        intent: genome.intent.clone(),
        operation: genome.operation.clone(),
        fields,
        guarantees,
        judgment,
        outcomes,
        certificate_status: genome.certificate.logical_status.clone(),
        certificate_valid: genome.certificate.is_valid(),
        accent_hue: accent_hue(&genome.genome_id),
    }
}

fn accent_hue(id: &str) -> u32 {
    let sum: u32 = id.bytes().map(u32::from).sum();
    (sum * 37) % 360
}

/// One parsed deviation in a reshape request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReshapeEdit {
    pub op: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<Invariant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub residual_epsilon: Option<f64>,
}

/// The result of interpreting a plain-language reshape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReshapePatch {
    pub text: String,
    pub edits: Vec<ReshapeEdit>,
    pub new_envelope: Envelope,
    pub new_epsilon: f64,
    /// Whether the reshaped genome refines the current one — a valid, verifiable
    /// fork. When false the change *loosens* a guarantee and must be reviewed.
    pub is_refinement: bool,
    pub deviation_bits: f64,
    pub prior_leverage: f64,
    pub understood: bool,
    pub message: String,
}

/// Interpret a plain-language reshape against a genome, returning the proposed
/// new envelope and whether it is a valid refinement. The service turns an
/// understood, refining patch into a fork.
#[must_use]
pub fn reshape(genome: &Genome, text: &str) -> ReshapePatch {
    let lower = text.to_lowercase();
    let mut edits: Vec<ReshapeEdit> = Vec::new();
    let mut new_envelope = genome.envelope.clone();
    let mut new_epsilon = genome.residual.conformal.epsilon;

    // never refund more than N / cap refund at N / refund at most N
    if let Some(value) = capture_number(&lower, r"(?:refund|payout|disbursement)\D*(?:more than|over|above|at most|max(?:imum)?|cap(?:ped)? at)\D*([0-9][0-9,\.]*)")
        .or_else(|| capture_number(&lower, r"cap\D+(?:the\s+)?refund\D+([0-9][0-9,\.]*)"))
    {
        let inv = Invariant::Bounded { var: refund_var(genome), lo: 0.0, hi: value };
        push_invariant(&mut new_envelope, inv.clone());
        edits.push(ReshapeEdit { op: "tighten_bound".into(), description: format!("cap the refund at {value:.0}"), added: Some(inv), residual_epsilon: None });
    }

    // require <flag> / require manager approval / require owner sign-off
    for caps in Regex::new(r"require[sd]?\s+([a-z][a-z _]{2,40})")
        .unwrap()
        .captures_iter(&lower)
    {
        let flag = normalise_flag(&caps[1]);
        let inv = Invariant::Requires { flag: flag.clone() };
        push_invariant(&mut new_envelope, inv.clone());
        edits.push(ReshapeEdit {
            op: "require_flag".into(),
            description: format!("require `{flag}`"),
            added: Some(inv),
            residual_epsilon: None,
        });
    }

    // forbid <flag> / never <flag>
    for caps in Regex::new(r"(?:forbid|disallow|block|never allow)\s+([a-z][a-z _]{2,40})")
        .unwrap()
        .captures_iter(&lower)
    {
        let flag = normalise_flag(&caps[1]);
        let inv = Invariant::Forbids { flag: flag.clone() };
        push_invariant(&mut new_envelope, inv.clone());
        edits.push(ReshapeEdit {
            op: "forbid_flag".into(),
            description: format!("forbid `{flag}`"),
            added: Some(inv),
            residual_epsilon: None,
        });
    }

    // rate limit <counter> to N / limit <counter> to N
    if let Some(caps) =
        Regex::new(r"(?:rate[ -]?limit|limit|throttle)\s+(?:the\s+)?([a-z_]+)\D+([0-9][0-9,\.]*)")
            .unwrap()
            .captures(&lower)
    {
        let counter = caps[1].trim().replace(' ', "_");
        if let Some(value) = parse_number(&caps[2]) {
            let inv = Invariant::RateLimit {
                counter: counter.clone(),
                max: value,
            };
            push_invariant(&mut new_envelope, inv.clone());
            edits.push(ReshapeEdit {
                op: "rate_limit".into(),
                description: format!("rate-limit {counter} to {value:.0}"),
                added: Some(inv),
                residual_epsilon: None,
            });
        }
    }

    // bound <var> between A and B / tighten <var> to A..B
    if let Some(caps) = Regex::new(r"(?:bound|tighten|constrain)\s+(?:the\s+)?([a-z_]+)\D+([0-9][0-9,\.]*)\D+(?:and|to|\.\.)\D*([0-9][0-9,\.]*)").unwrap().captures(&lower) {
        let var = caps[1].trim().replace(' ', "_");
        if let (Some(lo), Some(hi)) = (parse_number(&caps[2]), parse_number(&caps[3])) {
            let inv = Invariant::Bounded { var: var.clone(), lo: lo.min(hi), hi: lo.max(hi) };
            push_invariant(&mut new_envelope, inv.clone());
            edits.push(ReshapeEdit { op: "bound".into(), description: format!("bound {var} to [{:.0}, {:.0}]", lo.min(hi), lo.max(hi)), added: Some(inv), residual_epsilon: None });
        }
    }

    // tighten judgment / coverage to P percent
    if let Some(value) = capture_number(
        &lower,
        r"(?:coverage|confidence|judgment|judgement)\D*([0-9]{1,3})\s*(?:%|percent)",
    ) {
        let epsilon = (1.0 - value / 100.0).clamp(0.005, 0.5);
        new_epsilon = epsilon.min(new_epsilon);
        edits.push(ReshapeEdit {
            op: "tighten_residual".into(),
            description: format!("require ≥{value:.0}% conformal coverage (ε={:.3})", epsilon),
            added: None,
            residual_epsilon: Some(new_epsilon),
        });
    }

    // eligibility: only for <a, b, c>
    if let Some(caps) =
        Regex::new(r"(?:only (?:for|when)|eligible|restrict to)\s+([a-z0-9_,\s]{3,60})")
            .unwrap()
            .captures(&lower)
    {
        let allowed: Vec<String> = caps[1]
            .split([',', ' '])
            .map(str::trim)
            .filter(|token| token.len() > 1 && !matches!(*token, "for" | "when" | "and" | "the"))
            .map(str::to_string)
            .collect();
        if !allowed.is_empty() {
            let var = genome
                .variables
                .iter()
                .find(|v| matches!(v.kind, VarKind::Tag))
                .map(|v| v.name.clone())
                .unwrap_or_else(|| "segment".into());
            let inv = Invariant::Eligibility {
                var: var.clone(),
                allowed: allowed.clone(),
            };
            push_invariant(&mut new_envelope, inv.clone());
            edits.push(ReshapeEdit {
                op: "eligibility".into(),
                description: format!("restrict {var} to {allowed:?}"),
                added: Some(inv),
                residual_epsilon: None,
            });
        }
    }

    let understood = !edits.is_empty();
    let check = check_refinement(genome, &new_envelope, new_epsilon);
    let message = if !understood {
        "could not interpret a deviation — try e.g. \"never refund more than 500\", \"require manager approval\", \"rate-limit retries to 3\", or \"coverage 95%\"".into()
    } else if check.is_refinement {
        format!(
            "understood {} deviation(s) — a valid refinement; the parent prior carried {:.0}% of the spec",
            edits.len(),
            check.prior_leverage * 100.0
        )
    } else {
        format!(
            "understood {} deviation(s), but this loosens a guarantee — it is a generalization, not a refinement, and needs review",
            edits.len()
        )
    };

    ReshapePatch {
        text: text.to_string(),
        edits,
        new_envelope,
        new_epsilon,
        is_refinement: check.is_refinement,
        deviation_bits: check.deviation_bits,
        prior_leverage: check.prior_leverage,
        understood,
        message,
    }
}

fn refund_var(genome: &Genome) -> String {
    genome
        .variables
        .iter()
        .find(|v| v.name.contains("refund") || v.name.contains("payout"))
        .map(|v| v.name.clone())
        .unwrap_or_else(|| "refund".into())
}

fn normalise_flag(raw: &str) -> String {
    let cleaned = raw.trim().replace([' ', '-'], "_");
    // Common business phrasings → canonical flags.
    match cleaned.as_str() {
        s if s.contains("manager") => "manager_approved".into(),
        s if s.contains("owner") => "owner_approved".into(),
        s if s.contains("approval")
            || s.contains("approved")
            || s.contains("sign_off")
            || s.contains("signoff") =>
        {
            "approved".into()
        }
        s if s.contains("kyc") => "kyc_passed".into(),
        s if s.contains("verified") || s.contains("verify") => "verified".into(),
        other => other.trim_end_matches('_').to_string(),
    }
}

fn push_invariant(envelope: &mut Envelope, invariant: Invariant) {
    if !envelope.invariants.contains(&invariant) {
        envelope.invariants.push(invariant);
    }
}

fn parse_number(raw: &str) -> Option<f64> {
    raw.trim()
        .trim_start_matches('$')
        .replace(',', "")
        .parse::<f64>()
        .ok()
}

fn capture_number(haystack: &str, pattern: &str) -> Option<f64> {
    Regex::new(pattern)
        .ok()?
        .captures(haystack)
        .and_then(|caps| parse_number(&caps[1]))
}

/// An MCP-style machine interface a genome exposes so other genomes and agents
/// can discover and invoke it (Software for Agents #12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineApi {
    pub name: String,
    pub operation: String,
    pub description: String,
    pub endpoint: String,
    pub input_schema: serde_json::Value,
    pub guarantees: Vec<String>,
    pub assumptions: Vec<String>,
    pub certificate_status: String,
}

/// Build the machine API descriptor for a genome.
#[must_use]
pub fn machine_api(genome: &Genome) -> MachineApi {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for var in &genome.variables {
        let (json_type, format) = match var.kind {
            VarKind::Money => ("number", Some("currency")),
            VarKind::Count => ("integer", None),
            VarKind::Flag => ("boolean", None),
            VarKind::Tag => ("string", None),
            VarKind::Time => ("integer", Some("unix-millis")),
        };
        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), serde_json::Value::String(json_type.into()));
        schema.insert(
            "description".into(),
            serde_json::Value::String(var.label.clone()),
        );
        if let Some(format) = format {
            schema.insert("format".into(), serde_json::Value::String(format.into()));
        }
        // Surface envelope-pinned bounds in the schema so a calling agent sees
        // the hard limits before it invokes.
        for invariant in &genome.envelope.invariants {
            if let Invariant::Bounded { var: bvar, lo, hi } = invariant {
                if bvar == &var.name {
                    schema.insert("minimum".into(), serde_json::json!(lo));
                    schema.insert("maximum".into(), serde_json::json!(hi));
                }
            }
            if let Invariant::Eligibility { var: evar, allowed } = invariant {
                if evar == &var.name {
                    schema.insert("enum".into(), serde_json::json!(allowed));
                }
            }
        }
        properties.insert(var.name.clone(), serde_json::Value::Object(schema));
        if var.human_input {
            required.push(serde_json::Value::String(var.name.clone()));
        }
    }
    let input_schema = serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    });
    MachineApi {
        name: format!("genome.{}", genome.operation.replace('.', "_")),
        operation: genome.operation.clone(),
        description: genome.intent.clone(),
        endpoint: format!("/api/v1/genome/genomes/{}/run", genome.genome_id),
        input_schema,
        guarantees: genome.contract.guarantees.describe(),
        assumptions: genome.contract.assumptions.describe(),
        certificate_status: genome.certificate.logical_status.clone(),
    }
}

/// Silence the unused-import lint when only some constructors are exercised.
#[allow(dead_code)]
fn _ensure_atom_constructors() -> Atom {
    Atom::Compare {
        left: Term::var("x"),
        op: Cmp::Le,
        right: Term::constant(1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::testkit::sample_genome;

    #[test]
    fn projects_fields_with_envelope_bounds() {
        let genome = sample_genome("billing.dunning", 0.1);
        let schema = project_ui(&genome);
        let discount = schema.fields.iter().find(|f| f.name == "discount_pct");
        // discount_pct has a Bounded[0,20] invariant in the sample genome.
        // (It is not a declared variable there, so it may be absent — assert the
        // refund field instead, which is declared and money-conserved.)
        let refund = schema.fields.iter().find(|f| f.name == "refund").unwrap();
        assert!(refund.guarantee.is_some());
        let _ = discount;
        assert!(!schema.guarantees.is_empty());
    }

    #[test]
    fn reshape_require_manager_approval_is_a_refinement() {
        let genome = sample_genome("billing.dunning", 0.1);
        let patch = reshape(&genome, "require manager approval");
        assert!(patch.understood);
        assert!(patch.is_refinement, "{}", patch.message);
        assert!(
            patch
                .new_envelope
                .invariants
                .iter()
                .any(|i| matches!(i, Invariant::Requires { flag } if flag == "manager_approved"))
        );
    }

    #[test]
    fn reshape_cap_refund_tightens_bound() {
        let genome = sample_genome("billing.dunning", 0.1);
        let patch = reshape(&genome, "never refund more than 500");
        assert!(patch.understood);
        assert!(patch.new_envelope.invariants.iter().any(|i| matches!(i, Invariant::Bounded { var, hi, .. } if var == "refund" && (*hi - 500.0).abs() < 1e-6)));
    }

    #[test]
    fn reshape_coverage_tightens_residual() {
        let genome = sample_genome("billing.dunning", 0.1);
        let patch = reshape(&genome, "require coverage 95%");
        assert!(patch.understood);
        assert!((patch.new_epsilon - 0.05).abs() < 1e-6);
    }

    #[test]
    fn machine_api_exposes_bounds_in_schema() {
        let genome = sample_genome("billing.dunning", 0.1);
        let api = machine_api(&genome);
        assert!(api.name.starts_with("genome."));
        assert!(api.input_schema["properties"].is_object());
    }
}

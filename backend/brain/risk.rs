// ═══════════════════════════════════════════════════════════════
// Risk Manager — Tri-Shield Objective + Zero-Vulnerability Gate
// ═══════════════════════════════════════════════════════════════
// Every action goes through risk assessment before execution:
//
//   1. Robust Utility  R(s;x) = min_e U(s; x, e)
//   2. Detectability   D(s)   = 1 − Π(1 − q_k(s))
//   3. Containment     K(s)   = E[C(failure)|s,x] · (1 − Cov(G))
//
// Combined: s* = argmax(λ₁R + λ₂D − λ₃K − λ₄Complexity) × Security
//
// Gating:
//   EXECUTE  — high confidence, low risk
//   SANDBOX  — moderate risk, verify first
//   REFUSE   — critical vulns or high risk

use serde::Serialize;

use super::verifier::VerificationReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum GatingMode {
    Execute,
    Sandbox,
    Refuse,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskAssessment {
    pub robust_utility: f64,
    pub detectability: f64,
    pub containment_risk: f64,
    pub complexity: f64,
    pub security_score: f64,
    pub tri_shield_score: f64,
    pub confidence: f64,
    pub risk_level: f64,
    pub mode: GatingMode,
    pub has_critical_vulns: bool,
}

impl Default for RiskAssessment {
    fn default() -> Self {
        Self {
            robust_utility: 0.0,
            detectability: 0.0,
            containment_risk: 0.0,
            complexity: 0.0,
            security_score: 1.0,
            tri_shield_score: 0.0,
            confidence: 0.0,
            risk_level: 1.0,
            mode: GatingMode::Refuse,
            has_critical_vulns: false,
        }
    }
}

/// Tri-Shield risk configuration parameters.
#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub lambda_robust: f64,
    pub lambda_detect: f64,
    pub lambda_contain: f64,
    pub lambda_complexity: f64,
    pub confidence_threshold: f64,
    pub risk_threshold: f64,
    pub sandbox_confidence_threshold: f64,
    pub sandbox_risk_threshold: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            lambda_robust: 0.35,
            lambda_detect: 0.25,
            lambda_contain: 0.25,
            lambda_complexity: 0.15,
            confidence_threshold: 0.7,
            risk_threshold: 0.6,
            sandbox_confidence_threshold: 0.4,
            sandbox_risk_threshold: 0.8,
        }
    }
}

pub struct RiskManager {
    config: RiskConfig,
    total_assessments: u64,
    total_executed: u64,
    total_sandboxed: u64,
    total_refused: u64,
}

impl RiskManager {
    pub fn new() -> Self {
        Self {
            config: RiskConfig::default(),
            total_assessments: 0,
            total_executed: 0,
            total_sandboxed: 0,
            total_refused: 0,
        }
    }

    pub fn with_config(config: RiskConfig) -> Self {
        Self {
            config,
            total_assessments: 0,
            total_executed: 0,
            total_sandboxed: 0,
            total_refused: 0,
        }
    }

    /// Full risk assessment for a candidate solution.
    pub fn assess_risk(
        &mut self,
        candidate: &str,
        verification: &VerificationReport,
        action_type: &str,
    ) -> RiskAssessment {
        self.total_assessments += 1;
        let mut assessment = RiskAssessment::default();

        // (i) Robust Utility: R(s;x) = min_e U(s; x, e)
        assessment.robust_utility = self.compute_robust_utility(verification, action_type);

        // (ii) Detectability: D(s) = 1 − Π(1 − q_k(s))
        assessment.detectability = self.compute_detectability(verification);

        // (iii) Containment: K(s) = E[C(failure)|s,x] · (1 − Cov(G))
        assessment.containment_risk = self.compute_containment(verification, action_type);

        // Complexity measure
        assessment.complexity = self.compute_complexity(candidate);

        // Security from verifier
        assessment.security_score = verification.v_security;
        assessment.has_critical_vulns = verification.has_critical_vulns;

        // Combined Tri-Shield score
        let cfg = &self.config;
        assessment.tri_shield_score = (cfg.lambda_robust * assessment.robust_utility
            + cfg.lambda_detect * assessment.detectability
            - cfg.lambda_contain * assessment.containment_risk
            - cfg.lambda_complexity * assessment.complexity)
            * assessment.security_score.max(0.01);

        // Confidence and risk
        assessment.confidence = verification.confidence;
        assessment.risk_level = (1.0 - assessment.tri_shield_score).clamp(0.0, 1.0);

        // Gate the action
        assessment.mode = self.gate(
            assessment.confidence,
            assessment.risk_level,
            assessment.has_critical_vulns,
        );

        // Stats
        match assessment.mode {
            GatingMode::Execute => self.total_executed += 1,
            GatingMode::Sandbox => self.total_sandboxed += 1,
            GatingMode::Refuse => self.total_refused += 1,
        }

        assessment
    }

    /// R(s;x) = min_e U(s; x, e) — worst-case utility
    fn compute_robust_utility(&self, v: &VerificationReport, action_type: &str) -> f64 {
        let scores = [
            v.v_static,
            v.v_property,
            v.v_scenario,
            v.v_critic,
            v.v_code,
            v.v_security,
        ];
        let active: Vec<f64> = scores.iter().copied().filter(|&s| s > 0.0).collect();

        if active.is_empty() {
            return 0.0;
        }

        let min_score = active.iter().copied().fold(f64::INFINITY, f64::min);

        let risk_multiplier = match action_type {
            "code_execution" => 0.8,
            "file_write" => 0.7,
            "file_delete" => 0.5,
            "web_request" => 0.85,
            "system_command" => 0.4,
            _ => 1.0,
        };

        min_score * risk_multiplier
    }

    /// D(s) = 1 − Π(1 − q_k(s)) — bug detection probability
    fn compute_detectability(&self, v: &VerificationReport) -> f64 {
        let q_values = [
            v.v_static * 0.6,
            v.v_property * 0.7,
            v.v_scenario * 0.8,
            v.v_critic * 0.75,
        ];

        let product: f64 = q_values.iter().map(|q| 1.0 - q).product();
        1.0 - product
    }

    /// K(s) = E[C(failure)|s,x] · (1 − Cov(G)) — expected damage
    fn compute_containment(&self, v: &VerificationReport, action_type: &str) -> f64 {
        let failure_cost = match action_type {
            "code_execution" => 0.4,
            "file_write" => 0.5,
            "file_delete" => 0.9,
            "web_request" => 0.3,
            "system_command" => 0.8,
            _ => 0.1,
        };

        failure_cost * (1.0 - v.confidence)
    }

    /// Complexity measure normalized to [0, 1]
    fn compute_complexity(&self, candidate: &str) -> f64 {
        let words = candidate.split_whitespace().count();
        let lines = candidate.lines().count();
        let word_complexity = (words as f64 / 1000.0).min(1.0);
        let line_complexity = (lines as f64 / 100.0).min(1.0);
        (word_complexity + line_complexity) / 2.0
    }

    /// Safe Action Gating with zero-vulnerability override
    fn gate(&self, confidence: f64, risk: f64, has_critical_vulns: bool) -> GatingMode {
        if has_critical_vulns {
            return GatingMode::Refuse;
        }

        let cfg = &self.config;

        if confidence >= cfg.confidence_threshold && risk <= cfg.risk_threshold {
            GatingMode::Execute
        } else if risk <= cfg.sandbox_risk_threshold
            && confidence >= cfg.sandbox_confidence_threshold
        {
            GatingMode::Sandbox
        } else {
            GatingMode::Refuse
        }
    }

    /// Select the best candidate from multiple assessments.
    pub fn select_best(&self, assessments: &[RiskAssessment]) -> usize {
        if assessments.is_empty() {
            return 0;
        }
        assessments
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.tri_shield_score
                    .partial_cmp(&b.tri_shield_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "engine": "RiskManager",
            "total_assessments": self.total_assessments,
            "total_executed": self.total_executed,
            "total_sandboxed": self.total_sandboxed,
            "total_refused": self.total_refused,
            "config": {
                "confidence_threshold": self.config.confidence_threshold,
                "risk_threshold": self.config.risk_threshold,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_action_executed() {
        let mut rm = RiskManager::new();
        let v = VerificationReport {
            v_static: 0.9,
            v_property: 0.8,
            v_scenario: 0.85,
            v_critic: 0.7,
            v_code: 0.8,
            v_security: 0.9,
            v_symbolic: 0.9,
            confidence: 0.85,
            passed: true,
            has_critical_vulns: false,
            ..Default::default()
        };
        let assessment = rm.assess_risk("good solution", &v, "general");
        assert_eq!(assessment.mode, GatingMode::Execute);
    }

    #[test]
    fn test_critical_vulns_refused() {
        let mut rm = RiskManager::new();
        let v = VerificationReport {
            v_static: 0.9,
            v_property: 0.8,
            v_scenario: 0.8,
            v_critic: 0.7,
            v_code: 0.8,
            v_security: 0.0,
            v_symbolic: 0.9,
            confidence: 0.85,
            passed: false,
            has_critical_vulns: true,
            ..Default::default()
        };
        let assessment = rm.assess_risk("dangerous code", &v, "code_execution");
        assert_eq!(assessment.mode, GatingMode::Refuse);
    }

    #[test]
    fn test_select_best() {
        let rm = RiskManager::new();
        let assessments = vec![
            RiskAssessment {
                tri_shield_score: 0.3,
                ..Default::default()
            },
            RiskAssessment {
                tri_shield_score: 0.8,
                ..Default::default()
            },
            RiskAssessment {
                tri_shield_score: 0.5,
                ..Default::default()
            },
        ];
        assert_eq!(rm.select_best(&assessments), 1);
    }
}

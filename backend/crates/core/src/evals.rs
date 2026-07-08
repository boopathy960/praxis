use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::verification::{Check, ProofContract, run};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCase {
    pub name: String,
    pub check: Check,
    #[serde(default)]
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSuite {
    pub suite_id: String,
    pub name: String,
    pub description: String,
    pub cases: Vec<EvalCase>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateEvalSuiteRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<EvalCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunEvalSuiteRequest {
    #[serde(default)]
    pub contract: Option<ProofContract>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalCaseResult {
    pub name: String,
    pub pass: bool,
    pub detail: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalScorecard {
    pub run_id: String,
    pub suite_id: String,
    pub passed: usize,
    pub total: usize,
    pub weighted_score: f64,
    pub pass: bool,
    pub cases: Vec<EvalCaseResult>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttachEvalSuiteRequest {
    pub target_kind: String,
    pub target_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalAttachment {
    pub attachment_id: String,
    pub suite_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Default)]
pub struct EvalService {
    suites: Shared<HashMap<String, EvalSuite>>,
    runs: Shared<Vec<EvalScorecard>>,
    attachments: Shared<Vec<EvalAttachment>>,
}

impl EvalService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            suites: Shared::new(RwLock::new(HashMap::new())),
            runs: Shared::new(RwLock::new(Vec::new())),
            attachments: Shared::new(RwLock::new(Vec::new())),
        }
    }

    pub fn create_suite(&self, request: CreateEvalSuiteRequest) -> Result<EvalSuite, AppError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(AppError::Validation("eval suite name is empty".into()));
        }
        if request.cases.is_empty() {
            return Err(AppError::Validation(
                "eval suite must include at least one case".into(),
            ));
        }
        let suite = EvalSuite {
            suite_id: new_id("eval_suite"),
            name: name.to_string(),
            description: request.description,
            cases: request.cases,
            created_at_ms: now_ms(),
        };
        self.suites
            .write()
            .insert(suite.suite_id.clone(), suite.clone());
        Ok(suite)
    }

    pub fn suites(&self) -> Vec<EvalSuite> {
        let mut suites = self.suites.read().values().cloned().collect::<Vec<_>>();
        suites.sort_by_key(|suite| std::cmp::Reverse(suite.created_at_ms));
        suites
    }

    pub fn get_suite(&self, suite_id: &str) -> Result<EvalSuite, AppError> {
        self.suites
            .read()
            .get(suite_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("eval suite {suite_id}")))
    }

    pub fn run_suite(
        &self,
        suite_id: &str,
        request: RunEvalSuiteRequest,
        device: &DeviceCapabilities,
    ) -> Result<EvalScorecard, AppError> {
        let mut cases = self.get_suite(suite_id)?.cases;
        if let Some(contract) = request.contract {
            cases.extend(
                contract
                    .checks
                    .into_iter()
                    .enumerate()
                    .map(|(idx, check)| EvalCase {
                        name: format!("contract_check_{}", idx + 1),
                        check,
                        weight: Some(1.0),
                    }),
            );
        }
        let results = cases
            .iter()
            .map(|case| {
                let outcome = run(&case.check, device, None);
                EvalCaseResult {
                    name: case.name.clone(),
                    pass: outcome.pass,
                    detail: outcome.detail,
                    weight: case.weight.unwrap_or(1.0).max(0.0),
                }
            })
            .collect::<Vec<_>>();
        let total_weight = results.iter().map(|case| case.weight).sum::<f64>().max(1.0);
        let score = results
            .iter()
            .filter(|case| case.pass)
            .map(|case| case.weight)
            .sum::<f64>()
            / total_weight;
        let scorecard = EvalScorecard {
            run_id: new_id("eval_run"),
            suite_id: suite_id.to_string(),
            passed: results.iter().filter(|case| case.pass).count(),
            total: results.len(),
            weighted_score: score,
            pass: score >= 1.0,
            cases: results,
            created_at_ms: now_ms(),
        };
        self.runs.write().push(scorecard.clone());
        Ok(scorecard)
    }

    pub fn runs(&self, limit: usize) -> Vec<EvalScorecard> {
        self.runs
            .read()
            .iter()
            .rev()
            .take(limit.clamp(1, 500))
            .cloned()
            .collect()
    }

    pub fn attach(
        &self,
        suite_id: &str,
        request: AttachEvalSuiteRequest,
    ) -> Result<EvalAttachment, AppError> {
        let _ = self.get_suite(suite_id)?;
        let target_kind = request.target_kind.trim();
        let target_id = request.target_id.trim();
        if target_kind.is_empty() || target_id.is_empty() {
            return Err(AppError::Validation(
                "target_kind and target_id must not be empty".into(),
            ));
        }
        let attachment = EvalAttachment {
            attachment_id: new_id("eval_attach"),
            suite_id: suite_id.to_string(),
            target_kind: target_kind.to_string(),
            target_id: target_id.to_string(),
            created_at_ms: now_ms(),
        };
        self.attachments.write().push(attachment.clone());
        Ok(attachment)
    }

    pub fn attachments(&self) -> Vec<EvalAttachment> {
        self.attachments.read().iter().rev().cloned().collect()
    }
}

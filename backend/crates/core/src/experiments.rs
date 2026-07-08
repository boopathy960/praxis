use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms};
use crate::device_agent::DeviceCapabilities;
use crate::proof_economy::{Claim, ClaimKind, ProofEconomy, ProposeRequest};
use crate::verification::{ProofContract, run_contract};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Proposed,
    Verified,
    Refuted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub experiment_id: String,
    pub idea: String,
    pub hypothesis: String,
    pub contract: ProofContract,
    pub status: ExperimentStatus,
    pub detail: String,
    #[serde(default)]
    pub claim_id: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateExperimentRequest {
    pub idea: String,
    pub hypothesis: String,
    #[serde(default)]
    pub contract: ProofContract,
    #[serde(default)]
    pub mint_claim: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExperimentRun {
    pub experiment: Experiment,
    pub claim: Option<Claim>,
}

#[derive(Clone)]
pub struct ExperimentFactory {
    experiments: Shared<HashMap<String, Experiment>>,
    device: Arc<DeviceCapabilities>,
    economy: ProofEconomy,
}

impl ExperimentFactory {
    #[must_use]
    pub fn new(device: Arc<DeviceCapabilities>, economy: ProofEconomy) -> Self {
        Self {
            experiments: Shared::new(RwLock::new(HashMap::new())),
            device,
            economy,
        }
    }

    pub fn create_and_run(
        &self,
        request: CreateExperimentRequest,
    ) -> Result<ExperimentRun, AppError> {
        let idea = request.idea.trim();
        let hypothesis = request.hypothesis.trim();
        if idea.is_empty() || hypothesis.is_empty() {
            return Err(AppError::Validation(
                "experiment idea and hypothesis must not be empty".into(),
            ));
        }
        let now = now_ms();
        let outcome = run_contract(&request.contract, &self.device, None);
        let status = if outcome.pass {
            ExperimentStatus::Verified
        } else {
            ExperimentStatus::Refuted
        };
        let mut experiment = Experiment {
            experiment_id: new_id("experiment"),
            idea: idea.to_string(),
            hypothesis: hypothesis.to_string(),
            contract: request.contract.clone(),
            status,
            detail: outcome.detail,
            claim_id: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        let claim = if outcome.pass && request.mint_claim.unwrap_or(true) {
            let claim = self.economy.propose(ProposeRequest {
                statement: format!("experiment verified: {}", experiment.hypothesis),
                kind: ClaimKind::Assertion,
                proposer: "experiment_factory".into(),
                verification: experiment.contract.primary_check(),
                evidence: vec![format!("experiment:{}", experiment.experiment_id)],
                depends_on: Vec::new(),
            })?;
            experiment.claim_id = Some(claim.claim_id.clone());
            Some(claim)
        } else {
            None
        };
        self.experiments
            .write()
            .insert(experiment.experiment_id.clone(), experiment.clone());
        Ok(ExperimentRun { experiment, claim })
    }

    pub fn list(&self, limit: usize) -> Vec<Experiment> {
        let mut experiments = self
            .experiments
            .read()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        experiments.sort_by_key(|experiment| std::cmp::Reverse(experiment.created_at_ms));
        experiments.truncate(limit.clamp(1, 500));
        experiments
    }

    pub fn get(&self, id: &str) -> Result<Experiment, AppError> {
        self.experiments
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("experiment {id}")))
    }
}

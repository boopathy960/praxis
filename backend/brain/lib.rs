pub mod certified;

#[path = "mod.rs"]
pub mod brain;

pub mod crypto {
    pub mod hash {
        use sha3::{Digest, Sha3_256};

        #[must_use]
        pub fn sha3_256(data: &[u8]) -> Vec<u8> {
            Sha3_256::digest(data).to_vec()
        }

        #[must_use]
        pub fn sha3_256_hex(data: &[u8]) -> String {
            let bytes = sha3_256(data);
            bytes.iter().map(|byte| format!("{byte:02x}")).collect()
        }
    }
}

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum AstraError {
        #[error("internal error: {0}")]
        Internal(String),
        #[error("validation error: {0}")]
        Validation(String),
        #[error("not found: {0}")]
        NotFound(String),
        #[error(transparent)]
        Io(#[from] std::io::Error),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
    }

    pub type AstraResult<T> = Result<T, AstraError>;
}

pub mod intelligence {
    pub mod reasoning {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ReasoningStrategy {
            ChainOfThought,
            TreeOfThought,
            Decomposition,
            BackwardChaining,
            HypothesisTest,
            AnalogicalReasoning,
            SelfCritique,
            FormalReasoning,
            CausalReasoning,
            MetaCognition,
            BayesianInference,
            CounterfactualReasoning,
            AbductiveReasoning,
            ConstraintSatisfaction,
            AdversarialReasoning,
            Deductive,
            Inductive,
            Causal,
            Counterfactual,
            ConstraintSolving,
            MonteCarloTreeSearch,
            MetaReasoning,
        }
    }
}

pub use brain::*;
pub use certified::*;

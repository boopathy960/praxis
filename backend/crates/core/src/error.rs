use thiserror::Error;

pub type AstraResult<T> = Result<T, AstraError>;

#[derive(Debug, Error)]
pub enum AstraError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("persistence error: {0}")]
    Persistence(String),
}

impl From<std::io::Error> for AstraError {
    fn from(error: std::io::Error) -> Self {
        Self::Persistence(error.to_string())
    }
}

impl From<serde_json::Error> for AstraError {
    fn from(error: serde_json::Error) -> Self {
        Self::Persistence(error.to_string())
    }
}

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: T,
}

impl<T> ApiResponse<T> {
    #[must_use]
    pub fn ok(data: T) -> Self {
        Self { ok: true, data }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub event_id: String,
    pub actor_id: String,
    pub action: String,
    pub target: String,
    pub metadata: serde_json::Value,
    pub occurred_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenantScope {
    Global,
    Organization(String),
    Workspace(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub service_name: String,
    pub host: String,
    pub port: u16,
    pub environment: String,
    pub public_base_url: String,
    pub data_dir: String,
    pub cors_allowed_origins: Vec<String>,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub allow_cleartext_loopback: bool,
    pub admin_token: Option<String>,
}

impl AppConfig {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            service_name: env::var("ASTRA_SERVICE_NAME")
                .unwrap_or_else(|_| "astra-personal-assistant".into()),
            host: env::var("ASTRA_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: env::var("ASTRA_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8080),
            environment: env::var("ASTRA_ENV").unwrap_or_else(|_| "development".into()),
            public_base_url: env::var("ASTRA_PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            data_dir: env::var("ASTRA_DATA_DIR").unwrap_or_else(|_| "./data".into()),
            cors_allowed_origins: env::var("ASTRA_CORS_ALLOWED_ORIGINS")
                .ok()
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_else(|| vec!["http://127.0.0.1:8080".into()]),
            tls_cert_path: env::var("ASTRA_TLS_CERT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            tls_key_path: env::var("ASTRA_TLS_KEY_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            allow_cleartext_loopback: env::var("ASTRA_ALLOW_CLEARTEXT_LOOPBACK")
                .ok()
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
                .unwrap_or(true),
            admin_token: env::var("ASTRA_ADMIN_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    pub fn validate(self) -> Result<Self, AppError> {
        if self.host.trim().is_empty() {
            return Err(AppError::Validation("ASTRA_HOST must not be empty".into()));
        }
        if self.port == 0 {
            return Err(AppError::Validation("ASTRA_PORT must be non-zero".into()));
        }
        if self.service_name.trim().is_empty() {
            return Err(AppError::Validation(
                "ASTRA_SERVICE_NAME must not be empty".into(),
            ));
        }
        if self.tls_cert_path.is_some() ^ self.tls_key_path.is_some() {
            return Err(AppError::Validation(
                "ASTRA_TLS_CERT_PATH and ASTRA_TLS_KEY_PATH must be set together".into(),
            ));
        }
        if !self.is_development() && self.admin_token.is_none() {
            return Err(AppError::Validation(
                "ASTRA_ADMIN_TOKEN is required outside development/test".into(),
            ));
        }

        fs::create_dir_all(&self.data_dir).map_err(|error| {
            AppError::Internal(format!(
                "failed to create data directory {}: {error}",
                self.data_dir
            ))
        })?;
        Ok(self)
    }

    #[must_use]
    pub fn is_development(&self) -> bool {
        matches!(self.environment.as_str(), "development" | "dev" | "test")
    }

    #[must_use]
    pub fn data_path(&self, file_name: &str) -> PathBuf {
        PathBuf::from(&self.data_dir).join(file_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub service: String,
    pub version: String,
    pub environment: String,
    pub dependencies: BTreeMap<String, String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    Validation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({
            "ok": false,
            "error": self.to_string(),
        }))
    }
}

/// Runs a blocking task (e.g. `reqwest::blocking` HTTP exchange) on a dedicated
/// OS thread so its internal tokio runtime is never created or dropped inside an
/// async runtime context, which would panic with "Cannot drop a runtime in a
/// context where blocking is not allowed".
pub fn run_blocking_io<T, F>(task: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name("astra-blocking-io".into())
        .spawn(task)
        .map_err(|error| AppError::Internal(format!("failed to spawn blocking io thread: {error}")))?
        .join()
        .map_err(|_| AppError::Internal("blocking io thread panicked".into()))?
}

#[must_use]
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

#[must_use]
pub fn seconds_from_now(seconds: u64) -> i64 {
    now_ms() + (seconds as i64 * 1000)
}

#[must_use]
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::now_v7().simple())
}

#[must_use]
pub fn random_token(prefix: &str, length: usize) -> String {
    let mut rng = rand::rng();
    let suffix = Alphanumeric.sample_string(&mut rng, length);
    format!("{prefix}_{suffix}")
}

#[must_use]
pub fn sha3_hex(input: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha3_256::new();
    hasher.update(input.as_ref());
    hex::encode(hasher.finalize())
}

/// Compares a provided secret against the expected one without leaking the
/// match position or the secret length through timing. Both sides are hashed
/// to a fixed width, then folded with XOR.
#[must_use]
pub fn constant_time_token_eq(provided: &str, expected: &str) -> bool {
    let provided = Sha3_256::digest(provided.as_bytes());
    let expected = Sha3_256::digest(expected.as_bytes());
    provided
        .iter()
        .zip(expected.iter())
        .fold(0u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

#[must_use]
pub fn audit_event(
    actor_id: impl Into<String>,
    action: impl Into<String>,
    target: impl Into<String>,
    metadata: serde_json::Value,
) -> AuditEvent {
    AuditEvent {
        event_id: new_id("audit"),
        actor_id: actor_id.into(),
        action: action.into(),
        target: target.into(),
        metadata,
        occurred_at_ms: now_ms(),
    }
}

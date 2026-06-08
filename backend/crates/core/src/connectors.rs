use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms, sha3_hex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorProvider {
    Reddit,
    Discord,
    Twitter,
    Telegram,
    Facebook,
    Instagram,
}

impl ConnectorProvider {
    #[must_use]
    pub fn from_path(value: &str) -> Result<Self, AppError> {
        match value.to_ascii_lowercase().as_str() {
            "reddit" => Ok(Self::Reddit),
            "discord" => Ok(Self::Discord),
            "twitter" | "x" => Ok(Self::Twitter),
            "telegram" => Ok(Self::Telegram),
            "facebook" => Ok(Self::Facebook),
            "instagram" => Ok(Self::Instagram),
            _ => Err(AppError::Validation(format!(
                "unsupported connector provider {value}"
            ))),
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reddit => "reddit",
            Self::Discord => "discord",
            Self::Twitter => "twitter",
            Self::Telegram => "telegram",
            Self::Facebook => "facebook",
            Self::Instagram => "instagram",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorProfile {
    pub provider: ConnectorProvider,
    pub display_name: String,
    pub official_api_available: bool,
    pub public_web_fallback: bool,
    pub configured: bool,
    pub required_scopes: Vec<String>,
    pub rate_limit_per_minute: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorAccount {
    pub account_id: String,
    pub principal_id: String,
    pub provider: ConnectorProvider,
    pub display_name: String,
    pub scopes: Vec<String>,
    pub token_fingerprint: Option<String>,
    pub connected: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectConnectorRequest {
    pub display_name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub access_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorResearchRequest {
    pub query: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub allow_public_web_fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorSource {
    pub source_id: String,
    pub provider: ConnectorProvider,
    pub url: String,
    pub title: String,
    pub acquisition_mode: String,
    pub source_score: f64,
    pub policy_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorResearchJob {
    pub job_id: String,
    pub provider: ConnectorProvider,
    pub principal_id: String,
    pub query: String,
    pub status: String,
    pub sources: Vec<ConnectorSource>,
    pub warnings: Vec<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: i64,
}

#[derive(Default)]
struct ConnectorStore {
    accounts: HashMap<String, ConnectorAccount>,
    jobs: HashMap<String, ConnectorResearchJob>,
}

#[derive(Clone)]
pub struct ConnectorService {
    store: Shared<ConnectorStore>,
    configured: HashMap<ConnectorProvider, bool>,
}

impl ConnectorService {
    #[must_use]
    pub fn new() -> Self {
        let mut configured = HashMap::new();
        for provider in [
            ConnectorProvider::Reddit,
            ConnectorProvider::Discord,
            ConnectorProvider::Twitter,
            ConnectorProvider::Telegram,
            ConnectorProvider::Facebook,
            ConnectorProvider::Instagram,
        ] {
            let env_key = format!("URAI_{}_CLIENT_ID", provider.as_str().to_ascii_uppercase());
            configured.insert(provider, std::env::var(env_key).is_ok());
        }
        Self {
            store: Shared::new(RwLock::new(ConnectorStore::default())),
            configured,
        }
    }

    #[must_use]
    pub fn profiles(&self) -> Vec<ConnectorProfile> {
        [
            ConnectorProvider::Reddit,
            ConnectorProvider::Discord,
            ConnectorProvider::Twitter,
            ConnectorProvider::Telegram,
            ConnectorProvider::Facebook,
            ConnectorProvider::Instagram,
        ]
        .into_iter()
        .map(|provider| ConnectorProfile {
            provider,
            display_name: provider_display_name(provider).into(),
            official_api_available: true,
            public_web_fallback: true,
            configured: self.configured.get(&provider).copied().unwrap_or(false),
            required_scopes: provider_scopes(provider),
            rate_limit_per_minute: provider_rate_limit(provider),
        })
        .collect()
    }

    pub fn connect(
        &self,
        principal_id: &str,
        provider: ConnectorProvider,
        request: ConnectConnectorRequest,
    ) -> ConnectorAccount {
        let now = now_ms();
        let token_fingerprint = request
            .access_token
            .as_deref()
            .map(|token| sha3_hex(token.as_bytes())[..16].to_string());
        let account = ConnectorAccount {
            account_id: new_id("connector_acct"),
            principal_id: principal_id.into(),
            provider,
            display_name: request
                .display_name
                .unwrap_or_else(|| provider_display_name(provider).into()),
            scopes: if request.scopes.is_empty() {
                provider_scopes(provider)
            } else {
                request.scopes
            },
            token_fingerprint,
            connected: true,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.store
            .write()
            .accounts
            .insert(account.account_id.clone(), account.clone());
        account
    }

    pub fn disconnect(
        &self,
        principal_id: &str,
        provider: ConnectorProvider,
    ) -> Result<ConnectorAccount, AppError> {
        let mut store = self.store.write();
        let account = store
            .accounts
            .values_mut()
            .find(|account| account.principal_id == principal_id && account.provider == provider)
            .ok_or_else(|| AppError::NotFound(format!("connector {}", provider.as_str())))?;
        account.connected = false;
        account.updated_at_ms = now_ms();
        Ok(account.clone())
    }

    #[must_use]
    pub fn accounts_for(&self, principal_id: &str) -> Vec<ConnectorAccount> {
        self.store
            .read()
            .accounts
            .values()
            .filter(|account| account.principal_id == principal_id)
            .cloned()
            .collect()
    }

    pub fn research(
        &self,
        principal_id: &str,
        provider: ConnectorProvider,
        request: ConnectorResearchRequest,
    ) -> Result<ConnectorResearchJob, AppError> {
        if request.query.trim().is_empty() {
            return Err(AppError::Validation(
                "connector research query must not be empty".into(),
            ));
        }
        let account_connected = self
            .accounts_for(principal_id)
            .iter()
            .any(|account| account.provider == provider && account.connected);
        let configured = self.configured.get(&provider).copied().unwrap_or(false);
        let mut warnings = Vec::new();
        let acquisition_mode = if configured && account_connected {
            "official_api"
        } else if request.allow_public_web_fallback {
            warnings.push(
                "official connector credentials unavailable; public HTTPA fallback used".into(),
            );
            "public_httpa_fallback"
        } else {
            return Err(AppError::Forbidden(format!(
                "{} is not connected and public fallback was not allowed",
                provider.as_str()
            )));
        };
        let sources = if request.urls.is_empty() {
            vec![ConnectorSource {
                source_id: new_id("connector_source"),
                provider,
                url: format!(
                    "https://{}.com/search?q={}",
                    provider.as_str(),
                    request.query
                ),
                title: format!("{} public search seed", provider_display_name(provider)),
                acquisition_mode: acquisition_mode.into(),
                source_score: if acquisition_mode == "official_api" {
                    0.82
                } else {
                    0.62
                },
                policy_warnings: warnings.clone(),
            }]
        } else {
            request
                .urls
                .iter()
                .map(|url| ConnectorSource {
                    source_id: new_id("connector_source"),
                    provider,
                    url: url.clone(),
                    title: format!("{} source", provider_display_name(provider)),
                    acquisition_mode: acquisition_mode.into(),
                    source_score: if acquisition_mode == "official_api" {
                        0.86
                    } else {
                        0.66
                    },
                    policy_warnings: warnings.clone(),
                })
                .collect()
        };
        let now = now_ms();
        let job = ConnectorResearchJob {
            job_id: new_id("connector_job"),
            provider,
            principal_id: principal_id.into(),
            query: request.query,
            status: "completed".into(),
            sources,
            warnings,
            created_at_ms: now,
            completed_at_ms: now_ms(),
        };
        self.store
            .write()
            .jobs
            .insert(job.job_id.clone(), job.clone());
        Ok(job)
    }

    pub fn get_job(&self, job_id: &str) -> Result<ConnectorResearchJob, AppError> {
        self.store
            .read()
            .jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("connector research job {job_id}")))
    }
}

impl Default for ConnectorService {
    fn default() -> Self {
        Self::new()
    }
}

fn provider_display_name(provider: ConnectorProvider) -> &'static str {
    match provider {
        ConnectorProvider::Reddit => "Reddit",
        ConnectorProvider::Discord => "Discord",
        ConnectorProvider::Twitter => "Twitter/X",
        ConnectorProvider::Telegram => "Telegram",
        ConnectorProvider::Facebook => "Facebook",
        ConnectorProvider::Instagram => "Instagram",
    }
}

fn provider_scopes(provider: ConnectorProvider) -> Vec<String> {
    match provider {
        ConnectorProvider::Reddit => vec!["read".into(), "history".into()],
        ConnectorProvider::Discord => {
            vec!["identify".into(), "guilds".into(), "messages.read".into()]
        }
        ConnectorProvider::Twitter => vec!["tweet.read".into(), "users.read".into()],
        ConnectorProvider::Telegram => vec!["messages.read".into()],
        ConnectorProvider::Facebook => {
            vec!["public_profile".into(), "pages_read_engagement".into()]
        }
        ConnectorProvider::Instagram => vec!["instagram_basic".into()],
    }
}

fn provider_rate_limit(provider: ConnectorProvider) -> u32 {
    match provider {
        ConnectorProvider::Discord | ConnectorProvider::Telegram => 60,
        ConnectorProvider::Twitter => 30,
        _ => 45,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_fallback_research_returns_policy_warning() {
        let service = ConnectorService::new();
        let job = service
            .research(
                "principal_test",
                ConnectorProvider::Reddit,
                ConnectorResearchRequest {
                    query: "httpa".into(),
                    urls: vec!["https://www.reddit.com/r/rust".into()],
                    allow_public_web_fallback: true,
                },
            )
            .expect("fallback should work");

        assert_eq!(job.status, "completed");
        assert!(!job.warnings.is_empty());
        assert_eq!(job.sources[0].acquisition_mode, "public_httpa_fallback");
    }
}

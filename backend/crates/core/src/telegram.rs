//! Telegram bot front-end for Astra.
//!
//! Users talk to the bot, register their own LLM API key (BYOK — bring your
//! own key) for Gemini, OpenRouter, or DeepSeek, and every objective they send
//! is executed as an ASC-II mission on the device running this server: the
//! agent swarm spawns locally, guarded by the sandbox reference monitor and
//! recorded in the chronicle, while only the reasoning calls go to the
//! provider the user configured.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::asc2::{Asc2MissionRequest, RemoteReasoningExecutor};
use crate::chronicle::{EpisodeKind, RecordEpisodeRequest};
use crate::common::{AppError, TenantScope, now_ms, sha3_hex};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";
/// Telegram caps messages at 4096 chars; leave room for the footer.
const REPLY_TEXT_LIMIT: usize = 3600;
/// How many crawled pages are folded into the LLM prompt as live evidence.
const MAX_EVIDENCE_PAGES: usize = 4;
/// Per-page excerpt budget in the evidence block, in characters.
const EVIDENCE_EXCERPT_CHARS: usize = 400;

// ─────────────────────────────────────────────────────────────
// Providers
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotProvider {
    Gemini,
    OpenRouter,
    DeepSeek,
}

impl BotProvider {
    pub const ALL: [Self; 3] = [Self::Gemini, Self::OpenRouter, Self::DeepSeek];

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gemini" | "google" => Some(Self::Gemini),
            "openrouter" | "open_router" | "or" => Some(Self::OpenRouter),
            "deepseek" | "deep_seek" | "ds" => Some(Self::DeepSeek),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::OpenRouter => "openrouter",
            Self::DeepSeek => "deepseek",
        }
    }

    /// OpenAI-compatible base endpoint; ASC-II appends `/chat/completions`.
    #[must_use]
    pub fn endpoint(self) -> &'static str {
        match self {
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::DeepSeek => "https://api.deepseek.com",
        }
    }

    #[must_use]
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Gemini => "gemini-2.5-flash",
            Self::OpenRouter => "openrouter/auto",
            Self::DeepSeek => "deepseek-chat",
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub bot_token: String,
    /// Empty means every chat may use the bot (each with their own key).
    pub allowed_chat_ids: Vec<i64>,
    pub poll_timeout_secs: u64,
}

impl TelegramConfig {
    /// Reads `ASTRA_TELEGRAM_BOT_TOKEN` (required to enable the bot) and
    /// `ASTRA_TELEGRAM_ALLOWED_CHAT_IDS` (optional comma-separated allowlist).
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let bot_token = std::env::var("ASTRA_TELEGRAM_BOT_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())?;
        let allowed_chat_ids = std::env::var("ASTRA_TELEGRAM_ALLOWED_CHAT_IDS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .filter_map(|item| item.trim().parse::<i64>().ok())
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            bot_token: bot_token.trim().to_string(),
            allowed_chat_ids,
            poll_timeout_secs: 50,
        })
    }
}

// ─────────────────────────────────────────────────────────────
// Per-chat key store
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProviderKey {
    pub provider: BotProvider,
    pub api_key: String,
    pub model: String,
    pub updated_at_ms: i64,
}

#[derive(Clone)]
pub struct TelegramKeyStore {
    store: Arc<Mutex<Connection>>,
}

impl TelegramKeyStore {
    pub fn new(data_dir: impl AsRef<std::path::Path>) -> Result<Self, AppError> {
        let store_path = data_dir.as_ref().join("telegram.sqlite");
        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::Internal(format!("failed to create telegram data directory: {error}"))
            })?;
        }
        let connection = Connection::open(&store_path).map_err(|error| {
            AppError::Internal(format!("failed to open telegram store: {error}"))
        })?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS bot_keys (
                    chat_id INTEGER NOT NULL,
                    provider TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    model TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY (chat_id, provider)
                );
                CREATE TABLE IF NOT EXISTS bot_settings (
                    chat_id INTEGER PRIMARY KEY,
                    active_provider TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );",
            )
            .map_err(|error| {
                AppError::Internal(format!("failed to initialize telegram store: {error}"))
            })?;
        Ok(Self {
            store: Arc::new(Mutex::new(connection)),
        })
    }

    pub fn set_key(
        &self,
        chat_id: i64,
        provider: BotProvider,
        api_key: &str,
        model: &str,
    ) -> Result<(), AppError> {
        let connection = self.store.lock();
        connection
            .execute(
                "INSERT INTO bot_keys (chat_id, provider, api_key, model, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (chat_id, provider)
                 DO UPDATE SET api_key = ?3, model = ?4, updated_at_ms = ?5",
                params![chat_id, provider.as_str(), api_key, model, now_ms()],
            )
            .map_err(|error| AppError::Internal(format!("failed to save key: {error}")))?;
        connection
            .execute(
                "INSERT INTO bot_settings (chat_id, active_provider, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (chat_id)
                 DO UPDATE SET active_provider = ?2, updated_at_ms = ?3",
                params![chat_id, provider.as_str(), now_ms()],
            )
            .map_err(|error| AppError::Internal(format!("failed to set provider: {error}")))?;
        Ok(())
    }

    pub fn get_key(
        &self,
        chat_id: i64,
        provider: BotProvider,
    ) -> Result<Option<StoredProviderKey>, AppError> {
        let connection = self.store.lock();
        let mut statement = connection
            .prepare(
                "SELECT api_key, model, updated_at_ms FROM bot_keys
                 WHERE chat_id = ?1 AND provider = ?2",
            )
            .map_err(|error| AppError::Internal(format!("failed to read key: {error}")))?;
        let row = statement
            .query_row(params![chat_id, provider.as_str()], |row| {
                Ok(StoredProviderKey {
                    provider,
                    api_key: row.get(0)?,
                    model: row.get(1)?,
                    updated_at_ms: row.get(2)?,
                })
            })
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(AppError::Internal(format!("failed to read key: {other}"))),
            })?;
        Ok(row)
    }

    pub fn set_active_provider(&self, chat_id: i64, provider: BotProvider) -> Result<(), AppError> {
        self.store
            .lock()
            .execute(
                "INSERT INTO bot_settings (chat_id, active_provider, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (chat_id)
                 DO UPDATE SET active_provider = ?2, updated_at_ms = ?3",
                params![chat_id, provider.as_str(), now_ms()],
            )
            .map_err(|error| AppError::Internal(format!("failed to set provider: {error}")))?;
        Ok(())
    }

    pub fn active_provider(&self, chat_id: i64) -> Result<Option<BotProvider>, AppError> {
        let connection = self.store.lock();
        let mut statement = connection
            .prepare("SELECT active_provider FROM bot_settings WHERE chat_id = ?1")
            .map_err(|error| AppError::Internal(format!("failed to read settings: {error}")))?;
        let provider = statement
            .query_row(params![chat_id], |row| row.get::<_, String>(0))
            .map(|value| BotProvider::parse(&value))
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(AppError::Internal(format!(
                    "failed to read settings: {other}"
                ))),
            })?;
        Ok(provider)
    }

    /// Returns the credentials for the chat's active provider, if any.
    pub fn active_key(&self, chat_id: i64) -> Result<Option<StoredProviderKey>, AppError> {
        match self.active_provider(chat_id)? {
            Some(provider) => self.get_key(chat_id, provider),
            None => Ok(None),
        }
    }

    pub fn set_model(
        &self,
        chat_id: i64,
        provider: BotProvider,
        model: &str,
    ) -> Result<bool, AppError> {
        let updated = self
            .store
            .lock()
            .execute(
                "UPDATE bot_keys SET model = ?3, updated_at_ms = ?4
                 WHERE chat_id = ?1 AND provider = ?2",
                params![chat_id, provider.as_str(), model, now_ms()],
            )
            .map_err(|error| AppError::Internal(format!("failed to set model: {error}")))?;
        Ok(updated > 0)
    }

    pub fn delete_key(&self, chat_id: i64, provider: BotProvider) -> Result<bool, AppError> {
        let deleted = self
            .store
            .lock()
            .execute(
                "DELETE FROM bot_keys WHERE chat_id = ?1 AND provider = ?2",
                params![chat_id, provider.as_str()],
            )
            .map_err(|error| AppError::Internal(format!("failed to delete key: {error}")))?;
        Ok(deleted > 0)
    }

    pub fn delete_all_keys(&self, chat_id: i64) -> Result<usize, AppError> {
        let connection = self.store.lock();
        let deleted = connection
            .execute("DELETE FROM bot_keys WHERE chat_id = ?1", params![chat_id])
            .map_err(|error| AppError::Internal(format!("failed to delete keys: {error}")))?;
        connection
            .execute(
                "DELETE FROM bot_settings WHERE chat_id = ?1",
                params![chat_id],
            )
            .map_err(|error| AppError::Internal(format!("failed to delete settings: {error}")))?;
        Ok(deleted)
    }

    pub fn keys_for_chat(&self, chat_id: i64) -> Result<Vec<StoredProviderKey>, AppError> {
        let connection = self.store.lock();
        let mut statement = connection
            .prepare(
                "SELECT provider, api_key, model, updated_at_ms FROM bot_keys
                 WHERE chat_id = ?1 ORDER BY provider",
            )
            .map_err(|error| AppError::Internal(format!("failed to list keys: {error}")))?;
        let rows = statement
            .query_map(params![chat_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|error| AppError::Internal(format!("failed to list keys: {error}")))?
            .filter_map(Result::ok)
            .filter_map(|(provider, api_key, model, updated_at_ms)| {
                BotProvider::parse(&provider).map(|provider| StoredProviderKey {
                    provider,
                    api_key,
                    model,
                    updated_at_ms,
                })
            })
            .collect();
        Ok(rows)
    }
}

/// Short non-reversible display form so the key never appears in replies.
#[must_use]
pub fn mask_key(api_key: &str) -> String {
    let fingerprint = sha3_hex(api_key.as_bytes());
    format!("…{} (sha3 {})", last_chars(api_key, 4), &fingerprint[..8])
}

fn last_chars(value: &str, count: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= count {
        "•".repeat(chars.len())
    } else {
        chars[chars.len() - count..].iter().collect()
    }
}

// ─────────────────────────────────────────────────────────────
// Command parsing
// ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotCommand {
    Start,
    Help,
    SetKey {
        provider: BotProvider,
        api_key: String,
        model: Option<String>,
    },
    SetProvider(BotProvider),
    SetModel(String),
    DeleteKey(Option<BotProvider>),
    Status,
    Mission(String),
    Invalid(String),
}

#[must_use]
pub fn parse_command(text: &str) -> BotCommand {
    let text = text.trim();
    if !text.starts_with('/') {
        return BotCommand::Mission(text.to_string());
    }
    let mut parts = text.split_whitespace();
    // Telegram appends "@BotName" to commands in groups.
    let command = parts
        .next()
        .unwrap_or_default()
        .split('@')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match command.as_str() {
        "/start" => BotCommand::Start,
        "/help" => BotCommand::Help,
        "/setkey" => {
            let provider = parts.next().and_then(BotProvider::parse);
            let api_key = parts.next().map(str::to_string);
            let model = parts.next().map(str::to_string);
            match (provider, api_key) {
                (Some(provider), Some(api_key)) if !api_key.is_empty() => BotCommand::SetKey {
                    provider,
                    api_key,
                    model,
                },
                _ => BotCommand::Invalid(
                    "Usage: /setkey <gemini|openrouter|deepseek> <api_key> [model]".into(),
                ),
            }
        }
        "/provider" => match parts.next().and_then(BotProvider::parse) {
            Some(provider) => BotCommand::SetProvider(provider),
            None => BotCommand::Invalid("Usage: /provider <gemini|openrouter|deepseek>".into()),
        },
        "/model" => match parts.next() {
            Some(model) => BotCommand::SetModel(model.to_string()),
            None => BotCommand::Invalid("Usage: /model <model-name>".into()),
        },
        "/delkey" => match parts.next() {
            None | Some("all") => BotCommand::DeleteKey(None),
            Some(raw) => match BotProvider::parse(raw) {
                Some(provider) => BotCommand::DeleteKey(Some(provider)),
                None => {
                    BotCommand::Invalid("Usage: /delkey [gemini|openrouter|deepseek|all]".into())
                }
            },
        },
        "/status" => BotCommand::Status,
        other => BotCommand::Invalid(format!("Unknown command {other}. Try /help.")),
    }
}

const HELP_TEXT: &str = "Astra — personal AI agents that run on this device.\n\n\
1. Save your own LLM API key (kept only in the local store of the device running Astra):\n\
   /setkey gemini <key>\n\
   /setkey openrouter <key> [model]\n\
   /setkey deepseek <key> [model]\n\
2. Send any question as a plain message — the agent searches and crawls the web (via your SearXNG instance), then your provider's model answers grounded in the real sources it found. Sources are listed under each answer. Without SearXNG it falls back to plain reasoning.\n\n\
Commands:\n\
/setkey <provider> <key> [model] — save a key (then delete your message!)\n\
/provider <name> — switch the active provider\n\
/model <name> — change the model of the active provider\n\
/status — show configured providers (keys are never echoed)\n\
/delkey [provider|all] — remove stored keys\n\
/help — this message";

// ─────────────────────────────────────────────────────────────
// Bot service
// ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TelegramBot {
    config: TelegramConfig,
    keys: TelegramKeyStore,
    state: AppState,
    client: reqwest::Client,
}

impl TelegramBot {
    pub fn new(state: AppState, config: TelegramConfig) -> Result<Self, AppError> {
        let keys = TelegramKeyStore::new(state.config.data_path("telegram"))?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.poll_timeout_secs + 30))
            .build()
            .map_err(|error| AppError::Internal(format!("telegram client failed: {error}")))?;
        Ok(Self {
            config,
            keys,
            state,
            client,
        })
    }

    fn api_url(&self, method: &str) -> String {
        format!("{TELEGRAM_API_BASE}/bot{}/{method}", self.config.bot_token)
    }

    /// Long-poll loop; never returns under normal operation.
    pub async fn run(self) {
        tracing::info!("telegram bot worker started (long polling)");
        let mut offset: i64 = 0;
        loop {
            match self.fetch_updates(offset).await {
                Ok(updates) => {
                    for update in updates {
                        if let Some(update_id) = update.get("update_id").and_then(|v| v.as_i64()) {
                            offset = offset.max(update_id + 1);
                        }
                        self.handle_update(&update).await;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "telegram getUpdates failed; backing off");
                    actix_web::rt::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn fetch_updates(&self, offset: i64) -> Result<Vec<serde_json::Value>, AppError> {
        let response = self
            .client
            .post(self.api_url("getUpdates"))
            .json(&serde_json::json!({
                "offset": offset,
                "timeout": self.config.poll_timeout_secs,
                "allowed_updates": ["message"],
            }))
            .send()
            .await
            .map_err(|error| AppError::Internal(format!("getUpdates request failed: {error}")))?;
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|error| AppError::Internal(format!("getUpdates decode failed: {error}")))?;
        if !value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(AppError::Internal(format!(
                "telegram getUpdates returned error: {}",
                value
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            )));
        }
        Ok(value
            .get("result")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default())
    }

    async fn send_message(&self, chat_id: i64, text: &str) {
        let result = self
            .client
            .post(self.api_url("sendMessage"))
            .json(&serde_json::json!({ "chat_id": chat_id, "text": text }))
            .send()
            .await;
        match result {
            Ok(response) if !response.status().is_success() => {
                tracing::warn!(status = %response.status(), "telegram sendMessage rejected");
            }
            Err(error) => tracing::warn!(%error, "telegram sendMessage failed"),
            _ => {}
        }
    }

    async fn handle_update(&self, update: &serde_json::Value) {
        let Some(message) = update.get("message") else {
            return;
        };
        let Some(chat_id) = message.pointer("/chat/id").and_then(|v| v.as_i64()) else {
            return;
        };
        let chat_type = message
            .pointer("/chat/type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let Some(text) = message.get("text").and_then(|v| v.as_str()) else {
            return;
        };
        if !self.config.allowed_chat_ids.is_empty()
            && !self.config.allowed_chat_ids.contains(&chat_id)
        {
            self.send_message(chat_id, "This Astra instance is private.")
                .await;
            return;
        }
        if chat_type != "private" {
            self.send_message(
                chat_id,
                "Please message me privately — API keys and missions are per-user.",
            )
            .await;
            return;
        }
        let reply = self.dispatch(chat_id, text).await;
        self.send_message(chat_id, &reply).await;
    }

    async fn dispatch(&self, chat_id: i64, text: &str) -> String {
        match parse_command(text) {
            BotCommand::Start | BotCommand::Help => HELP_TEXT.to_string(),
            BotCommand::Invalid(message) => message,
            BotCommand::SetKey {
                provider,
                api_key,
                model,
            } => self.handle_set_key(chat_id, provider, api_key, model).await,
            BotCommand::SetProvider(provider) => self.handle_set_provider(chat_id, provider),
            BotCommand::SetModel(model) => self.handle_set_model(chat_id, &model),
            BotCommand::DeleteKey(provider) => self.handle_delete_key(chat_id, provider),
            BotCommand::Status => self.handle_status(chat_id),
            BotCommand::Mission(objective) => self.handle_mission(chat_id, objective).await,
        }
    }

    async fn handle_set_key(
        &self,
        chat_id: i64,
        provider: BotProvider,
        api_key: String,
        model: Option<String>,
    ) -> String {
        let model = model.unwrap_or_else(|| provider.default_model().to_string());
        if let Err(reason) = self.validate_provider_key(provider, &api_key, &model).await {
            return format!(
                "Key check against {} failed: {reason}\nNothing was saved.",
                provider.as_str()
            );
        }
        match self.keys.set_key(chat_id, provider, &api_key, &model) {
            Ok(()) => format!(
                "Saved and verified {} key {} with model {model}. It is now your active provider.\n\n\
                 The key is stored only on the device running Astra. \
                 Delete your message above so it doesn't stay in this chat history.",
                provider.as_str(),
                mask_key(&api_key),
            ),
            Err(error) => {
                tracing::warn!(%error, "telegram key save failed");
                "Could not save the key (local store error). Try again.".into()
            }
        }
    }

    /// One tiny chat completion against the provider to prove the key works
    /// before it is stored.
    async fn validate_provider_key(
        &self,
        provider: BotProvider,
        api_key: &str,
        model: &str,
    ) -> Result<(), String> {
        let response = self
            .client
            .post(format!("{}/chat/completions", provider.endpoint()))
            .bearer_auth(api_key)
            .json(&serde_json::json!({
                "model": model,
                "messages": [{ "role": "user", "content": "Reply with exactly: OK" }],
            }))
            .send()
            .await
            .map_err(|error| format!("request failed: {error}"))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let detail = body
            .pointer("/error/message")
            .or_else(|| body.pointer("/0/error/message"))
            .and_then(|v| v.as_str())
            .unwrap_or("no detail from provider");
        Err(format!("HTTP {status} — {detail}"))
    }

    fn handle_set_provider(&self, chat_id: i64, provider: BotProvider) -> String {
        match self.keys.get_key(chat_id, provider) {
            Ok(Some(stored)) => match self.keys.set_active_provider(chat_id, provider) {
                Ok(()) => format!(
                    "Active provider is now {} (model {}).",
                    provider.as_str(),
                    stored.model
                ),
                Err(_) => "Could not switch provider (local store error).".into(),
            },
            Ok(None) => format!(
                "No {} key saved yet. Add one with /setkey {} <api_key>.",
                provider.as_str(),
                provider.as_str()
            ),
            Err(_) => "Could not read the local key store.".into(),
        }
    }

    fn handle_set_model(&self, chat_id: i64, model: &str) -> String {
        match self.keys.active_provider(chat_id) {
            Ok(Some(provider)) => match self.keys.set_model(chat_id, provider, model) {
                Ok(true) => format!("Model for {} set to {model}.", provider.as_str()),
                Ok(false) => format!(
                    "No {} key saved yet. Add one with /setkey {} <api_key> {model}.",
                    provider.as_str(),
                    provider.as_str()
                ),
                Err(_) => "Could not update the model (local store error).".into(),
            },
            Ok(None) => "No active provider. Save a key first with /setkey.".into(),
            Err(_) => "Could not read the local key store.".into(),
        }
    }

    fn handle_delete_key(&self, chat_id: i64, provider: Option<BotProvider>) -> String {
        match provider {
            Some(provider) => match self.keys.delete_key(chat_id, provider) {
                Ok(true) => format!("Deleted your {} key.", provider.as_str()),
                Ok(false) => format!("No {} key was stored.", provider.as_str()),
                Err(_) => "Could not delete the key (local store error).".into(),
            },
            None => match self.keys.delete_all_keys(chat_id) {
                Ok(count) => format!("Deleted {count} stored key(s) and your settings."),
                Err(_) => "Could not delete keys (local store error).".into(),
            },
        }
    }

    fn handle_status(&self, chat_id: i64) -> String {
        let keys = match self.keys.keys_for_chat(chat_id) {
            Ok(keys) => keys,
            Err(_) => return "Could not read the local key store.".into(),
        };
        if keys.is_empty() {
            return "No providers configured. Start with /setkey gemini <api_key> — see /help."
                .into();
        }
        let active = self.keys.active_provider(chat_id).ok().flatten();
        let mut lines = vec!["Your providers on this device:".to_string()];
        for stored in keys {
            lines.push(format!(
                "{} {} — model {}, key {}",
                if active == Some(stored.provider) {
                    "▶"
                } else {
                    "•"
                },
                stored.provider.as_str(),
                stored.model,
                mask_key(&stored.api_key),
            ));
        }
        lines.push(String::new());
        lines.push(
            "Missions execute on the device running Astra; only reasoning calls go to your provider."
                .into(),
        );
        lines.join("\n")
    }

    /// Runs the governed discovery+crawl pipeline (SearXNG → safe_fetch →
    /// semantic render → rank) for a query and returns an evidence block to
    /// fold into the LLM prompt plus the source URLs. Returns `None` when the
    /// web layer is unavailable (e.g. no SearXNG instance configured) so the
    /// caller falls back to a plain mission. Runs on the blocking pool because
    /// the crawl does synchronous network I/O.
    async fn gather_web_evidence(&self, query: &str) -> Option<(String, Vec<String>)> {
        let device = self.state.device.clone();
        let query_owned = query.to_string();
        let outcome = actix_web::web::block(move || {
            device.execute(
                "deep_crawl",
                &serde_json::json!({
                    "query": query_owned,
                    "max_pages": MAX_EVIDENCE_PAGES,
                    "max_depth": 0,
                }),
            )
        })
        .await;
        let value = match outcome {
            Ok(Ok(value)) => value,
            Ok(Err(reason)) => {
                tracing::info!(%reason, "telegram web evidence unavailable; falling back");
                return None;
            }
            Err(error) => {
                tracing::warn!(%error, "telegram web evidence task join failed");
                return None;
            }
        };
        let pages = value.get("pages").and_then(serde_json::Value::as_array)?;
        if pages.is_empty() {
            return None;
        }
        let mut block = String::from("WEB EVIDENCE (gathered live; cite by [n]):\n");
        let mut sources = Vec::new();
        for (index, page) in pages.iter().take(MAX_EVIDENCE_PAGES).enumerate() {
            let url = page
                .get("url")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let title = page
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("(untitled)");
            let excerpt: String = page
                .get("text_excerpt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .chars()
                .take(EVIDENCE_EXCERPT_CHARS)
                .collect();
            block.push_str(&format!("\n[{}] {title} — {url}\n{excerpt}\n", index + 1));
            sources.push(format!("[{}] {url}", index + 1));
        }
        Some((block, sources))
    }

    async fn handle_mission(&self, chat_id: i64, objective: String) -> String {
        if objective.trim().is_empty() {
            return "Send an objective as text, or /help for commands.".into();
        }
        let Ok(Some(stored)) = self.keys.active_key(chat_id) else {
            return "No API key configured yet. Save one first, e.g. /setkey gemini <api_key> — see /help.".into();
        };
        let executor = Arc::new(RemoteReasoningExecutor::new(
            stored.provider.endpoint().to_string(),
            Some(stored.api_key.clone()),
            stored.model.clone(),
        ));

        // Agentic step: before reasoning, the agent searches and crawls the web
        // for this query, then hands the fresh evidence to the user's LLM so the
        // answer is grounded in real sources. Falls back to plain reasoning when
        // the web layer is unavailable.
        let (objective_for_llm, sources) = match self.gather_web_evidence(&objective).await {
            Some((evidence, sources)) => {
                let augmented = format!(
                    "{objective}\n\nAnswer the question above using the fresh web evidence below. \
                     Be specific and cite sources by their [n] marker.\n\n{evidence}"
                );
                (augmented, sources)
            }
            None => (objective.clone(), Vec::new()),
        };

        let request = Asc2MissionRequest {
            objective: objective_for_llm,
            activity_kind: "telegram".into(),
            requested_tools: Vec::new(),
            sensitive: false,
            owner_authorized: false,
            side_effecting: false,
            action_token: None,
        };
        // Same perimeter as the HTTP route: reference-monitor guard plus
        // chronicle episode, so bot missions are indistinguishable from API
        // missions in the audit trail.
        let sandbox_action = self.state.sandbox.action_for_activity(
            request.activity_kind.clone(),
            objective.clone(),
            request.requested_tools.clone(),
            serde_json::json!({
                "side_effecting": false,
                "sensitive": false,
                "protocol": "httpa",
                "channel": "telegram",
                "chat_id": chat_id,
            }),
            request.owner_authorized,
        );
        let mission = match self
            .state
            .asc2
            .execute_mission_with_executor(request, Some(executor))
            .await
        {
            Ok(mission) => mission,
            Err(error) => return format!("Mission failed: {error}"),
        };
        if let Err(error) = self.state.sandbox.guard(sandbox_action, false) {
            tracing::warn!(%error, "telegram mission sandbox guard failed");
        }
        let record = self.state.chronicle.record(RecordEpisodeRequest {
            kind: EpisodeKind::Event,
            content: format!("telegram mission '{objective}' executed for chat {chat_id}"),
            rationale: None,
            source: Some("telegram".into()),
            source_ref: Some(mission.mission_id.clone()),
            tags: vec!["telegram".into(), "asc2".into()],
            importance: Some(0.5),
            due_at_ms: None,
            tenant_scope: TenantScope::Global,
        });
        if let Err(error) = record {
            tracing::warn!(%error, "telegram chronicle recording skipped");
        }

        let mut answer = mission.answer.trim().to_string();
        if answer.is_empty() {
            answer = "(the mission produced no textual answer)".into();
        }
        if answer.chars().count() > REPLY_TEXT_LIMIT {
            answer = answer.chars().take(REPLY_TEXT_LIMIT).collect::<String>() + "…";
        }
        // Show the agent's real sources so the user can verify the answer.
        if !sources.is_empty() {
            answer.push_str("\n\nSources:\n");
            answer.push_str(&sources.join("\n"));
        }
        let web_note = if sources.is_empty() {
            "no live web sources (set up SearXNG to enable)".to_string()
        } else {
            format!("{} live web sources", sources.len())
        };
        let footer = format!(
            "\n\n— {} agents spawned locally · {} {} · {} · {}",
            mission.diagnostics.agents.len(),
            stored.provider.as_str(),
            stored.model,
            if mission.diagnostics.remote_used {
                "remote reasoning used"
            } else {
                "answered by the local core"
            },
            web_note,
        );
        format!("{answer}{footer}")
    }
}

/// Spawns the bot on a dedicated thread with its own async runtime, mirroring
/// the autonomy worker pattern, so polling never competes with HTTP workers.
pub fn spawn_telegram_worker(state: AppState) {
    let Some(config) = TelegramConfig::from_env() else {
        tracing::info!("telegram bot disabled (ASTRA_TELEGRAM_BOT_TOKEN not set)");
        return;
    };
    std::thread::Builder::new()
        .name("astra-telegram-worker".into())
        .spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                match TelegramBot::new(state, config) {
                    Ok(bot) => bot.run().await,
                    Err(error) => tracing::error!(%error, "telegram bot failed to start"),
                }
            });
        })
        .expect("failed to spawn telegram worker thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_providers_with_aliases() {
        assert_eq!(BotProvider::parse("Gemini"), Some(BotProvider::Gemini));
        assert_eq!(
            BotProvider::parse("OPENROUTER"),
            Some(BotProvider::OpenRouter)
        );
        assert_eq!(BotProvider::parse("ds"), Some(BotProvider::DeepSeek));
        assert_eq!(BotProvider::parse("mystery"), None);
    }

    #[test]
    fn parses_setkey_command() {
        let parsed = parse_command("/setkey gemini abc123 gemini-2.5-pro");
        assert_eq!(
            parsed,
            BotCommand::SetKey {
                provider: BotProvider::Gemini,
                api_key: "abc123".into(),
                model: Some("gemini-2.5-pro".into()),
            }
        );
        assert!(matches!(
            parse_command("/setkey gemini"),
            BotCommand::Invalid(_)
        ));
    }

    #[test]
    fn group_suffix_is_stripped_and_text_is_mission() {
        assert_eq!(parse_command("/help@AstraBot"), BotCommand::Help);
        assert_eq!(
            parse_command("explain entropy"),
            BotCommand::Mission("explain entropy".into())
        );
    }

    #[test]
    fn key_store_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "astra-telegram-test-{}",
            crate::common::new_id("t")
        ));
        let store = TelegramKeyStore::new(&dir).expect("store");
        store
            .set_key(7, BotProvider::Gemini, "secret-key", "gemini-2.5-flash")
            .expect("set");
        let stored = store
            .get_key(7, BotProvider::Gemini)
            .expect("get")
            .expect("some");
        assert_eq!(stored.api_key, "secret-key");
        assert_eq!(
            store.active_provider(7).expect("active"),
            Some(BotProvider::Gemini)
        );

        store
            .set_key(7, BotProvider::DeepSeek, "other-key", "deepseek-chat")
            .expect("set second");
        assert_eq!(
            store.active_provider(7).expect("active"),
            Some(BotProvider::DeepSeek)
        );
        store
            .set_active_provider(7, BotProvider::Gemini)
            .expect("switch");
        let active = store.active_key(7).expect("active key").expect("some");
        assert_eq!(active.provider, BotProvider::Gemini);

        assert!(
            store
                .set_model(7, BotProvider::Gemini, "gemini-2.5-pro")
                .expect("model")
        );
        assert!(store.delete_key(7, BotProvider::Gemini).expect("delete"));
        assert!(
            store
                .get_key(7, BotProvider::Gemini)
                .expect("get")
                .is_none()
        );
        assert_eq!(store.delete_all_keys(7).expect("delete all"), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn masked_key_never_contains_the_secret() {
        let masked = mask_key("super-secret-api-key-12345");
        assert!(!masked.contains("super-secret"));
        assert!(masked.contains("2345"));
    }
}

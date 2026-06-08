use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use log::info;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::protocol::GovernanceClearance;
use crate::error::{AstraError, AstraResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpaSession {
    pub session_id: String,
    pub agent_id: String,
    pub token: String,
    pub clearance: GovernanceClearance,
    pub capabilities: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub is_active: bool,
    pub request_count: u64,
    pub last_activity: DateTime<Utc>,
}

impl HttpaSession {
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn is_alive(&self) -> bool {
        self.is_active && !self.is_expired()
    }

    pub fn touch(&mut self, ttl_seconds: u64) {
        let now = Utc::now();
        self.last_activity = now;
        self.expires_at = now + Duration::seconds(ttl_seconds as i64);
        self.request_count += 1;
    }
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<DashMap<String, HttpaSession>>,
    ttl_seconds: u64,
    max_sessions: usize,
}

impl SessionManager {
    pub fn new(ttl_seconds: u64, max_sessions: usize) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            ttl_seconds,
            max_sessions: max_sessions.max(1),
        }
    }

    pub fn create_session(
        &self,
        agent_id: &str,
        capabilities: Vec<String>,
        clearance: GovernanceClearance,
    ) -> AstraResult<HttpaSession> {
        if self.sessions.len() >= self.max_sessions {
            self.evict_expired();
            if self.sessions.len() >= self.max_sessions {
                return Err(AstraError::HandshakeFailed(
                    "Maximum sessions reached".into(),
                ));
            }
        }

        let session_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let session = HttpaSession {
            session_id: session_id.clone(),
            agent_id: agent_id.to_string(),
            token: generate_session_token(),
            clearance,
            capabilities,
            created_at: now,
            expires_at: now + Duration::seconds(self.ttl_seconds as i64),
            is_active: true,
            request_count: 0,
            last_activity: now,
        };

        self.sessions.insert(session_id.clone(), session.clone());
        info!(
            "HTTPA session created: {} for agent '{}'",
            session_id, agent_id
        );
        Ok(session)
    }

    pub fn get_session(&self, session_id: &str) -> AstraResult<HttpaSession> {
        let entry = self
            .sessions
            .get(session_id)
            .ok_or_else(|| AstraError::SessionNotFound(session_id.to_string()))?;

        let session = entry.value().clone();
        if !session.is_alive() {
            drop(entry);
            self.sessions.remove(session_id);
            return Err(AstraError::SessionExpired(session_id.to_string()));
        }

        Ok(session)
    }

    pub fn touch_session(&self, session_id: &str) -> AstraResult<()> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AstraError::SessionNotFound(session_id.to_string()))?;

        if !entry.is_alive() {
            return Err(AstraError::SessionExpired(session_id.to_string()));
        }

        entry.touch(self.ttl_seconds);
        Ok(())
    }

    pub fn verify_session_token(&self, session_id: &str, token: &str) -> AstraResult<HttpaSession> {
        let session = self.get_session(session_id)?;
        if !constant_time_eq(session.token.as_bytes(), token.as_bytes()) {
            return Err(AstraError::AuthRequired);
        }
        Ok(session)
    }

    pub fn terminate_session(&self, session_id: &str) -> AstraResult<()> {
        if let Some((_, mut session)) = self.sessions.remove(session_id) {
            session.is_active = false;
            info!("HTTPA session terminated: {}", session_id);
            Ok(())
        } else {
            Err(AstraError::SessionNotFound(session_id.to_string()))
        }
    }

    pub fn evict_expired(&self) -> Vec<String> {
        let expired = self
            .sessions
            .iter()
            .filter(|entry| !entry.value().is_alive())
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();

        for session_id in &expired {
            self.sessions.remove(session_id);
        }

        if !expired.is_empty() {
            info!("HTTPA evicted {} expired sessions", expired.len());
        }

        expired
    }

    pub fn active_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|entry| entry.value().is_alive())
            .count()
    }

    pub fn get_stats(&self) -> SessionStats {
        let total = self.sessions.len();
        let active = self.active_count();
        SessionStats {
            total_sessions: total,
            active_sessions: active,
            expired_sessions: total - active,
            max_sessions: self.max_sessions,
            ttl_seconds: self.ttl_seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub expired_sessions: usize,
    pub max_sessions: usize,
    pub ttl_seconds: u64,
}

fn generate_session_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Constant-time byte comparison to prevent timing attacks on token verification.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_tokens_are_verified_and_touch_renews_ttl() {
        let manager = SessionManager::new(60, 8);
        let session = manager
            .create_session(
                "agent-a",
                vec!["research".into()],
                GovernanceClearance::Standard,
            )
            .expect("session should be created");
        let original_expiry = session.expires_at;

        manager
            .verify_session_token(&session.session_id, &session.token)
            .expect("token should authenticate session");
        assert!(matches!(
            manager.verify_session_token(&session.session_id, "wrong-token"),
            Err(AstraError::AuthRequired)
        ));

        manager
            .touch_session(&session.session_id)
            .expect("touch should succeed");
        let touched = manager
            .get_session(&session.session_id)
            .expect("session should still exist");
        assert_eq!(touched.request_count, 1);
        assert!(touched.expires_at >= original_expiry);
    }

    #[test]
    fn terminated_sessions_are_removed_immediately() {
        let manager = SessionManager::new(60, 8);
        let session = manager
            .create_session(
                "agent-a",
                vec!["research".into()],
                GovernanceClearance::Standard,
            )
            .expect("session should be created");

        manager
            .terminate_session(&session.session_id)
            .expect("termination should succeed");
        assert!(matches!(
            manager.get_session(&session.session_id),
            Err(AstraError::SessionNotFound(_))
        ));
    }
}

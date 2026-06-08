use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewaySessionScope {
    Shared,
    PerSender,
    Isolated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLifecycleEvent {
    pub session_key: String,
    pub reason: String,
    pub parent_session_key: Option<String>,
    pub label: Option<String>,
    pub display_name: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySessionRecord {
    pub session_key: String,
    pub httpa_session_id: String,
    pub agent_id: String,
    pub scope: GatewaySessionScope,
    pub label: Option<String>,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}

pub struct OpenClawSessionHub {
    sessions: Arc<DashMap<String, GatewaySessionRecord>>,
    lifecycle_events: Arc<Mutex<Vec<SessionLifecycleEvent>>>,
}

impl OpenClawSessionHub {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            lifecycle_events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn open_session(
        &self,
        httpa_session_id: &str,
        agent_id: &str,
        scope: GatewaySessionScope,
        label: Option<String>,
        display_name: Option<String>,
    ) -> GatewaySessionRecord {
        let now = Utc::now();
        let session_key = build_session_key(agent_id, &scope, httpa_session_id);
        let record = GatewaySessionRecord {
            session_key: session_key.clone(),
            httpa_session_id: httpa_session_id.to_string(),
            agent_id: agent_id.to_string(),
            scope,
            label: label.clone(),
            display_name: display_name.clone(),
            created_at: now,
            last_activity: now,
        };
        self.sessions.insert(session_key.clone(), record.clone());
        self.emit_event(SessionLifecycleEvent {
            session_key,
            reason: "opened".into(),
            parent_session_key: None,
            label,
            display_name,
            recorded_at: now,
        });
        record
    }

    pub fn touch_session(&self, httpa_session_id: &str, reason: &str) {
        if let Some(mut entry) = self
            .sessions
            .iter_mut()
            .find(|session| session.httpa_session_id == httpa_session_id)
        {
            entry.last_activity = Utc::now();
            let event = SessionLifecycleEvent {
                session_key: entry.session_key.clone(),
                reason: reason.to_string(),
                parent_session_key: None,
                label: entry.label.clone(),
                display_name: entry.display_name.clone(),
                recorded_at: entry.last_activity,
            };
            drop(entry);
            self.emit_event(event);
        }
    }

    pub fn close_session(&self, httpa_session_id: &str, reason: &str) {
        let key = self
            .sessions
            .iter()
            .find(|session| session.httpa_session_id == httpa_session_id)
            .map(|session| session.session_key.clone());

        if let Some(key) = key {
            if let Some((_, record)) = self.sessions.remove(&key) {
                self.emit_event(SessionLifecycleEvent {
                    session_key: record.session_key,
                    reason: reason.to_string(),
                    parent_session_key: None,
                    label: record.label,
                    display_name: record.display_name,
                    recorded_at: Utc::now(),
                });
            }
        }
    }

    #[must_use]
    pub fn session_for_httpa_id(&self, httpa_session_id: &str) -> Option<GatewaySessionRecord> {
        self.sessions
            .iter()
            .find(|session| session.httpa_session_id == httpa_session_id)
            .map(|session| session.value().clone())
    }

    #[must_use]
    pub fn sessions(&self) -> Vec<GatewaySessionRecord> {
        let mut sessions = self
            .sessions
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        sessions
    }

    #[must_use]
    pub fn recent_events(&self, limit: usize) -> Vec<SessionLifecycleEvent> {
        let events = self
            .lifecycle_events
            .lock()
            .expect("lifecycle mutex poisoned");
        let len = events.len();
        let start = len.saturating_sub(limit);
        events[start..].to_vec()
    }

    fn emit_event(&self, event: SessionLifecycleEvent) {
        let mut events = self
            .lifecycle_events
            .lock()
            .expect("lifecycle mutex poisoned");
        events.push(event);
        if events.len() > 512 {
            let overflow = events.len() - 512;
            events.drain(0..overflow);
        }
    }
}

impl Default for OpenClawSessionHub {
    fn default() -> Self {
        Self::new()
    }
}

fn build_session_key(
    agent_id: &str,
    scope: &GatewaySessionScope,
    httpa_session_id: &str,
) -> String {
    let suffix = httpa_session_id.chars().take(8).collect::<String>();
    match scope {
        GatewaySessionScope::Shared => format!("shared/{agent_id}/main"),
        GatewaySessionScope::PerSender => format!("agent/{agent_id}/main/{suffix}"),
        GatewaySessionScope::Isolated => format!("agent/{agent_id}/isolated/{suffix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_hub_records_lifecycle_events() {
        let hub = OpenClawSessionHub::new();
        let record = hub.open_session(
            "session-1234",
            "default",
            GatewaySessionScope::PerSender,
            Some("main".into()),
            Some("Default".into()),
        );
        hub.touch_session("session-1234", "intent_received");
        hub.close_session("session-1234", "terminated");

        assert!(record.session_key.contains("agent/default/main"));
        assert_eq!(hub.sessions().len(), 0);
        assert_eq!(hub.recent_events(10).len(), 3);
    }
}

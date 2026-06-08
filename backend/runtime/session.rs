// ─────────────────────────────────────────────────────────────
// Session Manager — Runtime Session Lifecycle
// ─────────────────────────────────────────────────────────────
// Port of claw-code-main/src/runtime.py RuntimeSession + session_store.py

use std::sync::Arc;

use chrono::{DateTime, Utc};
use claw_runtime::{
    ContentBlock, ConversationMessage, MessageRole, Session as ConversationSession,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::error::{AstraError, AstraResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeBudgetSnapshot {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSessionStatus {
    Active,
    Compacted,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSession {
    pub id: String,
    pub linked_httpa_session: Option<String>,
    pub workspace: String,
    pub tags: Vec<String>,
    pub conversation: ConversationSession,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: RuntimeSessionStatus,
    pub retained_messages: usize,
    pub total_messages: usize,
    pub total_tool_calls: usize,
    pub budget: RuntimeBudgetSnapshot,
    pub last_stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSessionSummary {
    pub id: String,
    pub linked_httpa_session: Option<String>,
    pub workspace: String,
    pub tags: Vec<String>,
    pub status: RuntimeSessionStatus,
    pub retained_messages: usize,
    pub total_messages: usize,
    pub total_tool_calls: usize,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeFleetSummary {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub compacted_sessions: usize,
    pub closed_sessions: usize,
    pub linked_httpa_sessions: usize,
    pub total_messages: usize,
    pub total_tool_calls: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeSessionEvent {
    Snapshot {
        session_id: String,
        session: ConversationSession,
    },
    Message {
        session_id: String,
        message: ConversationMessage,
    },
    Status {
        session_id: String,
        status: RuntimeSessionStatus,
        reason: Option<String>,
    },
}

#[derive(Clone)]
struct ManagedRuntimeSession {
    session: RuntimeSession,
    events: broadcast::Sender<RuntimeSessionEvent>,
}

pub struct SessionManager {
    sessions: Arc<DashMap<String, ManagedRuntimeSession>>,
    max_sessions: usize,
    max_messages_per_session: usize,
    event_buffer: usize,
    default_workspace: String,
    session_prefix: String,
}

impl SessionManager {
    #[must_use]
    pub fn new(
        max_sessions: usize,
        max_messages_per_session: usize,
        event_buffer: usize,
        default_workspace: impl Into<String>,
        session_prefix: impl Into<String>,
    ) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            max_sessions,
            max_messages_per_session,
            event_buffer,
            default_workspace: default_workspace.into(),
            session_prefix: session_prefix.into(),
        }
    }

    pub fn create_session(
        &self,
        linked_httpa_session: Option<String>,
        workspace: Option<String>,
        tags: Vec<String>,
    ) -> AstraResult<RuntimeSession> {
        self.evict_if_needed();
        if self.sessions.len() >= self.max_sessions {
            return Err(AstraError::RuntimeCapacityExceeded);
        }

        let now = Utc::now();
        let session = RuntimeSession {
            id: format!(
                "{}-{}",
                self.session_prefix,
                &uuid::Uuid::new_v4().simple().to_string()[..12]
            ),
            linked_httpa_session,
            workspace: workspace.unwrap_or_else(|| self.default_workspace.clone()),
            tags,
            conversation: ConversationSession::new(),
            created_at: now,
            updated_at: now,
            status: RuntimeSessionStatus::Active,
            retained_messages: 0,
            total_messages: 0,
            total_tool_calls: 0,
            budget: RuntimeBudgetSnapshot {
                input_tokens: 0,
                output_tokens: 0,
                tool_calls: 0,
            },
            last_stop_reason: None,
        };

        let (events, _) = broadcast::channel(self.event_buffer.max(16));
        let managed = ManagedRuntimeSession {
            session: session.clone(),
            events,
        };
        self.sessions.insert(session.id.clone(), managed);
        Ok(session)
    }

    pub fn append_user_text(&self, session_id: &str, text: impl Into<String>) -> AstraResult<()> {
        self.append_message(session_id, ConversationMessage::user_text(text))
    }

    pub fn append_assistant_text(
        &self,
        session_id: &str,
        text: impl Into<String>,
    ) -> AstraResult<()> {
        self.append_message(
            session_id,
            ConversationMessage::assistant(vec![ContentBlock::Text { text: text.into() }]),
        )
    }

    pub fn append_tool_result(
        &self,
        session_id: &str,
        tool_name: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> AstraResult<()> {
        self.append_message(
            session_id,
            ConversationMessage::tool_result(
                format!("tool-{}", uuid::Uuid::new_v4().simple()),
                tool_name,
                output,
                is_error,
            ),
        )
    }

    pub fn set_stop_reason(
        &self,
        session_id: &str,
        reason: impl Into<String>,
    ) -> AstraResult<RuntimeSession> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AstraError::RuntimeSessionNotFound(session_id.to_string()))?;

        entry.session.last_stop_reason = Some(reason.into());
        entry.session.updated_at = Utc::now();
        let snapshot = entry.session.clone();
        let _ = entry.events.send(RuntimeSessionEvent::Status {
            session_id: snapshot.id.clone(),
            status: snapshot.status,
            reason: snapshot.last_stop_reason.clone(),
        });

        Ok(snapshot)
    }

    pub fn close_session(
        &self,
        session_id: &str,
        reason: impl Into<String>,
    ) -> AstraResult<RuntimeSession> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AstraError::RuntimeSessionNotFound(session_id.to_string()))?;

        entry.session.status = RuntimeSessionStatus::Closed;
        entry.session.last_stop_reason = Some(reason.into());
        entry.session.updated_at = Utc::now();
        let snapshot = entry.session.clone();
        let _ = entry.events.send(RuntimeSessionEvent::Status {
            session_id: snapshot.id.clone(),
            status: snapshot.status,
            reason: snapshot.last_stop_reason.clone(),
        });

        Ok(snapshot)
    }

    pub fn subscribe(
        &self,
        session_id: &str,
    ) -> AstraResult<broadcast::Receiver<RuntimeSessionEvent>> {
        let entry = self
            .sessions
            .get(session_id)
            .ok_or_else(|| AstraError::RuntimeSessionNotFound(session_id.to_string()))?;
        Ok(entry.events.subscribe())
    }

    pub fn get_session(&self, session_id: &str) -> AstraResult<RuntimeSession> {
        self.sessions
            .get(session_id)
            .map(|entry| entry.session.clone())
            .ok_or_else(|| AstraError::RuntimeSessionNotFound(session_id.to_string()))
    }

    #[must_use]
    pub fn list_sessions(&self, limit: usize) -> Vec<RuntimeSessionSummary> {
        let mut sessions = self
            .sessions
            .iter()
            .map(|entry| RuntimeSessionSummary {
                id: entry.session.id.clone(),
                linked_httpa_session: entry.session.linked_httpa_session.clone(),
                workspace: entry.session.workspace.clone(),
                tags: entry.session.tags.clone(),
                status: entry.session.status,
                retained_messages: entry.session.retained_messages,
                total_messages: entry.session.total_messages,
                total_tool_calls: entry.session.total_tool_calls,
                updated_at: entry.session.updated_at,
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        sessions.truncate(limit);
        sessions
    }

    #[must_use]
    pub fn fleet_summary(&self) -> RuntimeFleetSummary {
        let sessions = self
            .sessions
            .iter()
            .map(|entry| entry.session.clone())
            .collect::<Vec<_>>();

        RuntimeFleetSummary {
            total_sessions: sessions.len(),
            active_sessions: sessions
                .iter()
                .filter(|session| session.status == RuntimeSessionStatus::Active)
                .count(),
            compacted_sessions: sessions
                .iter()
                .filter(|session| session.status == RuntimeSessionStatus::Compacted)
                .count(),
            closed_sessions: sessions
                .iter()
                .filter(|session| session.status == RuntimeSessionStatus::Closed)
                .count(),
            linked_httpa_sessions: sessions
                .iter()
                .filter(|session| session.linked_httpa_session.is_some())
                .count(),
            total_messages: sessions.iter().map(|session| session.total_messages).sum(),
            total_tool_calls: sessions
                .iter()
                .map(|session| session.total_tool_calls)
                .sum(),
        }
    }

    fn append_message(&self, session_id: &str, message: ConversationMessage) -> AstraResult<()> {
        let mut entry = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AstraError::RuntimeSessionNotFound(session_id.to_string()))?;

        let estimated_tokens = estimate_message_tokens(&message);
        match message.role {
            MessageRole::User | MessageRole::System => {
                entry.session.budget.input_tokens += estimated_tokens;
            }
            MessageRole::Assistant => {
                entry.session.budget.output_tokens += estimated_tokens;
            }
            MessageRole::Tool => {
                entry.session.budget.tool_calls += 1;
                entry.session.total_tool_calls += 1;
            }
        }

        entry.session.conversation.messages.push(message.clone());
        entry.session.total_messages += 1;
        entry.session.retained_messages = entry.session.conversation.messages.len();
        entry.session.updated_at = Utc::now();
        self.compact_if_needed(&mut entry.session);
        let snapshot = entry.session.clone();

        let _ = entry.events.send(RuntimeSessionEvent::Message {
            session_id: snapshot.id.clone(),
            message,
        });
        let _ = entry.events.send(RuntimeSessionEvent::Snapshot {
            session_id: snapshot.id,
            session: snapshot.conversation,
        });

        Ok(())
    }

    fn compact_if_needed(&self, session: &mut RuntimeSession) {
        if session.conversation.messages.len() <= self.max_messages_per_session {
            session.retained_messages = session.conversation.messages.len();
            return;
        }

        let overflow = session.conversation.messages.len() - self.max_messages_per_session + 1;
        session.conversation.messages.drain(0..overflow);
        session.conversation.messages.insert(
            0,
            ConversationMessage {
                role: MessageRole::System,
                blocks: vec![ContentBlock::Text {
                    text: format!(
                        "Astra runtime compacted {overflow} older messages to keep the session within enterprise retention limits."
                    ),
                }],
                usage: None,
            },
        );
        session.retained_messages = session.conversation.messages.len();
        session.status = RuntimeSessionStatus::Compacted;
        session.last_stop_reason = Some("message_retention_compaction".into());
    }

    fn evict_if_needed(&self) {
        while self.sessions.len() >= self.max_sessions {
            let candidate = self
                .sessions
                .iter()
                .map(|entry| {
                    (
                        entry.key().clone(),
                        entry.session.status,
                        entry.session.updated_at,
                    )
                })
                .min_by(|left, right| {
                    session_rank(left.1)
                        .cmp(&session_rank(right.1))
                        .then(left.2.cmp(&right.2))
                })
                .map(|(key, _, _)| key);

            if let Some(session_id) = candidate {
                self.sessions.remove(&session_id);
            } else {
                break;
            }
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(2048, 96, 128, "workspaces/astra", "rt")
    }
}

fn estimate_message_tokens(message: &ConversationMessage) -> u64 {
    let chars = message
        .blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => text.len(),
            ContentBlock::ToolUse { input, .. } => input.len(),
            ContentBlock::ToolResult { output, .. } => output.len(),
        })
        .sum::<usize>();
    ((chars / 4).max(1)) as u64
}

fn session_rank(status: RuntimeSessionStatus) -> u8 {
    match status {
        RuntimeSessionStatus::Closed => 0,
        RuntimeSessionStatus::Compacted => 1,
        RuntimeSessionStatus::Active => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_sessions_compact_when_retention_is_exceeded() {
        let manager = SessionManager::new(16, 3, 8, "workspaces/test", "rt");
        let session = manager
            .create_session(Some("httpa-1".into()), None, vec!["browser".into()])
            .expect("session");

        manager
            .append_user_text(&session.id, "first message")
            .expect("append 1");
        manager
            .append_assistant_text(&session.id, "second message")
            .expect("append 2");
        manager
            .append_user_text(&session.id, "third message")
            .expect("append 3");
        manager
            .append_assistant_text(&session.id, "fourth message")
            .expect("append 4");

        let stored = manager.get_session(&session.id).expect("stored");
        assert_eq!(stored.status, RuntimeSessionStatus::Compacted);
        assert_eq!(stored.total_messages, 4);
        assert!(stored.retained_messages <= 3);
        assert!(matches!(
            stored.conversation.messages.first(),
            Some(ConversationMessage {
                role: MessageRole::System,
                ..
            })
        ));
    }
}

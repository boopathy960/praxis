use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantIdentity {
    pub agent_id: String,
    pub name: String,
    pub avatar: String,
    pub emoji: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: String,
    pub sender: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Full,
    Minimal,
    None,
}

impl ContextMode {
    #[must_use]
    pub fn from_label(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "minimal" => Self::Minimal,
            "none" => Self::None,
            _ => Self::Full,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Minimal => "minimal",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRuntimeInfo {
    pub host: String,
    pub workspace_dir: String,
    pub channel: String,
    pub capabilities: Vec<String>,
    pub reasoning_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextArtifact {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPolicy {
    pub send_policy: String,
    pub reply_policy: String,
    pub tool_budget: usize,
    pub history_limit: usize,
    pub context_mode: ContextMode,
    pub compaction_threshold: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextPacket {
    pub system_context: String,
    pub current_message: String,
    pub history_excerpt: String,
    pub rendered_context: String,
    pub sections: Vec<String>,
    pub runtime_line: String,
    pub tool_manifest: Vec<String>,
    pub warnings: Vec<String>,
    pub context_mode: String,
    pub history_turn_count: usize,
    pub compacted: bool,
}

#[derive(Debug, Clone)]
pub struct ContextPacketRequest<'a> {
    pub identity: &'a AssistantIdentity,
    pub session_key: &'a str,
    pub objective: &'a str,
    pub history: &'a [ConversationTurn],
    pub policy: &'a ConnectionPolicy,
    pub runtime: &'a ConnectionRuntimeInfo,
    pub available_tools: &'a [String],
    pub workspace_notes: &'a [String],
    pub context_files: &'a [ContextArtifact],
    pub extra_context: Option<&'a str>,
}

#[must_use]
pub fn build_context_packet(request: ContextPacketRequest<'_>) -> ContextPacket {
    let history_limit = request.policy.history_limit.max(1);
    let (history_excerpt, compacted, mut warnings) = render_history(request.history, history_limit);

    let current_message = request
        .history
        .iter()
        .rev()
        .find(|turn| turn.role == "user" || turn.role == "tool")
        .map(|turn| turn.body.clone())
        .unwrap_or_else(|| request.objective.to_string());

    let runtime_line = format!(
        "Runtime: host={} | workspace={} | channel={} | reasoning_profile={} | capabilities={}",
        request.runtime.host,
        request.runtime.workspace_dir,
        request.runtime.channel,
        request.runtime.reasoning_profile,
        if request.runtime.capabilities.is_empty() {
            "none".to_string()
        } else {
            request.runtime.capabilities.join(",")
        }
    );

    let tool_manifest = if request.available_tools.is_empty() {
        vec!["session_memory".to_string()]
    } else {
        request.available_tools.to_vec()
    };

    if request.objective.len() > 240 {
        warnings.push(
            "Objective is long; prioritize the freshest user goal and verify before acting.".into(),
        );
    }

    let identity_line = format!(
        "You are {}, the Rust-native OpenClaw coordination runtime for HTTPA session `{}`.",
        request.identity.name, request.session_key
    );
    let session_policy = format!(
        "Session policy: send={} | reply={} | tool_budget={} | history_limit={} | compaction_threshold={}",
        request.policy.send_policy,
        request.policy.reply_policy,
        request.policy.tool_budget,
        request.policy.history_limit,
        request.policy.compaction_threshold
    );
    let tooling = format!("Allowed tools: {}", tool_manifest.join(", "));
    let workspace = format!(
        "Workspace root: {}. Treat this as the authoritative project root for planning and file operations.",
        request.runtime.workspace_dir
    );
    let objective = format!("Objective: {}", request.objective);

    let mut sections = vec![identity_line, session_policy, runtime_line.clone()];
    if request.policy.context_mode != ContextMode::None {
        sections.push(tooling);
        sections.push(workspace);
        sections.push(objective);
    }

    if request.policy.context_mode == ContextMode::Full {
        sections.push(
            "Operating style: be tool-aware, verify before acting, compact stale context, and only parallelize when the task materially benefits from coordinated branches.".into(),
        );
        if !request.workspace_notes.is_empty() {
            sections.push(format!(
                "Workspace notes: {}",
                request.workspace_notes.join(" | ")
            ));
        }
        if let Some(extra_context) = request
            .extra_context
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            sections.push(format!("Additional context: {extra_context}"));
        }
        if !request.context_files.is_empty() {
            sections.push("Project context files loaded into this execution packet:".into());
            for file in request.context_files {
                sections.push(format!("Context file {}:\n{}", file.path, file.content));
            }
        }
    }

    let system_context = sections.join("\n\n");
    let rendered_context = if history_excerpt.is_empty() {
        format!("{system_context}\n\nCurrent message:\n{current_message}")
    } else {
        format!(
            "{system_context}\n\nConversation history:\n{history_excerpt}\n\nCurrent message:\n{current_message}"
        )
    };

    ContextPacket {
        system_context,
        current_message,
        history_excerpt,
        rendered_context,
        sections,
        runtime_line,
        tool_manifest,
        warnings,
        context_mode: request.policy.context_mode.as_str().to_string(),
        history_turn_count: request.history.len(),
        compacted,
    }
}

fn render_history(
    history: &[ConversationTurn],
    history_limit: usize,
) -> (String, bool, Vec<String>) {
    if history.is_empty() {
        return (String::new(), false, Vec::new());
    }

    let compacted = history.len() > history_limit;
    let offset = history.len().saturating_sub(history_limit);
    let excerpt = history
        .iter()
        .skip(offset)
        .map(|turn| format!("{} [{}]: {}", turn.sender, turn.role, turn.body))
        .collect::<Vec<_>>()
        .join("\n");

    if compacted {
        (
            format!("[Compacted {} earlier turns]\n{}", offset, excerpt),
            true,
            vec![format!(
                "Context history was compacted to the latest {} turns before execution.",
                history_limit
            )],
        )
    } else {
        (excerpt, false, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_packet_prefers_latest_user_message() {
        let identity = AssistantIdentity {
            agent_id: "default".into(),
            name: "Assistant".into(),
            avatar: "A".into(),
            emoji: None,
        };
        let history = vec![
            ConversationTurn {
                role: "assistant".into(),
                sender: "Assistant".into(),
                body: "hello".into(),
                timestamp: Utc::now(),
            },
            ConversationTurn {
                role: "user".into(),
                sender: "User".into(),
                body: "analyze this page".into(),
                timestamp: Utc::now(),
            },
        ];
        let packet = build_context_packet(ContextPacketRequest {
            identity: &identity,
            session_key: "agent/default/main",
            objective: "analyze page",
            history: &history,
            policy: &ConnectionPolicy {
                send_policy: "allow".into(),
                reply_policy: "auto".into(),
                tool_budget: 8,
                history_limit: 8,
                context_mode: ContextMode::Full,
                compaction_threshold: 16,
            },
            runtime: &ConnectionRuntimeInfo {
                host: "httpa".into(),
                workspace_dir: "astra_state/openclaw/workspace".into(),
                channel: "httpa".into(),
                capabilities: vec!["web_fetch".into()],
                reasoning_profile: "openclaw-rust".into(),
            },
            available_tools: &["web_fetch".into(), "session_memory".into()],
            workspace_notes: &[],
            context_files: &[],
            extra_context: None,
        });
        assert!(packet.current_message.contains("analyze this page"));
    }

    #[test]
    fn context_packet_compacts_long_history() {
        let identity = AssistantIdentity {
            agent_id: "default".into(),
            name: "Assistant".into(),
            avatar: "A".into(),
            emoji: None,
        };
        let history = (0..6)
            .map(|index| ConversationTurn {
                role: if index % 2 == 0 { "user" } else { "assistant" }.into(),
                sender: if index % 2 == 0 { "User" } else { "Assistant" }.into(),
                body: format!("turn-{index}"),
                timestamp: Utc::now(),
            })
            .collect::<Vec<_>>();

        let packet = build_context_packet(ContextPacketRequest {
            identity: &identity,
            session_key: "agent/default/main",
            objective: "analyze page",
            history: &history,
            policy: &ConnectionPolicy {
                send_policy: "allow".into(),
                reply_policy: "auto".into(),
                tool_budget: 8,
                history_limit: 3,
                context_mode: ContextMode::Minimal,
                compaction_threshold: 6,
            },
            runtime: &ConnectionRuntimeInfo {
                host: "httpa".into(),
                workspace_dir: "astra_state/openclaw/workspace".into(),
                channel: "httpa".into(),
                capabilities: vec![],
                reasoning_profile: "openclaw-rust".into(),
            },
            available_tools: &["session_memory".into()],
            workspace_notes: &[],
            context_files: &[],
            extra_context: None,
        });

        assert!(packet.compacted);
        assert!(packet.history_excerpt.contains("Compacted 3 earlier turns"));
    }
}

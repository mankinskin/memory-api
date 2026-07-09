use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use std::path::PathBuf;

pub const SESSION_SCHEMA_VERSION: u32 = 1;

pub fn default_session_schema_version() -> u32 {
    SESSION_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionLinks {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ticket_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spec_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doc_evidence_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_ids: Vec<String>,
}

impl SessionLinks {
    pub fn links_to_ticket(
        &self,
        ticket_id: &str,
    ) -> bool {
        self.ticket_ids.iter().any(|id| id == ticket_id)
    }

    pub fn links_to_spec(
        &self,
        spec_id: &str,
    ) -> bool {
        self.spec_ids.iter().any(|id| id == spec_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurnEventMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_success: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_requests_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_arguments_json: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurn {
    pub sequence: usize,
    pub role: SessionRole,
    pub content: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_meta: Option<SessionTurnEventMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub workspace_slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copilot_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vscode_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<SessionWorktreeAssignment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorktreeAllocationMode {
    New,
    Reused,
    Rotated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionWorktreeStatus {
    Active,
    Superseded,
    Invalidated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorktreeAssignment {
    pub path: PathBuf,
    pub branch: String,
    pub allocation_mode: SessionWorktreeAllocationMode,
    pub status: SessionWorktreeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    #[serde(default = "default_session_schema_version")]
    pub schema_version: u32,
    pub session_id: String,
    pub source: String,
    pub started_at: DateTime<Utc>,
    pub captured_at: DateTime<Utc>,
    pub metadata: SessionMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<SessionTurn>,
    #[serde(default)]
    pub links: SessionLinks,
}

impl SessionRecord {
    pub fn has_turns(&self) -> bool {
        !self.turns.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    use super::{
        SessionLinks,
        SessionMetadata,
        SessionRecord,
        SessionRole,
        SessionTurn,
        SessionTurnEventMeta,
        SessionWorktreeAllocationMode,
        SessionWorktreeAssignment,
        SessionWorktreeStatus,
    };
    use crate::SESSION_SCHEMA_VERSION;

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 12, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn session_record_round_trips_through_serde() {
        let record = SessionRecord {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: "session-123".to_string(),
            source: "copilot-hook".to_string(),
            started_at: sample_time(),
            captured_at: sample_time(),
            metadata: SessionMetadata {
                workspace_slug: "context-engine".to_string(),
                conversation_id: Some("conversation-1".to_string()),
                agent_id: Some("github-copilot-gpt-5.4".to_string()),
                ticket_id: Some("ticket-1".to_string()),
                model: Some("GPT-5.4".to_string()),
                trigger: Some("post-turn".to_string()),
                producer: Some("copilot-agent".to_string()),
                copilot_version: Some("0.55.0".to_string()),
                vscode_version: Some("1.127.0".to_string()),
                protocol_version: Some(1),
                worktree: Some(SessionWorktreeAssignment {
                    path: PathBuf::from("worktrees/session-123"),
                    branch: "session/session-123".to_string(),
                    allocation_mode: SessionWorktreeAllocationMode::New,
                    status: SessionWorktreeStatus::Active,
                    predecessor_session_id: None,
                    predecessor_path: None,
                }),
            },
            turns: vec![SessionTurn {
                sequence: 0,
                role: SessionRole::User,
                content: "Summarize the test failures".to_string(),
                captured_at: sample_time(),
                tool_name: None,
                event_meta: Some(SessionTurnEventMeta {
                    event_id: Some("evt-1".to_string()),
                    parent_event_id: None,
                    event_type: Some("user.message".to_string()),
                    turn_id: Some("turn-1".to_string()),
                    message_id: Some("msg-1".to_string()),
                    tool_call_id: None,
                    tool_success: None,
                    reasoning_text: None,
                    tool_requests_json: None,
                    tool_arguments_json: None,
                }),
            }],
            links: SessionLinks {
                ticket_ids: vec!["ticket-1".to_string()],
                spec_ids: vec!["spec-1".to_string()],
                doc_evidence_ids: vec!["doc-1".to_string()],
                log_ids: vec!["log-1".to_string()],
            },
        };

        let json = serde_json::to_string_pretty(&record).unwrap();
        let reparsed: SessionRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(reparsed, record);
        assert!(record.links.links_to_ticket("ticket-1"));
        assert!(record.links.links_to_spec("spec-1"));
    }
}

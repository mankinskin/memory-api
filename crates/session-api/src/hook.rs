use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    SessionError,
    SessionLinks,
    SessionMetadata,
    SessionRecord,
    SessionRole,
    SessionTurn,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotHookMessage {
    pub role: SessionRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopilotHookPayload {
    pub session_id: String,
    pub workspace_slug: String,
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default)]
    pub messages: Vec<CopilotHookMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCaptureRequest {
    pub source: String,
    pub payload: CopilotHookPayload,
    #[serde(default)]
    pub links: SessionLinks,
}

impl SessionCaptureRequest {
    pub fn copilot(payload: CopilotHookPayload) -> Self {
        Self {
            source: "copilot-hook".to_string(),
            payload,
            links: SessionLinks::default(),
        }
    }

    pub fn into_record(self) -> Result<SessionRecord, SessionError> {
        let payload = self.payload;
        if payload.session_id.trim().is_empty() {
            return Err(SessionError::MissingSessionId);
        }
        if payload.messages.is_empty() {
            return Err(SessionError::EmptyTurns);
        }

        let captured_at = payload.captured_at;
        let turns: Vec<SessionTurn> = payload
            .messages
            .into_iter()
            .enumerate()
            .map(|(sequence, message)| SessionTurn {
                sequence,
                role: message.role,
                content: message.content,
                captured_at: message.captured_at.unwrap_or(captured_at),
                tool_name: message.tool_name,
            })
            .collect();
        let started_at = turns
            .first()
            .map(|turn| turn.captured_at)
            .unwrap_or(captured_at);

        Ok(SessionRecord {
            session_id: payload.session_id,
            source: self.source,
            started_at,
            captured_at,
            metadata: SessionMetadata {
                workspace_slug: payload.workspace_slug,
                conversation_id: payload.conversation_id,
                agent_id: payload.agent_id,
                model: payload.model,
                trigger: payload.trigger,
            },
            turns,
            links: self.links,
        })
    }
}

impl TryFrom<SessionCaptureRequest> for SessionRecord {
    type Error = SessionError;

    fn try_from(value: SessionCaptureRequest) -> Result<Self, Self::Error> {
        value.into_record()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;

    use crate::{
        SessionError,
        SessionRole,
    };

    use super::{
        CopilotHookMessage,
        CopilotHookPayload,
        SessionCaptureRequest,
    };

    fn sample_time() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 12, 30, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn capture_request_maps_hook_payload_into_session_record() {
        let payload = CopilotHookPayload {
            session_id: "session-123".to_string(),
            workspace_slug: "context-engine".to_string(),
            captured_at: sample_time(),
            conversation_id: Some("conversation-42".to_string()),
            agent_id: Some("github-copilot-gpt-5.4".to_string()),
            model: Some("GPT-5.4".to_string()),
            trigger: Some("post-turn".to_string()),
            messages: vec![
                CopilotHookMessage {
                    role: SessionRole::User,
                    content: "Create the session scaffold".to_string(),
                    tool_name: None,
                    captured_at: Some(sample_time()),
                },
                CopilotHookMessage {
                    role: SessionRole::Assistant,
                    content: "Scaffold planned.".to_string(),
                    tool_name: None,
                    captured_at: None,
                },
            ],
        };
        let mut request = SessionCaptureRequest::copilot(payload);
        request.links.ticket_ids.push("ticket-session".to_string());

        let record = request.into_record().unwrap();

        assert_eq!(record.session_id, "session-123");
        assert_eq!(record.source, "copilot-hook");
        assert_eq!(record.metadata.workspace_slug, "context-engine");
        assert_eq!(record.turns.len(), 2);
        assert!(record.links.links_to_ticket("ticket-session"));
        assert!(record.has_turns());
    }

    #[test]
    fn capture_request_rejects_missing_session_id() {
        let payload = CopilotHookPayload {
            session_id: "   ".to_string(),
            workspace_slug: "context-engine".to_string(),
            captured_at: sample_time(),
            conversation_id: None,
            agent_id: None,
            model: None,
            trigger: None,
            messages: vec![CopilotHookMessage {
                role: SessionRole::User,
                content: "Hello".to_string(),
                tool_name: None,
                captured_at: None,
            }],
        };

        let error = SessionCaptureRequest::copilot(payload)
            .into_record()
            .unwrap_err();

        assert!(matches!(error, SessionError::MissingSessionId));
    }
}
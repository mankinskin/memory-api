use std::{
    fs::File,
    io::{
        BufRead,
        BufReader,
    },
    path::Path,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    de::DeserializeOwned,
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
                ticket_id: None,
                model: payload.model,
                trigger: payload.trigger,
                worktree: None,
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

#[derive(Debug, Deserialize)]
struct TranscriptEventEnvelope {
    #[serde(rename = "type")]
    event_type: String,
    timestamp: DateTime<Utc>,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct TranscriptSessionStartData {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    producer: Option<String>,
    #[serde(default, rename = "startTime")]
    start_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessageData {
    #[serde(default)]
    content: String,
}

pub fn copilot_payload_from_transcript_path(
    transcript_path: impl AsRef<Path>,
    workspace_slug: impl Into<String>,
    trigger: Option<String>,
) -> Result<CopilotHookPayload, SessionError> {
    let transcript_path = transcript_path.as_ref();
    let file = File::open(transcript_path).map_err(|source| SessionError::Io {
        path: transcript_path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);

    copilot_payload_from_transcript_reader_with_path(
        reader,
        transcript_path,
        workspace_slug.into(),
        trigger,
    )
}

pub fn copilot_payload_from_transcript_reader<R: BufRead>(
    reader: R,
    workspace_slug: impl Into<String>,
    trigger: Option<String>,
) -> Result<CopilotHookPayload, SessionError> {
    copilot_payload_from_transcript_reader_with_path(
        reader,
        Path::new("<copilot-transcript>"),
        workspace_slug.into(),
        trigger,
    )
}

fn copilot_payload_from_transcript_reader_with_path<R: BufRead>(
    reader: R,
    transcript_path: &Path,
    workspace_slug: String,
    trigger: Option<String>,
) -> Result<CopilotHookPayload, SessionError> {
    let mut session_id = None;
    let mut agent_id = None;
    let mut captured_at = None;
    let mut started_at = None;
    let mut messages = vec![];

    for line in reader.lines() {
        let line = line.map_err(|source| SessionError::Io {
            path: transcript_path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }

        let event: TranscriptEventEnvelope = deserialize_transcript_event(&line, transcript_path)?;

        match event.event_type.as_str() {
            "session.start" => {
                let data: TranscriptSessionStartData =
                    deserialize_transcript_data(event.data, transcript_path)?;
                if session_id.is_none() {
                    session_id = Some(data.session_id);
                }
                if agent_id.is_none() {
                    agent_id = data.producer.filter(|value| !value.trim().is_empty());
                }
                if started_at.is_none() {
                    started_at = data.start_time.or(Some(event.timestamp));
                }
                if captured_at.is_none() {
                    captured_at = Some(event.timestamp);
                }
            }
            "user.message" => {
                let data: TranscriptMessageData =
                    deserialize_transcript_data(event.data, transcript_path)?;
                if data.content.trim().is_empty() {
                    continue;
                }
                captured_at = Some(event.timestamp);
                messages.push(CopilotHookMessage {
                    role: SessionRole::User,
                    content: data.content,
                    tool_name: None,
                    captured_at: Some(event.timestamp),
                });
            }
            "assistant.message" => {
                let data: TranscriptMessageData =
                    deserialize_transcript_data(event.data, transcript_path)?;
                if data.content.trim().is_empty() {
                    continue;
                }
                captured_at = Some(event.timestamp);
                messages.push(CopilotHookMessage {
                    role: SessionRole::Assistant,
                    content: data.content,
                    tool_name: None,
                    captured_at: Some(event.timestamp),
                });
            }
            _ => {}
        }
    }

    let session_id = session_id.ok_or(SessionError::MissingSessionId)?;
    if messages.is_empty() {
        return Err(SessionError::EmptyTurns);
    }

    Ok(CopilotHookPayload {
        session_id,
        workspace_slug,
        captured_at: captured_at.or(started_at).unwrap_or_else(Utc::now),
        conversation_id: None,
        agent_id,
        model: None,
        trigger,
        messages,
    })
}

fn deserialize_transcript_event(
    line: &str,
    transcript_path: &Path,
) -> Result<TranscriptEventEnvelope, SessionError> {
    serde_json::from_str(line).map_err(|source| SessionError::Deserialize {
        path: transcript_path.to_path_buf(),
        source,
    })
}

fn deserialize_transcript_data<T: DeserializeOwned>(
    value: serde_json::Value,
    transcript_path: &Path,
) -> Result<T, SessionError> {
    serde_json::from_value(value).map_err(|source| SessionError::Deserialize {
        path: transcript_path.to_path_buf(),
        source,
    })
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
        copilot_payload_from_transcript_reader,
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
        assert_eq!(record.metadata.ticket_id, None);
        assert_eq!(record.metadata.worktree, None);
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

    #[test]
    fn transcript_reader_maps_visible_messages_into_payload() {
        let transcript = r#"{"type":"session.start","timestamp":"2026-06-02T23:06:54.049Z","data":{"sessionId":"session-123","producer":"copilot-agent","startTime":"2026-06-02T23:06:54.049Z"}}
{"type":"user.message","timestamp":"2026-06-02T23:07:00.000Z","data":{"content":"Hello"}}
{"type":"assistant.message","timestamp":"2026-06-02T23:07:05.000Z","data":{"content":"World"}}
{"type":"assistant.message","timestamp":"2026-06-02T23:07:06.000Z","data":{"content":"   "}}
{"type":"tool.execution_complete","timestamp":"2026-06-02T23:07:07.000Z","data":{"toolName":"read_file"}}"#;

        let payload = copilot_payload_from_transcript_reader(
            std::io::Cursor::new(transcript),
            "context-engine",
            Some("stop".to_string()),
        )
        .unwrap();

        assert_eq!(payload.session_id, "session-123");
        assert_eq!(payload.workspace_slug, "context-engine");
        assert_eq!(payload.agent_id.as_deref(), Some("copilot-agent"));
        assert_eq!(payload.trigger.as_deref(), Some("stop"));
        assert_eq!(payload.messages.len(), 2);
        assert_eq!(payload.messages[0].role, SessionRole::User);
        assert_eq!(payload.messages[0].content, "Hello");
        assert_eq!(payload.messages[1].role, SessionRole::Assistant);
        assert_eq!(payload.messages[1].content, "World");
        assert_eq!(payload.captured_at, chrono::Utc.with_ymd_and_hms(2026, 6, 2, 23, 7, 5).single().unwrap());
    }
}
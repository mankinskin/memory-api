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

#[derive(Debug)]
struct TranscriptEventEnvelope {
    event_type: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    data: serde_json::Value,
    role_hint: Option<SessionRole>,
    content_hint: Option<String>,
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

        let event = deserialize_transcript_event(&line, transcript_path)?;

        match event.event_type.as_deref() {
            Some("session.start") | Some("session_start") | Some("sessionStart") => {
                handle_session_start_event(
                event,
                &mut session_id,
                &mut agent_id,
                &mut started_at,
                &mut captured_at,
            )?
            }
            Some("user.message") | Some("user_message") => handle_message_event(
                event,
                SessionRole::User,
                &mut captured_at,
                &mut messages,
            )?,
            Some("assistant.message") | Some("assistant_message") => handle_message_event(
                event,
                SessionRole::Assistant,
                &mut captured_at,
                &mut messages,
            )?,
            _ => {
                if let Some(role) = event.role_hint.clone() {
                    handle_message_event(
                        event,
                        role,
                        &mut captured_at,
                        &mut messages,
                    )?;
                }
            }
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

fn handle_session_start_event(
    event: TranscriptEventEnvelope,
    session_id: &mut Option<String>,
    agent_id: &mut Option<String>,
    started_at: &mut Option<DateTime<Utc>>,
    captured_at: &mut Option<DateTime<Utc>>,
) -> Result<(), SessionError> {
    let data = &event.data;

    let session_id_value = json_string(data, &["sessionId", "session_id", "id"]);
    let producer_value = json_string(data, &["producer", "agentId", "agent_id"]);
    let start_time_value = json_timestamp(data, &["startTime", "start_time"]);

    if session_id.is_none() {
        *session_id = session_id_value;
    }
    if agent_id.is_none() {
        *agent_id = producer_value;
    }
    if started_at.is_none() {
        *started_at = start_time_value.or(event.timestamp);
    }
    if captured_at.is_none() {
        *captured_at = event.timestamp;
    }
    Ok(())
}

fn handle_message_event(
    event: TranscriptEventEnvelope,
    role: SessionRole,
    captured_at: &mut Option<DateTime<Utc>>,
    messages: &mut Vec<CopilotHookMessage>,
) -> Result<(), SessionError> {
    let content = event.content_hint.unwrap_or_default();
    if content.trim().is_empty() {
        return Ok(());
    }
    let timestamp = event.timestamp.unwrap_or_else(Utc::now);
    *captured_at = Some(timestamp);
    messages.push(CopilotHookMessage {
        role,
        content,
        tool_name: json_string(&event.data, &["toolName", "tool_name"]),
        captured_at: Some(timestamp),
    });
    Ok(())
}

fn deserialize_transcript_event(
    line: &str,
    transcript_path: &Path,
) -> Result<TranscriptEventEnvelope, SessionError> {
    let value: serde_json::Value = serde_json::from_str(line).map_err(|source| SessionError::Deserialize {
        path: transcript_path.to_path_buf(),
        source,
    })?;

    let data = value
        .get("data")
        .cloned()
        .unwrap_or_else(|| value.clone());

    let event_type = first_non_empty_string(&[
        value.get("type").and_then(serde_json::Value::as_str),
        value.get("event").and_then(serde_json::Value::as_str),
        value.get("name").and_then(serde_json::Value::as_str),
    ])
    .map(ToString::to_string);

    let timestamp = value
        .get("timestamp")
        .and_then(parse_timestamp_value)
        .or_else(|| value.get("ts").and_then(parse_timestamp_value))
        .or_else(|| data.get("timestamp").and_then(parse_timestamp_value))
        .or_else(|| data.get("ts").and_then(parse_timestamp_value));

    let role_hint = parse_role(
        value
            .get("role")
            .and_then(serde_json::Value::as_str)
            .or_else(|| data.get("role").and_then(serde_json::Value::as_str)),
    );

    let content_hint = first_non_empty_string(&[
        value.get("content").and_then(serde_json::Value::as_str),
        value.get("text").and_then(serde_json::Value::as_str),
        data.get("content").and_then(serde_json::Value::as_str),
        data.get("text").and_then(serde_json::Value::as_str),
    ])
    .map(ToString::to_string);

    Ok(TranscriptEventEnvelope {
        event_type,
        timestamp,
        data,
        role_hint,
        content_hint,
    })
}

fn json_string(
    value: &serde_json::Value,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
}

fn json_timestamp(
    value: &serde_json::Value,
    keys: &[&str],
) -> Option<DateTime<Utc>> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(parse_timestamp_value)
}

fn parse_timestamp_value(
    value: &serde_json::Value,
) -> Option<DateTime<Utc>> {
    if let Some(text) = value.as_str() {
        return DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|timestamp| timestamp.with_timezone(&Utc));
    }
    if let Some(millis) = value.as_i64() {
        return DateTime::<Utc>::from_timestamp_millis(millis);
    }
    None
}

fn parse_role(
    role: Option<&str>,
) -> Option<SessionRole> {
    match role?.trim().to_ascii_lowercase().as_str() {
        "user" => Some(SessionRole::User),
        "assistant" | "model" => Some(SessionRole::Assistant),
        _ => None,
    }
}

fn first_non_empty_string<'a>(
    values: &[Option<&'a str>],
) -> Option<&'a str> {
    values
        .iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
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

    #[test]
    fn transcript_reader_supports_modern_message_shape() {
        let transcript = r#"{"event":"session_start","ts":1717372014049,"data":{"session_id":"session-modern","producer":"copilot-agent"}}
{"event":"message","timestamp":"2026-06-02T23:07:00.000Z","role":"user","content":"Hello modern"}
{"event":"message","timestamp":"2026-06-02T23:07:05.000Z","data":{"role":"assistant","text":"Hi modern"}}"#;

        let payload = copilot_payload_from_transcript_reader(
            std::io::Cursor::new(transcript),
            "default",
            Some("stop".to_string()),
        )
        .unwrap();

        assert_eq!(payload.session_id, "session-modern");
        assert_eq!(payload.messages.len(), 2);
        assert_eq!(payload.messages[0].role, SessionRole::User);
        assert_eq!(payload.messages[0].content, "Hello modern");
        assert_eq!(payload.messages[1].role, SessionRole::Assistant);
        assert_eq!(payload.messages[1].content, "Hi modern");
    }
}
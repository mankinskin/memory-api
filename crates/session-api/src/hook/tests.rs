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
    copilot_payload_from_transcript_reader,
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
                event_meta: None,
            },
            CopilotHookMessage {
                role: SessionRole::Assistant,
                content: "Scaffold planned.".to_string(),
                tool_name: None,
                captured_at: None,
                event_meta: None,
            },
        ],
        events: vec![],
        runtime: None,
    };
    let mut request = SessionCaptureRequest::copilot(payload);
    request.links.ticket_ids.push("ticket-session".to_string());

    let (record, events) = request.into_record_and_events().unwrap();

    assert_eq!(record.session_id, "session-123");
    assert!(events.is_empty());
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
            event_meta: None,
        }],
        events: vec![],
        runtime: None,
    };

    let error = SessionCaptureRequest::copilot(payload)
        .into_record()
        .unwrap_err();

    assert!(matches!(error, SessionError::MissingSessionId));
}

#[test]
fn transcript_reader_maps_visible_messages_into_payload() {
    let transcript = r#"{"id":"evt-start","type":"session.start","timestamp":"2026-06-02T23:06:54.049Z","data":{"sessionId":"session-123","producer":"copilot-agent","copilotVersion":"0.55.0","vscodeVersion":"1.127.0","version":1,"startTime":"2026-06-02T23:06:54.049Z"}}
{"id":"evt-1","parentId":"evt-start","type":"user.message","timestamp":"2026-06-02T23:07:00.000Z","data":{"content":"Hello"}}
{"id":"evt-2","parentId":"evt-1","type":"assistant.message","timestamp":"2026-06-02T23:07:05.000Z","data":{"messageId":"m-1","turnId":"t-1","reasoningText":"r","toolRequests":[{"name":"read_file"}],"content":"World"}}
{"id":"evt-3","type":"tool.execution_complete","timestamp":"2026-06-02T23:07:07.000Z","data":{"toolCallId":"call-1","toolName":"read_file","arguments":{"a":1},"success":true}}"#;

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
    assert_eq!(payload.events.len(), 4);
    assert!(
        payload.events[2]
            .data_json
            .as_ref()
            .and_then(|json| json.get("toolRequests"))
            .is_some()
    );
    assert!(
        payload.events[3]
            .raw_event_json
            .as_ref()
            .and_then(|json| json.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("tool.execution_complete")
    );
    assert_eq!(payload.messages[0].role, SessionRole::User);
    assert_eq!(payload.messages[0].content, "Hello");
    assert_eq!(payload.messages[1].role, SessionRole::Assistant);
    assert_eq!(payload.messages[1].content, "World");
    assert_eq!(
        payload.messages[1]
            .event_meta
            .as_ref()
            .and_then(|m| m.message_id.as_deref()),
        Some("m-1")
    );
    assert_eq!(
        payload
            .runtime
            .as_ref()
            .and_then(|r| r.copilot_version.as_deref()),
        Some("0.55.0")
    );
    assert_eq!(
        payload.captured_at,
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 2, 23, 7, 5)
            .single()
            .unwrap()
    );
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

#[test]
fn transcript_reader_destringifies_nested_json_payloads() {
    let transcript = r#"{"id":"evt-start","type":"session.start","timestamp":"2026-06-02T23:06:54.049Z","data":"{\"sessionId\":\"session-json\",\"producer\":\"copilot-agent\"}"}
{"id":"evt-1","type":"assistant.message","timestamp":"2026-06-02T23:07:05.000Z","data":{"messageId":"m-1","content":"World","arguments":"{\"path\":\"src/lib.rs\",\"line\":42}","toolRequests":"[{\"name\":\"read_file\"}]"}}"#;

    let payload = copilot_payload_from_transcript_reader(
        std::io::Cursor::new(transcript),
        "default",
        Some("stop".to_string()),
    )
    .unwrap();

    assert_eq!(payload.session_id, "session-json");
    assert_eq!(payload.events.len(), 2);

    let event_meta = payload.messages[0].event_meta.as_ref().unwrap();
    assert_eq!(
        event_meta
            .tool_arguments_json
            .as_ref()
            .and_then(|value| value.get("path"))
            .and_then(serde_json::Value::as_str),
        Some("src/lib.rs")
    );
    assert_eq!(
        event_meta
            .tool_requests_json
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .and_then(|items| items.first())
            .and_then(|value| value.get("name"))
            .and_then(serde_json::Value::as_str),
        Some("read_file")
    );

    assert!(
        payload.events[1]
            .data_json
            .as_ref()
            .and_then(|value| value.get("arguments"))
            .and_then(|value| value.get("line"))
            .and_then(serde_json::Value::as_i64)
            == Some(42)
    );
}

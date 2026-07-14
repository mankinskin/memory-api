use chrono::TimeZone;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use crate::{
    CopilotHookMessage,
    CopilotHookPayload,
    PersistedSessionEvents,
    PersistedSessionManifest,
    PersistedSessionTranscript,
    SESSION_SCHEMA_VERSION,
    SessionAuditSelector,
    SessionCaptureRequest,
    SessionError,
    SessionQuery,
    SessionRole,
    SessionRuntimeInitRequest,
    SessionStoreConfig,
    SessionTicketStateResolver,
    SessionWorkflowEdgeKind,
    SessionWorkflowNodeDraft,
    SessionWorkflowNodeKind,
    SessionWorkflowNodeRequirement,
    SessionWorkflowNodeStatus,
    SessionWorktreeAllocationMode,
    SessionWorktreeCheckInRequest,
    SessionWorktreeStatus,
};

fn sample_time() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .with_ymd_and_hms(2026, 6, 2, 13, 0, 0)
        .single()
        .unwrap()
}

fn sample_time_later() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .with_ymd_and_hms(2026, 6, 2, 13, 5, 0)
        .single()
        .unwrap()
}

fn sample_payload(
    session_id: &str,
    conversation_id: Option<&str>,
    captured_at: chrono::DateTime<chrono::Utc>,
    messages: &[&str],
) -> CopilotHookPayload {
    CopilotHookPayload {
        session_id: session_id.to_string(),
        workspace_slug: "context-engine".to_string(),
        captured_at,
        conversation_id: conversation_id.map(str::to_string),
        agent_id: Some("github-copilot-gpt-5.4".to_string()),
        model: Some("GPT-5.4".to_string()),
        trigger: Some("post-turn".to_string()),
        messages: messages
            .iter()
            .enumerate()
            .map(|(index, content)| CopilotHookMessage {
                role: if index % 2 == 0 {
                    SessionRole::User
                } else {
                    SessionRole::Assistant
                },
                content: (*content).to_string(),
                tool_name: None,
                captured_at: None,
                event_meta: None,
            })
            .collect(),
        events: vec![],
        runtime: None,
    }
}

fn sample_request(
    session_id: &str,
    conversation_id: Option<&str>,
    captured_at: chrono::DateTime<chrono::Utc>,
    messages: &[&str],
) -> SessionCaptureRequest {
    SessionCaptureRequest::copilot(sample_payload(
        session_id,
        conversation_id,
        captured_at,
        messages,
    ))
}

fn sample_worktree_request(
    session_id: &str,
    owner_id: &str,
    ticket_id: &str,
    worktree_path: std::path::PathBuf,
    branch: &str,
) -> SessionWorktreeCheckInRequest {
    SessionWorktreeCheckInRequest {
        session_id: session_id.to_string(),
        owner_id: owner_id.to_string(),
        ticket_id: ticket_id.to_string(),
        worktree_path,
        branch: branch.to_string(),
        predecessor_session_id: None,
    }
}

#[test]
fn store_plan_uses_session_id_in_paths() {
    let config = SessionStoreConfig::new(".session", "context-engine");
    let plan = config
        .plan_capture(sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time(),
            &["Persist this chat"],
        ))
        .unwrap();

    assert_eq!(
        plan.paths.manifest_path,
        std::path::PathBuf::from(".session/sessions/session-abc/session.json")
    );
    assert_eq!(
        plan.paths.transcript_path,
        std::path::PathBuf::from(
            ".session/sessions/session-abc/transcript.json"
        )
    );
}

#[test]
fn store_plan_rejects_invalid_path_segments() {
    let config = SessionStoreConfig::new(".session", "context-engine");
    let mut request = sample_request(
        "session-abc",
        Some("conversation-abc"),
        sample_time(),
        &["Persist this chat"],
    );
    request.payload.session_id = "session/abc".to_string();

    let error = config.plan_capture(request).unwrap_err();

    assert!(matches!(
        error,
        SessionError::InvalidSessionId(ref value) if value == "session/abc"
    ));
}

#[test]
fn persist_capture_writes_manifest_and_transcript_files() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let plan = config
        .persist_capture(sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time(),
            &["Persist this chat"],
        ))
        .unwrap();
    let manifest_text =
        std::fs::read_to_string(&plan.paths.manifest_path).unwrap();
    let transcript_text =
        std::fs::read_to_string(&plan.paths.transcript_path).unwrap();

    let manifest: PersistedSessionManifest =
        serde_json::from_str(&manifest_text).unwrap();
    let transcript: PersistedSessionTranscript =
        serde_json::from_str(&transcript_text).unwrap();

    assert_eq!(manifest.session_id, "session-abc");
    assert_eq!(manifest.metadata.workspace_slug, "context-engine");
    assert_eq!(transcript.session_id, "session-abc");
    assert_eq!(transcript.turns.len(), 1);
    assert_eq!(transcript.turns[0].content, "Persist this chat");
}

#[test]
fn persist_capture_appends_only_new_turns_from_later_capture() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    config
        .persist_capture(sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time(),
            &["first"],
        ))
        .unwrap();

    let plan = config
        .persist_capture(sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time_later(),
            &["first", "second"],
        ))
        .unwrap();
    config
        .persist_capture(sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time_later(),
            &["first", "second"],
        ))
        .unwrap();
    let transcript_text =
        std::fs::read_to_string(&plan.paths.transcript_path).unwrap();
    let transcript: PersistedSessionTranscript =
        serde_json::from_str(&transcript_text).unwrap();

    assert_eq!(transcript.turns.len(), 2);
    assert_eq!(transcript.turns[0].content, "first");
    assert_eq!(transcript.turns[0].captured_at, sample_time());
    assert_eq!(transcript.turns[1].content, "second");
    assert_eq!(transcript.turns[1].captured_at, sample_time_later());
}

#[test]
fn read_session_reconstructs_persisted_record() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    config
        .persist_capture(sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time(),
            &["first"],
        ))
        .unwrap();
    config
        .persist_capture(sample_request(
            "session-abc",
            Some("conversation-abc"),
            sample_time_later(),
            &["first", "second"],
        ))
        .unwrap();

    let record = config.read_session("session-abc").unwrap();

    assert_eq!(record.session_id, "session-abc");
    assert_eq!(record.started_at, sample_time());
    assert_eq!(record.captured_at, sample_time_later());
    assert_eq!(record.turns.len(), 2);
    assert_eq!(record.turns[0].content, "first");
    assert_eq!(record.turns[1].content, "second");
}

#[test]
fn capture_copilot_hook_persists_payload() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let plan = config
        .capture_copilot_hook(sample_payload(
            "session-hook",
            Some("conversation-hook"),
            sample_time(),
            &["Persist from hook"],
        ))
        .unwrap();
    let record = config.read_session("session-hook").unwrap();

    assert!(plan.paths.manifest_path.exists());
    assert_eq!(record.session_id, "session-hook");
    assert_eq!(record.turns.len(), 1);
    assert_eq!(record.turns[0].content, "Persist from hook");
}

#[test]
fn persist_capture_keeps_distinct_id_less_events_using_raw_event_payload() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let mut first = sample_payload(
        "session-events",
        Some("conversation-events"),
        sample_time(),
        &["first"],
    );
    first.events = vec![crate::CopilotHookEvent {
        event_id: None,
        parent_event_id: None,
        event_type: Some("tool.execution_complete".to_string()),
        captured_at: Some(sample_time()),
        turn_id: None,
        message_id: None,
        tool_call_id: Some("call-1".to_string()),
        tool_name: Some("read_file".to_string()),
        tool_success: Some(true),
        reasoning_text: None,
        tool_requests_json: None,
        tool_arguments_json: Some(serde_json::json!({ "path": "A" })),
        data_json: Some(serde_json::json!({ "arguments": { "path": "A" } })),
        raw_event_json: Some(serde_json::json!({
            "type": "tool.execution_complete",
            "data": { "arguments": { "path": "A" } }
        })),
    }];
    config
        .persist_capture(SessionCaptureRequest::copilot(first))
        .unwrap();

    let mut second = sample_payload(
        "session-events",
        Some("conversation-events"),
        sample_time_later(),
        &["first", "second"],
    );
    second.events = vec![crate::CopilotHookEvent {
        event_id: None,
        parent_event_id: None,
        event_type: Some("tool.execution_complete".to_string()),
        captured_at: Some(sample_time()),
        turn_id: None,
        message_id: None,
        tool_call_id: Some("call-1".to_string()),
        tool_name: Some("read_file".to_string()),
        tool_success: Some(true),
        reasoning_text: None,
        tool_requests_json: None,
        tool_arguments_json: Some(serde_json::json!({ "path": "B" })),
        data_json: Some(serde_json::json!({ "arguments": { "path": "B" } })),
        raw_event_json: Some(serde_json::json!({
            "type": "tool.execution_complete",
            "data": { "arguments": { "path": "B" } }
        })),
    }];
    let plan = config
        .persist_capture(SessionCaptureRequest::copilot(second))
        .unwrap();

    let events_text = std::fs::read_to_string(&plan.paths.events_path).unwrap();
    let events: PersistedSessionEvents =
        serde_json::from_str(&events_text).unwrap();
    assert_eq!(events.events.len(), 2);
    assert!(events.events.iter().any(|event| {
        event
            .raw_event_json
            .as_ref()
            .and_then(|json| json.pointer("/data/arguments/path"))
            .and_then(serde_json::Value::as_str)
            == Some("A")
    }));
    assert!(events.events.iter().any(|event| {
        event
            .raw_event_json
            .as_ref()
            .and_then(|json| json.pointer("/data/arguments/path"))
            .and_then(serde_json::Value::as_str)
            == Some("B")
    }));
}

#[test]
fn query_sessions_filters_by_text_and_metadata() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    config
        .capture_copilot_hook(sample_payload(
            "session-alpha",
            Some("conversation-alpha"),
            sample_time(),
            &["Investigate failing test"],
        ))
        .unwrap();
    config
        .capture_copilot_hook(sample_payload(
            "session-beta",
            Some("conversation-beta"),
            sample_time_later(),
            &["Document hook query behavior"],
        ))
        .unwrap();

    let by_text = config
        .query_sessions(&SessionQuery {
            text: Some("hook query".to_string()),
            ..SessionQuery::default()
        })
        .unwrap();
    let by_conversation = config
        .query_sessions(&SessionQuery {
            conversation_id: Some("conversation-alpha".to_string()),
            ..SessionQuery::default()
        })
        .unwrap();

    assert_eq!(by_text.len(), 1);
    assert_eq!(by_text[0].session_id, "session-beta");
    assert_eq!(by_conversation.len(), 1);
    assert_eq!(by_conversation[0].session_id, "session-alpha");
}

#[test]
fn capture_copilot_transcript_persists_visible_transcript_messages() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let transcript_path = tempdir.path().join("copilot.jsonl");

    std::fs::write(
            &transcript_path,
            concat!(
                "{\"type\":\"session.start\",\"timestamp\":\"2026-06-02T23:06:54.049Z\",\"data\":{\"sessionId\":\"session-transcript\",\"producer\":\"copilot-agent\",\"startTime\":\"2026-06-02T23:06:54.049Z\"}}\n",
                "{\"type\":\"user.message\",\"timestamp\":\"2026-06-02T23:07:00.000Z\",\"data\":{\"content\":\"Persist this transcript\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-06-02T23:07:05.000Z\",\"data\":{\"content\":\"Transcript persisted.\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-06-02T23:07:06.000Z\",\"data\":{\"content\":\"\"}}\n"
            ),
        )
        .unwrap();

    let plan = config
        .capture_copilot_transcript(&transcript_path, "stop")
        .unwrap();
    let record = config.read_session("session-transcript").unwrap();

    assert!(plan.paths.manifest_path.exists());
    assert_eq!(record.session_id, "session-transcript");
    assert_eq!(record.metadata.trigger.as_deref(), Some("stop"));
    assert_eq!(record.turns.len(), 2);
    assert_eq!(record.turns[0].content, "Persist this transcript");
    assert_eq!(record.turns[1].content, "Transcript persisted.");
}

#[test]
fn capture_copilot_transcript_allows_divergent_newer_snapshot() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let transcript_path = tempdir.path().join("copilot.jsonl");

    std::fs::write(
            &transcript_path,
            concat!(
                "{\"type\":\"session.start\",\"timestamp\":\"2026-06-02T23:06:54.049Z\",\"data\":{\"sessionId\":\"session-sync\",\"producer\":\"copilot-agent\",\"startTime\":\"2026-06-02T23:06:54.049Z\"}}\n",
                "{\"type\":\"user.message\",\"timestamp\":\"2026-06-02T23:07:00.000Z\",\"data\":{\"content\":\"Original prompt\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-06-02T23:07:05.000Z\",\"data\":{\"content\":\"Original response\"}}\n"
            ),
        )
        .unwrap();

    config
        .capture_copilot_transcript(&transcript_path, "PostToolUse")
        .unwrap();

    std::fs::write(
            &transcript_path,
            concat!(
                "{\"type\":\"session.start\",\"timestamp\":\"2026-06-02T23:06:54.049Z\",\"data\":{\"sessionId\":\"session-sync\",\"producer\":\"copilot-agent\",\"startTime\":\"2026-06-02T23:06:54.049Z\"}}\n",
                "{\"type\":\"user.message\",\"timestamp\":\"2026-06-02T23:07:00.000Z\",\"data\":{\"content\":\"Edited prompt\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-06-02T23:07:05.000Z\",\"data\":{\"content\":\"Edited response\"}}\n",
                "{\"type\":\"assistant.message\",\"timestamp\":\"2026-06-02T23:07:07.000Z\",\"data\":{\"content\":\"Additional message\"}}\n"
            ),
        )
        .unwrap();

    config
        .capture_copilot_transcript(&transcript_path, "PostToolUse")
        .unwrap();

    let record = config.read_session("session-sync").unwrap();
    assert_eq!(record.turns.len(), 3);
    assert_eq!(record.turns[0].content, "Edited prompt");
    assert_eq!(record.turns[2].content, "Additional message");
}

#[test]
fn check_in_worktree_creates_and_returns_new_assignment() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let worktree_path = tempdir.path().join("worktrees").join("session-a");

    let receipt = config
        .check_in_worktree(sample_worktree_request(
            "session-a",
            "github-copilot",
            "ticket-a",
            worktree_path.clone(),
            "session/session-a",
        ))
        .unwrap();

    assert_eq!(receipt.session_id, "session-a");
    assert_eq!(receipt.owner_id, "github-copilot");
    assert_eq!(receipt.ticket_id, "ticket-a");
    assert_eq!(receipt.worktree_path, worktree_path);
    assert_eq!(receipt.branch, "session/session-a");
    assert_eq!(receipt.allocation_mode, SessionWorktreeAllocationMode::New);
    assert_eq!(receipt.status, SessionWorktreeStatus::Active);
    assert!(receipt.worktree_path.exists());
}

#[test]
fn check_in_worktree_reuses_existing_assignment_for_same_session() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let worktree_path = tempdir.path().join("worktrees").join("session-a");

    config
        .check_in_worktree(sample_worktree_request(
            "session-a",
            "github-copilot",
            "ticket-a",
            worktree_path.clone(),
            "session/session-a",
        ))
        .unwrap();

    let receipt = config
        .check_in_worktree(sample_worktree_request(
            "session-a",
            "github-copilot",
            "ticket-a",
            worktree_path.clone(),
            "session/session-a",
        ))
        .unwrap();

    assert_eq!(
        receipt.allocation_mode,
        SessionWorktreeAllocationMode::Reused
    );
    assert_eq!(receipt.worktree_path, worktree_path);

    let lookup = config.lookup_worktree("session-a").unwrap();
    assert_eq!(
        lookup.allocation_mode,
        SessionWorktreeAllocationMode::Reused
    );
    assert_eq!(lookup.status, SessionWorktreeStatus::Active);
}

#[test]
fn check_in_worktree_rotates_for_handoff_and_supersedes_predecessor() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let first_path = tempdir.path().join("worktrees").join("session-a");
    let second_path = tempdir.path().join("worktrees").join("session-b");

    config
        .check_in_worktree(sample_worktree_request(
            "session-a",
            "github-copilot",
            "ticket-a",
            first_path.clone(),
            "session/session-a",
        ))
        .unwrap();

    let mut handoff = sample_worktree_request(
        "session-b",
        "github-copilot-2",
        "ticket-a",
        second_path.clone(),
        "session/session-b",
    );
    handoff.predecessor_session_id = Some("session-a".to_string());

    let receipt = config.check_in_worktree(handoff).unwrap();
    let predecessor = config.read_session("session-a").unwrap();

    assert_eq!(
        receipt.allocation_mode,
        SessionWorktreeAllocationMode::Rotated
    );
    assert_eq!(receipt.predecessor_session_id.as_deref(), Some("session-a"));
    assert_eq!(receipt.predecessor_path, Some(first_path));
    assert_eq!(
        predecessor.metadata.worktree.unwrap().status,
        SessionWorktreeStatus::Superseded
    );
}

#[test]
fn check_in_worktree_rotates_when_existing_path_is_missing() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let first_path = tempdir.path().join("worktrees").join("session-a");
    let second_path =
        tempdir.path().join("worktrees").join("session-a-rotated");

    config
        .check_in_worktree(sample_worktree_request(
            "session-a",
            "github-copilot",
            "ticket-a",
            first_path.clone(),
            "session/session-a",
        ))
        .unwrap();
    std::fs::remove_dir_all(&first_path).unwrap();

    let receipt = config
        .check_in_worktree(sample_worktree_request(
            "session-a",
            "github-copilot",
            "ticket-a",
            second_path.clone(),
            "session/session-a",
        ))
        .unwrap();

    assert_eq!(
        receipt.allocation_mode,
        SessionWorktreeAllocationMode::Rotated
    );
    assert_eq!(receipt.predecessor_session_id, None);
    assert_eq!(receipt.predecessor_path, Some(first_path));
    assert_eq!(receipt.worktree_path, second_path);
    assert!(receipt.worktree_path.exists());
}

#[test]
fn cross_session_reuse_requires_adopt_flow() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let shared_path = tempdir.path().join("worktrees").join("session-a");

    config
        .check_in_worktree(sample_worktree_request(
            "session-a",
            "github-copilot",
            "ticket-a",
            shared_path.clone(),
            "session/session-a",
        ))
        .unwrap();

    let mut handoff = sample_worktree_request(
        "session-b",
        "github-copilot-2",
        "ticket-a",
        shared_path.clone(),
        "session/session-b",
    );
    handoff.predecessor_session_id = Some("session-a".to_string());

    let error = config.check_in_worktree(handoff).unwrap_err();

    assert!(matches!(
        error,
        SessionError::CrossSessionReuseRequiresAdopt { .. }
    ));
}

#[test]
fn read_session_rejects_unknown_schema_version() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let plan = config
        .persist_capture(sample_request(
            "session-schema",
            Some("conversation-schema"),
            sample_time(),
            &["check schema"],
        ))
        .unwrap();

    let mut manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&plan.paths.manifest_path).unwrap(),
    )
    .unwrap();
    manifest["schema_version"] = serde_json::json!(SESSION_SCHEMA_VERSION + 1);
    std::fs::write(
        &plan.paths.manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let err = config.read_session("session-schema").unwrap_err();
    assert!(matches!(err, SessionError::SchemaVersionMismatch { .. }));
}

#[test]
fn session_audit_supports_latest_and_explicit_session_selectors() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let mut older = sample_payload(
        "session-old",
        Some("conversation-old"),
        sample_time(),
        &["first"],
    );
    older.events = vec![crate::CopilotHookEvent {
        event_id: Some("evt-old-1".to_string()),
        parent_event_id: None,
        event_type: Some("assistant.tool_plan".to_string()),
        captured_at: Some(sample_time()),
        turn_id: None,
        message_id: None,
        tool_call_id: None,
        tool_name: None,
        tool_success: None,
        reasoning_text: None,
        tool_requests_json: None,
        tool_arguments_json: None,
        data_json: Some(serde_json::json!({})),
        raw_event_json: None,
    }];
    config
        .persist_capture(SessionCaptureRequest::copilot(older))
        .unwrap();

    let mut newer = sample_payload(
        "session-new",
        Some("conversation-new"),
        sample_time_later(),
        &["latest"],
    );
    newer.events = vec![crate::CopilotHookEvent {
        event_id: Some("evt-new-1".to_string()),
        parent_event_id: None,
        event_type: Some("tool.execution_result".to_string()),
        captured_at: Some(sample_time_later()),
        turn_id: None,
        message_id: None,
        tool_call_id: Some("call-1".to_string()),
        tool_name: Some("run_in_terminal".to_string()),
        tool_success: Some(true),
        reasoning_text: None,
        tool_requests_json: None,
        tool_arguments_json: None,
        data_json: Some(serde_json::json!({
            "blocker": "sync-terminal-state-ambiguous"
        })),
        raw_event_json: None,
    }];
    config
        .persist_capture(SessionCaptureRequest::copilot(newer))
        .unwrap();

    let latest = config.session_audit(SessionAuditSelector::Latest).unwrap();
    let explicit = config
        .session_audit(SessionAuditSelector::SessionId(
            "session-old".to_string(),
        ))
        .unwrap();

    assert_eq!(latest.session_id, "session-new");
    assert_eq!(latest.schema_version, SESSION_SCHEMA_VERSION);
    assert_eq!(latest.metrics.tool_execution_result_count, 1);
    assert_eq!(latest.metrics.ambiguous_sync_terminal_count, 1);
    assert_eq!(explicit.session_id, "session-old");
    assert_eq!(explicit.metrics.assistant_tool_plan_count, 1);
}

#[test]
fn context_schema_init_is_idempotent_without_forcing_a_new_run() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let first = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let second = config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(
                first.context.workspace_session_id.clone(),
            ),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();

    assert!(first.created_workspace);
    assert!(first.created_run);
    assert!(!second.created_workspace);
    assert!(!second.created_run);
    assert_eq!(
        first.context.workspace_session_id,
        second.context.workspace_session_id
    );
    assert_eq!(first.context.active_run_id, second.context.active_run_id);
    assert_eq!(second.context.runs.len(), 1);
}

#[test]
fn run_lineage_init_resume_creates_distinct_linked_run() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let first = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let resumed = config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(
                first.context.workspace_session_id.clone(),
            ),
            predecessor_run_id: Some(first.context.active_run_id.clone()),
            force_new_run: true,
        })
        .unwrap();

    assert_eq!(
        first.context.workspace_session_id,
        resumed.context.workspace_session_id
    );
    assert_ne!(first.context.active_run_id, resumed.context.active_run_id);
    assert_eq!(resumed.context.runs.len(), 2);
    assert_eq!(
        resumed.run.predecessor_run_id.as_deref(),
        Some(first.context.active_run_id.as_str())
    );
}

#[test]
fn context_pin_unpin_is_idempotent_and_persistent() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;
    let urn = "ce://default/tickets/effba966-f0a8-4d7d-b289-b7feba826cf8";

    let pinned_once = config
        .pin_runtime_entity(
            &workspace_id,
            urn,
            Some("primary-focus".to_string()),
            Some("epic context".to_string()),
        )
        .unwrap();
    let pinned_twice = config
        .pin_runtime_entity(&workspace_id, urn, None, None)
        .unwrap();

    assert_eq!(pinned_once.pinned_entities.len(), 1);
    assert_eq!(pinned_twice.pinned_entities.len(), 1);

    let loaded = config.read_runtime_context(&workspace_id).unwrap();
    assert_eq!(loaded.pinned_entities.len(), 1);

    let unpinned_once =
        config.unpin_runtime_entity(&workspace_id, urn).unwrap();
    let unpinned_twice =
        config.unpin_runtime_entity(&workspace_id, urn).unwrap();
    assert!(unpinned_once.pinned_entities.is_empty());
    assert!(unpinned_twice.pinned_entities.is_empty());
}

#[test]
fn context_pin_rejects_malformed_entity_urn_segments() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();

    let error = config
        .pin_runtime_entity(
            &init.context.workspace_session_id,
            "ce:///tickets/",
            None,
            None,
        )
        .unwrap_err();

    assert!(matches!(error, SessionError::InvalidEntityUrn(_)));
}

#[test]
fn context_view_returns_headers_only() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .pin_runtime_entity(
            &workspace_id,
            "ce://default/specs/709f067a-21b6-41b6-8879-3cacef4bacaf",
            Some("guard".to_string()),
            Some("runtime contract".to_string()),
        )
        .unwrap();

    let view = config.view_runtime_context(&workspace_id).unwrap();
    let json = serde_json::to_string(&view).unwrap();

    assert_eq!(view.pinned_count, 1);
    assert!(json.contains("pinned_headers"));
    assert!(!json.contains("body"));
    assert!(!json.contains("content"));
}

#[test]
fn pinned_rule_render_contains_only_rule_pins_in_canonical_order() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;
    let mut rule_store =
        rule_api::RuleStore::open_or_init(&store_root.join(".rule")).unwrap();

    let mut later = rule_api::RuleManifest::new(
        "session/render/later",
        "Later",
        ".instructions",
        "later",
        "Later guidance.",
    );
    later.set_order_key(20);
    let later_id = rule_store.create(&later, None).unwrap();
    let mut earlier = rule_api::RuleManifest::new(
        "session/render/earlier",
        "Earlier",
        ".instructions",
        "earlier",
        "Earlier guidance.",
    );
    earlier.set_order_key(10);
    let earlier_id = rule_store.create(&earlier, None).unwrap();

    config
        .pin_runtime_entity(
            &workspace_id,
            &format!("ce://context-engine/rules/{later_id}"),
            None,
            None,
        )
        .unwrap();
    config
        .pin_runtime_entity(
            &workspace_id,
            "ce://context-engine/tickets/11111111-1111-4111-8111-111111111111",
            None,
            None,
        )
        .unwrap();
    config
        .pin_runtime_entity(
            &workspace_id,
            &format!("ce://context-engine/rules/{earlier_id}"),
            None,
            None,
        )
        .unwrap();

    let rendered = config
        .render_pinned_rule_instructions(&workspace_id)
        .unwrap();
    assert!(rendered.contains("Earlier guidance."));
    assert!(rendered.contains("Later guidance."));
    assert!(!rendered.contains("11111111-1111-4111-8111-111111111111"));
    assert!(
        rendered.find("Earlier guidance.").unwrap()
            < rendered.find("Later guidance.").unwrap()
    );
}

#[test]
fn pinned_rule_render_fails_for_missing_rule() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    rule_api::RuleStore::open_or_init(&store_root.join(".rule")).unwrap();
    config
        .pin_runtime_entity(
            &init.context.workspace_session_id,
            "ce://context-engine/rules/22222222-2222-4222-8222-222222222222",
            None,
            None,
        )
        .unwrap();

    let error = config
        .render_pinned_rule_instructions(&init.context.workspace_session_id)
        .unwrap_err();
    assert!(matches!(error, SessionError::InvalidHookInput(_)));
}

#[test]
fn context_capture_persistence_isolation_is_byte_stable() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let capture = config
        .persist_capture(sample_request(
            "session-isolation",
            Some("conversation-isolation"),
            sample_time(),
            &["capture first"],
        ))
        .unwrap();
    let manifest_before = std::fs::read(&capture.paths.manifest_path).unwrap();
    let transcript_before =
        std::fs::read(&capture.paths.transcript_path).unwrap();

    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;
    config
        .pin_runtime_entity(
            &workspace_id,
            "ce://default/rules/084fd4e6-660b-4227-a13e-514edf44e393",
            Some("handoff".to_string()),
            None,
        )
        .unwrap();

    let manifest_after = std::fs::read(&capture.paths.manifest_path).unwrap();
    let transcript_after =
        std::fs::read(&capture.paths.transcript_path).unwrap();
    assert_eq!(manifest_before, manifest_after);
    assert_eq!(transcript_before, transcript_after);

    let runtime_paths =
        config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let runtime_before = std::fs::read(&runtime_paths.context_path).unwrap();

    config
        .persist_capture(sample_request(
            "session-isolation",
            Some("conversation-isolation"),
            sample_time_later(),
            &["capture first", "capture second"],
        ))
        .unwrap();

    let runtime_after = std::fs::read(&runtime_paths.context_path).unwrap();
    assert_eq!(runtime_before, runtime_after);
}

struct MockTicketResolver {
    missing_urn: String,
}

impl SessionTicketStateResolver for MockTicketResolver {
    fn resolve_ticket_state(
        &self,
        ticket_urn: &str,
    ) -> Result<Option<String>, String> {
        if ticket_urn == self.missing_urn {
            Err("ticket not found".to_string())
        } else {
            Ok(Some("in-review".to_string()))
        }
    }
}

#[test]
fn workflow_persists_mutation_and_reload() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let after_ticket = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-ticket".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Implement runtime model".to_string(),
                ticket_urn: Some(
                    "ce://default/tickets/412964a3-e1c3-47da-94ad-268ff20441c0"
                        .to_string(),
                ),
                cached_ticket_title: Some(
                    "Runtime session context".to_string(),
                ),
                validation_spec_id: None,
            },
        )
        .unwrap();
    assert_eq!(after_ticket.workflow.nodes.len(), 1);

    let after_action = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-action".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Write workflow tests".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    assert_eq!(after_action.workflow.nodes.len(), 2);

    let linked = config
        .workflow_add_edge(
            &workspace_id,
            "node-action",
            "node-ticket",
            SessionWorkflowEdgeKind::DependsOn,
        )
        .unwrap();
    assert_eq!(linked.workflow.edges.len(), 1);

    let updated = config
        .workflow_update_node_status(
            &workspace_id,
            "node-action",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();
    assert_eq!(
        updated
            .workflow
            .nodes
            .iter()
            .find(|node| node.node_id == "node-action")
            .unwrap()
            .status,
        SessionWorkflowNodeStatus::Done
    );

    let reloaded = config.read_runtime_context(&workspace_id).unwrap();
    assert_eq!(reloaded.workflow.nodes.len(), 2);
    assert_eq!(reloaded.workflow.edges.len(), 1);
}

#[test]
fn workflow_promotion_preserves_node_identity() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-temp".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "Investigate follow-up".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let promoted = config
        .workflow_promote_node_to_ticket(
            &workspace_id,
            "node-temp",
            "ce://default/tickets/70cd7056-c342-4433-ad60-5bc798f61aa6",
            Some("Workflow persistence".to_string()),
        )
        .unwrap();

    let node = promoted
        .workflow
        .nodes
        .iter()
        .find(|node| node.node_id == "node-temp")
        .unwrap();
    assert_eq!(node.kind, SessionWorkflowNodeKind::Ticket);
    assert_eq!(
        node.ticket_urn.as_deref(),
        Some("ce://default/tickets/70cd7056-c342-4433-ad60-5bc798f61aa6")
    );
}

#[test]
fn workflow_ticket_node_rejects_non_ticket_urn() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();

    let error = config
        .workflow_add_node(
            &init.context.workspace_session_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-ticket".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "bad type".to_string(),
                ticket_urn: Some(
                    "ce://default/specs/709f067a-21b6-41b6-8879-3cacef4bacaf"
                        .to_string(),
                ),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap_err();

    assert!(matches!(error, SessionError::InvalidHookInput(_)));
}

#[test]
fn workflow_snapshot_resolves_live_state_and_emits_missing_diagnostics() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-live".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Existing ticket".to_string(),
                ticket_urn: Some(
                    "ce://default/tickets/412964a3-e1c3-47da-94ad-268ff20441c0"
                        .to_string(),
                ),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    let missing_urn =
        "ce://default/tickets/deadbeef-dead-beef-dead-beefdeadbeef";
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-missing".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Missing ticket".to_string(),
                ticket_urn: Some(missing_urn.to_string()),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let snapshot = config
        .workflow_snapshot(
            &workspace_id,
            Some(&MockTicketResolver {
                missing_urn: missing_urn.to_string(),
            }),
        )
        .unwrap();

    assert!(
        snapshot
            .resolutions
            .iter()
            .any(|item| item.node_id == "node-live"
                && item.live_ticket_state.as_deref() == Some("in-review"))
    );
    assert!(
        snapshot
            .diagnostics
            .iter()
            .any(|diag| diag.node_id == "node-missing"
                && diag.code == "ticket-state-unavailable")
    );
}

#[test]
fn workflow_render_outputs_are_deterministic_and_escaped() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-a".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Run \"workflow\" check".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-b".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "Ticket fallback".to_string(),
                ticket_urn: Some(
                    "ce://default/tickets/deadbeef-dead-beef-dead-beefdeadbeef"
                        .to_string(),
                ),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_add_edge(
            &workspace_id,
            "node-a",
            "node-b",
            SessionWorkflowEdgeKind::DependsOn,
        )
        .unwrap();

    let resolver = MockTicketResolver {
        missing_urn:
            "ce://default/tickets/deadbeef-dead-beef-dead-beefdeadbeef"
                .to_string(),
    };

    let terminal_first = config
        .workflow_render_terminal(&workspace_id, Some(&resolver))
        .unwrap();
    let terminal_second = config
        .workflow_render_terminal(&workspace_id, Some(&resolver))
        .unwrap();
    assert_eq!(terminal_first, terminal_second);
    assert!(terminal_first.contains("ticket-state-unavailable"));
    assert!(terminal_first.contains("node-a"));
    assert!(terminal_first.contains("blockers=node-b"));

    let mermaid_first = config
        .workflow_render_mermaid(&workspace_id, Some(&resolver))
        .unwrap();
    let mermaid_second = config
        .workflow_render_mermaid(&workspace_id, Some(&resolver))
        .unwrap();
    assert_eq!(mermaid_first, mermaid_second);
    assert!(mermaid_first.starts_with("flowchart TD\n"));
    assert!(mermaid_first.contains("Run \\\"workflow\\\" check"));
    assert!(mermaid_first.contains("-->|depends_on|"));
}

#[test]
fn workflow_render_is_read_only_for_runtime_persistence() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-read-only".to_string()),
                kind: SessionWorkflowNodeKind::Checkpoint,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "render check".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let runtime_paths =
        config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let before = std::fs::read(&runtime_paths.context_path).unwrap();

    let _ = config
        .workflow_render_terminal(&workspace_id, None)
        .unwrap();
    let _ = config.workflow_render_mermaid(&workspace_id, None).unwrap();

    let after = std::fs::read(&runtime_paths.context_path).unwrap();
    assert_eq!(before, after);
}

#[test]
fn handoff_persists_before_render_and_resume_links_new_run() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let _rendered = config
        .render_handoff_terminal(
            &workspace_id,
            vec![crate::SessionValidationGate {
                validation_spec_id: "val-session-handoff-continuity"
                    .to_string(),
                required: true,
                outcome: Some("passed".to_string()),
            }],
            None,
        )
        .unwrap();

    let paths = config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let handoff_files = std::fs::read_dir(&paths.handoffs_dir)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(handoff_files.len(), 1);

    let handoff_path = handoff_files[0].path();
    let handoff: crate::SessionHandoffRecord =
        serde_json::from_slice(&std::fs::read(handoff_path).unwrap()).unwrap();
    assert_eq!(handoff.workspace_session_id, workspace_id);
    assert_eq!(handoff.outgoing_run_id, init.context.active_run_id);
    assert!(handoff.resume_command.contains(&workspace_id));
    assert!(handoff.resume_command.contains(&handoff.outgoing_run_id));

    let resumed = config
        .resume_workspace_context(&workspace_id, &handoff.outgoing_run_id)
        .unwrap();
    assert_eq!(resumed.context.workspace_session_id, workspace_id);
    assert_ne!(resumed.run.run_id, handoff.outgoing_run_id);
    assert_eq!(
        resumed.run.predecessor_run_id.as_deref(),
        Some(handoff.outgoing_run_id.as_str())
    );
}

#[test]
fn workflow_finish_enforces_gates_and_is_idempotent() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("required-node".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "must finish".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("optional-node".to_string()),
                kind: SessionWorkflowNodeKind::Checkpoint,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "may defer".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let blocked = config.finish_workflow(
        &workspace_id,
        vec![crate::SessionValidationGate {
            validation_spec_id: "val-session-workflow-finish".to_string(),
            required: true,
            outcome: Some("passed".to_string()),
        }],
        vec![],
        None,
    );
    assert!(matches!(
        blocked,
        Err(crate::SessionError::FinishBlocked { .. })
    ));

    config
        .workflow_update_node_status(
            &workspace_id,
            "required-node",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();
    config
        .workflow_update_node_status(
            &workspace_id,
            "optional-node",
            SessionWorkflowNodeStatus::Deferred,
            Some("not needed for this handoff".to_string()),
        )
        .unwrap();

    let blocked_validation = config.finish_workflow(
        &workspace_id,
        vec![crate::SessionValidationGate {
            validation_spec_id: "val-session-workflow-finish".to_string(),
            required: true,
            outcome: Some("failed".to_string()),
        }],
        vec!["optional-node".to_string()],
        None,
    );
    assert!(matches!(
        blocked_validation,
        Err(crate::SessionError::FinishBlocked { .. })
    ));

    let finished = config
        .finish_workflow(
            &workspace_id,
            vec![crate::SessionValidationGate {
                validation_spec_id: "val-session-workflow-finish".to_string(),
                required: true,
                outcome: Some("passed".to_string()),
            }],
            vec!["optional-node".to_string()],
            None,
        )
        .unwrap();
    assert!(!finished.already_finished);

    let finished_again = config
        .finish_workflow(
            &workspace_id,
            vec![crate::SessionValidationGate {
                validation_spec_id: "val-session-workflow-finish".to_string(),
                required: true,
                outcome: Some("passed".to_string()),
            }],
            vec!["optional-node".to_string()],
            None,
        )
        .unwrap();
    assert!(finished_again.already_finished);
    assert_eq!(finished_again.record.run_id, finished.record.run_id);
}

#[test]
fn workflow_finish_blocks_when_required_validation_guard_is_missing() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("required-validation".to_string()),
                kind: SessionWorkflowNodeKind::Validation,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "must pass".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: Some(
                    "val-session-workflow-finish".to_string(),
                ),
            },
        )
        .unwrap();
    config
        .workflow_update_node_status(
            &workspace_id,
            "required-validation",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();

    let error = config
        .finish_workflow(&workspace_id, vec![], vec![], None)
        .unwrap_err();
    assert!(matches!(error, SessionError::FinishBlocked { .. }));
}

// ── Remediation regression coverage ─────────────────────────────────────────

/// A resolver returning a caller-controlled state for a specific URN.
struct FixedStateResolver {
    urn: String,
    state: Option<String>,
}

impl SessionTicketStateResolver for FixedStateResolver {
    fn resolve_ticket_state(
        &self,
        ticket_urn: &str,
    ) -> Result<Option<String>, String> {
        if ticket_urn == self.urn {
            Ok(self.state.clone())
        } else {
            Err(format!("unexpected urn: {ticket_urn}"))
        }
    }
}

struct BlockingTerminalResolver {
    urn: String,
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl SessionTicketStateResolver for BlockingTerminalResolver {
    fn resolve_ticket_state(
        &self,
        ticket_urn: &str,
    ) -> Result<Option<String>, String> {
        if ticket_urn != self.urn {
            return Err(format!("unexpected urn: {ticket_urn}"));
        }
        self.entered.send(()).map_err(|error| error.to_string())?;
        self.release
            .lock()
            .map_err(|error| error.to_string())?
            .recv()
            .map_err(|error| error.to_string())?;
        Ok(Some("done".to_string()))
    }
}

fn test_store_for(store_root: &std::path::Path) -> test_api::TestStoreConfig {
    test_api::TestStoreConfig::new(store_root.join(".test"), "context-engine")
}

fn seed_validation_spec(
    store: &test_api::TestStoreConfig,
    spec_id: &str,
) {
    store
        .record_spec(&test_api::ValidationSpec::new(spec_id, spec_id))
        .unwrap();
}

fn seed_execution(
    store: &test_api::TestStoreConfig,
    exec_id: &str,
    spec_id: &str,
    outcome: test_api::ValidationOutcome,
) {
    let mut execution = test_api::ValidationExecution::new(
        exec_id,
        spec_id,
        outcome,
        chrono::Utc::now(),
    );
    execution.provenance.domain = Some("session-api".to_string());
    execution.provenance.operation = Some("workflow-finish".to_string());
    execution.provenance.run_id = Some("remediation-test-run".to_string());
    execution.links.spec_ids = vec![spec_id.to_string()];
    store.record_execution(&execution).unwrap();
}

fn add_required_validation_node(
    config: &SessionStoreConfig,
    workspace_id: &str,
    spec_id: &str,
) {
    config
        .workflow_add_node(
            workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("required-validation".to_string()),
                kind: SessionWorkflowNodeKind::Validation,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "authoritative gate".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: Some(spec_id.to_string()),
            },
        )
        .unwrap();
    config
        .workflow_update_node_status(
            workspace_id,
            "required-validation",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();
}

/// Critical: a caller submitting `passed` cannot override an authoritative
/// `failed` execution recorded in test-api.
#[test]
fn workflow_finish_rejects_caller_passed_when_authoritative_failed() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let spec_id = "val-remediation-authority";
    let test_store = test_store_for(&store_root);
    seed_validation_spec(&test_store, spec_id);
    seed_execution(
        &test_store,
        "exec-authority-failed",
        spec_id,
        test_api::ValidationOutcome::Failed,
    );

    add_required_validation_node(&config, &workspace_id, spec_id);

    let error = config
        .finish_workflow(
            &workspace_id,
            vec![crate::SessionValidationGate {
                validation_spec_id: spec_id.to_string(),
                required: true,
                outcome: Some("passed".to_string()),
            }],
            vec![],
            None,
        )
        .unwrap_err();
    assert!(matches!(error, SessionError::FinishBlocked { .. }));
}

/// Critical: a caller submitting `passed` cannot substitute for a missing
/// authoritative execution record.
#[test]
fn workflow_finish_rejects_caller_passed_when_no_execution_exists() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let spec_id = "val-remediation-missing-exec";
    let test_store = test_store_for(&store_root);
    seed_validation_spec(&test_store, spec_id);
    // Intentionally record no execution.

    add_required_validation_node(&config, &workspace_id, spec_id);

    let error = config
        .finish_workflow(
            &workspace_id,
            vec![crate::SessionValidationGate {
                validation_spec_id: spec_id.to_string(),
                required: true,
                outcome: Some("passed".to_string()),
            }],
            vec![],
            None,
        )
        .unwrap_err();
    assert!(matches!(error, SessionError::FinishBlocked { .. }));
}

/// Positive control: finish succeeds only when the authoritative execution is
/// `passed`, regardless of caller-provided outcomes.
#[test]
fn workflow_finish_accepts_authoritative_passed_execution() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let spec_id = "val-remediation-passed";
    let test_store = test_store_for(&store_root);
    seed_validation_spec(&test_store, spec_id);
    seed_execution(
        &test_store,
        "exec-authority-passed",
        spec_id,
        test_api::ValidationOutcome::Passed,
    );

    add_required_validation_node(&config, &workspace_id, spec_id);

    // Caller omits any gate; authority alone certifies the outcome.
    let finished = config
        .finish_workflow(&workspace_id, vec![], vec![], None)
        .unwrap();
    assert!(!finished.already_finished);
    assert!(finished.record.validation.iter().any(|gate| {
        gate.validation_spec_id == spec_id
            && gate.outcome.as_deref() == Some("passed")
    }));
}

/// Critical: a ticket node marked locally `Done` must not certify completion
/// when the live ticket state is non-terminal.
#[test]
fn workflow_finish_rejects_local_done_when_live_ticket_non_terminal() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root, "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let ticket_urn =
        "ce://context-engine/tickets/11111111-1111-4111-8111-111111111111";
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("ticket-node".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "ticket-backed".to_string(),
                ticket_urn: Some(ticket_urn.to_string()),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    // Local status is Done, but live state below is non-terminal.
    config
        .workflow_update_node_status(
            &workspace_id,
            "ticket-node",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();

    let resolver = FixedStateResolver {
        urn: ticket_urn.to_string(),
        state: Some("in-implementation".to_string()),
    };
    let error = config
        .finish_workflow(&workspace_id, vec![], vec![], Some(&resolver))
        .unwrap_err();
    assert!(matches!(error, SessionError::FinishBlocked { .. }));

    // Positive control: a live terminal state permits finish.
    let terminal = FixedStateResolver {
        urn: ticket_urn.to_string(),
        state: Some("done".to_string()),
    };
    let finished = config
        .finish_workflow(&workspace_id, vec![], vec![], Some(&terminal))
        .unwrap();
    assert!(!finished.already_finished);
}

/// High: production path — the real default resolver blocks finish when a
/// required ticket node references a non-terminal live ticket.
#[test]
fn workflow_finish_production_path_blocks_non_terminal_ticket() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let ticket_id =
        uuid::Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();
    let ticket_store = ticket_api::storage::TicketStore::open_or_init(
        &store_root.join(".ticket"),
    )
    .unwrap();
    ticket_store
        .create(
            Some(ticket_id),
            "tracker-improvement",
            Some("live ticket"),
            Some("in-implementation"),
            std::collections::BTreeMap::new(),
            None,
            None,
        )
        .unwrap();

    let ticket_urn = format!("ce://context-engine/tickets/{ticket_id}");
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("ticket-node".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "live ticket".to_string(),
                ticket_urn: Some(ticket_urn),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_update_node_status(
            &workspace_id,
            "ticket-node",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();

    // resolver=None exercises the real default resolver + store layout.
    let error = config
        .finish_workflow(&workspace_id, vec![], vec![], None)
        .unwrap_err();
    assert!(matches!(error, SessionError::FinishBlocked { .. }));
}

/// High: production path — an absent required ticket resolves to an unavailable
/// diagnostic that blocks finish (fail closed).
#[test]
fn workflow_finish_production_path_blocks_missing_ticket() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    // Initialize an empty ticket store so the resolver can open it, but the
    // referenced ticket does not exist.
    ticket_api::storage::TicketStore::open_or_init(&store_root.join(".ticket"))
        .unwrap();

    let ticket_urn =
        "ce://context-engine/tickets/33333333-3333-4333-8333-333333333333";
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("ticket-node".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "missing ticket".to_string(),
                ticket_urn: Some(ticket_urn.to_string()),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_update_node_status(
            &workspace_id,
            "ticket-node",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();

    let error = config
        .finish_workflow(&workspace_id, vec![], vec![], None)
        .unwrap_err();
    let SessionError::FinishBlocked { reason } = error else {
        panic!("expected FinishBlocked, got {error:?}");
    };
    assert!(
        reason.contains("unavailable"),
        "expected unavailable diagnostic, got: {reason}"
    );
}

/// High: cross-workspace ticket routing is rejected explicitly rather than
/// silently resolved against the wrong store.
#[test]
fn workflow_finish_rejects_cross_workspace_ticket_routing() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    ticket_api::storage::TicketStore::open_or_init(&store_root.join(".ticket"))
        .unwrap();

    // URN addresses a different workspace than the session's `context-engine`.
    let ticket_urn =
        "ce://other-workspace/tickets/44444444-4444-4444-8444-444444444444";
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("ticket-node".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "cross-workspace".to_string(),
                ticket_urn: Some(ticket_urn.to_string()),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_update_node_status(
            &workspace_id,
            "ticket-node",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();

    let error = config
        .finish_workflow(&workspace_id, vec![], vec![], None)
        .unwrap_err();
    let SessionError::FinishBlocked { reason } = error else {
        panic!("expected FinishBlocked, got {error:?}");
    };
    assert!(
        reason.contains("cross-workspace") || reason.contains("unavailable"),
        "expected routing rejection, got: {reason}"
    );
}

/// High: finished workspaces are immutable — every workflow/pin mutation is
/// rejected after finish.
#[test]
fn finished_workspace_rejects_all_mutations() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root, "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("seed-node".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "seed".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_update_node_status(
            &workspace_id,
            "seed-node",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();

    let finished = config
        .finish_workflow(&workspace_id, vec![], vec![], None)
        .unwrap();
    assert!(!finished.already_finished);

    // Adding a node after finish is rejected.
    let add_err = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("post-finish-node".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "should be rejected".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap_err();
    assert!(matches!(add_err, SessionError::WorkspaceFinished { .. }));

    // Updating a node status after finish is rejected.
    let status_err = config
        .workflow_update_node_status(
            &workspace_id,
            "seed-node",
            SessionWorkflowNodeStatus::InProgress,
            None,
        )
        .unwrap_err();
    assert!(matches!(status_err, SessionError::WorkspaceFinished { .. }));

    // Pinning after finish is rejected.
    let pin_err = config
        .pin_runtime_entity(
            &workspace_id,
            "ce://context-engine/tickets/55555555-5555-4555-8555-555555555555",
            None,
            None,
        )
        .unwrap_err();
    assert!(matches!(pin_err, SessionError::WorkspaceFinished { .. }));
}

/// A live lock cannot be stolen solely because its metadata is older than the
/// former 30-second stale threshold, and releasing it preserves the stable lock
/// file used by successor owners.
#[test]
fn aged_live_lock_blocks_second_owner_and_releases_safely() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root, "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let paths = config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let lock_path = paths.workspace_dir.join(".context.lock");

    let formerly_stale =
        (chrono::Utc::now() - chrono::Duration::seconds(31)).to_rfc3339();
    std::fs::write(&lock_path, formerly_stale).unwrap();
    let first_owner = config.acquire_runtime_lock(&workspace_id).unwrap();

    let conflict = match config.acquire_runtime_lock(&workspace_id) {
        Ok(_) => panic!("a second owner acquired the aged live lock"),
        Err(error) => error,
    };
    assert!(matches!(
        conflict,
        SessionError::RuntimeMutationConflict { .. }
    ));

    drop(first_owner);
    let successor = config.acquire_runtime_lock(&workspace_id).unwrap();
    assert!(lock_path.exists());
    drop(successor);
    assert!(lock_path.exists());

    let final_owner = config.acquire_runtime_lock(&workspace_id).unwrap();
    drop(final_owner);
}

#[cfg(windows)]
#[test]
fn failed_windows_replacement_preserves_previous_bytes() {
    use std::os::windows::fs::OpenOptionsExt;

    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path().join("durable.json");
    super::write_json(&path, &serde_json::json!({ "version": "old" })).unwrap();
    let previous = std::fs::read(&path).unwrap();

    let destination = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0x0000_0001 | 0x0000_0002)
        .open(&path)
        .unwrap();

    let error = super::write_json(
        &path,
        &serde_json::json!({ "version": "replacement" }),
    )
    .unwrap_err();
    assert!(matches!(error, SessionError::Io { .. }));
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    drop(destination);
}

#[test]
fn finish_excludes_mutation_init_and_resume_until_terminal_commit() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;
    let predecessor_run_id = init.run.run_id;
    let ticket_urn =
        "ce://context-engine/tickets/66666666-6666-4666-8666-666666666666";

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("terminal-ticket".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "terminal ticket".to_string(),
                ticket_urn: Some(ticket_urn.to_string()),
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let finish_config = config.clone();
    let finish_workspace_id = workspace_id.clone();
    let finish_thread = std::thread::spawn(move || {
        let resolver = BlockingTerminalResolver {
            urn: ticket_urn.to_string(),
            entered: entered_tx,
            release: std::sync::Mutex::new(release_rx),
        };
        finish_config.finish_workflow(
            &finish_workspace_id,
            vec![],
            vec![],
            Some(&resolver),
        )
    });

    entered_rx.recv().unwrap();

    let mutation_error = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("racing-mutation".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "must not interleave".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap_err();
    assert!(matches!(
        mutation_error,
        SessionError::RuntimeMutationConflict { .. }
    ));

    let init_error = config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(workspace_id.clone()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap_err();
    assert!(matches!(
        init_error,
        SessionError::RuntimeMutationConflict { .. }
    ));

    let resume_error = config
        .resume_workspace_context(&workspace_id, &predecessor_run_id)
        .unwrap_err();
    assert!(matches!(
        resume_error,
        SessionError::RuntimeMutationConflict { .. }
    ));

    release_tx.send(()).unwrap();
    let finished = finish_thread.join().unwrap().unwrap();
    assert!(!finished.already_finished);

    let context = config.read_runtime_context(&workspace_id).unwrap();
    assert_eq!(context.runs.len(), 1);
    assert!(
        context
            .workflow
            .nodes
            .iter()
            .all(|node| node.node_id != "racing-mutation")
    );
}

/// Helper: create a workspace with one required Action node marked done and
/// then finish it, returning the config and workspace id for immutability tests.
fn finished_workspace() -> (SessionStoreConfig, String, TempDir) {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root, "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("seed-node".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "seed".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_update_node_status(
            &workspace_id,
            "seed-node",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();
    let finished = config
        .finish_workflow(&workspace_id, vec![], vec![], None)
        .unwrap();
    assert!(!finished.already_finished);
    (config, workspace_id, tempdir)
}

/// High: resume/init lineage updates are immutable after finish. Appending a
/// new run to a finished workspace must be rejected under the lock, not
/// silently drift the run lineage of a terminal workspace.
#[test]
fn finished_workspace_rejects_resume_run_creation() {
    let (config, workspace_id, _tempdir) = finished_workspace();

    let resume_err = config
        .resume_workspace_context(&workspace_id, "any-predecessor")
        .unwrap_err();
    assert!(matches!(resume_err, SessionError::WorkspaceFinished { .. }));

    let force_err = config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(workspace_id.clone()),
            predecessor_run_id: None,
            force_new_run: true,
        })
        .unwrap_err();
    assert!(matches!(force_err, SessionError::WorkspaceFinished { .. }));
}

#[test]
fn finished_workspace_plain_init_is_read_only_and_byte_stable() {
    let (config, workspace_id, _tempdir) = finished_workspace();
    let paths = config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let active_path = config.active_workspace_session_path().unwrap();
    let context_before = std::fs::read(&paths.context_path).unwrap();
    let active_before = std::fs::read(&active_path).unwrap();

    let init = config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(workspace_id.clone()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();

    assert!(!init.created_workspace);
    assert!(!init.created_run);
    assert_eq!(init.context.workspace_session_id, workspace_id);
    assert_eq!(std::fs::read(&paths.context_path).unwrap(), context_before);
    assert_eq!(std::fs::read(active_path).unwrap(), active_before);
}

/// High: the finished-workspace check runs *under* the mutation lock. When a
/// finished workspace also has a live lock held, the mutation must fail with a
/// lock conflict (lock acquired first) rather than the finished error — proving
/// the ordering that closes the finish-versus-mutation race.
#[test]
fn finished_check_runs_under_mutation_lock() {
    let (config, workspace_id, _tempdir) = finished_workspace();

    let _lock = config.acquire_runtime_lock(&workspace_id).unwrap();

    let err = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("post-finish-locked".to_string()),
                kind: SessionWorkflowNodeKind::Action,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "blocked".to_string(),
                ticket_urn: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap_err();
    assert!(matches!(err, SessionError::RuntimeMutationConflict { .. }));
}

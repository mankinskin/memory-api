    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use crate::{
        CopilotHookMessage,
        CopilotHookPayload,
        PersistedSessionEvents,
        PersistedSessionManifest,
        PersistedSessionTranscript,
        SessionCaptureRequest,
        SessionError,
        SessionQuery,
        SessionRole,
        SessionStoreConfig,
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
            std::path::PathBuf::from(
                ".session/sessions/session-abc/session.json"
            )
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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );

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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );

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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );

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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );

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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );

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
            tool_arguments_json: Some("{\"path\":\"A\"}".to_string()),
            data_json: Some("{\"arguments\":{\"path\":\"A\"}}".to_string()),
            raw_event_json: Some("{\"type\":\"tool.execution_complete\",\"data\":{\"arguments\":{\"path\":\"A\"}}}".to_string()),
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
            tool_arguments_json: Some("{\"path\":\"B\"}".to_string()),
            data_json: Some("{\"arguments\":{\"path\":\"B\"}}".to_string()),
            raw_event_json: Some("{\"type\":\"tool.execution_complete\",\"data\":{\"arguments\":{\"path\":\"B\"}}}".to_string()),
        }];
        let plan = config
            .persist_capture(SessionCaptureRequest::copilot(second))
            .unwrap();

        let events_text =
            std::fs::read_to_string(&plan.paths.events_path).unwrap();
        let events: PersistedSessionEvents =
            serde_json::from_str(&events_text).unwrap();
        assert_eq!(events.events.len(), 2);
        assert!(events.events.iter().any(|event| {
            event
                .raw_event_json
                .as_deref()
                .unwrap_or("")
                .contains("\"path\":\"A\"")
        }));
        assert!(events.events.iter().any(|event| {
            event
                .raw_event_json
                .as_deref()
                .unwrap_or("")
                .contains("\"path\":\"B\"")
        }));
    }

    #[test]
    fn query_sessions_filters_by_text_and_metadata() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );

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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );
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
    fn check_in_worktree_creates_and_returns_new_assignment() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );
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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );
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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );
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
        assert_eq!(
            receipt.predecessor_session_id.as_deref(),
            Some("session-a")
        );
        assert_eq!(receipt.predecessor_path, Some(first_path));
        assert_eq!(
            predecessor.metadata.worktree.unwrap().status,
            SessionWorktreeStatus::Superseded
        );
    }

    #[test]
    fn check_in_worktree_rotates_when_existing_path_is_missing() {
        let tempdir = TempDir::new().unwrap();
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );
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
        let config = SessionStoreConfig::new(
            tempdir.path().join("store"),
            "context-engine",
        );
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

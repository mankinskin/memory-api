use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
};

use crate::{
    SessionLinks,
    SessionMetadata,
    SessionRecord,
    SessionWorktreeAssignment,
};

/// Writes a session's `session.json`/`transcript.json` directly to disk,
/// bypassing `check_in_worktree` (which always pairs a worktree assignment
/// with a `ticket_id`). This reproduces the historical on-disk shape the
/// backfill exists to repair: a `worktree` assignment present with no
/// `ticket_id` yet written.
fn write_raw_session(
    store_root: &std::path::Path,
    session_id: &str,
    ticket_id: Option<&str>,
    branch: Option<&str>,
    worktree_path: Option<PathBuf>,
) {
    let session_dir = store_root.join("sessions").join(session_id);
    fs::create_dir_all(&session_dir).unwrap();

    let worktree = if branch.is_some() || worktree_path.is_some() {
        Some(SessionWorktreeAssignment {
            path: worktree_path.unwrap_or_else(|| PathBuf::from("unused")),
            branch: branch.unwrap_or("unused").to_string(),
            allocation_mode: SessionWorktreeAllocationMode::New,
            status: SessionWorktreeStatus::Active,
            predecessor_session_id: None,
            predecessor_path: None,
        })
    } else {
        None
    };

    let record = SessionRecord {
        schema_version: SESSION_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        source: "test-fixture".to_string(),
        started_at: sample_time(),
        captured_at: sample_time(),
        metadata: SessionMetadata {
            workspace_slug: "context-engine".to_string(),
            conversation_id: None,
            agent_id: None,
            ticket_id: ticket_id.map(str::to_string),
            model: None,
            trigger: None,
            producer: None,
            copilot_version: None,
            vscode_version: None,
            protocol_version: None,
            worktree,
        },
        turns: vec![],
        links: SessionLinks::default(),
        track_id: None,
        anchor_ticket_id: None,
        parent_session_id: None,
        spawned_session_id: None,
        emitted_handoff_ids: vec![],
        picked_up_handoff_ids: vec![],
    };

    let manifest = PersistedSessionManifest::from(&record);
    let transcript = PersistedSessionTranscript::from(&record);
    fs::write(
        session_dir.join("session.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        session_dir.join("transcript.json"),
        serde_json::to_string_pretty(&transcript).unwrap(),
    )
    .unwrap();
}

/// Writes only `context.json`, mirroring the two deliberate corrupt fixture
/// entries in the real store (`session.json`/`transcript.json` absent).
fn write_corrupt_session(store_root: &std::path::Path, session_id: &str) {
    let session_dir = store_root.join("sessions").join(session_id);
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("context.json"),
        r#"{"schema_version":1,"workspace_session_id":"x"}"#,
    )
    .unwrap();
}

fn seed_ticket(ticket_store_root: &std::path::Path, ticket_id: uuid::Uuid) {
    let store =
        ticket_api::storage::TicketStore::open_or_init(ticket_store_root)
            .unwrap();
    store
        .create(
            Some(ticket_id),
            "task",
            Some("backfill fixture ticket"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
}

#[test]
fn backfill_links_via_agent_branch_shape() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");

    let ticket_id =
        uuid::Uuid::parse_str("aaaaaaaa-1111-4111-8111-111111111111")
            .unwrap();
    seed_ticket(&store_root.join(".ticket"), ticket_id);

    write_raw_session(
        &store_root,
        "session-branch",
        None,
        Some("agent/aaaaaaaa-some-slug"),
        Some(PathBuf::from("/tmp/worktrees/aaaaaaaa-some-slug")),
    );

    let dry = config.backfill_ticket_links(false).unwrap();
    assert_eq!(dry.total_sessions, 1);
    assert_eq!(dry.linked_via_branch, 1);
    assert_eq!(dry.total_would_link, 1);
    assert_eq!(
        config.read_session("session-branch").unwrap().metadata.ticket_id,
        None,
        "dry run must not write"
    );

    let written = config.backfill_ticket_links(true).unwrap();
    assert_eq!(written.linked_via_branch, 1);
    let record = config.read_session("session-branch").unwrap();
    assert_eq!(record.metadata.ticket_id.as_deref(), Some(ticket_id.to_string().as_str()));

    let matches = config
        .sessions_for_ticket(&ticket_id.to_string(), RelationStrength::Strict)
        .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].session_id, "session-branch");
}

#[test]
fn backfill_falls_back_to_worktree_path_when_branch_absent() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");

    let ticket_id =
        uuid::Uuid::parse_str("bbbbbbbb-2222-4222-8222-222222222222")
            .unwrap();
    seed_ticket(&store_root.join(".ticket"), ticket_id);

    write_raw_session(
        &store_root,
        "session-worktree-path",
        None,
        None,
        Some(PathBuf::from(
            "/repo/.worktrees/bbbbbbbb-some-other-slug",
        )),
    );

    let report = config.backfill_ticket_links(true).unwrap();
    assert_eq!(report.linked_via_branch, 0);
    assert_eq!(report.linked_via_worktree_path, 1);
    let record = config.read_session("session-worktree-path").unwrap();
    assert_eq!(
        record.metadata.ticket_id.as_deref(),
        Some(ticket_id.to_string().as_str())
    );
}

#[test]
fn backfill_branch_present_and_unmatched_does_not_fall_back() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");

    let ticket_id =
        uuid::Uuid::parse_str("cccccccc-3333-4333-8333-333333333333")
            .unwrap();
    seed_ticket(&store_root.join(".ticket"), ticket_id);

    // Branch does not match `agent/<8hex>-slug`; worktree_path does encode a
    // valid short id, but branch presence must not be skipped in favor of it
    // unless the branch itself fails to parse as the agent shape.
    write_raw_session(
        &store_root,
        "session-plain-branch",
        None,
        Some("main"),
        Some(PathBuf::from("/repo/.worktrees/cccccccc-some-slug")),
    );

    let report = config.backfill_ticket_links(true).unwrap();
    // "main" does not match the agent/<8hex>-slug shape, so the worktree_path
    // fallback is exercised and the ticket resolves.
    assert_eq!(report.linked_via_worktree_path, 1);
    let record = config.read_session("session-plain-branch").unwrap();
    assert_eq!(
        record.metadata.ticket_id.as_deref(),
        Some(ticket_id.to_string().as_str())
    );
}

#[test]
fn backfill_handoff_links_multiple_target_tickets() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");

    let ticket_one =
        uuid::Uuid::parse_str("dddddddd-4444-4444-8444-444444444444")
            .unwrap();
    let ticket_two =
        uuid::Uuid::parse_str("eeeeeeee-5555-4555-8555-555555555555")
            .unwrap();
    seed_ticket(&store_root.join(".ticket"), ticket_one);
    seed_ticket(&store_root.join(".ticket"), ticket_two);

    config
        .capture_copilot_hook(sample_payload(
            "session-handoff",
            Some("conversation-handoff"),
            sample_time(),
            &["Handed off for follow-up"],
        ))
        .unwrap();
    config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some("session-handoff".to_string()),
            ..Default::default()
        })
        .unwrap();
    config
        .create_handoff_record(
            "session-handoff",
            Some(SessionHandoffPackage {
                objective: "Follow up on two tickets".to_string(),
                target_tickets: vec![
                    ticket_one.to_string(),
                    ticket_two.to_string(),
                ],
                ..Default::default()
            }),
            vec![],
            None,
        )
        .unwrap();

    let report = config.backfill_ticket_links(true).unwrap();
    assert_eq!(report.linked_via_handoff, 2);
    assert!(report.handoff_already_at_mentioned);

    let record = config.read_session("session-handoff").unwrap();
    assert_eq!(record.metadata.ticket_id, None, "handoff writes linked tier, not strict");
    assert!(record.links.links_to_ticket(&ticket_one.to_string()));
    assert!(record.links.links_to_ticket(&ticket_two.to_string()));

    for ticket in [&ticket_one, &ticket_two] {
        let strict = config
            .sessions_for_ticket(&ticket.to_string(), RelationStrength::Strict)
            .unwrap();
        assert!(strict.is_empty());
        let linked = config
            .sessions_for_ticket(&ticket.to_string(), RelationStrength::Linked)
            .unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].session_id, "session-handoff");
    }
}

#[test]
fn backfill_skips_unresolvable_short_id_without_writing() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");

    // No ticket store at all: every short id is unresolvable.
    write_raw_session(
        &store_root,
        "session-unresolvable",
        None,
        Some("agent/ffffffff-no-such-ticket"),
        None,
    );

    let report = config.backfill_ticket_links(true).unwrap();
    assert_eq!(report.skipped_unresolvable_shortid, 1);
    assert_eq!(report.linked_via_branch, 0);
    assert_eq!(report.total_would_link, 0);
    assert_eq!(
        config.read_session("session-unresolvable").unwrap().metadata.ticket_id,
        None
    );
}

#[test]
fn backfill_skips_corrupt_entry_and_continues() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");

    let ticket_id =
        uuid::Uuid::parse_str("11111111-6666-4666-8666-666666666666")
            .unwrap();
    seed_ticket(&store_root.join(".ticket"), ticket_id);

    write_corrupt_session(&store_root, "session-corrupt");
    write_raw_session(
        &store_root,
        "session-good",
        None,
        Some("agent/11111111-good-slug"),
        None,
    );

    let report = config.backfill_ticket_links(true).unwrap();
    assert_eq!(report.total_sessions, 2);
    assert_eq!(report.skipped_corrupt, 1);
    assert_eq!(report.linked_via_branch, 1);
    let record = config.read_session("session-good").unwrap();
    assert_eq!(
        record.metadata.ticket_id.as_deref(),
        Some(ticket_id.to_string().as_str())
    );
}

#[test]
fn backfill_is_idempotent_and_never_overwrites_real_check_in() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");

    let ticket_id =
        uuid::Uuid::parse_str("22222222-7777-4777-8777-777777777777")
            .unwrap();
    seed_ticket(&store_root.join(".ticket"), ticket_id);

    // A real check-in already has ticket_id + worktree populated together;
    // its ticket_id must never be touched by the backfill.
    config
        .check_in_worktree(crate::SessionWorktreeCheckInRequest {
            session_id: "session-real-checkin".to_string(),
            owner_id: "agent-real".to_string(),
            ticket_id: "manually-assigned-ticket".to_string(),
            worktree_path: tempdir.path().join("wt-real"),
            branch: "agent/22222222-different-slug".to_string(),
            predecessor_session_id: None,
        })
        .unwrap();

    write_raw_session(
        &store_root,
        "session-branch",
        None,
        Some("agent/22222222-some-slug"),
        None,
    );

    let first = config.backfill_ticket_links(true).unwrap();
    assert_eq!(first.linked_via_branch, 1);
    assert_eq!(first.already_linked_untouched, 1);

    let second = config.backfill_ticket_links(true).unwrap();
    assert_eq!(second.linked_via_branch, 0);
    assert_eq!(second.total_would_link, 0);
    assert_eq!(second.already_linked_untouched, 2);

    assert_eq!(
        config
            .read_session("session-real-checkin")
            .unwrap()
            .metadata
            .ticket_id
            .as_deref(),
        Some("manually-assigned-ticket"),
        "real check-in ticket_id must never be overwritten"
    );
    assert_eq!(
        config.read_session("session-branch").unwrap().metadata.ticket_id.as_deref(),
        Some(ticket_id.to_string().as_str())
    );
}

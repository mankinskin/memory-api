use std::path::PathBuf;

use chrono::Utc;
use tempfile::tempdir;

use session_api::{
    CopilotHookMessage,
    CopilotHookPayload,
    SessionCaptureRequest,
    SessionRole,
    SessionStoreConfig,
};
use session_cli::{
    CliOutput,
    machine_output_format,
    parse_cli_from,
    run,
};

fn seed_session(
    config: &SessionStoreConfig,
    session_id: &str,
    agent: &str,
) {
    let payload = CopilotHookPayload {
        session_id: session_id.to_string(),
        workspace_slug: "default".to_string(),
        captured_at: Utc::now(),
        conversation_id: Some("conv-1".to_string()),
        agent_id: Some(agent.to_string()),
        model: None,
        trigger: None,
        messages: vec![
            CopilotHookMessage {
                role: SessionRole::User,
                content: "first turn body\nsecond line".to_string(),
                tool_name: None,
                captured_at: None,
                event_meta: None,
            },
            CopilotHookMessage {
                role: SessionRole::Assistant,
                content: "second turn body".to_string(),
                tool_name: None,
                captured_at: None,
                event_meta: None,
            },
        ],
        events: vec![],
        runtime: None,
    };
    config
        .persist_capture(SessionCaptureRequest::copilot(payload))
        .expect("seed session");
}

fn run_machine(args: &[&str]) -> serde_json::Value {
    let cli = parse_cli_from(args).expect("parse cli");
    match run(cli).expect("run command") {
        CliOutput::Machine(value, format) => {
            assert_eq!(format, machine_output_format(true, false).unwrap());
            value
        },
        CliOutput::Text(text) =>
            panic!("expected machine output, got text: {text}"),
    }
}

#[test]
fn check_in_and_lookup_roundtrip() {
    let dir = tempdir().unwrap();
    let store_root: PathBuf = dir.path().join(".session");
    let store_root_str = store_root.to_string_lossy().to_string();
    let worktree = dir.path().join("wt-1");
    let worktree_str = worktree.to_string_lossy().to_string();

    let receipt = run_machine(&[
        "session",
        "--json",
        "--store-root",
        &store_root_str,
        "check-in",
        "--session-id",
        "sess-1",
        "--owner-id",
        "agent-1",
        "--ticket-id",
        "ticket-1",
        "--worktree-path",
        &worktree_str,
        "--branch",
        "feature/x",
    ]);
    assert_eq!(receipt["session_id"], "sess-1");
    assert_eq!(receipt["branch"], "feature/x");

    let lookup = run_machine(&[
        "session",
        "--json",
        "--store-root",
        &store_root_str,
        "lookup",
        "--session-id",
        "sess-1",
    ]);
    assert_eq!(lookup["ticket_id"], "ticket-1");
    assert_eq!(lookup["owner_id"], "agent-1");
}

#[test]
fn query_returns_seeded_session() {
    let dir = tempdir().unwrap();
    let store_root = dir.path().join(".session");
    let store_root_str = store_root.to_string_lossy().to_string();
    let config =
        SessionStoreConfig::new(store_root.clone(), "default".to_string());
    seed_session(&config, "sess-q", "agent-q");

    let result = run_machine(&[
        "session",
        "--json",
        "--store-root",
        &store_root_str,
        "query",
        "--agent-id",
        "agent-q",
    ]);
    assert_eq!(result["count"], 1);
    assert_eq!(result["sessions"][0]["session_id"], "sess-q");
}

#[test]
fn peek_range_and_skeleton() {
    let dir = tempdir().unwrap();
    let store_root = dir.path().join(".session");
    let store_root_str = store_root.to_string_lossy().to_string();
    let config =
        SessionStoreConfig::new(store_root.clone(), "default".to_string());
    seed_session(&config, "sess-p", "agent-p");

    let range = run_machine(&[
        "session",
        "--json",
        "--store-root",
        &store_root_str,
        "peek-range",
        "--session-id",
        "sess-p",
        "--start",
        "1",
    ]);
    assert_eq!(range["total_turns"], 2);
    assert_eq!(range["start"], 1);
    assert_eq!(range["turns"].as_array().unwrap().len(), 1);

    let skeleton = run_machine(&[
        "session",
        "--json",
        "--store-root",
        &store_root_str,
        "peek-skeleton",
        "--session-id",
        "sess-p",
    ]);
    assert_eq!(skeleton["total_turns"], 2);
    assert_eq!(skeleton["entries"][0]["preview"], "first turn body");
}

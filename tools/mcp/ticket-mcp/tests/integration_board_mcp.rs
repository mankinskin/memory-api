//! Integration tests for the board MCP tools.
//!
//! These tests exercise the full board tool cycle (show → check-in →
//! heartbeat → check-out → show) via the `TicketServer` methods directly.

use std::collections::BTreeMap;
use std::path::Path;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;
use tempfile::TempDir;
use ticket_api::storage::store::TicketStore;

// Re-use the server under test.
use ticket_mcp::server::{
    BoardCheckInInput, BoardCheckOutInput, BoardCleanApplyInput, BoardCleanPreviewInput,
    BoardConfigureInput, BoardHeartbeatInput, BoardRenameFileInput, BoardShowInput,
    BoardUpdateFilesInput, TicketServer,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn make_sandbox() -> (TempDir, TicketServer) {
    let tmp = TempDir::new().expect("tempdir");
    let server = TicketServer::new(tmp.path().to_path_buf());
    (tmp, server)
}

/// Seed a single ticket in the store and return its full UUID string.
fn seed_ticket(store_root: &Path, title: &str) -> String {
    let store = TicketStore::open(store_root).expect("open store");
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some(title),
            Some("new"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket");
    ticket_id.to_string()
}

fn ws() -> String {
    "default".to_string()
}

// ── tests ────────────────────────────────────────────────────────────────────

/// Full board lifecycle: show → check-in → heartbeat → check-out → show.
#[tokio::test]
async fn board_full_lifecycle_mcp() {
    let (tmp, server) = make_sandbox();
    let ticket_id = seed_ticket(tmp.path(), "MCP board test ticket");

    // 1. board_show — empty board.
    let result = server
        .board_show(Parameters(BoardShowInput {
            workspace: ws(),
            agent_id: None,
        }))
        .await
        .expect("board_show ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["snapshot"]["active_count"], 0);

    // 2. board_check_in.
    let result = server
        .board_check_in(Parameters(BoardCheckInInput {
            workspace: ws(),
            ticket_id: ticket_id.clone(),
            agent_id: "test-agent".to_string(),
            intent: Some("implementing MCP board tools".to_string()),
            files: vec!["server.rs".to_string()],
            ttl_secs: Some(3600),
        }))
        .await
        .expect("board_check_in ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    let entry_id = json["entry"]["entry_id"]
        .as_str()
        .expect("entry_id present")
        .to_string();
    assert_eq!(json["entry"]["agent_id"], "test-agent");

    // 3. board_show — agent_id triggers heartbeat path.
    let result = server
        .board_show(Parameters(BoardShowInput {
            workspace: ws(),
            agent_id: Some("test-agent".to_string()),
        }))
        .await
        .expect("board_show with agent ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["snapshot"]["active_count"], 1);
    // heartbeat array should be populated since agent has an active entry.
    assert!(json["heartbeat"].is_array() || !json["heartbeat"].is_null());

    // 4. board_heartbeat — refresh TTL.
    let result = server
        .board_heartbeat(Parameters(BoardHeartbeatInput {
            workspace: ws(),
            entry_id: entry_id.clone(),
        }))
        .await
        .expect("board_heartbeat ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["entry"]["entry_id"], entry_id);

    // 5. board_check_out.
    let result = server
        .board_check_out(Parameters(BoardCheckOutInput {
            workspace: ws(),
            ticket_id: ticket_id.clone(),
            agent_id: Some("test-agent".to_string()),
            reason: Some("work done".to_string()),
        }))
        .await
        .expect("board_check_out ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");

    // 6. board_show — board should show no active entries.
    let result = server
        .board_show(Parameters(BoardShowInput {
            workspace: ws(),
            agent_id: None,
        }))
        .await
        .expect("board_show final ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["snapshot"]["active_count"], 0);
}

/// board_configure: read then patch max_wip.
#[tokio::test]
async fn board_configure_round_trip_mcp() {
    let (tmp, server) = make_sandbox();

    // Read current (defaults).
    let result = server
        .board_configure(Parameters(BoardConfigureInput {
            workspace: ws(),
            max_wip: None,
            stale_after_secs: None,
            completed_audit_window_secs: None,
        }))
        .await
        .expect("configure read ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    let original_max_wip = json["config"]["max_wip"].as_u64().expect("max_wip present");

    // Write a new value.
    let new_wip = original_max_wip + 2;
    let result = server
        .board_configure(Parameters(BoardConfigureInput {
            workspace: ws(),
            max_wip: Some(new_wip as u32),
            stale_after_secs: None,
            completed_audit_window_secs: None,
        }))
        .await
        .expect("configure write ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["config"]["max_wip"], new_wip);

    // Reading back confirms persistence.
    let result = server
        .board_configure(Parameters(BoardConfigureInput {
            workspace: ws(),
            max_wip: None,
            stale_after_secs: None,
            completed_audit_window_secs: None,
        }))
        .await
        .expect("configure read-back ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["config"]["max_wip"], new_wip);

    let _ = tmp; // keep alive
}

/// board_clean_preview + board_clean_apply cycle.
#[tokio::test]
async fn board_clean_preview_and_apply_mcp() {
    let (tmp, server) = make_sandbox();

    // Preview on an empty board — no candidates.
    let result = server
        .board_clean_preview(Parameters(BoardCleanPreviewInput {
            workspace: ws(),
            include_stale: Some(false),
        }))
        .await
        .expect("preview ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    let token = json["preview"]["token"]
        .as_str()
        .expect("token present")
        .to_string();
    assert_eq!(json["preview"]["entry_count"], 0);

    // Apply with the token — should succeed (no-op on empty board).
    let result = server
        .board_clean_apply(Parameters(BoardCleanApplyInput {
            workspace: ws(),
            token,
            include_stale: Some(false),
        }))
        .await
        .expect("apply ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["result"]["removed_count"], 0);

    let _ = tmp;
}

/// board_update_files and board_rename_file.
#[tokio::test]
async fn board_update_and_rename_file_mcp() {
    let (tmp, server) = make_sandbox();
    let ticket_id = seed_ticket(tmp.path(), "file ops ticket");

    // Check in with one file.
    server
        .board_check_in(Parameters(BoardCheckInInput {
            workspace: ws(),
            ticket_id: ticket_id.clone(),
            agent_id: "agent-files".to_string(),
            intent: None,
            files: vec!["a.rs".to_string()],
            ttl_secs: None,
        }))
        .await
        .expect("check_in ok");

    // Add another file.
    let result = server
        .board_update_files(Parameters(BoardUpdateFilesInput {
            workspace: ws(),
            ticket_id: ticket_id.clone(),
            agent_id: "agent-files".to_string(),
            add: vec!["b.rs".to_string()],
            remove: vec![],
        }))
        .await
        .expect("update_files ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    let files: Vec<&str> = json["entry"]["owned_files"]
        .as_array()
        .expect("owned_files array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(files.contains(&"b.rs"), "b.rs should be owned: {files:?}");

    // Rename b.rs → c.rs.
    let result = server
        .board_rename_file(Parameters(BoardRenameFileInput {
            workspace: ws(),
            ticket_id: ticket_id.clone(),
            agent_id: "agent-files".to_string(),
            old_path: "b.rs".to_string(),
            new_path: "c.rs".to_string(),
        }))
        .await
        .expect("rename_file ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    let files: Vec<&str> = json["entry"]["owned_files"]
        .as_array()
        .expect("owned_files array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(!files.contains(&"b.rs"), "b.rs should be gone: {files:?}");
    assert!(files.contains(&"c.rs"), "c.rs should be owned: {files:?}");

    let _ = tmp;
}

/// board_check_out without agent_id resolves from snapshot.
#[tokio::test]
async fn board_check_out_resolves_agent_from_snapshot_mcp() {
    let (tmp, server) = make_sandbox();
    let ticket_id = seed_ticket(tmp.path(), "auto-resolve ticket");

    server
        .board_check_in(Parameters(BoardCheckInInput {
            workspace: ws(),
            ticket_id: ticket_id.clone(),
            agent_id: "auto-agent".to_string(),
            intent: None,
            files: vec![],
            ttl_secs: None,
        }))
        .await
        .expect("check_in ok");

    // Check out without specifying agent_id.
    let result = server
        .board_check_out(Parameters(BoardCheckOutInput {
            workspace: ws(),
            ticket_id: ticket_id.clone(),
            agent_id: None,
            reason: None,
        }))
        .await
        .expect("check_out ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["entry"]["agent_id"], "auto-agent");

    let _ = tmp;
}

// ── utility ───────────────────────────────────────────────────────────────────

fn extract_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .find_map(|c| {
            if let rmcp::model::RawContent::Text(t) = &c.raw {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .expect("text content in result")
}

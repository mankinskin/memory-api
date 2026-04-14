//! Integration tests for the `ticket board` subcommand family.
//!
//! Each test runs against a fully isolated `Sandbox` and exercises the real
//! `ticket` binary (via `CARGO_BIN_EXE_ticket`). No internal Rust APIs are
//! called directly; all assertions are made on the JSON output.

mod common;

use common::{Sandbox, create_ticket};

// ---------------------------------------------------------------------------
// Full lifecycle: check-in → heartbeat → update-files → show → check-out → show
// ---------------------------------------------------------------------------

#[test]
fn board_full_lifecycle() {
    let s = Sandbox::new();
    let ticket_id = create_ticket(&s, "Board lifecycle ticket");

    // ── check-in ──────────────────────────────────────────────────────────────
    let check_in = s.ticket_json(&[
        "board",
        "check-in",
        &ticket_id,
        "--agent",
        "agent-alpha",
        "--intent",
        "implement feature X",
        "--file",
        "src/foo.rs",
        "--ttl-secs",
        "3600",
    ]);
    assert_eq!(check_in["status"], "ok", "check-in should succeed: {check_in}");
    assert_eq!(check_in["agent_id"], "agent-alpha");
    let entry_id = check_in["entry_id"].as_str().expect("entry_id must be present").to_string();
    assert_eq!(check_in["owned_files"].as_array().unwrap().len(), 1);

    // ── heartbeat ─────────────────────────────────────────────────────────────
    let heartbeat = s.ticket_json(&["board", "heartbeat", &entry_id]);
    assert_eq!(heartbeat["status"], "ok", "heartbeat should succeed: {heartbeat}");
    assert_eq!(heartbeat["entry_id"], entry_id.as_str());

    // ── update-files ──────────────────────────────────────────────────────────
    let update_files = s.ticket_json(&[
        "board",
        "update-files",
        &ticket_id,
        "--agent",
        "agent-alpha",
        "--add",
        "src/bar.rs",
        "--remove",
        "src/foo.rs",
    ]);
    assert_eq!(update_files["status"], "ok", "update-files should succeed: {update_files}");
    let files = update_files["owned_files"].as_array().unwrap();
    assert!(
        files.iter().any(|f| f.as_str() == Some("src/bar.rs")),
        "bar.rs should be present after update: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.as_str() == Some("src/foo.rs")),
        "foo.rs should be removed: {files:?}"
    );

    // ── show — assert active count = 1 ────────────────────────────────────────
    let show_active = s.ticket_json(&["board", "show"]);
    assert_eq!(show_active["status"], "ok", "show should succeed: {show_active}");
    assert_eq!(
        show_active["active_count"].as_u64().unwrap(),
        1,
        "active_count should be 1 before check-out"
    );
    let entries = show_active["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["agent_id"], "agent-alpha");
    assert_eq!(entries[0]["status"], "active");

    // ── check-out ─────────────────────────────────────────────────────────────
    let check_out = s.ticket_json(&[
        "board",
        "check-out",
        &ticket_id,
        "--agent",
        "agent-alpha",
        "--reason",
        "done with feature X",
    ]);
    assert_eq!(check_out["status"], "ok", "check-out should succeed: {check_out}");
    assert_eq!(check_out["agent_id"], "agent-alpha");

    // ── show — assert active count = 0 ────────────────────────────────────────
    let show_after = s.ticket_json(&["board", "show"]);
    assert_eq!(show_after["status"], "ok", "show after check-out should succeed");
    assert_eq!(
        show_after["active_count"].as_u64().unwrap(),
        0,
        "active_count should be 0 after check-out"
    );
}

// ---------------------------------------------------------------------------
// configure: read current config, then update and verify
// ---------------------------------------------------------------------------

#[test]
fn board_configure_round_trip() {
    let s = Sandbox::new();

    // Read default config.
    let cfg = s.ticket_json(&["board", "configure"]);
    assert_eq!(cfg["status"], "ok");
    let default_max_wip = cfg["config"]["max_wip"].as_u64().unwrap();
    assert!(default_max_wip > 0);

    // Patch max_wip.
    let new_max = (default_max_wip + 3) as u32;
    let patched = s.ticket_json(&[
        "board",
        "configure",
        "--max-wip",
        &new_max.to_string(),
    ]);
    assert_eq!(patched["status"], "ok");
    assert_eq!(patched["config"]["max_wip"].as_u64().unwrap(), new_max as u64);

    // Read back and verify persistence.
    let readback = s.ticket_json(&["board", "configure"]);
    assert_eq!(readback["config"]["max_wip"].as_u64().unwrap(), new_max as u64);
}

// ---------------------------------------------------------------------------
// clean: preview → apply removes completed entries
// ---------------------------------------------------------------------------

#[test]
fn board_clean_preview_and_apply() {
    let s = Sandbox::new();
    let ticket_id = create_ticket(&s, "Clean test ticket");

    // Check in.
    let ci = s.ticket_json(&[
        "board", "check-in", &ticket_id, "--agent", "agent-beta",
    ]);
    assert_eq!(ci["status"], "ok");

    // Check out (marks entry completed).
    let co = s.ticket_json(&[
        "board", "check-out", &ticket_id, "--agent", "agent-beta",
    ]);
    assert_eq!(co["status"], "ok");

    // Preview — should see 1 completed entry eligible for removal.
    let preview = s.ticket_json(&["board", "clean", "preview"]);
    assert_eq!(preview["status"], "ok");
    let token = preview["token"].as_str().expect("token must be present").to_string();
    assert!(preview["entry_count"].as_u64().unwrap() >= 1);

    // Apply.
    let apply = s.ticket_json(&["board", "clean", "apply", &token]);
    assert_eq!(apply["status"], "ok");
    assert!(apply["removed_count"].as_u64().unwrap() >= 1);
}

// ---------------------------------------------------------------------------
// rename-file: check-in with a file, then rename it
// ---------------------------------------------------------------------------

#[test]
fn board_rename_file() {
    let s = Sandbox::new();
    let ticket_id = create_ticket(&s, "Rename file ticket");

    s.ticket_json(&[
        "board",
        "check-in",
        &ticket_id,
        "--agent",
        "agent-gamma",
        "--file",
        "old_name.rs",
    ]);

    let renamed = s.ticket_json(&[
        "board",
        "rename-file",
        &ticket_id,
        "--agent",
        "agent-gamma",
        "--from",
        "old_name.rs",
        "--to",
        "new_name.rs",
    ]);
    assert_eq!(renamed["status"], "ok");
    let files = renamed["owned_files"].as_array().unwrap();
    assert!(files.iter().any(|f| f.as_str() == Some("new_name.rs")));
    assert!(!files.iter().any(|f| f.as_str() == Some("old_name.rs")));
}

// ---------------------------------------------------------------------------
// show --agent refreshes heartbeats for the caller's active entries
// ---------------------------------------------------------------------------

#[test]
fn board_show_with_agent_refreshes_heartbeat() {
    let s = Sandbox::new();
    let ticket_id = create_ticket(&s, "Heartbeat refresh ticket");

    let ci = s.ticket_json(&[
        "board", "check-in", &ticket_id, "--agent", "agent-delta",
    ]);
    assert_eq!(ci["status"], "ok");

    // show --agent should succeed and report the caller's active entry.
    let show = s.ticket_json(&["board", "show", "--agent", "agent-delta"]);
    assert_eq!(show["status"], "ok");
    assert_eq!(show["active_count"].as_u64().unwrap(), 1);
}

// ---------------------------------------------------------------------------
// update --board-check-in: update ticket and check in atomically
// ---------------------------------------------------------------------------

#[test]
fn update_with_board_check_in() {
    let s = Sandbox::new();
    let ticket_id = create_ticket(&s, "Update+check-in ticket");

    let result = s.ticket_json(&[
        "update",
        &ticket_id,
        "--to-state",
        "ready",
        "--board-check-in",
        "--board-agent",
        "agent-epsilon",
        "--board-intent",
        "refining the spec",
    ]);

    assert_eq!(result["status"], "ok");
    assert_eq!(result["state"], "ready");
    assert!(
        !result["board_entry"].is_null(),
        "board_entry should be present in update response"
    );
    assert_eq!(result["board_entry"]["agent_id"], "agent-epsilon");

    // Board show should confirm 1 active entry.
    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["active_count"].as_u64().unwrap(), 1);
}

// ---------------------------------------------------------------------------
// update --board-check-in without --board-agent should error
// ---------------------------------------------------------------------------

#[test]
fn update_board_check_in_without_agent_fails() {
    let s = Sandbox::new();
    let ticket_id = create_ticket(&s, "Missing agent ticket");

    let (code, _stderr) = s.ticket_fail(&["update", &ticket_id, "--board-check-in"]);
    assert!(code != 0, "should exit non-zero when --board-agent is missing");
}

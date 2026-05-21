//! Integration tests for the `ticket board` subcommand family.
//!
//! Each test runs against a fully isolated `Sandbox` and exercises the real
//! `ticket` binary (via `CARGO_BIN_EXE_ticket`). No internal Rust APIs are
//! called directly; all assertions are made on the JSON output.

mod common;

use std::process::Command;

use chrono::{
    DateTime,
    Datelike,
    Timelike,
    Utc,
};

use common::{
    Sandbox,
    create_ticket,
};

const TICKET: &str = env!("CARGO_BIN_EXE_ticket");

fn format_expected_board_created_at(created_at: &str) -> String {
    let timestamp = DateTime::parse_from_rfc3339(created_at)
        .expect("board recommendation created_at should be RFC3339")
        .with_timezone(&Utc);
    let month = timestamp.format("%b");

    format!(
        "{month} {} {} {:02}:{:02} UTC",
        timestamp.day(),
        timestamp.year(),
        timestamp.hour(),
        timestamp.minute()
    )
}

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
    assert_eq!(
        check_in["status"], "ok",
        "check-in should succeed: {check_in}"
    );
    assert_eq!(check_in["agent_id"], "agent-alpha");
    let entry_id = check_in["entry_id"]
        .as_str()
        .expect("entry_id must be present")
        .to_string();
    assert_eq!(check_in["owned_files"].as_array().unwrap().len(), 1);

    // ── heartbeat ─────────────────────────────────────────────────────────────
    let heartbeat = s.ticket_json(&["board", "heartbeat", &entry_id]);
    assert_eq!(
        heartbeat["status"], "ok",
        "heartbeat should succeed: {heartbeat}"
    );
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
    assert_eq!(
        update_files["status"], "ok",
        "update-files should succeed: {update_files}"
    );
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
    assert_eq!(
        show_active["status"], "ok",
        "show should succeed: {show_active}"
    );
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
    assert_eq!(
        check_out["status"], "ok",
        "check-out should succeed: {check_out}"
    );
    assert_eq!(check_out["agent_id"], "agent-alpha");

    // ── show — assert active count = 0 ────────────────────────────────────────
    let show_after = s.ticket_json(&["board", "show"]);
    assert_eq!(
        show_after["status"], "ok",
        "show after check-out should succeed"
    );
    assert_eq!(
        show_after["active_count"].as_u64().unwrap(),
        0,
        "active_count should be 0 after check-out"
    );
    assert!(
        show_after["entries"].as_array().unwrap().is_empty(),
        "completed entries should no longer appear in board show"
    );

    let history = s.ticket_json(&["board", "history"]);
    assert_eq!(history["status"], "ok");
    let history_entries = history["entries"].as_array().unwrap();
    assert_eq!(history_entries.len(), 1);
    assert_eq!(history_entries[0]["ticket_id"], ticket_id.as_str());
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
    assert_eq!(
        patched["config"]["max_wip"].as_u64().unwrap(),
        new_max as u64
    );

    // Read back and verify persistence.
    let readback = s.ticket_json(&["board", "configure"]);
    assert_eq!(
        readback["config"]["max_wip"].as_u64().unwrap(),
        new_max as u64
    );
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
        "board",
        "check-in",
        &ticket_id,
        "--agent",
        "agent-beta",
    ]);
    assert_eq!(ci["status"], "ok");

    // Check out (marks entry completed).
    let co = s.ticket_json(&[
        "board",
        "check-out",
        &ticket_id,
        "--agent",
        "agent-beta",
    ]);
    assert_eq!(co["status"], "ok");

    // Preview — should see 1 completed entry eligible for removal.
    let preview = s.ticket_json(&["board", "clean", "preview"]);
    assert_eq!(preview["status"], "ok");
    let token = preview["token"]
        .as_str()
        .expect("token must be present")
        .to_string();
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
        "board",
        "check-in",
        &ticket_id,
        "--agent",
        "agent-delta",
    ]);
    assert_eq!(ci["status"], "ok");

    // show --agent should succeed and report the caller's active entry.
    let show = s.ticket_json(&["board", "show", "--agent", "agent-delta"]);
    assert_eq!(show["status"], "ok");
    assert_eq!(show["active_count"].as_u64().unwrap(), 1);
}

#[test]
fn board_show_recommends_next_work_when_board_is_empty() {
    let s = Sandbox::new();
    let next_ticket = create_ticket(&s, "Top ticket for board suggestions");

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");
    assert!(show["current_work"].as_array().unwrap().is_empty());

    let recommended = show["recommended_next"].as_array().unwrap();
    assert!(!recommended.is_empty(), "board should recommend ready work");
    assert_eq!(recommended[0]["ticket_id"], next_ticket.as_str());
    assert_eq!(recommended[0]["title"], "Top ticket for board suggestions");

    let actions = show["actions"].as_array().unwrap();
    assert!(
        !actions.is_empty(),
        "board should include actionable guidance"
    );

    let human = show["human"].as_str().unwrap();
    assert!(human.contains("Current Work:"));
    assert!(human.contains("(no active board entries)"));
    assert!(human.contains("Next Up:"));
    assert!(human.contains("Top ticket for board suggestions"));
}

#[test]
fn board_show_lists_ten_recommendations_when_available() {
    let s = Sandbox::new();
    let mut ticket_ids = Vec::new();

    for index in 1..=12 {
        let title = format!("Candidate {:02}", index);
        ticket_ids.push((title.clone(), create_ticket(&s, &title)));
    }

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");

    let recommended = show["recommended_next"].as_array().unwrap();
    assert_eq!(
        recommended.len(),
        10,
        "board show should surface 10 next-up entries when available"
    );
    assert_eq!(recommended[0]["title"], "Candidate 12");
    assert_eq!(recommended[9]["title"], "Candidate 03");

    let human = show["human"].as_str().unwrap();
    assert!(human.contains("Candidate 12"));
    assert!(human.contains("Candidate 03"));
    assert!(!human.contains("Candidate 02"));
    assert!(!human.contains("Candidate 01"));
}

#[test]
fn board_show_text_output_stops_after_dashboard() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let next_ticket = create_ticket(&s, "Top ticket for board suggestions");

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root)
        .args(["board", "show"])
        .output()
        .expect("failed to run ticket board show");

    assert!(
        out.status.success(),
        "board show should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout)
        .expect("board show stdout should be valid UTF-8");
    let short_ticket = &next_ticket[..8];

    assert!(stdout.contains("Board: [0/5 active]"));
    assert!(stdout.contains("Next Up:"));
    assert!(stdout.contains(&format!(
        "#1  {short_ticket}  Top ticket for board suggestions"
    )));
    assert!(stdout.contains(&format!("ticket_id: {next_ticket}")));
    assert!(!stdout.contains("board_show ok"));
    assert!(!stdout.contains("[recommended_next]"));
}

#[test]
fn next_text_output_uses_pretty_card_format() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let next_ticket = create_ticket(&s, "Top ticket for next suggestions");

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root)
        .args(["next"])
        .output()
        .expect("failed to run ticket next");

    assert!(
        out.status.success(),
        "next should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout)
        .expect("next stdout should be valid UTF-8");
    let short_ticket = &next_ticket[..8];

    assert!(stdout.contains("next ok"));
    assert!(stdout.contains("count: 1"));
    assert!(stdout.contains("Next Up:"));
    assert!(stdout.contains(&format!(
        "#1  {short_ticket}  Top ticket for next suggestions"
    )));
    assert!(stdout.contains(&format!("ticket_id: {next_ticket}")));
    assert!(!stdout.contains("[items]"));
}

#[test]
fn next_and_board_prefer_newer_tickets_before_older_ones() {
    let s = Sandbox::new();
    let older = create_ticket(&s, "Alpha older candidate");
    let newer = create_ticket(&s, "Zulu newer candidate");

    for ticket_id in [&older, &newer] {
        let ready =
            s.ticket_json(&["update", ticket_id, "--to-state", "ready"]);
        assert_eq!(ready["status"], "ok");

        let priority =
            s.ticket_json(&["update", ticket_id, "--field", "priority=high"]);
        assert_eq!(priority["status"], "ok");
    }

    let next = s.ticket_json(&["next"]);
    assert_eq!(next["status"], "ok");
    assert!(
        next.get("board").is_none(),
        "ticket next should not embed a duplicate board summary"
    );
    let next_items = next["items"].as_array().unwrap();
    assert!(next_items.len() >= 2);
    assert_eq!(next_items[0]["id"], newer.as_str());
    assert_eq!(next_items[1]["id"], older.as_str());

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");
    let recommended = show["recommended_next"].as_array().unwrap();
    assert!(recommended.len() >= 2);
    assert_eq!(recommended[0]["ticket_id"], newer.as_str());
    assert_eq!(recommended[1]["ticket_id"], older.as_str());
}

#[test]
fn next_and_board_prefer_more_dependees_before_newer_tickets() {
    let s = Sandbox::new();
    let older_more_dependees = create_ticket(&s, "Alpha older blocker");
    let newer_fewer_dependees = create_ticket(&s, "Zulu newer blocker");
    let dependent_one = create_ticket(&s, "Dependent one");
    let dependent_two = create_ticket(&s, "Dependent two");

    for ticket_id in [&older_more_dependees, &newer_fewer_dependees] {
        let ready =
            s.ticket_json(&["update", ticket_id, "--to-state", "ready"]);
        assert_eq!(ready["status"], "ok");

        let priority =
            s.ticket_json(&["update", ticket_id, "--field", "priority=high"]);
        assert_eq!(priority["status"], "ok");
    }

    for dependent in [&dependent_one, &dependent_two] {
        let linked = s.ticket_json(&[
            "link",
            "--from",
            dependent,
            "--to",
            &older_more_dependees,
            "--kind",
            "depends_on",
        ]);
        assert_eq!(linked["status"], "ok");
    }

    let next = s.ticket_json(&["next"]);
    assert_eq!(next["status"], "ok");
    let next_items = next["items"].as_array().unwrap();
    assert!(next_items.len() >= 2);
    assert_eq!(next_items[0]["id"], older_more_dependees.as_str());
    assert_eq!(next_items[0]["dependees"], 2);
    assert_eq!(next_items[1]["id"], newer_fewer_dependees.as_str());
    assert_eq!(next_items[1]["dependees"], 0);

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");
    let recommended = show["recommended_next"].as_array().unwrap();
    assert!(recommended.len() >= 2);
    assert_eq!(recommended[0]["ticket_id"], older_more_dependees.as_str());
    assert_eq!(recommended[0]["dependees"], 2);
    let first_created_at = recommended[0]["created_at"]
        .as_str()
        .expect("board show should preserve created_at");
    let pretty_created_at = format_expected_board_created_at(first_created_at);
    assert_eq!(recommended[1]["ticket_id"], newer_fewer_dependees.as_str());
    assert_eq!(recommended[1]["dependees"], 0);

    let human = show["human"].as_str().unwrap();
    assert!(human.contains(&format!(
        "#1  {}  Alpha older blocker",
        &older_more_dependees[..8]
    )));
    assert!(human.contains(
        "state: ready  priority: high  dependees: 2  dependency_count: 0"
    ));
    assert!(human.contains(&format!("created_at: {pretty_created_at}")));
    assert!(human.contains(&format!("ticket_id: {older_more_dependees}")));
    assert!(!human.contains("DEPENDEES"));
    assert!(!human.contains(first_created_at));
}

#[test]
fn board_show_excludes_history_and_board_history_lists_recent_completions() {
    let s = Sandbox::new();
    let active_ticket = create_ticket(&s, "Active board work");
    let completed_ticket = create_ticket(&s, "Recently completed board work");
    let next_ticket = create_ticket(&s, "Ready board follow-up");

    let ready = s.ticket_json(&["update", &next_ticket, "--to-state", "ready"]);
    assert_eq!(ready["status"], "ok");

    let active = s.ticket_json(&[
        "board",
        "check-in",
        &active_ticket,
        "--agent",
        "agent-zeta",
        "--intent",
        "active implementation",
    ]);
    assert_eq!(active["status"], "ok");

    let completed = s.ticket_json(&[
        "board",
        "check-in",
        &completed_ticket,
        "--agent",
        "agent-eta",
        "--intent",
        "wrap up",
    ]);
    assert_eq!(completed["status"], "ok");
    let checked_out = s.ticket_json(&[
        "board",
        "check-out",
        &completed_ticket,
        "--agent",
        "agent-eta",
        "--reason",
        "validated and handed off",
    ]);
    assert_eq!(checked_out["status"], "ok");

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");

    let current_work = show["current_work"].as_array().unwrap();
    assert_eq!(current_work.len(), 1);
    assert_eq!(current_work[0]["ticket_id"], active_ticket.as_str());
    assert_eq!(current_work[0]["title"], "Active board work");
    assert_eq!(
        show["entries"].as_array().unwrap().len(),
        1,
        "completed entries should be excluded from board show"
    );

    let recommended = show["recommended_next"].as_array().unwrap();
    assert!(!recommended.is_empty());
    assert_eq!(recommended[0]["ticket_id"], next_ticket.as_str());
    assert_eq!(recommended[0]["title"], "Ready board follow-up");

    let human = show["human"].as_str().unwrap();
    let current_index = human.find("Current Work:").unwrap();
    let next_index = human.find("Next Up:").unwrap();
    assert!(current_index < next_index);
    assert!(human.contains("Active board work"));
    assert!(human.contains("Ready board follow-up"));
    assert!(!human.contains("Recent Completions:"));

    let history = s.ticket_json(&["board", "history"]);
    assert_eq!(history["status"], "ok");
    let history_entries = history["entries"].as_array().unwrap();
    assert_eq!(history_entries.len(), 1);
    assert_eq!(history_entries[0]["ticket_id"], completed_ticket.as_str());
    assert_eq!(history_entries[0]["title"], "Recently completed board work");

    let history_human = history["human"].as_str().unwrap();
    assert!(history_human.contains("Completed Work:"));
    assert!(history_human.contains("Recently completed board work"));
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

    let (code, _stderr) =
        s.ticket_fail(&["update", &ticket_id, "--board-check-in"]);
    assert!(
        code != 0,
        "should exit non-zero when --board-agent is missing"
    );
}

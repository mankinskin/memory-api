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
fn next_with_root_returns_actionable_remaining_blockers() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Shared prerequisite");
    let blocked_dependent = create_ticket(&s, "Reachable blocked dependent");
    let actionable_blocker = create_ticket(&s, "Actionable blocker");
    let blocked_blocker = create_ticket(&s, "Blocked blocker");
    let nested_prerequisite = create_ticket(&s, "Nested prerequisite");
    let unrelated = create_ticket(&s, "Unrelated actionable work");

    for (from, to) in [
        (&blocked_dependent, &root),
        (&blocked_dependent, &actionable_blocker),
        (&blocked_dependent, &blocked_blocker),
        (&blocked_blocker, &nested_prerequisite),
    ] {
        let linked = s.ticket_json(&[
            "link",
            "--from",
            from,
            "--to",
            to,
            "--kind",
            "depends_on",
        ]);
        assert_eq!(linked["status"], "ok");
    }

    let next = s.ticket_json(&["next", &root]);
    assert_eq!(next["status"], "ok");
    assert_eq!(next["root"]["id"], root.as_str());
    assert_eq!(next["reachable_dependents"], 1);
    assert_eq!(next["blocked_dependents"], 1);
    assert_eq!(next["remaining_blocker_count"], 2);
    assert_eq!(next["count"], 1);

    let items = next["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], actionable_blocker.as_str());
    assert_ne!(items[0]["id"], blocked_dependent.as_str());
    assert_ne!(items[0]["id"], blocked_blocker.as_str());
    assert_ne!(items[0]["id"], nested_prerequisite.as_str());
    assert_ne!(items[0]["id"], unrelated.as_str());
}

#[test]
fn next_with_root_text_output_shows_root_scope() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Completed prerequisite");
    let blocker = create_ticket(&s, "Scoped blocker");
    let dependent = create_ticket(&s, "Blocked dependent");

    for to in [&root, &blocker] {
        let linked = s.ticket_json(&[
            "link",
            "--from",
            &dependent,
            "--to",
            to,
            "--kind",
            "depends_on",
        ]);
        assert_eq!(linked["status"], "ok");
    }

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root)
        .args(["next", &root])
        .output()
        .expect("failed to run ticket next with root scope");

    assert!(
        out.status.success(),
        "next with root should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout =
        String::from_utf8(out.stdout).expect("next stdout should be UTF-8");
    let short_ticket = &blocker[..8];

    assert!(stdout.contains("next ok"));
    assert!(stdout.contains("[root]"));
    assert!(stdout.contains(&format!("id: {root}")));
    assert!(stdout.contains("reachable_dependents: 1"));
    assert!(stdout.contains("blocked_dependents: 1"));
    assert!(stdout.contains("remaining_blocker_count: 1"));
    assert!(stdout.contains("Next Up:"));
    assert!(stdout.contains(&format!("#1  {short_ticket}  Scoped blocker")));
    assert!(stdout.contains(&format!("ticket_id: {blocker}")));
    assert!(!stdout.contains("[items]"));
}

#[test]
fn blockers_returns_nested_dependency_tree() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Root blocker target");
    let direct_leaf = create_ticket(&s, "Direct frontier leaf");
    let nested_parent = create_ticket(&s, "Nested parent");
    let nested_leaf = create_ticket(&s, "Nested frontier leaf");

    for (from, to) in [
        (&root, &nested_parent),
        (&root, &direct_leaf),
        (&nested_parent, &nested_leaf),
    ] {
        let linked = s.ticket_json(&[
            "link",
            "--from",
            from,
            "--to",
            to,
            "--kind",
            "depends_on",
        ]);
        assert_eq!(linked["status"], "ok");
    }

    let blockers = s.ticket_json(&["blockers", &root]);
    assert_eq!(blockers["status"], "ok");
    assert_eq!(blockers["kind"], "blockers");
    assert_eq!(blockers["root"]["id"], root.as_str());
    assert_eq!(blockers["root"]["remaining_blocker_count"], 2);
    assert_eq!(blockers["root"]["unresolved_frontier_leaf_count"], 2);

    let children = blockers["root"]["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0]["id"], direct_leaf.as_str());
    assert_eq!(children[1]["id"], nested_parent.as_str());
    assert_eq!(children[0]["is_frontier"], true);
    assert_eq!(children[1]["children"][0]["id"], nested_leaf.as_str());

    let frontier_items = blockers["frontier_items"].as_array().unwrap();
    assert_eq!(blockers["frontier_count"], 2);
    assert_eq!(frontier_items.len(), 2);
    assert_eq!(frontier_items[0]["id"], direct_leaf.as_str());
    assert_eq!(frontier_items[1]["id"], nested_leaf.as_str());
}

#[test]
fn unblocked_by_returns_nested_unlock_tree_and_frontier_summary() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Shared prerequisite");
    let actionable = create_ticket(&s, "Direct dependent");
    let extra_blocker = create_ticket(&s, "Other blocker");
    let still_blocked = create_ticket(&s, "Still blocked dependent");
    let transitive = create_ticket(&s, "Transitive dependent");

    let priority = s.ticket_json(&[
        "update",
        &still_blocked,
        "--field",
        "priority=critical",
    ]);
    assert_eq!(priority["status"], "ok");

    for (from, to) in [
        (&actionable, &root),
        (&still_blocked, &root),
        (&still_blocked, &extra_blocker),
        (&transitive, &actionable),
    ] {
        let linked = s.ticket_json(&[
            "link",
            "--from",
            from,
            "--to",
            to,
            "--kind",
            "depends_on",
        ]);
        assert_eq!(linked["status"], "ok");
    }

    let unblocked = s.ticket_json(&["unblocked-by", &root]);
    assert_eq!(unblocked["status"], "ok");
    assert_eq!(unblocked["kind"], "unblocked_by");
    assert_eq!(unblocked["root"]["id"], root.as_str());
    assert_eq!(unblocked["reachable_dependents"], 3);
    assert_eq!(unblocked["blocked_dependents"], 2);

    let children = unblocked["root"]["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0]["id"], actionable.as_str());
    assert_eq!(children[0]["is_frontier"], true);
    assert_eq!(children[0]["children"][0]["id"], transitive.as_str());
    assert_eq!(children[1]["id"], still_blocked.as_str());
    assert_eq!(children[1]["remaining_blocker_count"], 1);
    assert_eq!(children[1]["priority"], "critical");

    let frontier_items = unblocked["frontier_items"].as_array().unwrap();
    assert_eq!(unblocked["frontier_count"], 2);
    assert_eq!(frontier_items.len(), 2);
    assert_eq!(frontier_items[0]["id"], actionable.as_str());
    assert_eq!(frontier_items[0]["remaining_blocker_count"], 0);
    assert!(frontier_items[0].get("became_actionable_at").is_some());
    assert!(frontier_items[0].get("last_blocker_progress_at").is_some());
    assert_eq!(frontier_items[1]["id"], still_blocked.as_str());
    assert_eq!(frontier_items[1]["remaining_blocker_count"], 1);
}

#[test]
fn blockers_text_output_shows_nested_tree_and_frontier_summary() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Root blocker target");
    let direct_leaf = create_ticket(&s, "Direct frontier leaf");
    let nested_parent = create_ticket(&s, "Nested parent");
    let nested_leaf = create_ticket(&s, "Nested frontier leaf");

    for (from, to) in [
        (&root, &nested_parent),
        (&root, &direct_leaf),
        (&nested_parent, &nested_leaf),
    ] {
        let linked = s.ticket_json(&[
            "link",
            "--from",
            from,
            "--to",
            to,
            "--kind",
            "depends_on",
        ]);
        assert_eq!(linked["status"], "ok");
    }

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root)
        .args(["blockers", &root])
        .output()
        .expect("failed to run ticket blockers");

    assert!(
        out.status.success(),
        "blockers should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout =
        String::from_utf8(out.stdout).expect("blockers stdout should be valid UTF-8");

    assert!(stdout.contains("blockers ok"));
    assert!(stdout.contains("frontier_count: 2"));
    assert!(stdout.contains("Blocker Tree:"));
    assert!(stdout.contains(&root[..8]));
    assert!(stdout.contains(&direct_leaf[..8]));
    assert!(stdout.contains(&nested_parent[..8]));
    assert!(stdout.contains(&nested_leaf[..8]));
    assert!(stdout.contains("Frontier Leaves:"));
    assert!(!stdout.contains("[root]"));
    assert!(!stdout.contains("[frontier_items]"));
}

#[test]
fn unblocked_by_text_output_shows_nested_tree_and_frontier_summary() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Completed prerequisite");
    let actionable = create_ticket(&s, "Unlocked dependent");
    let extra_blocker = create_ticket(&s, "Still-open blocker");
    let blocked = create_ticket(&s, "Still blocked dependent");

    let linked = s.ticket_json(&[
        "link",
        "--from",
        &actionable,
        "--to",
        &root,
        "--kind",
        "depends_on",
    ]);
    assert_eq!(linked["status"], "ok");

    for to in [&root, &extra_blocker] {
        let linked = s.ticket_json(&[
            "link",
            "--from",
            &blocked,
            "--to",
            to,
            "--kind",
            "depends_on",
        ]);
        assert_eq!(linked["status"], "ok");
    }

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root)
        .args(["unblocked-by", &root])
        .output()
        .expect("failed to run ticket unblocked-by");

    assert!(
        out.status.success(),
        "unblocked-by should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout)
        .expect("unblocked-by stdout should be valid UTF-8");

    assert!(stdout.contains("unblocked_by ok"));
    assert!(stdout.contains("reachable_dependents: 2"));
    assert!(stdout.contains("blocked_dependents: 1"));
    assert!(stdout.contains("frontier_count: 2"));
    assert!(stdout.contains("Unlock Tree:"));
    assert!(stdout.contains(&root[..8]));
    assert!(stdout.contains(&actionable[..8]));
    assert!(stdout.contains(&blocked[..8]));
    assert!(stdout.contains("Frontier Leaves:"));
    assert!(stdout.contains("remaining_blockers: 1"));
    assert!(!stdout.contains("[root]"));
    assert!(!stdout.contains("[frontier_items]"));
    assert!(!stdout.contains("Still Blocked:"));
    assert!(!stdout.contains("Next Up:"));
}

#[test]
fn blockers_reports_empty_leaf_cleanly_in_json_and_text() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Isolated blocker root");

    let blockers = s.ticket_json(&["blockers", &root]);
    assert_eq!(blockers["status"], "ok");
    assert_eq!(blockers["kind"], "blockers");
    assert_eq!(blockers["root"]["id"], root.as_str());
    assert_eq!(blockers["root"]["remaining_blocker_count"], 0);
    assert_eq!(blockers["root"]["unresolved_frontier_leaf_count"], 1);
    assert_eq!(blockers["root"]["is_frontier"], true);
    assert!(blockers["root"]["children"].as_array().unwrap().is_empty());
    assert_eq!(blockers["frontier_count"], 1);
    let frontier_items = blockers["frontier_items"].as_array().unwrap();
    assert_eq!(frontier_items.len(), 1);
    assert_eq!(frontier_items[0]["id"], root.as_str());

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root)
        .args(["blockers", &root])
        .output()
        .expect("failed to run ticket blockers for empty leaf case");

    assert!(
        out.status.success(),
        "blockers leaf case should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout =
        String::from_utf8(out.stdout).expect("blockers leaf stdout should be valid UTF-8");

    assert!(stdout.contains("blockers ok"));
    assert!(stdout.contains("frontier_count: 1"));
    assert!(stdout.contains("Blocker Tree:"));
    assert!(stdout.contains(&root[..8]));
    assert!(stdout.contains("Frontier Leaves:"));
    assert!(stdout.contains("#1"));
    assert!(!stdout.contains("[root]"));
    assert!(!stdout.contains("[frontier_items]"));
}

#[test]
fn unblocked_by_reports_empty_leaf_cleanly_in_json_and_text() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Isolated prerequisite root");

    let unblocked = s.ticket_json(&["unblocked-by", &root]);
    assert_eq!(unblocked["status"], "ok");
    assert_eq!(unblocked["kind"], "unblocked_by");
    assert_eq!(unblocked["root"]["id"], root.as_str());
    assert_eq!(unblocked["reachable_dependents"], 0);
    assert_eq!(unblocked["blocked_dependents"], 0);
    assert_eq!(unblocked["root"]["is_frontier"], false);
    assert!(unblocked["root"]["children"].as_array().unwrap().is_empty());
    assert_eq!(unblocked["frontier_count"], 1);
    let frontier_items = unblocked["frontier_items"].as_array().unwrap();
    assert_eq!(frontier_items.len(), 1);
    assert_eq!(frontier_items[0]["id"], root.as_str());

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root)
        .args(["unblocked-by", &root])
        .output()
        .expect("failed to run ticket unblocked-by for empty leaf case");

    assert!(
        out.status.success(),
        "unblocked-by leaf case should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout)
        .expect("unblocked-by leaf stdout should be valid UTF-8");

    assert!(stdout.contains("unblocked_by ok"));
    assert!(stdout.contains("reachable_dependents: 0"));
    assert!(stdout.contains("blocked_dependents: 0"));
    assert!(stdout.contains("frontier_count: 1"));
    assert!(stdout.contains("Unlock Tree:"));
    assert!(stdout.contains(&root[..8]));
    assert!(stdout.contains("Frontier Leaves:"));
    assert!(stdout.contains("#1"));
    assert!(!stdout.contains("[root]"));
    assert!(!stdout.contains("[frontier_items]"));
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
fn next_and_board_prefer_recently_actionable_candidates_and_surface_timing_metadata() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let recently_actionable = create_ticket(&s, "Alpha recently actionable");
    let steadier_newer = create_ticket(&s, "Zulu steady ready work");
    let transient_blocker = create_ticket(&s, "Transient blocker");

    for ticket_id in [&recently_actionable, &steadier_newer] {
        let ready =
            s.ticket_json(&["update", ticket_id, "--to-state", "ready"]);
        assert_eq!(ready["status"], "ok");

        let priority =
            s.ticket_json(&["update", ticket_id, "--field", "priority=high"]);
        assert_eq!(priority["status"], "ok");
    }

    for state in ["ready", "in-implementation", "in-review"] {
        let updated = s.ticket_json(&[
            "update",
            &transient_blocker,
            "--to-state",
            state,
        ]);
        assert_eq!(updated["status"], "ok");
    }

    let linked = s.ticket_json(&[
        "link",
        "--from",
        &recently_actionable,
        "--to",
        &transient_blocker,
        "--kind",
        "depends_on",
    ]);
    assert_eq!(linked["status"], "ok");

    let closed = s.ticket_json(&["close", &transient_blocker]);
    assert_eq!(closed["status"], "ok");

    let next = s.ticket_json(&["next"]);
    assert_eq!(next["status"], "ok");
    let items = next["items"].as_array().unwrap();
    assert!(items.len() >= 2, "expected at least two candidates: {items:?}");
    assert_eq!(items[0]["id"], recently_actionable.as_str());
    assert_eq!(items[1]["id"], steadier_newer.as_str());
    assert!(items[0]["became_actionable_at"].as_str().is_some());
    assert!(items[0]["last_blocker_progress_at"].is_null());
    assert!(items[1]["became_actionable_at"].as_str().is_some());

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");
    let recommended = show["recommended_next"].as_array().unwrap();
    assert!(recommended.len() >= 2);
    assert_eq!(recommended[0]["ticket_id"], recently_actionable.as_str());
    assert_eq!(recommended[1]["ticket_id"], steadier_newer.as_str());
    assert!(recommended[0]["became_actionable_at"].as_str().is_some());
    assert!(recommended[0]["last_blocker_progress_at"].is_null());
    assert!(recommended[1]["became_actionable_at"].as_str().is_some());
}

#[test]
fn next_and_board_promote_convergence_before_unrelated_ready_work() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");

    let lagging_prerequisite = create_ticket(&s, "Lagging prerequisite");
    let unrelated_ready = create_ticket(&s, "Unrelated ready work");
    let advanced_dependent = create_ticket(&s, "Advanced dependent");

    let unrelated_ready_state = s.ticket_json(&[
        "update",
        &unrelated_ready,
        "--to-state",
        "ready",
    ]);
    assert_eq!(unrelated_ready_state["status"], "ok");

    for state in ["ready", "in-implementation", "in-review"] {
        let dependent_state = s.ticket_json(&[
            "update",
            &advanced_dependent,
            "--to-state",
            state,
        ]);
        assert_eq!(dependent_state["status"], "ok");
    }

    for ticket_id in [&lagging_prerequisite, &unrelated_ready] {
        let priority =
            s.ticket_json(&["update", ticket_id, "--field", "priority=high"]);
        assert_eq!(priority["status"], "ok");
    }

    let linked = s.ticket_json(&[
        "link",
        "--from",
        &advanced_dependent,
        "--to",
        &lagging_prerequisite,
        "--kind",
        "depends_on",
    ]);
    assert_eq!(linked["status"], "ok");

    let next = s.ticket_json(&["next"]);
    assert_eq!(next["status"], "ok");
    let next_items = next["items"].as_array().unwrap();
    assert!(next_items.len() >= 2, "expected two next items: {next_items:?}");
    assert_eq!(next_items[0]["id"], lagging_prerequisite.as_str());
    assert_eq!(
        next_items[0]["max_affected_dependent_state"],
        "in-review"
    );
    assert_eq!(next_items[0]["affected_reverse_dependent_reach"], 1);
    assert_eq!(next_items[0]["dependency_state_gap"], 3);
    assert_eq!(next_items[1]["id"], unrelated_ready.as_str());

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");
    let recommended = show["recommended_next"].as_array().unwrap();
    assert!(
        recommended.len() >= 2,
        "expected two board recommendations: {recommended:?}"
    );
    assert_eq!(recommended[0]["ticket_id"], lagging_prerequisite.as_str());
    assert_eq!(recommended[1]["ticket_id"], unrelated_ready.as_str());
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

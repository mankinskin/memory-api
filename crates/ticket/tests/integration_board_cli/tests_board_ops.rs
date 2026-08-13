use super::*;

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

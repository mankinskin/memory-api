use super::*;

fn next_and_board_prefer_newer_tickets_before_older_ones() {
    let s = Sandbox::new();
    let older = create_ticket(&s, "Alpha older candidate");
    let newer = create_ticket(&s, "Zulu newer candidate");

    for ticket_id in [&older, &newer] {
        let ready =
            s.ticket_json(&["update", ticket_id, "--to-state", "planned"]);
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
            s.ticket_json(&["update", ticket_id, "--to-state", "planned"]);
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
    assert_eq!(next_items[0]["dependee_count"], 2);
    assert_eq!(next_items[1]["id"], newer_fewer_dependees.as_str());
    assert_eq!(next_items[1]["dependee_count"], 0);

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");
    let recommended = show["recommended_next"].as_array().unwrap();
    assert!(recommended.len() >= 2);
    assert_eq!(recommended[0]["ticket_id"], older_more_dependees.as_str());
    assert_eq!(recommended[0]["dependee_count"], 2);
    let first_created_at = recommended[0]["created_at"]
        .as_str()
        .expect("board show should preserve created_at");
    let pretty_created_at = format_expected_board_created_at(first_created_at);
    assert_eq!(recommended[1]["ticket_id"], newer_fewer_dependees.as_str());
    assert_eq!(recommended[1]["dependee_count"], 0);

    let human = show["human"].as_str().unwrap();
    assert!(human.contains(&format!(
        "#1  {}  Alpha older blocker",
        &older_more_dependees[..8]
    )));
    assert!(human.contains(
        "state: planned  priority: high  effort: -  dependee_count: 2  dependency_count: 0"
    ));
    assert!(human.contains(&format!("created_at: {pretty_created_at}")));
    assert!(human.contains(&format!("ticket_id: {older_more_dependees}")));
    assert!(!human.contains("DEPENDEES"));
    assert!(!human.contains(first_created_at));
    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");

    // --- typed struct assertions (Finding 2) ---
    // Deserialise the recommended_next array into `NextTicketEntry` values so
    // that the compiler catches any field-name or type changes at build time
    // rather than at runtime via string matching.
    let entries: Vec<NextTicketEntry> = serde_json::from_value(
        show["recommended_next"].clone(),
    )
    .expect("recommended_next should deserialise into Vec<NextTicketEntry>");
    assert!(
        entries.len() >= 2,
        "expected at least 2 recommended entries"
    );
    assert_eq!(
        entries[0],
        NextTicketEntry {
            ticket_id: older_more_dependees.clone(),
            state: Some("planned".into()),
            priority: "high".into(),
            effort: None,
            dependee_count: 2,
            dependency_count: 0,
        }
    );
    assert_eq!(
        entries[1],
        NextTicketEntry {
            ticket_id: newer_fewer_dependees.clone(),
            state: Some("planned".into()),
            priority: "high".into(),
            effort: None,
            dependee_count: 0,
            dependency_count: 0,
        }
    );

    // --- human rendering spot-checks ---
    // These verify that the human output is formatted correctly; the
    // data content is already covered by the typed assertions above.
    let first_created_at = show["recommended_next"][0]["created_at"]
        .as_str()
        .expect("board show should preserve created_at");
    let pretty_created_at = format_expected_board_created_at(first_created_at);
    let human = show["human"].as_str().unwrap();
    assert!(human.contains(&format!(
        "#1  {}  Alpha older blocker",
        &older_more_dependees[..8]
    )));
    assert!(human.contains(&format!("created_at: {pretty_created_at}")));
    assert!(human.contains(&format!("ticket_id: {older_more_dependees}")));
    assert!(!human.contains("DEPENDEES"));
    assert!(!human.contains(first_created_at));
}

#[test]
fn next_and_board_prefer_recently_actionable_candidates_and_surface_timing_metadata()
 {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let recently_actionable = create_ticket(&s, "Alpha recently actionable");
    let steadier_newer = create_ticket(&s, "Zulu steady ready work");
    let transient_blocker = create_ticket(&s, "Transient blocker");

    for ticket_id in [&recently_actionable, &steadier_newer] {
        let ready =
            s.ticket_json(&["update", ticket_id, "--to-state", "planned"]);
        assert_eq!(ready["status"], "ok");

        let priority =
            s.ticket_json(&["update", ticket_id, "--field", "priority=high"]);
        assert_eq!(priority["status"], "ok");
    }

    for state in ["planned", "in-implementation", "in-review"] {
        let updated =
            s.ticket_json(&["update", &transient_blocker, "--to-state", state]);
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
    assert!(
        items.len() >= 2,
        "expected at least two candidates: {items:?}"
    );
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
    assert!(recommended[0].get("last_blocker_progress_at").is_none());
    assert!(recommended[1]["became_actionable_at"].as_str().is_some());
}

#[test]
fn next_and_board_promote_convergence_before_unrelated_ready_work() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");

    let lagging_prerequisite = create_ticket(&s, "Lagging prerequisite");
    let unrelated_ready = create_ticket(&s, "Unrelated ready work");
    let advanced_dependent = create_ticket(&s, "Advanced dependent");

    let unrelated_ready_state =
        s.ticket_json(&["update", &unrelated_ready, "--to-state", "planned"]);
    assert_eq!(unrelated_ready_state["status"], "ok");

    for state in ["planned", "in-implementation", "in-review"] {
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
    assert!(
        next_items.len() >= 2,
        "expected two next items: {next_items:?}"
    );
    assert_eq!(next_items[0]["id"], lagging_prerequisite.as_str());
    assert_eq!(next_items[0]["max_affected_dependent_state"], "in-review");
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

    let ready =
        s.ticket_json(&["update", &next_ticket, "--to-state", "planned"]);
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
        "planned",
        "--board-check-in",
        "--board-agent",
        "agent-epsilon",
        "--board-intent",
        "refining the spec",
    ]);

    assert_eq!(result["status"], "ok");
    assert_eq!(result["state"], "planned");
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

#[test]
fn board_help_uses_canonical_common_flag_names() {
    let show_help = Command::new(TICKET)
        .args(["board", "show", "--help"])
        .output()
        .expect("show help should run");
    assert!(show_help.status.success());
    let show_stdout = String::from_utf8_lossy(&show_help.stdout);
    assert!(show_stdout.contains("--agent <AGENT>"));
    assert!(!show_stdout.contains("--agent-id"));

    let check_in_help = Command::new(TICKET)
        .args(["board", "check-in", "--help"])
        .output()
        .expect("check-in help should run");
    assert!(check_in_help.status.success());
    let check_in_stdout = String::from_utf8_lossy(&check_in_help.stdout);
    assert!(check_in_stdout.contains("--agent <AGENT>"));
    assert!(check_in_stdout.contains("--file <FILES>"));
    assert!(check_in_stdout.contains("--ttl-secs <TTL_SECS>"));
    assert!(!check_in_stdout.contains("--agent-id"));
    assert!(!check_in_stdout.contains("--files"));
    assert!(!check_in_stdout.contains("--ttl <TTL>"));

    let rename_help = Command::new(TICKET)
        .args(["board", "rename-file", "--help"])
        .output()
        .expect("rename-file help should run");
    assert!(rename_help.status.success());
    let rename_stdout = String::from_utf8_lossy(&rename_help.stdout);
    assert!(rename_stdout.contains("--agent <AGENT>"));
    assert!(rename_stdout.contains("--from <FROM>"));
    assert!(rename_stdout.contains("--to <TO>"));
    assert!(!rename_stdout.contains("--agent-id"));
    assert!(!rename_stdout.contains("--old-path"));
    assert!(!rename_stdout.contains("--new-path"));
}

#[test]
fn board_long_form_aliases_still_parse() {
    let s = Sandbox::new();
    s.ticket_json(&["init"]);
    let ticket_id = create_ticket(&s, "Board alias compatibility ticket");

    let check_in = s.ticket_json(&[
        "board",
        "check-in",
        &ticket_id,
        "--agent-id",
        "agent-zeta",
        "--intent",
        "keep docs-compatible",
        "--files",
        "src/legacy.rs",
        "--ttl",
        "3600",
    ]);
    assert_eq!(check_in["status"], "ok");
    assert_eq!(check_in["agent_id"], "agent-zeta");

    let show = s.ticket_json(&["board", "show", "--agent-id", "agent-zeta"]);
    assert_eq!(show["status"], "ok");
    assert_eq!(show["active_count"].as_u64().unwrap(), 1);

    let rename = s.ticket_json(&[
        "board",
        "rename-file",
        &ticket_id,
        "--agent-id",
        "agent-zeta",
        "--old-path",
        "src/legacy.rs",
        "--new-path",
        "src/current.rs",
    ]);
    assert_eq!(rename["status"], "ok");
    let files = rename["owned_files"].as_array().unwrap();
    assert!(files.iter().any(|f| f.as_str() == Some("src/current.rs")));
    assert!(!files.iter().any(|f| f.as_str() == Some("src/legacy.rs")));
}

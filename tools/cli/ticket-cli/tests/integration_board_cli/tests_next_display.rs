use super::*;

fn board_show_text_output_stops_after_dashboard() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let next_ticket = create_ticket(&s, "Top ticket for board suggestions");

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root())
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
fn board_show_immediate_actions_include_state_and_escaped_title() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let title = "Fix \"isometric\" layout defaults";
    let next_ticket = create_ticket(&s, title);
    let short_ticket = &next_ticket[..8];
    let expected_action = format!(
        "Board is clear. Start new {short_ticket} \"Fix \\\"isometric\\\" layout defaults\" next."
    );

    let show = s.ticket_json(&["board", "show"]);
    assert_eq!(show["status"], "ok");
    assert_eq!(show["actions"][0], expected_action.as_str());

    let human = show["human"].as_str().unwrap();
    assert!(human.contains("Immediate Actions:"));
    assert!(human.contains(&expected_action));
}

#[test]
fn next_text_output_uses_pretty_card_format() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let next_ticket = create_ticket(&s, "Top ticket for next suggestions");

    let out = Command::new(TICKET)
        .arg("--index-root")
        .arg(&s.index_root())
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
fn next_with_root_returns_unblocked_blocker_leaves() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Root ticket to unblock");
    let direct_blocker = create_ticket(&s, "Direct actionable blocker");
    let intermediate_blocker = create_ticket(&s, "Intermediate blocked blocker");
    let nested_leaf = create_ticket(&s, "Nested actionable blocker");
    let unrelated = create_ticket(&s, "Unrelated actionable work");

    for (from, to) in [
        (&root, &direct_blocker),
        (&root, &intermediate_blocker),
        (&intermediate_blocker, &nested_leaf),
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
    assert_eq!(next["reachable_dependencies"], 3);
    assert_eq!(next["blocked_dependencies"], 1);
    assert_eq!(next["remaining_blocker_count"], 3);
    assert_eq!(next["frontier_count"], 2);
    assert_eq!(next["count"], 2);

    let items = next["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let item_ids = items
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(item_ids.contains(direct_blocker.as_str()));
    assert!(item_ids.contains(nested_leaf.as_str()));
    assert!(!item_ids.contains(intermediate_blocker.as_str()));
    assert!(!item_ids.contains(unrelated.as_str()));

    let tree = &next["blocker_tree"];
    assert_eq!(tree["id"], root.as_str());
    assert_eq!(tree["remaining_blocker_count"], 2);
    let children = tree["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
}

#[test]
fn next_with_root_text_output_shows_root_scope() {
    let s = Sandbox::new();
    assert_eq!(s.ticket_json(&["init"])["status"], "ok");
    let root = create_ticket(&s, "Root ticket");
    let blocker = create_ticket(&s, "Scoped blocker");
    let nested = create_ticket(&s, "Nested blocker");
    let intermediate = create_ticket(&s, "Intermediate blocker");

    for (from, to) in [(&root, &blocker), (&root, &intermediate), (&intermediate, &nested)] {
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
        .arg(&s.index_root())
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
    assert!(stdout.contains("reachable_dependencies: 3"));
    assert!(stdout.contains("blocked_dependencies: 1"));
    assert!(stdout.contains("remaining_blocker_count: 3"));
    assert!(stdout.contains("Blocker Tree:"));
    assert!(stdout.contains("Next Up:"));
    assert!(stdout.contains("Scoped blocker"));
    assert!(stdout.contains(&short_ticket));
    assert!(stdout.contains(&format!("ticket_id: {blocker}")));
    assert!(!stdout.contains("[items]"));
}


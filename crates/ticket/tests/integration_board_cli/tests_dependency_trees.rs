use super::*;

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
        .arg(&s.index_root())
        .args(["blockers", &root])
        .output()
        .expect("failed to run ticket blockers");

    assert!(
        out.status.success(),
        "blockers should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout)
        .expect("blockers stdout should be valid UTF-8");

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
        .arg(&s.index_root())
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
        .arg(&s.index_root())
        .args(["blockers", &root])
        .output()
        .expect("failed to run ticket blockers for empty leaf case");

    assert!(
        out.status.success(),
        "blockers leaf case should succeed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let stdout = String::from_utf8(out.stdout)
        .expect("blockers leaf stdout should be valid UTF-8");

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
        .arg(&s.index_root())
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

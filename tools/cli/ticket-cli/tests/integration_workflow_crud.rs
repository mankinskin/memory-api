//! Sandboxed integration tests — ticket CRUD, search, scan, and lease workflows.
//!
//! Every test creates an isolated `Sandbox` backed by its own temp directory.
//! All operations go through the real `ticket` binary; no internal Rust APIs
//! are called directly.  JSON output is asserted via field access so tests
//! are independent of human-readable formatting.

mod common;

use common::{Sandbox, create_ticket};

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

#[test]
fn create_and_get_roundtrip() {
    let s = Sandbox::new();

    let created = s.ticket_json(&[
        "create",
        "--title",
        "Fix login bug",
        "--type",
        "tracker-improvement",
    ]);
    assert_eq!(created["status"], "ok");
    assert_eq!(created["type"], "tracker-improvement");

    let id = created["id"].as_str().expect("id must be present");

    let got = s.ticket_json(&["get", id]);
    assert_eq!(got["status"], "ok");
    assert_eq!(got["ticket"]["id"], id);
    assert_eq!(got["ticket"]["fields"]["title"], "Fix login bug");
    assert_eq!(got["ticket"]["fields"]["state"], "new");
    assert_eq!(got["ticket"]["fields"]["type"], "tracker-improvement");
    // Interview metadata is schema-supported but optional, so it should not be
    // auto-initialized for tickets without an active interview.
    assert!(got["ticket"]["fields"]["interview_file_type"].is_null());
    assert!(got["ticket"]["fields"]["interview_files"].is_null());
}

#[test]
fn create_multiple_and_list_all() {
    let s = Sandbox::new();

    for title in &["Alpha feature", "Beta fix", "Gamma refactor"] {
        let r = s.ticket_json(&["create", "--title", title, "--type", "tracker-improvement"]);
        assert_eq!(r["status"], "ok");
    }

    let list = s.ticket_json(&["list"]);
    assert_eq!(list["status"], "ok");
    assert_eq!(list["count"].as_u64().unwrap(), 3);

    let titles: Vec<&str> = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["title"].as_str())
        .collect();

    assert!(titles.contains(&"Alpha feature"));
    assert!(titles.contains(&"Beta fix"));
    assert!(titles.contains(&"Gamma refactor"));
}

#[test]
fn list_filters_by_state() {
    let s = Sandbox::new();

    create_ticket(&s, "Stays new");
    let id2 = create_ticket(&s, "Goes in-refinement");
    s.ticket_json(&["update", &id2, "--to-state", "in-refinement"]);

    let new_tickets = s.ticket_json(&["list", "--state", "new"]);
    assert_eq!(new_tickets["count"].as_u64().unwrap(), 1);
    assert_eq!(new_tickets["items"][0]["title"], "Stays new");

    let in_ref = s.ticket_json(&["list", "--state", "in-refinement"]);
    assert_eq!(in_ref["count"].as_u64().unwrap(), 1);
    assert_eq!(in_ref["items"][0]["id"], id2.as_str());
}

#[test]
fn list_with_repro_includes_reproduction_status() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Repro status ticket");

    let _ = s.ticket_json(&[
        "repro",
        &id,
        "--outcome",
        "reproduced",
        "--command",
        "cargo test -p context-read validate_triple_repeat -- --nocapture",
    ]);

    let listed = s.ticket_json(&["list", "--with-repro"]);
    assert_eq!(listed["status"].as_str().unwrap(), "ok");
    assert!(listed["with_repro"].as_bool().unwrap());

    let item = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .expect("ticket should be present in list output");

    assert_eq!(item["repro"]["count"].as_u64().unwrap(), 1);
    assert_eq!(item["repro"]["last_outcome"], "reproduced");
    assert!(item["repro"]["last_commit"].as_str().is_some());
}

#[test]
fn update_fields_and_state_transition() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Needs work");

    let updated = s.ticket_json(&[
        "update",
        &id,
        "--field",
        "title=Updated title",
        "--to-state",
        "in-refinement",
    ]);
    assert_eq!(updated["status"], "ok");

    let got = s.ticket_json(&["get", &id]);
    assert_eq!(got["ticket"]["fields"]["title"], "Updated title");
    assert_eq!(got["ticket"]["fields"]["state"], "in-refinement");
}

#[test]
fn delete_removes_ticket_from_list() {
    let s = Sandbox::new();
    let del_id = create_ticket(&s, "Will be deleted");
    let keep_id = create_ticket(&s, "Will survive");

    let del = s.ticket_json(&["delete", &del_id]);
    assert_eq!(del["status"], "ok");

    let list = s.ticket_json(&["list"]);
    assert_eq!(list["count"].as_u64().unwrap(), 1);
    assert_eq!(list["items"][0]["id"], keep_id.as_str());
    assert_eq!(list["items"][0]["title"], "Will survive");
}

#[test]
fn get_after_delete_exits_nonzero() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Temporary ticket");
    s.ticket_json(&["delete", &id]);

    let (exit_code, _stderr) = s.ticket_fail(&["get", &id]);
    assert_eq!(exit_code, 1);
}

// ---------------------------------------------------------------------------
// Full-text search
// ---------------------------------------------------------------------------

#[test]
fn search_returns_matching_titles() {
    let s = Sandbox::new();
    create_ticket(&s, "Fix the database connection pool");
    create_ticket(&s, "Improve UI rendering performance");
    create_ticket(&s, "Refactor database migration scripts");

    let results = s.ticket_json(&["search", "database"]);
    assert_eq!(results["status"], "ok");

    let count = results["count"].as_u64().unwrap();
    assert!(
        count >= 2,
        "expected >= 2 results for 'database', got {count}"
    );

    // Both matching titles must appear somewhere in the result set.
    let titles: Vec<&str> = results["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["title"].as_str())
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("database")),
        "at least one result should contain 'database': {titles:?}"
    );
}

#[test]
fn search_returns_empty_for_unknown_query() {
    let s = Sandbox::new();
    create_ticket(&s, "Fix the login page");
    create_ticket(&s, "Improve the dashboard");

    let results = s.ticket_json(&["search", "zxqwerty_nonexistent_phrase"]);
    assert_eq!(results["status"], "ok");
    assert_eq!(results["count"].as_u64().unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Scan / reindex
// ---------------------------------------------------------------------------

#[test]
fn scan_reindex_preserves_searchability() {
    let s = Sandbox::new();
    create_ticket(&s, "Audit log improvements");
    create_ticket(&s, "Performance benchmark suite");

    // Run a full reindex — rebuilds the Tantivy search index from the
    // filesystem source of truth.
    let scan = s.ticket_json(&["scan", "--reindex"]);
    assert_eq!(scan["status"], "ok");
    assert_eq!(
        scan["integrated"].as_u64().unwrap(),
        2,
        "reindex should re-integrate both tickets"
    );

    // Search must still return the correct result.
    let results = s.ticket_json(&["search", "benchmark"]);
    assert_eq!(results["count"].as_u64().unwrap(), 1);
    assert_eq!(results["results"][0]["title"], "Performance benchmark suite");
}

// ---------------------------------------------------------------------------
// Lease / claim / unclaim
// ---------------------------------------------------------------------------

#[test]
fn claim_conflict_and_unclaim_cycle() {
    let s = Sandbox::new();
    let id = create_ticket(&s, "Work to claim");

    // Agent-1 claims the ticket successfully.
    let claim = s.ticket_json(&[
        "claim",
        &id,
        "--agent",
        "agent-1",
        "--ttl-secs",
        "300",
    ]);
    assert_eq!(claim["status"], "ok");
    assert_eq!(claim["working_by"], "agent-1");

    // Agent-2 attempts to claim the same ticket — must fail (lease conflict).
    let (_code, stderr) = s.ticket_fail(&["claim", &id, "--agent", "agent-2"]);
    assert!(
        stderr.contains("agent-1") || stderr.contains("lease") || stderr.contains("conflict"),
        "expected a lease-conflict error mentioning agent-1, got: {stderr}"
    );

    // The leases listing should show exactly one active lease.
    let leases = s.ticket_json(&["leases"]);
    assert_eq!(leases["count"].as_u64().unwrap(), 1);
    assert_eq!(leases["leases"][0]["working_by"], "agent-1");

    // Agent-1 releases the lease.
    let unclaim = s.ticket_json(&["unclaim", &id]);
    assert_eq!(unclaim["status"], "ok");

    // Agent-2 can now claim successfully.
    let reclaim = s.ticket_json(&["claim", &id, "--agent", "agent-2"]);
    assert_eq!(reclaim["status"], "ok");
    assert_eq!(reclaim["working_by"], "agent-2");
}

// ---------------------------------------------------------------------------
// Batch
// ---------------------------------------------------------------------------

#[test]
fn batch_reads_cli_lines_from_stdin() {
    let s = Sandbox::new();

    let input = concat!(
        "create --title \"Batch stdin A\" --type tracker-improvement\n",
        "create --title \"Batch stdin B\" --type tracker-improvement",
    );
    let result = s.ticket_json_stdin(&["batch"], input);
    assert_eq!(result["status"], "ok");
    assert_eq!(result["count"].as_u64().unwrap(), 2);

    let list = s.ticket_json(&["list"]);
    assert_eq!(list["count"].as_u64().unwrap(), 2);
}

#[test]
fn batch_cli_rolls_back_on_error() {
    let s = Sandbox::new();

    // First create succeeds; deleting a non-existent UUID fails; create is rolled back.
    let input = concat!(
        "create --title \"Should be rolled back\" --type tracker-improvement\n",
        "delete 00000000-0000-0000-0000-000000000000",
    );
    let result = s.ticket_json_stdin(&["batch"], input);
    assert_eq!(result["status"], "error");
    assert_eq!(result["completed"].as_u64().unwrap(), 1);
    assert_eq!(
        result["rolled_back"].as_bool().unwrap_or(false),
        true,
        "batch must report rolled_back=true after successful rollback"
    );

    let list = s.ticket_json(&["list"]);
    assert_eq!(
        list["count"].as_u64().unwrap(),
        0,
        "the rolled-back create must not appear in the store"
    );
}

#[test]
fn unlink_removes_existing_edge() {
    let s = Sandbox::new();

    let id_a = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    let id_b = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

    s.ticket_json(&[
        "create",
        "--id",
        id_a,
        "--title",
        "A",
        "--type",
        "tracker-improvement",
    ]);
    s.ticket_json(&[
        "create",
        "--id",
        id_b,
        "--title",
        "B",
        "--type",
        "tracker-improvement",
    ]);

    let linked = s.ticket_json(&[
        "link",
        "--from",
        id_a,
        "--to",
        id_b,
        "--kind",
        "depends_on",
    ]);
    assert_eq!(linked["status"], "ok");

    let before = s.ticket_json(&["links", id_a]);
    assert_eq!(before["count"].as_u64().unwrap(), 1);

    let unlinked = s.ticket_json(&[
        "unlink",
        "--from",
        id_a,
        "--to",
        id_b,
        "--kind",
        "depends_on",
    ]);
    assert_eq!(unlinked["status"], "ok");

    let after = s.ticket_json(&["links", id_a]);
    assert_eq!(after["count"].as_u64().unwrap(), 0);
}


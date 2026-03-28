//! Integration tests for `ticket history`, `ticket diff`, and `ticket revert`.
//!
//! These commands test the append-only `history.ndjson` revision log and its
//! forward-only revert semantics (revert creates a new revision, never erases).

mod common;

use common::Sandbox;

// ---------------------------------------------------------------------------
// history
// ---------------------------------------------------------------------------

/// Creating a ticket produces an initial revision (rev 1) visible in history.
#[test]
fn history_initial_revision_on_create() {
    let s = Sandbox::new();

    let created = s.ticket_json(&[
        "create",
        "--title",
        "Initial ticket",
        "--type",
        "tracker-improvement",
    ]);
    let id = created["id"].as_str().expect("id");

    let hist = s.ticket_json(&["history", id]);
    assert_eq!(hist["status"], "ok");
    assert_eq!(hist["count"], 1, "one revision on create");
    assert_eq!(hist["entries"][0]["rev"], 1);
    assert_eq!(hist["entries"][0]["fields"]["title"], "Initial ticket");
    assert_eq!(hist["entries"][0]["fields"]["state"], "open");
}

/// Each update appends a new revision; history is returned most-recent first.
#[test]
fn history_accumulates_revisions_on_update() {
    let s = Sandbox::new();

    let created = s.ticket_json(&["create", "--title", "Feature A", "--type", "tracker-improvement"]);
    let id = created["id"].as_str().expect("id");

    s.ticket_json(&["update", id, "--to-state", "in-refinement"]);
    s.ticket_json(&["update", id, "--field", "title=Feature A v2"]);

    let hist = s.ticket_json(&["history", id]);
    assert_eq!(hist["status"], "ok");
    // create + 2 updates = 3 revisions
    assert_eq!(hist["count"], 3, "three revisions");
    // history is most-recent first, so entries[0] is rev 3
    assert_eq!(hist["entries"][0]["rev"], 3);
    assert_eq!(hist["entries"][2]["rev"], 1);
}

/// `--limit` caps the number of entries returned (still most-recent first).
#[test]
fn history_limit_caps_entries() {
    let s = Sandbox::new();

    let created = s.ticket_json(&["create", "--title", "Ticket X", "--type", "tracker-improvement"]);
    let id = created["id"].as_str().expect("id");

    s.ticket_json(&["update", id, "--to-state", "in-refinement"]);
    s.ticket_json(&["update", id, "--field", "priority=high"]);

    let hist = s.ticket_json(&["history", id, "--limit", "2"]);
    assert_eq!(hist["status"], "ok");
    assert_eq!(hist["count"], 2, "limited to 2");
    // Most-recent (rev 3) comes first.
    assert_eq!(hist["entries"][0]["rev"], 3);
    assert_eq!(hist["entries"][1]["rev"], 2);
}

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

/// `diff` detects a state change between two revisions.
#[test]
fn diff_detects_state_change() {
    let s = Sandbox::new();

    let created = s.ticket_json(&["create", "--title", "Diffable", "--type", "tracker-improvement"]);
    let id = created["id"].as_str().expect("id");

    s.ticket_json(&["update", id, "--to-state", "in-refinement"]);

    let diff = s.ticket_json(&["diff", id, "--from", "1", "--to", "2"]);
    assert_eq!(diff["status"], "ok");
    assert_eq!(diff["from_rev"], 1);
    assert_eq!(diff["to_rev"], 2);

    // state changed: new → in-refinement
    let changed = &diff["changed"];
    assert_eq!(changed["state"]["from"], "new");
    assert_eq!(changed["state"]["to"], "in-refinement");
}

/// `--to latest` resolves to the most recent revision.
#[test]
fn diff_to_latest_resolves_correctly() {
    let s = Sandbox::new();

    let created = s.ticket_json(&["create", "--title", "Latest test", "--type", "tracker-improvement"]);
    let id = created["id"].as_str().expect("id");

    s.ticket_json(&["update", id, "--field", "note=added"]);

    let diff = s.ticket_json(&["diff", id, "--from", "1", "--to", "latest"]);
    assert_eq!(diff["status"], "ok");
    assert_eq!(diff["to_rev"], 2);
}

/// `diff` with equal revisions reports no changes.
#[test]
fn diff_same_revision_is_empty() {
    let s = Sandbox::new();

    let created = s.ticket_json(&["create", "--title", "Static", "--type", "tracker-improvement"]);
    let id = created["id"].as_str().expect("id");

    let diff = s.ticket_json(&["diff", id, "--from", "1", "--to", "1"]);
    assert_eq!(diff["status"], "ok");
    // No changes expected.
    let added = diff["added"].as_object().expect("added obj");
    let removed = diff["removed"].as_object().expect("removed obj");
    let changed = diff["changed"].as_object().expect("changed obj");
    assert!(added.is_empty(), "no added fields");
    assert!(removed.is_empty(), "no removed fields");
    assert!(changed.is_empty(), "no changed fields");
}

// ---------------------------------------------------------------------------
// revert
// ---------------------------------------------------------------------------

/// Revert to an earlier revision creates a new revision (forward-only) and
/// restores the old field values. Revert bypasses state-machine validation
/// so it can always go backwards in state.
#[test]
fn revert_creates_new_revision_with_old_state() {
    let s = Sandbox::new();

    let created = s.ticket_json(&["create", "--title", "Revertable", "--type", "tracker-improvement"]);
    let id = created["id"].as_str().expect("id");

    // Advance state to in-refinement (rev 2).
    s.ticket_json(&["update", id, "--to-state", "in-refinement"]);

    // Revert to rev 1 (state: new). Bypasses state machine — always succeeds.
    let rev_result = s.ticket_json(&["revert", id, "--to", "1"]);
    assert_eq!(rev_result["status"], "ok");
    let new_rev = rev_result["new_rev"].as_u64().unwrap_or(0);
    assert_eq!(new_rev, 3, "create(1) + update(2) + revert(3)");
    assert_eq!(rev_result["reverted_to"], 1);

    // History now has 3 entries.
    let hist = s.ticket_json(&["history", id]);
    assert_eq!(hist["count"], 3);
    // Most-recent entry (entries[0]) should show state=new (reverted).
    assert_eq!(hist["entries"][0]["fields"]["state"], "new");
}

/// Revert preserves forward-only invariant: history count never decreases.
#[test]
fn revert_forward_only_history_never_shrinks() {
    let s = Sandbox::new();

    let created = s.ticket_json(&["create", "--title", "Forward only", "--type", "tracker-improvement"]);
    let id = created["id"].as_str().expect("id");

    s.ticket_json(&["update", id, "--field", "note=v2"]);
    s.ticket_json(&["update", id, "--field", "note=v3"]);

    let before = s.ticket_json(&["history", id]);
    let before_count = before["count"].as_u64().unwrap_or(0);
    assert_eq!(before_count, 3);

    // Revert to rev 1 — bypasses state machine, always succeeds.
    let rev_result = s.ticket_json(&["revert", id, "--to", "1"]);
    assert_eq!(rev_result["status"], "ok");

    let after = s.ticket_json(&["history", id]);
    let after_count = after["count"].as_u64().unwrap_or(0);
    assert_eq!(after_count, before_count + 1, "revert adds exactly one revision");
}

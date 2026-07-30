use super::*;
#[test]
fn scan_skips_policy_ignored_scan_roots() {
    use memory_api::model::filesystem::{
        PolicyDecision,
        ScanRootMetadata,
        ScanRootSource,
    };

    // A separate fixture store whose tickets must not leak into the main store.
    let fixture_dir = tempdir().unwrap();
    let fixture = TicketStore::init(fixture_dir.path()).unwrap();
    let fixture_ticket = fixture
        .create(
            None,
            "tracker-improvement",
            Some("Fixture ticket that must be excluded"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let fixture_tickets_path = fixture.index_root.join("tickets");
    drop(fixture);

    let main_dir = tempdir().unwrap();
    let store = TicketStore::init(main_dir.path()).unwrap();

    // Register the fixture tickets directory as a policy-ignored scan root.
    store
        .add_scan_root_with_metadata(
            ScanRoot {
                path: fixture_tickets_path,
                label: "fixtures".to_string(),
            },
            ScanRootMetadata {
                source: ScanRootSource::Policy,
                policy_decision: PolicyDecision::Ignored,
                workspace_root: None,
            },
        )
        .unwrap();

    let report = store.scan(true).unwrap();

    // The ignored root is reported as skipped and its ticket is not indexed.
    assert!(report.skipped_roots.iter().any(|label| label == "fixtures"));
    assert!(store.get(&fixture_ticket).is_err());
}

#[test]
fn query_guard_excludes_tickets_under_ignored_roots() {
    use memory_api::model::filesystem::{
        PolicyDecision,
        ScanRootMetadata,
        ScanRootSource,
    };

    // Fixture store whose ticket rows will be indexed while the root is
    // `included`, then must disappear from query surfaces once the root is
    // flipped to `ignored` (the final query-time defense — no re-scan).
    let fixture_dir = tempdir().unwrap();
    let fixture = TicketStore::init(fixture_dir.path()).unwrap();
    let fixture_ticket = fixture
        .create(
            None,
            "tracker-improvement",
            Some("Fixture ticket zzytestmarker excluded"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let fixture_tickets_path = fixture.index_root.join("tickets");
    drop(fixture);

    let main_dir = tempdir().unwrap();
    let store = TicketStore::init(main_dir.path()).unwrap();

    // Register the fixture root as `included` and index it.
    store
        .add_scan_root_with_metadata(
            ScanRoot {
                path: fixture_tickets_path.clone(),
                label: "fixtures".to_string(),
            },
            ScanRootMetadata {
                source: ScanRootSource::Discovered,
                policy_decision: PolicyDecision::Included,
                workspace_root: None,
            },
        )
        .unwrap();
    store.scan(true).unwrap();

    // While included, the fixture ticket is visible via list and search.
    assert!(
        store
            .list(None, None, None)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == fixture_ticket)
    );
    assert!(
        store
            .search_tickets("zzytestmarker", 50)
            .unwrap()
            .iter()
            .any(|result| result.id == fixture_ticket)
    );

    // Flip the root to `ignored` WITHOUT re-scanning: stale index rows remain.
    store
        .add_scan_root_with_metadata(
            ScanRoot {
                path: fixture_tickets_path,
                label: "fixtures".to_string(),
            },
            ScanRootMetadata {
                source: ScanRootSource::Policy,
                policy_decision: PolicyDecision::Ignored,
                workspace_root: None,
            },
        )
        .unwrap();

    // The query-time guard must now exclude the ticket from both surfaces even
    // though its rows still exist in the index and search segments.
    assert!(
        !store
            .list(None, None, None)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == fixture_ticket)
    );
    assert!(
        !store
            .search_tickets("zzytestmarker", 50)
            .unwrap()
            .iter()
            .any(|result| result.id == fixture_ticket)
    );
}

#[test]
fn add_edge_rejects_targets_under_policy_ignored_roots() {
    use crate::error::StorageError;
    use memory_api::model::filesystem::{
        PolicyDecision,
        ScanRootMetadata,
        ScanRootSource,
    };

    let fixture_dir = tempdir().unwrap();
    let fixture = TicketStore::init(fixture_dir.path()).unwrap();
    let fixture_ticket = fixture
        .create(
            None,
            "tracker-improvement",
            Some("Fixture ticket blocked as edge target"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let fixture_tickets_path = fixture.index_root.join("tickets");
    drop(fixture);

    let main_dir = tempdir().unwrap();
    let store = TicketStore::init(main_dir.path()).unwrap();
    let source = store
        .create(
            None,
            "tracker-improvement",
            Some("Source ticket"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .add_scan_root_with_metadata(
            ScanRoot {
                path: fixture_tickets_path.clone(),
                label: "fixtures".to_string(),
            },
            ScanRootMetadata {
                source: ScanRootSource::Discovered,
                policy_decision: PolicyDecision::Included,
                workspace_root: None,
            },
        )
        .unwrap();
    store.scan(true).unwrap();

    // Flip the fixture root to ignored without re-scanning to mimic stale
    // index rows that must remain hidden by policy-aware query guards.
    store
        .add_scan_root_with_metadata(
            ScanRoot {
                path: fixture_tickets_path,
                label: "fixtures".to_string(),
            },
            ScanRootMetadata {
                source: ScanRootSource::Policy,
                policy_decision: PolicyDecision::Ignored,
                workspace_root: None,
            },
        )
        .unwrap();

    let err = store
        .add_edge(EdgeRecord {
            from: source,
            to: fixture_ticket,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap_err();
    assert!(matches!(err, StorageError::NotFound(id) if id == fixture_ticket));

    assert!(store.edges_from(&source).unwrap().is_empty());
    assert!(
        store
            .get(&source)
            .unwrap()
            .extra
            .get("depends_on")
            .is_none()
    );
}

// ---------------------------------------------------------------------------
// Slice 6 — consolidated end-to-end policy regression coverage.
//
// These exercise the full discovery -> scan -> query contract through
// `TicketStore::reapply_workspace_policy`, which loads the on-disk
// `.ticket/workspace-policy.toml`, discovers descendant/ancestor stores under
// the policy, re-registers scan roots with `policy_decision` metadata, rescans,
// and thereby makes the query-time guard authoritative.
// ---------------------------------------------------------------------------

/// Initialize a store at `index_root` and create one ticket in it.
fn init_store_with_ticket(
    index_root: &Path,
    title: &str,
) -> (TicketStore, Uuid) {
    let store = TicketStore::init(index_root).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some(title),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    (store, id)
}

fn write_policy(
    workspace_root: &Path,
    contents: &str,
) {
    let ticket_dir = workspace_root.join(".ticket");
    fs::create_dir_all(&ticket_dir).unwrap();
    fs::write(ticket_dir.join("workspace-policy.toml"), contents).unwrap();
}

#[test]
fn policy_e2e_child_included_by_default() {
    // No policy file -> compatibility mode -> descendants included, scanned,
    // and queryable.
    let dir = tempdir().unwrap();
    let repo = dir.path();
    let (main, _root) = init_store_with_ticket(&repo.join(".ticket"), "Root");
    let (child, child_id) =
        init_store_with_ticket(&repo.join("child").join(".ticket"), "Child");
    drop(child);

    let report = main.reapply_workspace_policy(repo).unwrap();

    assert!(report.skipped_roots.is_empty());
    assert!(
        main.get(&child_id).is_ok(),
        "child ticket should be scanned"
    );
    assert!(
        main.list(None, None, None)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == child_id),
        "child ticket should be queryable by default"
    );
}

#[test]
fn policy_e2e_child_ignored_via_marker() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    let (main, _root) = init_store_with_ticket(&repo.join(".ticket"), "Root");
    let (child, child_id) =
        init_store_with_ticket(&repo.join("child").join(".ticket"), "Child");
    drop(child);

    // Default policy present (ignore_markers includes `.ticket-ignore`).
    write_policy(repo, "");
    fs::write(repo.join("child").join(".ticket-ignore"), "").unwrap();

    main.reapply_workspace_policy(repo).unwrap();

    assert!(
        main.get(&child_id).is_err(),
        "marker-ignored child must not be scanned"
    );
    assert!(
        !main
            .list(None, None, None)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == child_id)
    );
}

#[test]
fn policy_e2e_child_ignored_via_glob() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    let (main, _root) = init_store_with_ticket(&repo.join(".ticket"), "Root");
    let (fixture, fixture_id) = init_store_with_ticket(
        &repo.join("fixtures").join(".ticket"),
        "Fixture",
    );
    drop(fixture);

    write_policy(repo, "ignore_workspaces = [\"fixtures*\"]\n");

    main.reapply_workspace_policy(repo).unwrap();

    assert!(
        main.get(&fixture_id).is_err(),
        "glob-ignored descendant must not be scanned"
    );
    assert!(
        !main
            .list(None, None, None)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == fixture_id)
    );
}

#[test]
fn policy_e2e_include_override_wins() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    let (main, _root) = init_store_with_ticket(&repo.join(".ticket"), "Root");
    let (fixture, fixture_id) = init_store_with_ticket(
        &repo.join("fixtures").join(".ticket"),
        "Fixture",
    );
    drop(fixture);

    // Matches both the ignore glob and an include override -> override wins.
    write_policy(
        repo,
        "ignore_workspaces = [\"fixtures*\"]\ninclude_overrides = [\"fixtures\"]\n",
    );

    main.reapply_workspace_policy(repo).unwrap();

    assert!(
        main.get(&fixture_id).is_ok(),
        "include override should re-include the descendant"
    );
    assert!(
        main.list(None, None, None)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == fixture_id)
    );
}

#[test]
fn policy_e2e_external_path_denied() {
    // Workspace is the child; an ancestor store outside its subtree must never
    // be indexed or queryable when external paths are denied.
    let dir = tempdir().unwrap();
    let repo = dir.path();
    let (ancestor, ancestor_id) =
        init_store_with_ticket(&repo.join(".ticket"), "Ancestor");
    drop(ancestor);
    let child_root = repo.join("child");
    let (main, _child_ticket) =
        init_store_with_ticket(&child_root.join(".ticket"), "Child");

    // Request ancestors but deny external paths -> ancestor suppressed.
    write_policy(
        &child_root,
        "include_ancestors = true\ndeny_external_paths = true\n",
    );

    main.reapply_workspace_policy(&child_root).unwrap();

    assert!(
        main.get(&ancestor_id).is_err(),
        "external ancestor ticket must not be indexed under deny_external_paths"
    );
    assert!(
        !main
            .list(None, None, None)
            .unwrap()
            .iter()
            .any(|ticket| ticket.id == ancestor_id)
    );
}

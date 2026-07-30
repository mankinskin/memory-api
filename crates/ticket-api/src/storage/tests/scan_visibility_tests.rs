use super::*;
fn run_scan_reconciliation_visibility_agreement(reindex: bool) {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child_repo_a = repo.join("memory-viewers").join("memory-api");
    let child_repo_b = repo.join("memory-viewers").join("viewer-api");
    fs::create_dir_all(&child_repo_a).unwrap();
    fs::create_dir_all(&child_repo_b).unwrap();

    let root_store = TicketStore::init(&repo).unwrap();
    let child_store_a = TicketStore::init(&child_repo_a).unwrap();
    let child_store_b = TicketStore::init(&child_repo_b).unwrap();
    root_store
        .add_scan_root(ScanRoot {
            path: child_store_a.index_root.join("tickets"),
            label: "memory-api".to_string(),
        })
        .unwrap();
    root_store
        .add_scan_root(ScanRoot {
            path: child_store_b.index_root.join("tickets"),
            label: "viewer-api".to_string(),
        })
        .unwrap();

    let stable_id = child_store_a
        .create(
            None,
            "tracker-improvement",
            Some("VisibilityFixture stable"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let delete_id = child_store_a
        .create(
            None,
            "tracker-improvement",
            Some("VisibilityFixture delete"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let move_id = child_store_b
        .create(
            None,
            "tracker-improvement",
            Some("VisibilityFixture move"),
            Some("in-review"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    root_store.scan(reindex).unwrap();

    let add_id = child_store_b
        .create(
            None,
            "tracker-improvement",
            Some("VisibilityFixture add"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let known_ids = vec![stable_id, delete_id, move_id, add_id];

    assert_visibility_surfaces_agree(
        &root_store,
        "VisibilityFixture",
        &known_ids,
        &[stable_id, delete_id, move_id],
    );

    root_store.scan(reindex).unwrap();
    assert_visibility_surfaces_agree(
        &root_store,
        "VisibilityFixture",
        &known_ids,
        &[stable_id, delete_id, move_id, add_id],
    );

    let mut stable_patch = BTreeMap::new();
    stable_patch.insert(
        "title".to_string(),
        Value::String("VisibilityFixture stable updated".to_string()),
    );
    child_store_a
        .update(
            &stable_id,
            stable_patch,
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();

    root_store.scan(reindex).unwrap();
    assert_visibility_surfaces_agree(
        &root_store,
        "VisibilityFixture",
        &known_ids,
        &[stable_id, delete_id, move_id, add_id],
    );
    assert_ticket_title_and_state(
        &root_store,
        "VisibilityFixture",
        stable_id,
        "VisibilityFixture stable updated",
        "in-implementation",
    );

    let mut move_patch = BTreeMap::new();
    move_patch.insert(
        "title".to_string(),
        Value::String("VisibilityFixture move repaired".to_string()),
    );
    child_store_b
        .update(
            &move_id,
            move_patch,
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();
    let expected_move = child_store_b.get_indexed(&move_id).unwrap().unwrap();

    let poisoned_index =
        RedbIndexStore::open(&root_store.index_root.join("tickets.db"))
            .unwrap();
    let mut poisoned = root_store.get_indexed(&move_id).unwrap().unwrap();
    poisoned.path = root_store
        .index_root
        .join("tickets")
        .join(move_id.to_string());
    poisoned.type_id = "wrong-type".to_string();
    poisoned.title = Some("Wrong title".to_string());
    poisoned.state = Some("open".to_string());
    poisoned.created_at = expected_move.created_at - Duration::days(1);
    poisoned_index.insert_ticket(&poisoned).unwrap();

    root_store.scan(reindex).unwrap();
    assert_visibility_surfaces_agree(
        &root_store,
        "VisibilityFixture",
        &known_ids,
        &[stable_id, delete_id, move_id, add_id],
    );
    let repaired_move = root_store.get_indexed(&move_id).unwrap().unwrap();
    assert_eq!(repaired_move.path, expected_move.path);
    assert_eq!(repaired_move.title, expected_move.title);
    assert_eq!(repaired_move.state, expected_move.state);
    assert_ticket_title_and_state(
        &root_store,
        "VisibilityFixture",
        move_id,
        "VisibilityFixture move repaired",
        "in-implementation",
    );

    let delete_path =
        child_store_a.get_indexed(&delete_id).unwrap().unwrap().path;
    fs::remove_dir_all(&delete_path).unwrap();

    root_store.scan(reindex).unwrap();
    assert_visibility_surfaces_agree(
        &root_store,
        "VisibilityFixture",
        &known_ids,
        &[stable_id, move_id, add_id],
    );

    fs::remove_dir_all(&child_repo_b).unwrap();

    root_store.scan(reindex).unwrap();
    assert_visibility_surfaces_agree(
        &root_store,
        "VisibilityFixture",
        &known_ids,
        &[stable_id],
    );
    assert_ticket_title_and_state(
        &root_store,
        "VisibilityFixture",
        stable_id,
        "VisibilityFixture stable updated",
        "in-implementation",
    );
}

#[test]
fn scan_reconciliation_visibility_agreement_without_reindex() {
    run_scan_reconciliation_visibility_agreement(false);
}

#[test]
fn scan_reconciliation_visibility_agreement_with_reindex() {
    run_scan_reconciliation_visibility_agreement(true);
}

#[test]
fn open_creates_gitignore_for_local_ticket_artifacts() {
    let dir = tempdir().unwrap();

    TicketStore::init(dir.path()).unwrap();

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("tickets.db"));
    assert!(gitignore.contains("tickets.db-shm"));
    assert!(gitignore.contains("tickets.db-wal"));
    assert!(gitignore.contains("search_index/"));
}

#[test]
fn open_registers_default_tickets_scan_root() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let roots = store.list_scan_roots().unwrap();

    assert!(roots.iter().any(|root| {
        root.path == store.index_root.join("tickets") && root.label == "tickets"
    }));
}

#[test]
fn open_uses_existing_hidden_ticket_store_from_repo_root() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let store_root = repo.join(".ticket");
    fs::create_dir_all(&store_root).unwrap();

    let store = TicketStore::init(&repo).unwrap();

    assert_eq!(
        canonical_existing_path(&store.index_root),
        canonical_existing_path(&store_root)
    );
}

#[test]
fn create_with_repo_root_target_places_ticket_under_hidden_store() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let store_root = repo.join(".ticket");
    fs::create_dir_all(&store_root).unwrap();
    let store = TicketStore::init(&store_root).unwrap();

    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Root path resolves to local store"),
            None,
            Default::default(),
            Some(&repo),
            None,
        )
        .unwrap();
    let indexed = store.get_indexed(&ticket_id).unwrap().unwrap();
    let indexed_path = canonical_existing_path(&indexed.path);
    let expected_root = canonical_existing_path(&store_root.join("tickets"));

    assert!(indexed_path.starts_with(&expected_root));
}

#[test]
fn create_rejects_non_workspace_target_root() {
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let store_root = dir.path().join(".ticket");
    let invalid_root = outside.path().join("stray-root");
    fs::create_dir_all(&store_root).unwrap();
    fs::create_dir_all(&invalid_root).unwrap();
    let store = TicketStore::init(&store_root).unwrap();

    let error = store
        .create(
            None,
            "tracker-improvement",
            Some("Reject invalid target root"),
            None,
            Default::default(),
            Some(&invalid_root),
            None,
        )
        .unwrap_err();

    assert!(error.to_string().contains("invalid ticket root"));
}

#[test]
fn scan_refreshes_nested_workspace_ticket_state_changes() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child_repo = repo.join("memory-viewers").join("viewer-api");
    fs::create_dir_all(&child_repo).unwrap();

    let root_store = TicketStore::init(&repo).unwrap();
    let child_store = TicketStore::init(&child_repo).unwrap();
    root_store
        .add_scan_root(ScanRoot {
            path: child_store.index_root.join("tickets"),
            label: "viewer-api".to_string(),
        })
        .unwrap();

    let ticket_id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Nested workspace ticket"),
            Some("in-review"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    root_store.scan(true).unwrap();
    let indexed = root_store.get_indexed(&ticket_id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("in-review"));

    child_store
        .update(
            &ticket_id,
            Default::default(),
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();

    root_store.scan(true).unwrap();
    let indexed = root_store.get_indexed(&ticket_id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("in-implementation"));
}

#[test]
fn scan_keeps_nested_workspace_tickets_searchable_without_reindex() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child_repo = repo.join("memory-viewers").join("memory-api");
    fs::create_dir_all(&child_repo).unwrap();

    let root_store = TicketStore::init(&repo).unwrap();
    let child_store = TicketStore::init(&child_repo).unwrap();
    root_store
        .add_scan_root(ScanRoot {
            path: child_store.index_root.join("tickets"),
            label: "memory-api".to_string(),
        })
        .unwrap();

    let ticket_id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Persist dependency edges in tracked ticket files"),
            Some("in-review"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    root_store.scan(false).unwrap();

    let results = root_store.search_tickets("Persist", 10).unwrap();

    assert!(
        results.iter().any(|result| result.id == ticket_id),
        "normal scans should refresh Tantivy entries for nested workspace tickets"
    );
}

#[test]
fn scan_repairs_corrupted_nested_workspace_ticket_path() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child_repo = repo.join("memory-viewers").join("viewer-api");
    fs::create_dir_all(&child_repo).unwrap();

    let root_store = TicketStore::init(&repo).unwrap();
    let child_store = TicketStore::init(&child_repo).unwrap();
    root_store
        .add_scan_root(ScanRoot {
            path: child_store.index_root.join("tickets"),
            label: "viewer-api".to_string(),
        })
        .unwrap();

    let ticket_id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Nested workspace ticket"),
            Some("in-review"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    root_store.scan(true).unwrap();
    let child_indexed = child_store.get_indexed(&ticket_id).unwrap().unwrap();
    let child_ticket_path = child_indexed.path.clone();

    let poisoned_index =
        RedbIndexStore::open(&root_store.index_root.join("tickets.db"))
            .unwrap();
    let mut poisoned = root_store.get_indexed(&ticket_id).unwrap().unwrap();
    poisoned.path = root_store
        .index_root
        .join("tickets")
        .join(ticket_id.to_string());
    poisoned.type_id = "wrong-type".to_string();
    poisoned.title = Some("Wrong title".to_string());
    poisoned.state = Some("in-review".to_string());
    poisoned.created_at = child_indexed.created_at - Duration::days(1);
    poisoned_index.insert_ticket(&poisoned).unwrap();

    child_store
        .update(
            &ticket_id,
            Default::default(),
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();

    root_store.scan(true).unwrap();

    let indexed = root_store.get_indexed(&ticket_id).unwrap().unwrap();
    assert_eq!(indexed.path, child_ticket_path);
    assert_eq!(indexed.type_id, child_indexed.type_id);
    assert_eq!(indexed.title.as_deref(), Some("Nested workspace ticket"));
    assert_eq!(indexed.state.as_deref(), Some("in-implementation"));
    assert_eq!(indexed.created_at, child_indexed.created_at);

    let manifest = root_store.get(&ticket_id).unwrap();
    assert_eq!(
        manifest.extra.get("state").and_then(|value| value.as_str()),
        Some("in-implementation")
    );
}

#[test]
fn scan_without_reindex_repairs_moved_nested_ticket_path_and_search_doc() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child_repo = repo.join("memory-viewers").join("viewer-api");
    fs::create_dir_all(&child_repo).unwrap();

    let root_store = TicketStore::init(&repo).unwrap();
    let child_store = TicketStore::init(&child_repo).unwrap();
    root_store
        .add_scan_root(ScanRoot {
            path: child_store.index_root.join("tickets"),
            label: "viewer-api".to_string(),
        })
        .unwrap();

    let ticket_id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Nested workspace ticket"),
            Some("in-implementation"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let dependent_id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Dependent on moved nested workspace ticket"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    child_store
        .add_edge(EdgeRecord {
            from: dependent_id,
            to: ticket_id,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    root_store.scan(true).unwrap();
    let expected = child_store.get_indexed(&ticket_id).unwrap().unwrap();

    let poisoned_index =
        RedbIndexStore::open(&root_store.index_root.join("tickets.db"))
            .unwrap();
    let mut poisoned = root_store.get_indexed(&ticket_id).unwrap().unwrap();
    poisoned.path = root_store
        .index_root
        .join("tickets")
        .join(ticket_id.to_string());
    poisoned.type_id = "wrong-type".to_string();
    poisoned.title = Some("Wrong title".to_string());
    poisoned.state = Some("in-review".to_string());
    poisoned.created_at = expected.created_at - Duration::days(1);
    poisoned_index.insert_ticket(&poisoned).unwrap();

    let report = root_store.scan(false).unwrap();

    assert_eq!(
        report
            .phase_timings_ms
            .get("workflow.incremental_root_count"),
        Some(&1)
    );
    assert_eq!(
        report
            .phase_timings_ms
            .get("workflow.incremental_affected_count"),
        Some(&2)
    );

    let indexed = root_store.get_indexed(&ticket_id).unwrap().unwrap();
    assert_eq!(indexed.path, expected.path);
    assert_eq!(indexed.type_id, expected.type_id);
    assert_eq!(indexed.title, expected.title);
    assert_eq!(indexed.state, expected.state);
    assert_eq!(indexed.created_at, expected.created_at);
    assert!(root_store.get(&ticket_id).is_ok());
    assert!(
        root_store
            .search_tickets("Nested workspace ticket", 10)
            .unwrap()
            .iter()
            .any(|result| {
                result.id == ticket_id
                    && result.title.as_deref()
                        == Some("Nested workspace ticket")
                    && result.state.as_deref() == Some("in-implementation")
            })
    );
    assert_eq!(
        root_store
            .get_workflow_facts(&dependent_id)
            .unwrap()
            .unwrap()
            .unresolved_dependency_count,
        1
    );
}

#[test]
fn scan_indexes_manual_ticket_with_missing_optional_fields() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = Uuid::new_v4();
    let manifest = TicketManifest::new(ticket_id, Utc::now());
    let ticket_path =
        store.index_root.join("tickets").join(ticket_id.to_string());

    fs::create_dir_all(&ticket_path).unwrap();
    fs::write(
        ticket_path.join("ticket.toml"),
        format_manifest_toml(&manifest),
    )
    .unwrap();

    store.scan(false).unwrap();

    let indexed = store.get_indexed(&ticket_id).unwrap().unwrap();
    assert_eq!(indexed.path, ticket_path);
    assert_eq!(indexed.type_id, "unknown");
    assert_eq!(indexed.title, None);
    assert_eq!(indexed.state, None);

    let stored = store.get(&ticket_id).unwrap();
    assert!(stored.extra.get("type").is_none());
    assert!(stored.extra.get("title").is_none());
    assert!(stored.extra.get("state").is_none());
}

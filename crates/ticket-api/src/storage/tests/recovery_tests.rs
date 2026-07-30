use super::*;
#[test]
fn open_or_init_profiled_reports_bootstrap_scan_timings() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Profile bootstrap open_or_init"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let index_root = store.index_root.clone();
    drop(store);

    fs::remove_file(index_root.join("tickets.db")).unwrap();
    let _ = fs::remove_file(index_root.join("tickets.db-shm"));
    let _ = fs::remove_file(index_root.join("tickets.db-wal"));
    let _ = fs::remove_dir_all(index_root.join("search_index"));

    let (rebuilt, report) =
        TicketStore::open_or_init_profiled(dir.path()).unwrap();

    assert!(report.initialized_store);
    assert!(
        report
            .phase_timings_ms
            .contains_key("open_or_init_total_ms")
    );
    assert!(report.phase_timings_ms.contains_key("open_sqlite_index_ms"));
    assert!(report.phase_timings_ms.contains_key("open_search_index_ms"));
    assert!(!report.scan_reports.is_empty());
    assert_eq!(rebuilt.get(&ticket_id).unwrap().id, ticket_id);
}

#[test]
fn open_rebuilds_existing_empty_index_from_manifests() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Repair empty ticket index"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let index_root = store.index_root.clone();
    drop(store);

    fs::remove_file(index_root.join("tickets.db")).unwrap();
    let _ = fs::remove_file(index_root.join("tickets.db-shm"));
    let _ = fs::remove_file(index_root.join("tickets.db-wal"));
    let _ = fs::remove_dir_all(index_root.join("search_index"));
    RedbIndexStore::open(&index_root.join("tickets.db")).unwrap();

    let reopened = TicketStore::open(dir.path()).unwrap();
    let manifest = reopened.get(&ticket_id).unwrap();

    assert_eq!(manifest.id, ticket_id);
}

#[test]
fn search_and_delete_self_heal_after_search_index_corruption() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Search repair ticket"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    corrupt_search_index_meta(&store.index_root);

    let results = store.search_tickets("Search repair ticket", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, ticket_id);

    corrupt_search_index_meta(&store.index_root);

    store.delete(&ticket_id).unwrap();

    let results = store.search_tickets("Search repair ticket", 10).unwrap();
    assert!(results.is_empty());
}

#[test]
fn scan_force_self_heals_after_search_index_corruption() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Scan repair ticket"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    corrupt_search_index_meta(&store.index_root);

    store.scan(true).unwrap();

    let results = store.search_tickets("Scan repair ticket", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, ticket_id);
}

#[test]
fn scan_force_backfills_legacy_db_only_edges_into_ticket_manifests() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let source_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Legacy source ticket"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let target_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Legacy target ticket"),
            Some("planned"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let legacy_index =
        RedbIndexStore::open(&store.index_root.join("tickets.db")).unwrap();
    legacy_index
        .insert_edge(&EdgeRecord {
            from: source_id,
            to: target_id,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    let manifest = store.get(&source_id).unwrap();
    assert!(manifest.extra.get("depends_on").is_none());

    store.scan(true).unwrap();
    store.scan(true).unwrap();

    let manifest = store.get(&source_id).unwrap();
    let targets = manifest
        .extra
        .get("depends_on")
        .and_then(|value| value.as_array())
        .unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].as_str(), Some(target_id.to_string().as_str()));

    let edges = store.edges_from(&source_id).unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to, target_id);
    assert_eq!(edges[0].kind, "depends_on");
}

#[test]
fn scan_force_does_not_restore_removed_dependency_edges() {
    let dir = tempdir().unwrap();
    let index_root;
    let source_id;
    let target_id;

    {
        let store = TicketStore::init(dir.path()).unwrap();
        index_root = store.index_root.clone();
        source_id = store
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
        target_id = store
            .create(
                None,
                "tracker-improvement",
                Some("Target ticket"),
                Some("planned"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let edge = EdgeRecord {
            from: source_id,
            to: target_id,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        };
        store.add_edge(edge.clone()).unwrap();
        store.remove_edge(edge).unwrap();

        let manifest = store.get(&source_id).unwrap();
        assert!(manifest.extra.get("depends_on").is_none());
    }

    fs::remove_file(index_root.join("tickets.db")).unwrap();
    let _ = fs::remove_file(index_root.join("tickets.db-shm"));
    let _ = fs::remove_file(index_root.join("tickets.db-wal"));
    let _ = fs::remove_dir_all(index_root.join("search_index"));

    let rebuilt = TicketStore::init(&index_root).unwrap();
    rebuilt.scan(true).unwrap();

    assert!(rebuilt.edges_from(&source_id).unwrap().is_empty());
}

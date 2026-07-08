use super::*;
#[test]
fn scan_force_prunes_row_for_physically_removed_ticket() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Removed from disk"),
            Some("in-review"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let ticket_path = store.get_indexed(&ticket_id).unwrap().unwrap().path;
    fs::remove_dir_all(&ticket_path).unwrap();

    store.scan(true).unwrap();

    assert!(store.get_indexed(&ticket_id).unwrap().is_none());
}

#[test]
fn scan_force_prunes_empty_uuid_artifact_folder_without_manifest() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let artifact_id = Uuid::parse_str("4ea42273-a134-4342-b601-1759df6d562f").unwrap();
    let artifact_dir = store.index_root.join("tickets").join(artifact_id.to_string());
    fs::create_dir_all(&artifact_dir).unwrap();
    assert!(artifact_dir.exists());
    assert!(!artifact_dir.join("ticket.toml").exists());

    let report = store.scan(true).unwrap();

    assert!(!artifact_dir.exists(), "scan should prune empty artifact dirs");
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diag| diag.path == artifact_dir.join("ticket.toml")
                && diag.reason.contains("missing ticket.toml"))
    );
}

#[test]
fn scan_without_reindex_prunes_deleted_nested_ticket_from_search_and_index() {
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
            Some("Deleted nested visibility ticket"),
            Some("in-review"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    root_store.scan(true).unwrap();
    assert!(root_store
        .search_tickets("Deleted nested visibility", 10)
        .unwrap()
        .iter()
        .any(|result| result.id == ticket_id));

    let ticket_path = child_store.get_indexed(&ticket_id).unwrap().unwrap().path;
    let parent_path = ticket_path.parent().unwrap().to_path_buf();
    fs::remove_dir_all(&ticket_path).unwrap();

    let report = root_store.scan(false).unwrap();

    assert_eq!(report.pruned, 1);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.path.starts_with(&parent_path)
            && diag.reason.contains("missing on disk")
    }));
    assert!(root_store.get_indexed(&ticket_id).unwrap().is_none());
    assert!(root_store.get(&ticket_id).is_err());
    assert!(!root_store
        .search_tickets("Deleted nested visibility", 10)
        .unwrap()
        .iter()
        .any(|result| result.id == ticket_id));
    assert!(!root_store
        .list(None, None, None)
        .unwrap()
        .iter()
        .any(|ticket| ticket.id == ticket_id));
}

#[test]
fn scan_without_reindex_prunes_removed_scan_root_visibility() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child_repo = repo.join("memory-viewers").join("viewer-api");
    fs::create_dir_all(&child_repo).unwrap();

    let root_store = TicketStore::init(&repo).unwrap();
    let ticket_id = {
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
                Some("Removed scan root ticket"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        root_store.scan(true).unwrap();
        ticket_id
    };

    let manifest_path = root_store
        .get_indexed(&ticket_id)
        .unwrap()
        .unwrap()
        .path
        .join("ticket.toml");
    assert!(root_store
        .search_tickets("Removed scan root", 10)
        .unwrap()
        .iter()
        .any(|result| result.id == ticket_id));

    fs::remove_dir_all(&child_repo).unwrap();

    let report = root_store.scan(false).unwrap();

    assert_eq!(report.pruned, 1);
    assert!(report.diagnostics.iter().any(|diag| {
        diag.path == manifest_path
            && diag.reason.contains("missing on disk")
    }));
    assert!(root_store.get_indexed(&ticket_id).unwrap().is_none());
    assert!(root_store.get(&ticket_id).is_err());
    assert!(!root_store
        .search_tickets("Removed scan root", 10)
        .unwrap()
        .iter()
        .any(|result| result.id == ticket_id));
    assert!(!root_store
        .list(None, None, None)
        .unwrap()
        .iter()
        .any(|ticket| ticket.id == ticket_id));
}

#[test]
fn scan_report_includes_phase_timings_and_root_counts() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    store
        .create(
            None,
            "tracker-improvement",
            Some("Profile scan timings"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let report = store.scan(true).unwrap();

    assert!(report.phase_timings_ms.contains_key("scan_total_ms"));
    assert!(report.phase_timings_ms.contains_key("list_scan_roots_ms"));
    assert!(report.phase_timings_ms.contains_key("rebuild_workflow_facts_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("integration.manifest_parse_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("integration.index_upsert_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("integration.edge_write_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("integration.description_read_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("integration.search_upsert_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("workflow.fetch_dependency_edges_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("workflow.fetch_dependency_tickets_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("workflow.compute_unresolved_ms"));
    assert!(report
        .phase_timings_ms
        .contains_key("workflow.write_facts_ms"));
    assert!(report
        .phase_timings_ms
        .keys()
        .any(|key| key.starts_with("scan_root_")));
    assert!(!report.root_entry_counts.is_empty());
}

#[test]
fn scan_without_reindex_skips_workflow_recompute_when_nothing_changed() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Stable blocker"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let dependent = store
        .create(
            None,
            "tracker-improvement",
            Some("Stable dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    store
        .add_edge(EdgeRecord {
            from: dependent,
            to: blocker,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    store.scan(true).unwrap();
    let report = store.scan(false).unwrap();

    assert_eq!(
        report.phase_timings_ms.get("workflow.incremental_root_count"),
        Some(&0)
    );
    assert_eq!(
        report
            .phase_timings_ms
            .get("workflow.incremental_affected_count"),
        Some(&0)
    );
    assert!(!report
        .phase_timings_ms
        .contains_key("workflow.fetch_dependency_edges_ms"));
}

#[test]
fn scan_without_reindex_recomputes_workflow_facts_for_changed_ticket_slice() {
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

    let blocker = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Changed blocker"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let dependent = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Changed dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    child_store
        .add_edge(EdgeRecord {
            from: dependent,
            to: blocker,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    root_store.scan(true).unwrap();
    let initial = root_store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(initial.unresolved_dependency_count, 1);

    child_store.close(&blocker, "done", None).unwrap();

    let report = root_store.scan(false).unwrap();
    assert_eq!(
        report.phase_timings_ms.get("workflow.incremental_root_count"),
        Some(&1)
    );
    assert_eq!(
        report
            .phase_timings_ms
            .get("workflow.incremental_affected_count"),
        Some(&2)
    );

    let updated = root_store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 0);
    assert!(updated.became_actionable_at.is_some());
}

#[test]
fn reconcile_known_tickets_is_noop_for_unchanged_ticket_and_unaffected_rows() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let touched = store
        .create(
            None,
            "tracker-improvement",
            Some("Known reconcile touched"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let unaffected = store
        .create(
            None,
            "tracker-improvement",
            Some("Known reconcile unaffected"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store.scan(true).unwrap();
    let before_touched = store.get_indexed(&touched).unwrap().unwrap().updated_at;
    let before_unaffected = store
        .get_indexed(&unaffected)
        .unwrap()
        .unwrap()
        .updated_at;

    let report = store.reconcile_known_tickets(&[touched]).unwrap();

    assert_eq!(report.integrated, 1);
    assert_eq!(report.pruned, 0);
    assert_eq!(
        report
            .phase_timings_ms
            .get("targeted_reconcile_known_count"),
        Some(&1)
    );
    assert_eq!(
        store.get_indexed(&touched).unwrap().unwrap().updated_at,
        before_touched
    );
    assert_eq!(
        store.get_indexed(&unaffected).unwrap().unwrap().updated_at,
        before_unaffected
    );
}

#[test]
fn reconcile_known_tickets_handles_move_and_updates_affected_dependents() {
    let dir = tempdir().unwrap();
    let source_workspace = dir.path().join("source");
    let target_workspace = dir.path().join("target");
    fs::create_dir_all(&source_workspace).unwrap();
    fs::create_dir_all(&target_workspace).unwrap();

    let source_store = TicketStore::init(&source_workspace).unwrap();
    let target_store = TicketStore::init(&target_workspace).unwrap();

    let blocker = source_store
        .create(
            None,
            "tracker-improvement",
            Some("Moved blocker"),
            Some("done"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let dependent = source_store
        .create(
            None,
            "tracker-improvement",
            Some("Dependent in source"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    source_store
        .add_edge(EdgeRecord {
            from: dependent,
            to: blocker,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    source_store.scan(true).unwrap();
    let initial = source_store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(initial.unresolved_dependency_count, 0);

    let source_path = source_store.get_indexed(&blocker).unwrap().unwrap().path;
    fs::create_dir_all(target_store.index_root.join("tickets")).unwrap();
    let target_path = target_store
        .index_root
        .join("tickets")
        .join(blocker.to_string());
    fs::rename(&source_path, &target_path).unwrap();

    let source_report = source_store.reconcile_known_tickets(&[blocker]).unwrap();
    let target_report = target_store.reconcile_known_tickets(&[blocker]).unwrap();

    assert_eq!(source_report.pruned, 1);
    assert_eq!(source_report.integrated, 0);
    assert_eq!(target_report.integrated, 1);
    assert!(source_store.get_indexed(&blocker).unwrap().is_none());
    assert!(target_store.get_indexed(&blocker).unwrap().is_some());

    let updated = source_store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 1);
}

#[test]
fn scan_force_skips_stale_db_edges_for_missing_ticket_folders() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let source_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Missing source ticket"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let target_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Remaining target ticket"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let legacy_index = RedbIndexStore::open(&store.index_root.join("tickets.db"))
        .unwrap();
    legacy_index
        .insert_edge(&EdgeRecord {
            from: source_id,
            to: target_id,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    let source_path = store.get_indexed(&source_id).unwrap().unwrap().path;
    fs::remove_dir_all(&source_path).unwrap();

    store.scan(true).unwrap();

    assert!(store.get_indexed(&source_id).unwrap().is_none());
    assert!(store.edges_from(&source_id).unwrap().is_empty());
    assert!(store.get_indexed(&target_id).unwrap().is_some());
}

#[test]
fn scan_force_rebuilds_dependency_edges_from_ticket_manifests() {
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
                Some("ready"),
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
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        store
            .add_edge(EdgeRecord {
                from: source_id,
                to: target_id,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();

        let manifest = store.get(&source_id).unwrap();
        let targets = manifest
            .extra
            .get("depends_on")
            .and_then(|value| value.as_array())
            .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].as_str(), Some(target_id.to_string().as_str()));
    }

    fs::remove_file(index_root.join("tickets.db")).unwrap();
    let _ = fs::remove_file(index_root.join("tickets.db-shm"));
    let _ = fs::remove_file(index_root.join("tickets.db-wal"));
    let _ = fs::remove_dir_all(index_root.join("search_index"));

    let rebuilt = TicketStore::init(&index_root).unwrap();
    rebuilt.scan(true).unwrap();

    let edges = rebuilt.edges_from(&source_id).unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to, target_id);
    assert_eq!(edges[0].kind, "depends_on");
}

#[test]
fn open_or_init_bootstraps_manifest_only_workspace() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Bootstrap ticket store"),
            Some("ready"),
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

    let rebuilt = TicketStore::open_or_init(dir.path()).unwrap();
    let manifest = rebuilt.get(&ticket_id).unwrap();

    assert_eq!(manifest.id, ticket_id);
}


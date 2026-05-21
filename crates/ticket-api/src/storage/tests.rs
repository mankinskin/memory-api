use chrono::{
    Duration,
    Utc,
};
use memory_api::model::edge::EdgeRecord;
use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use memory_api::{
    model::filesystem::ScanRoot,
    storage::index::RedbIndexStore,
};
use tempfile::tempdir;
use uuid::Uuid;

use super::{
    TicketStore,
    ticket_fs::TicketFs,
};
use crate::model::{
    manifest_format::format_manifest_toml,
    ticket::TicketManifest,
};

fn canonical_existing_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
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
            Some("in-review"),
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
    poisoned.deleted = true;
    poisoned_index.insert_ticket(&poisoned).unwrap();

    child_store
        .update(
            &ticket_id,
            Default::default(),
            Some("in-review"),
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
    assert!(!indexed.deleted);

    let manifest = root_store.get(&ticket_id).unwrap();
    assert_eq!(
        manifest.extra.get("state").and_then(|value| value.as_str()),
        Some("in-implementation")
    );
}

#[test]
fn scan_without_reindex_repairs_corrupted_nested_workspace_ticket_metadata() {
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
    poisoned.deleted = true;
    poisoned_index.insert_ticket(&poisoned).unwrap();

    root_store.scan(false).unwrap();

    let indexed = root_store.get_indexed(&ticket_id).unwrap().unwrap();
    assert_eq!(indexed.path, expected.path);
    assert_eq!(indexed.type_id, expected.type_id);
    assert_eq!(indexed.title, expected.title);
    assert_eq!(indexed.state, expected.state);
    assert_eq!(indexed.created_at, expected.created_at);
    assert!(!indexed.deleted);
    assert!(root_store.get(&ticket_id).is_ok());
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

#[test]
fn scan_force_prunes_existing_row_for_deleted_ticket_manifest() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Deleted on disk"),
            Some("in-review"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let ticket_path = store.get_indexed(&ticket_id).unwrap().unwrap().path;
    TicketFs::mark_deleted(&ticket_path).unwrap();

    store.scan(true).unwrap();

    assert!(store.get_indexed(&ticket_id).unwrap().is_none());
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

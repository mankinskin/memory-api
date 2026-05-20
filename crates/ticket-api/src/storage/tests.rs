use std::fs;
use std::path::Path;

use tempfile::tempdir;

use super::TicketStore;

#[test]
fn open_creates_gitignore_for_local_ticket_artifacts() {
    let dir = tempdir().unwrap();

    TicketStore::init(dir.path()).unwrap();

    let gitignore =
        fs::read_to_string(dir.path().join(".gitignore")).unwrap();
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
        root.path == dir.path().join("tickets") && root.label == "tickets"
    }));
}

#[test]
fn open_uses_existing_hidden_ticket_store_from_repo_root() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let store_root = repo.join(".ticket");
    fs::create_dir_all(&store_root).unwrap();

    let store = TicketStore::init(&repo).unwrap();

    assert_eq!(store.index_root, store_root);
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

    assert!(indexed.path.starts_with(store_root.join("tickets")));
    assert!(!indexed.path.starts_with(Path::new(&repo)) || indexed.path.starts_with(store_root.join("tickets")));
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
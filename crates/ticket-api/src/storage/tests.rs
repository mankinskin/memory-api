use std::fs;

use tempfile::tempdir;

use super::TicketStore;

#[test]
fn open_creates_gitignore_for_local_ticket_artifacts() {
    let dir = tempdir().unwrap();

    TicketStore::open(dir.path()).unwrap();

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
    let store = TicketStore::open(dir.path()).unwrap();

    let roots = store.list_scan_roots().unwrap();

    assert!(roots.iter().any(|root| {
        root.path == dir.path().join("tickets") && root.label == "tickets"
    }));
}
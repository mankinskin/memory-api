use super::*;

use std::path::PathBuf;

use tempfile::tempdir;

use super::*;
use crate::model::filesystem::ScanRoot;

#[test]
fn recovers_ticket_paths_from_relative_index_entries() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let store_root = repo.join("viewer").join(".ticket");
    std::fs::create_dir_all(&store_root).unwrap();

    let store = TicketStore::init(&store_root).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Recover relative ticket paths"),
            None,
            Default::default(),
            None,
            Some("Detailed context for recovery test."),
        )
        .unwrap();

    let absolute_scan_root = store.index_root.join("tickets");
    let absolute_ticket_path = absolute_scan_root.join(ticket_id.to_string());
    let relative_scan_root = PathBuf::from("viewer/.ticket/tickets");

    store
        .index
        .add_scan_root(&ScanRoot {
            path: relative_scan_root.clone(),
            label: "relative".to_string(),
        })
        .unwrap();

    let mut indexed = store.index.get_ticket(&ticket_id).unwrap().unwrap();
    indexed.path = relative_scan_root.join(ticket_id.to_string());
    store.index.insert_ticket(&indexed).unwrap();

    let roots = store.list_scan_roots().unwrap();
    assert_eq!(
        roots
            .iter()
            .filter(|root| root.path == absolute_scan_root)
            .count(),
        1
    );
    assert!(roots.iter().all(|root| root.path.is_absolute()));

    let indexed = store.get_indexed(&ticket_id).unwrap().unwrap();
    assert_eq!(indexed.path, absolute_ticket_path);
    assert_eq!(
        TicketFs::read_description(&indexed.path).as_deref(),
        Some("Detailed context for recovery test.")
    );
    assert!(store.get(&ticket_id).is_ok());
}

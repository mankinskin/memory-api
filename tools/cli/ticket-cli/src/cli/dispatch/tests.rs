use std::collections::BTreeMap;

use super::*;
use crate::cli::{
    IdArgs,
    ListArgs,
    ScanArgs,
    TextArgs,
};
use tempfile::tempdir;
use ticket_api::storage::index::RedbIndexStore;
use uuid::Uuid;

fn create_nested_ticket_fixture()
-> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child = repo.join("memory-api");
    std::fs::create_dir_all(&child).unwrap();

    let _root_store = TicketStore::init(&repo.join(".ticket")).unwrap();
    let child_store = TicketStore::init(&child.join(".ticket")).unwrap();
    let ticket_id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("Nested workspace ticket"),
            None,
            BTreeMap::<String, serde_json::Value>::new(),
            None,
            Some("Nested workspace ticket body"),
        )
        .unwrap();

    (dir, repo, child, ticket_id.to_string())
}

#[test]
fn dry_run_payload_is_returned_for_mutating_command() {
    let payload = dry_run_command_payload(&TicketCommandCli::Delete(IdArgs {
        id: Uuid::new_v4().to_string(),
        view: None,
        parts: None,
    }))
    .expect("delete should be dry-runnable");
    assert_eq!(payload["dry_run"], json!(true));
    assert_eq!(payload["command"], json!("delete"));
}

#[test]
fn dry_run_payload_is_none_for_read_only_command() {
    let payload = dry_run_command_payload(&TicketCommandCli::Leases);
    assert!(payload.is_none());
}

#[test]
fn resolve_index_root_prefers_explicit_workspace_root() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child = repo.join("memory-api");
    std::fs::create_dir_all(repo.join(".ticket")).unwrap();
    std::fs::create_dir_all(child.join(".ticket")).unwrap();

    let resolved =
        resolve_index_root_from(None, Some(&child), None, Some(&repo));

    assert_eq!(resolved, child.join(".ticket"));
}

#[test]
fn resolve_index_root_prefers_explicit_index_root_over_workspace_root() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child = repo.join("memory-api");
    std::fs::create_dir_all(repo.join(".ticket")).unwrap();
    std::fs::create_dir_all(child.join(".ticket")).unwrap();

    let resolved = resolve_index_root_from(
        Some(&repo.join(".ticket")),
        Some(&child),
        None,
        Some(&repo),
    );

    assert_eq!(resolved, repo.join(".ticket"));
}

#[test]
fn resolve_index_root_preserves_relative_explicit_index_root() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    let child = repo.join("memory-api");
    std::fs::create_dir_all(repo.join(".ticket")).unwrap();
    std::fs::create_dir_all(child.join(".ticket")).unwrap();

    let resolved = resolve_index_root_from(
        Some(Path::new(".ticket")),
        Some(&child),
        None,
        Some(&repo),
    );

    assert_eq!(resolved, repo.join(".ticket"));
}

#[test]
fn dispatch_get_reads_child_ticket_from_explicit_workspace_root() {
    let (_dir, _repo, child, ticket_id) = create_nested_ticket_fixture();

    let payload = dispatch(
        TicketCommandCli::Get(IdArgs {
            id: ticket_id.clone(),
            view: None,
            parts: None,
        }),
        None,
        Some(&child),
        None,
        true,
        false,
    )
    .unwrap();

    assert_eq!(payload["command"], "get");
    assert_eq!(payload["ticket"]["id"], ticket_id);
    assert_eq!(
        payload["ticket"]["fields"]["title"],
        "Nested workspace ticket"
    );
}

#[test]
fn dispatch_search_reads_child_ticket_from_explicit_workspace_root() {
    let (_dir, _repo, child, ticket_id) = create_nested_ticket_fixture();

    let payload = dispatch(
        TicketCommandCli::Search(TextArgs {
            expression: "Nested workspace ticket".to_string(),
            limit: 10,
        }),
        None,
        Some(&child),
        None,
        true,
        false,
    )
    .unwrap();

    assert_eq!(payload["command"], "search");
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["results"][0]["id"], ticket_id);
}

#[test]
fn dispatch_list_reads_child_ticket_from_explicit_workspace_root() {
    let (_dir, _repo, child, ticket_id) = create_nested_ticket_fixture();

    let payload = dispatch(
        TicketCommandCli::List(ListArgs {
            state: None,
            ticket_type: None,
            limit: Some(10),
            with_repro: false,
            where_clauses: Vec::new(),
        }),
        None,
        Some(&child),
        None,
        true,
        false,
    )
    .unwrap();

    assert_eq!(payload["command"], "list");
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["items"][0]["id"], ticket_id);
}

#[test]
fn dispatch_list_repairs_existing_empty_root_index() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let store = TicketStore::init(&repo).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Root workspace ticket"),
            None,
            BTreeMap::<String, serde_json::Value>::new(),
            None,
            Some("Root workspace ticket body"),
        )
        .unwrap();

    let index_root = store.index_root.clone();
    drop(store);

    std::fs::remove_file(index_root.join("tickets.db")).unwrap();
    let _ = std::fs::remove_file(index_root.join("tickets.db-shm"));
    let _ = std::fs::remove_file(index_root.join("tickets.db-wal"));
    let _ = std::fs::remove_dir_all(index_root.join("search_index"));
    RedbIndexStore::open(&index_root.join("tickets.db")).unwrap();

    let payload = dispatch(
        TicketCommandCli::List(ListArgs {
            state: None,
            ticket_type: None,
            limit: Some(10),
            with_repro: false,
            where_clauses: Vec::new(),
        }),
        None,
        Some(&repo),
        None,
        true,
        false,
    )
    .unwrap();

    assert_eq!(payload["command"], "list");
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["items"][0]["id"], ticket_id.to_string());
}

#[test]
fn dispatch_scan_registers_child_ticket_from_explicit_workspace_root() {
    let (_dir, repo, _child, ticket_id) = create_nested_ticket_fixture();

    let payload = dispatch(
        TicketCommandCli::Scan(ScanArgs {
            reindex: false,
            force: false,
        }),
        None,
        Some(&repo),
        None,
        true,
        false,
    )
    .unwrap();

    assert_eq!(payload["command"], "scan");

    let root_store = TicketStore::open(&repo.join(".ticket")).unwrap();
    let search_payload = dispatch_store_command(
        TicketCommandCli::Search(TextArgs {
            expression: "Nested workspace ticket".to_string(),
            limit: 10,
        }),
        root_store,
        false,
    )
    .unwrap();

    assert_eq!(search_payload["command"], "search");
    assert_eq!(search_payload["count"], 1);
    assert_eq!(search_payload["results"][0]["id"], ticket_id);
}

#[test]
fn dispatch_get_reads_child_ticket_after_scan_root_augmentation() {
    let (_dir, repo, _child, ticket_id) = create_nested_ticket_fixture();
    let root_store = TicketStore::open(&repo.join(".ticket")).unwrap();

    let reindex = register_descendant_scan_roots(&root_store, &repo).unwrap();
    assert!(reindex);
    root_store.scan(true).unwrap();

    let payload = dispatch_store_command(
        TicketCommandCli::Get(IdArgs {
            id: ticket_id.clone(),
            view: None,
            parts: None,
        }),
        root_store,
        false,
    )
    .unwrap();

    assert_eq!(payload["command"], "get");
    assert_eq!(payload["ticket"]["id"], ticket_id);
}

#[test]
fn dispatch_search_reads_child_ticket_after_scan_root_augmentation() {
    let (_dir, repo, _child, ticket_id) = create_nested_ticket_fixture();
    let root_store = TicketStore::open(&repo.join(".ticket")).unwrap();

    let reindex = register_descendant_scan_roots(&root_store, &repo).unwrap();
    assert!(reindex);
    root_store.scan(true).unwrap();

    let payload = dispatch_store_command(
        TicketCommandCli::Search(TextArgs {
            expression: "Nested workspace ticket".to_string(),
            limit: 10,
        }),
        root_store,
        false,
    )
    .unwrap();

    assert_eq!(payload["command"], "search");
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["results"][0]["id"], ticket_id);
}

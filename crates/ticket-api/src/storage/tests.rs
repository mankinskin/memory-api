use chrono::{
    Duration,
    Utc,
};
use memory_api::model::edge::EdgeRecord;
use serde_json::Value;
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
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

use super::TicketStore;
use crate::model::{
    manifest_format::format_manifest_toml,
    ticket::TicketManifest,
};

fn canonical_existing_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

fn known_id_set(ids: &[Uuid]) -> BTreeSet<Uuid> {
    ids.iter().copied().collect()
}

fn corrupt_search_index_meta(index_root: &Path) {
    let search_dir = index_root.join("search_index");
    fs::create_dir_all(&search_dir).unwrap();
    fs::write(search_dir.join("meta.json"), b"not valid json").unwrap();
}

fn assert_visibility_surfaces_agree(
    store: &TicketStore,
    query: &str,
    known_ids: &[Uuid],
    expected_ids: &[Uuid],
) {
    let known_ids = known_id_set(known_ids);
    let expected_ids = known_id_set(expected_ids);
    let search_ids: BTreeSet<Uuid> = store
        .search_tickets(query, known_ids.len().saturating_mul(4).max(8))
        .unwrap()
        .into_iter()
        .map(|ticket| ticket.id)
        .filter(|id| known_ids.contains(id))
        .collect();
    let list_ids: BTreeSet<Uuid> = store
        .list(None, None, None)
        .unwrap()
        .into_iter()
        .map(|ticket| ticket.id)
        .filter(|id| known_ids.contains(id))
        .collect();
    let indexed_ids: BTreeSet<Uuid> = store
        .get_indexed_many(&known_ids.iter().copied().collect::<Vec<_>>())
        .unwrap()
        .keys()
        .copied()
        .collect();

    assert_eq!(search_ids, expected_ids, "search visibility drifted");
    assert_eq!(list_ids, expected_ids, "list visibility drifted");
    assert_eq!(indexed_ids, expected_ids, "indexed visibility drifted");

    for id in &known_ids {
        if expected_ids.contains(id) {
            let _indexed = store.get_indexed(id).unwrap().unwrap();
            assert!(store.get(id).is_ok(), "visible ticket {id} should be readable");
        } else {
            assert!(
                store.get_indexed(id).unwrap().is_none(),
                "hidden ticket {id} should not remain indexed"
            );
            assert!(store.get(id).is_err(), "hidden ticket {id} should not be readable");
        }
    }
}

fn assert_ticket_title_and_state(
    store: &TicketStore,
    query: &str,
    id: Uuid,
    expected_title: &str,
    expected_state: &str,
) {
    let search_result = store
        .search_tickets(query, 20)
        .unwrap()
        .into_iter()
        .find(|ticket| ticket.id == id)
        .unwrap();
    assert_eq!(search_result.title.as_deref(), Some(expected_title));
    assert_eq!(search_result.state.as_deref(), Some(expected_state));

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.title.as_deref(), Some(expected_title));
    assert_eq!(indexed.state.as_deref(), Some(expected_state));

    let manifest = store.get(&id).unwrap();
    assert_eq!(
        manifest.extra.get("title").and_then(|value| value.as_str()),
        Some(expected_title)
    );
    assert_eq!(
        manifest.extra.get("state").and_then(|value| value.as_str()),
        Some(expected_state)
    );
}

#[test]
fn workflow_facts_set_became_actionable_at_when_blockers_resolve() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Blocking prerequisite"),
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
            Some("Blocked dependent"),
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

    let initial = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(initial.unresolved_dependency_count, 1);
    assert!(initial.became_actionable_at.is_none());

    store.close(&blocker, "done", None).unwrap();

    let updated = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 0);
    assert!(updated.became_actionable_at.is_some());
    assert!(updated.last_blocker_progress_at.is_none());
}

#[test]
fn workflow_facts_set_last_blocker_progress_at_while_ticket_remains_blocked() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let progressing_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Progressing blocker"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let persistent_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Persistent blocker"),
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
            Some("Still blocked dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    for blocker in [progressing_blocker, persistent_blocker] {
        store
            .add_edge(EdgeRecord {
                from: dependent,
                to: blocker,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();
    }

    let initial = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(initial.unresolved_dependency_count, 2);
    assert!(initial.last_blocker_progress_at.is_none());

    store
        .update(
            &progressing_blocker,
            BTreeMap::new(),
            Some(&[]),
            Some("ready"),
            None,
            None,
        )
        .unwrap();

    let updated = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 2);
    assert!(updated.last_blocker_progress_at.is_some());
    assert!(updated.became_actionable_at.is_none());
}

#[test]
fn update_allows_reverse_transitions_from_terminal_states() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let done_ticket = store
        .create(
            None,
            "tracker-improvement",
            Some("Reopen done ticket"),
            Some("done"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    store
        .update(
            &done_ticket,
            BTreeMap::new(),
            Some(&[]),
            Some("in-review"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        store.get_indexed(&done_ticket).unwrap().unwrap().state.as_deref(),
        Some("in-review")
    );

    let cancelled_ticket = store
        .create(
            None,
            "tracker-improvement",
            Some("Reopen cancelled ticket"),
            Some("cancelled"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    store
        .update(
            &cancelled_ticket,
            BTreeMap::new(),
            Some(&[]),
            Some("new"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        store
            .get_indexed(&cancelled_ticket)
            .unwrap()
            .unwrap()
            .state
            .as_deref(),
        Some("new")
    );
}

#[test]
fn workflow_facts_follow_depends_on_edge_removal() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Transient blocker"),
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
            Some("Edge-driven dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let edge = EdgeRecord {
        from: dependent,
        to: blocker,
        kind: "depends_on".to_string(),
        created_at: Utc::now(),
    };

    store.add_edge(edge.clone()).unwrap();
    assert_eq!(
        store
            .get_workflow_facts(&dependent)
            .unwrap()
            .unwrap()
            .unresolved_dependency_count,
        1
    );

    store.remove_edge(edge).unwrap();

    let updated = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 0);
    assert!(updated.became_actionable_at.is_some());
    assert!(updated.last_blocker_progress_at.is_none());
}

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
            Some("ready"),
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
            Some("ready"),
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
            Some("ready"),
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
    poisoned.state = Some("new".to_string());
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

    let delete_path = child_store_a.get_indexed(&delete_id).unwrap().unwrap().path;
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
            Some("ready"),
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
        report.phase_timings_ms.get("workflow.incremental_root_count"),
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
    assert!(root_store
        .search_tickets("Nested workspace ticket", 10)
        .unwrap()
        .iter()
        .any(|result| {
            result.id == ticket_id
                && result.title.as_deref() == Some("Nested workspace ticket")
                && result.state.as_deref() == Some("in-implementation")
        }));
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

#[test]
fn open_or_init_profiled_reports_bootstrap_scan_timings() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Profile bootstrap open_or_init"),
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

    let (rebuilt, report) = TicketStore::open_or_init_profiled(dir.path()).unwrap();

    assert!(report.initialized_store);
    assert!(report.phase_timings_ms.contains_key("open_or_init_total_ms"));
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
            Some("ready"),
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
            Some("ready"),
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
            Some("Legacy target ticket"),
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

#[test]
fn bug_7f4aaa05_state_preserved_on_field_patch_without_to_state() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    // Create ticket
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Advance to ready
    store
        .update(&id, BTreeMap::new(), Some(&[]), Some("ready"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("ready"));

    // BUG: Update description WITHOUT to_state - state should be preserved
    let mut patch = BTreeMap::new();
    patch.insert("custom_field".to_string(), Value::String("custom value".to_string()));

    store
        .update(&id, patch, None, None, None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(
        indexed.state.as_deref(),
        Some("ready"),
        "State should be preserved when patching fields without to_state"
    );
}

#[test]
fn bug_7f4aaa05_description_patch_with_to_state_transition() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    // Create ticket
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Advance to ready
    store
        .update(&id, BTreeMap::new(), Some(&[]), Some("ready"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("ready"));

    // Combined: patch fields AND transition in one call
    let mut patch = BTreeMap::new();
    patch.insert("custom_field".to_string(), Value::String("custom value".to_string()));

    store
        .update(&id, patch, None, Some("in-implementation"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(
        indexed.state.as_deref(),
        Some("in-implementation"),
        "State should transition to in-implementation"
    );
    assert_eq!(
        indexed.title.as_deref(),
        Some("Test ticket"),
        "Title should be preserved"
    );
}

#[test]
fn bug_7f4aaa05_transition_states_multi_step_path() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    // Create ticket
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Multi-step transition: new -> ready
    let transition_states = vec!["ready".to_string()];
    store
        .update(
            &id,
            BTreeMap::new(),
            Some(transition_states.as_slice()),
            None, // NO to_state
            None,
            None,
        )
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(
        indexed.state.as_deref(),
        Some("ready"),
        "transition_states should apply the final state from the path"
    );
}

#[test]
fn update_allows_reachable_multi_step_without_transition_states() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Reachable multi-step forward"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .update(
            &id,
            BTreeMap::new(),
            None,
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("in-implementation"));
}

#[test]
fn update_allows_reachable_reverse_multi_step_without_transition_states() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Reachable multi-step reverse"),
            Some("in-implementation"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .update(
            &id,
            BTreeMap::new(),
            None,
            Some("new"),
            None,
            None,
        )
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("new"));
}

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
            Some("ready"),
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
            Some("ready"),
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



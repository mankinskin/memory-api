use chrono::{
    Duration,
    Utc,
};
use memory_kernel::model::edge::EdgeRecord;
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

use memory_kernel::{
    model::filesystem::ScanRoot,
    storage::index::RedbIndexStore,
};
use tempfile::tempdir;
use uuid::Uuid;

use super::{
    DescriptionUpdate,
    DescriptionUpdateMode,
    REQUIRED_DESCRIPTION_MODE_ERROR,
    TicketStore,
};
use crate::model::{
    manifest_format::format_manifest_toml,
    ticket::{
        TicketManifest,
        TicketManifestExt,
    },
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
            assert!(
                store.get(id).is_ok(),
                "visible ticket {id} should be readable"
            );
        } else {
            assert!(
                store.get_indexed(id).unwrap().is_none(),
                "hidden ticket {id} should not remain indexed"
            );
            assert!(
                store.get(id).is_err(),
                "hidden ticket {id} should not be readable"
            );
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

mod history_append_failure_tests;
mod part_kind_rejection_tests;
mod policy_tests;
mod projection_tests;
mod recovery_tests;
mod scan_reconcile_tests;
mod scan_visibility_tests;
mod update_regression_tests;
mod workflow_tests;

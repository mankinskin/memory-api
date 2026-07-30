use super::{
    WorkspaceRegistry,
    WorkspaceResolveError,
    workspace_root_for_index_root,
};
use std::{
    collections::BTreeMap,
    sync::Arc,
};
use ticket_api::{
    model::filesystem::ScanRoot,
    storage::store::TicketStore,
};

#[test]
fn descendant_workspaces_use_workspace_root_name() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store =
        Arc::new(TicketStore::init(root.path()).expect("open parent store"));
    parent_store
        .add_scan_root(ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let child_index_root = root.path().join("child").join(".ticket");
    std::fs::create_dir_all(child_index_root.join("tickets"))
        .expect("mkdir child store");
    let child_store = Arc::new(
        TicketStore::init(&child_index_root).expect("open child store"),
    );
    child_store
        .add_scan_root(ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root");

    parent_store
        .add_scan_root(ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root to parent");

    let registry = WorkspaceRegistry::single_opened(Arc::clone(&parent_store));
    let workspace_infos = registry.workspace_infos();
    let root_workspace = workspace_root_for_index_root(root.path())
        .file_name()
        .and_then(|name| name.to_str())
        .expect("root workspace name")
        .to_string();
    assert!(workspace_infos.iter().any(|info| info.label == "child"));
    assert!(
        workspace_infos
            .iter()
            .any(|info| info.label == root_workspace)
    );
    assert!(!workspace_infos.iter().any(|info| info.label == "tickets"));

    let child_id = registry
        .workspace_infos()
        .into_iter()
        .find(|info| info.label == "child")
        .expect("child workspace info")
        .name;

    let rejected = registry
        .resolve_workspace_name("child")
        .expect_err("display labels should not resolve as public ids");
    assert_eq!(
        rejected,
        WorkspaceResolveError::DisplayLabelRejected {
            requested: "child".to_string(),
            canonical: child_id,
        }
    );
}

#[test]
fn duplicate_basename_workspaces_receive_distinct_public_ids() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store =
        Arc::new(TicketStore::init(root.path()).expect("open parent store"));
    parent_store
        .add_scan_root(ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let left_index_root =
        root.path().join("alpha").join("shared").join(".ticket");
    let right_index_root =
        root.path().join("beta").join("shared").join(".ticket");
    std::fs::create_dir_all(left_index_root.join("tickets"))
        .expect("mkdir left store");
    std::fs::create_dir_all(right_index_root.join("tickets"))
        .expect("mkdir right store");
    TicketStore::init(&left_index_root).expect("open left store");
    TicketStore::init(&right_index_root).expect("open right store");

    parent_store
        .add_scan_root(ScanRoot {
            path: left_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add left scan root");
    parent_store
        .add_scan_root(ScanRoot {
            path: right_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add right scan root");

    let registry = WorkspaceRegistry::single_opened(parent_store);
    let shared_workspaces = registry
        .workspace_infos()
        .into_iter()
        .filter(|info| info.label == "shared")
        .collect::<Vec<_>>();

    assert_eq!(shared_workspaces.len(), 2);
    assert_ne!(shared_workspaces[0].name, shared_workspaces[1].name);
    assert!(
        shared_workspaces
            .iter()
            .all(|info| info.name.starts_with("shared--"))
    );

    let ambiguous = registry
        .resolve_workspace_name("shared")
        .expect_err("duplicate basename should be ambiguous");
    assert_eq!(
        ambiguous,
        WorkspaceResolveError::AmbiguousLegacyLabel {
            requested: "shared".to_string(),
            matches: shared_workspaces
                .iter()
                .map(|info| info.name.clone())
                .collect(),
        }
    );
}

#[test]
fn nested_workspace_path_alias_resolves_to_canonical_workspace_id() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store =
        Arc::new(TicketStore::init(root.path()).expect("open parent store"));
    parent_store
        .add_scan_root(ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let child_index_root = root
        .path()
        .join("memory-viewers")
        .join("memory-api")
        .join(".ticket");
    std::fs::create_dir_all(child_index_root.join("tickets"))
        .expect("mkdir child store");
    TicketStore::init(&child_index_root).expect("open child store");

    parent_store
        .add_scan_root(ScanRoot {
            path: child_index_root.join("tickets"),
            label: "memory-viewers/memory-api".to_string(),
        })
        .expect("add child scan root to parent");

    let registry = WorkspaceRegistry::single_opened(parent_store);
    let child_id = registry
        .workspace_infos()
        .into_iter()
        .find(|info| info.label == "memory-api")
        .expect("child workspace info")
        .name;

    assert_eq!(
        registry
            .resolve_workspace_name("memory-viewers/memory-api")
            .expect("nested path alias should resolve"),
        Some(child_id),
    );
}

#[test]
fn manifest_only_hidden_child_store_is_discovered() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store =
        Arc::new(TicketStore::init(root.path()).expect("open parent store"));
    parent_store
        .add_scan_root(ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let child_index_root = root
        .path()
        .join("memory-viewers")
        .join("memory-api")
        .join(".ticket");
    std::fs::create_dir_all(child_index_root.join("tickets"))
        .expect("mkdir child store");
    let hidden_ticket_dir = child_index_root.join("tickets").join("hidden");
    std::fs::create_dir_all(&hidden_ticket_dir)
        .expect("mkdir hidden ticket dir");
    std::fs::write(hidden_ticket_dir.join("ticket.toml"), "")
        .expect("write hidden ticket manifest");

    parent_store
        .add_scan_root(ScanRoot {
            path: child_index_root.join("tickets"),
            label: "memory-viewers/memory-api".to_string(),
        })
        .expect("add child scan root to parent");

    let registry = WorkspaceRegistry::single_opened(parent_store);

    assert!(
        registry
            .workspace_infos()
            .into_iter()
            .any(|info| info.label == "memory-api")
    );
}

#[test]
fn resolve_indexed_many_prefers_deepest_existing_workspace() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store =
        Arc::new(TicketStore::init(root.path()).expect("open parent store"));
    parent_store
        .add_scan_root(ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let child_index_root = root.path().join("child").join(".ticket");
    std::fs::create_dir_all(child_index_root.join("tickets"))
        .expect("mkdir child store");
    let child_store = Arc::new(
        TicketStore::init(&child_index_root).expect("open child store"),
    );
    child_store
        .add_scan_root(ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root");

    let ticket_id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("child-owned ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create child ticket");

    parent_store
        .add_scan_root(ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root to parent");
    parent_store.scan(true).expect("scan parent store");

    let registry = WorkspaceRegistry::single_opened(Arc::clone(&parent_store));
    let resolved = registry
        .resolve_indexed_many(registry.primary_workspace_name(), &[ticket_id])
        .expect("resolve ticket");
    let resolved = resolved.get(&ticket_id).expect("resolved ticket");

    assert_eq!(
        resolved.workspace,
        crate::serve::registry::canonical_workspace_name_for_index_root(
            &child_index_root,
            "workspace",
        )
    );
    assert!(resolved.ticket.path.join("ticket.toml").is_file());
}

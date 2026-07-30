use axum::{
    body::to_bytes,
    extract::{
        Extension,
        Path,
        Query,
        State,
    },
    http::StatusCode,
};
use std::{
    collections::BTreeMap,
    sync::Arc,
};
use ticket_api::storage::search::TantivySearchIndex;
use uuid::Uuid;
use viewer_api::error::RequestIdExt;

use super::{
    super::{
        TicketAssetParam,
        TicketIdParam,
        WorkspaceParam,
        get_ticket,
        get_ticket_asset,
        get_ticket_description,
        get_ticket_history,
        list_ticket_files,
        list_tickets,
    },
    make_state,
    make_store,
};

#[tokio::test]
async fn list_tickets_uses_scan_root_label_for_ticket_ref_workspace() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());
    let child_root = dir.path().join("child").join("tickets");
    std::fs::create_dir_all(&child_root).expect("mkdir child root");

    store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_root.clone(),
            label: "child".to_string(),
        })
        .expect("add child scan root");

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("child-owned ticket"),
            Some("planned"),
            BTreeMap::new(),
            Some(child_root.as_path()),
            None,
        )
        .expect("create child ticket");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();
    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-child".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("child-owned".to_string()),
            limit: Some(10),
            cursor: None,
        }),
    )
    .await;

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json body");

    assert_eq!(payload["items"][0]["ticket_ref"]["workspace"], "child");
    assert_eq!(payload["items"][0]["ticket_ref"]["id"], id.to_string());
}

#[tokio::test]
async fn search_list_prefers_authoritative_mixed_workspace_hit() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(root.path())
            .expect("open parent store"),
    );
    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let child_index_root = root.path().join("child").join(".ticket");
    std::fs::create_dir_all(child_index_root.join("tickets"))
        .expect("mkdir child store");
    let child_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(&child_index_root)
            .expect("open child store"),
    );
    child_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root");

    let id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("mixed-workspace child authoritative ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            Some("mixed-workspace authoritative description"),
        )
        .expect("create child ticket");

    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root to parent");
    parent_store.scan(true).expect("scan parent store");

    let poisoned_index = ticket_api::storage::index::RedbIndexStore::open(
        &parent_store.index_root.join("tickets.db"),
    )
    .expect("open parent index");
    let mut poisoned = parent_store
        .get_indexed(&id)
        .expect("get parent indexed ticket")
        .expect("parent indexed ticket");
    poisoned.path =
        parent_store.index_root.join("tickets").join(id.to_string());
    poisoned.title =
        Some("mixed-workspace stale parent placeholder".to_string());
    poisoned.state = Some("open".to_string());
    poisoned_index
        .insert_ticket(&poisoned)
        .expect("poison parent indexed row");
    TantivySearchIndex::open_or_create(
        &parent_store.index_root.join("search_index"),
    )
    .expect("open parent search index")
    .upsert(
        &id,
        Some("mixed-workspace stale parent placeholder"),
        Some("mixed-workspace stale parent body"),
        Some("open"),
        Some("tracker-improvement"),
        Some(&chrono::Utc::now().to_rfc3339()),
        None,
    )
    .expect("upsert stale parent search doc");

    let state = make_state(Arc::clone(&parent_store));
    let workspace = state.registry.primary_workspace_name().to_string();
    let child_workspace = state
        .registry
        .workspace_infos()
        .into_iter()
        .find(|info| info.label == "child")
        .expect("child workspace info")
        .name;
    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-authoritative-hit".to_string())),
        Query(WorkspaceParam {
            workspace,
            state: None,
            query: Some("mixed-workspace".to_string()),
            limit: Some(10),
            cursor: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json body");
    let items = payload["items"].as_array().expect("items array");

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0]["ticket_ref"]["workspace"],
        child_workspace.as_str()
    );
    assert_eq!(items[0]["ticket_ref"]["id"], id.to_string());
    assert_eq!(
        items[0]["title"],
        serde_json::Value::String(
            "mixed-workspace child authoritative ticket".to_string()
        )
    );
    assert_eq!(items[0]["state"], "planned");
    assert_ne!(items[0]["created_at"], "1970-01-01T00:00:00Z");
}

#[tokio::test]
async fn get_ticket_and_history_include_ticket_refs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("detail ticket"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();
    let detail = get_ticket(
        State(state.clone()),
        Extension(RequestIdExt("rid-detail".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    let detail_bytes = to_bytes(detail.into_body(), 1024 * 1024)
        .await
        .expect("detail body");
    let detail_payload: serde_json::Value =
        serde_json::from_slice(&detail_bytes).expect("detail json");

    assert_eq!(detail_payload["active_workspace"], workspace.clone());
    assert_eq!(
        detail_payload["ticket"]["ticket_ref"]["workspace"],
        workspace.clone()
    );
    assert_eq!(detail_payload["ticket"]["ticket_ref"]["id"], id.to_string());

    let history = get_ticket_history(
        State(state),
        Extension(RequestIdExt("rid-history".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    let history_bytes = to_bytes(history.into_body(), 1024 * 1024)
        .await
        .expect("history body");
    let history_payload: serde_json::Value =
        serde_json::from_slice(&history_bytes).expect("history json");

    assert_eq!(history_payload["active_workspace"], workspace.clone());
    assert_eq!(history_payload["ticket_ref"]["workspace"], workspace);
    assert_eq!(history_payload["ticket_ref"]["id"], id.to_string());
}

#[tokio::test]
async fn mixed_workspace_search_followups_remain_reversible() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(root.path())
            .expect("open parent store"),
    );
    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let child_index_root = root.path().join("child").join(".ticket");
    std::fs::create_dir_all(child_index_root.join("tickets"))
        .expect("mkdir child store");
    let child_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(&child_index_root)
            .expect("open child store"),
    );
    child_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root");

    let id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("mixed-workspace child authoritative ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            Some("mixed-workspace child description"),
        )
        .expect("create child ticket");
    let ticket_dir = child_store
        .get_indexed(&id)
        .expect("get child indexed ticket")
        .expect("child indexed ticket");
    std::fs::create_dir_all(ticket_dir.path.join("assets"))
        .expect("mkdir assets");
    std::fs::write(
        ticket_dir.path.join("assets").join("plan.md"),
        "nested child asset",
    )
    .expect("write child asset");

    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root to parent");
    parent_store.scan(true).expect("scan parent store");

    let poisoned_index = ticket_api::storage::index::RedbIndexStore::open(
        &parent_store.index_root.join("tickets.db"),
    )
    .expect("open parent index");
    let mut poisoned = parent_store
        .get_indexed(&id)
        .expect("get parent indexed ticket")
        .expect("parent indexed ticket");
    poisoned.path =
        parent_store.index_root.join("tickets").join(id.to_string());
    poisoned.title =
        Some("mixed-workspace stale parent placeholder".to_string());
    poisoned.state = Some("open".to_string());
    poisoned_index
        .insert_ticket(&poisoned)
        .expect("poison parent indexed row");
    TantivySearchIndex::open_or_create(
        &parent_store.index_root.join("search_index"),
    )
    .expect("open parent search index")
    .upsert(
        &id,
        Some("mixed-workspace stale parent placeholder"),
        Some("mixed-workspace stale parent body"),
        Some("open"),
        Some("tracker-improvement"),
        Some(&chrono::Utc::now().to_rfc3339()),
        None,
    )
    .expect("upsert stale parent search doc");

    let state = make_state(Arc::clone(&parent_store));
    let workspace = state.registry.primary_workspace_name().to_string();
    let child_workspace = state
        .registry
        .workspace_infos()
        .into_iter()
        .find(|info| info.label == "child")
        .expect("child workspace info")
        .name;

    let list = list_tickets(
        State(state.clone()),
        Extension(RequestIdExt("rid-nested-list".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("mixed-workspace".to_string()),
            limit: Some(10),
            cursor: None,
        }),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_bytes = to_bytes(list.into_body(), 1024 * 1024)
        .await
        .expect("list body");
    let list_payload: serde_json::Value =
        serde_json::from_slice(&list_bytes).expect("list json");
    assert_eq!(
        list_payload["items"][0]["title"],
        serde_json::Value::String(
            "mixed-workspace child authoritative ticket".to_string()
        )
    );
    assert_eq!(
        list_payload["items"][0]["ticket_ref"]["workspace"],
        child_workspace.as_str()
    );
    assert_eq!(list_payload["items"][0]["ticket_ref"]["id"], id.to_string());

    let detail = get_ticket(
        State(state.clone()),
        Extension(RequestIdExt("rid-nested-detail".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_bytes = to_bytes(detail.into_body(), 1024 * 1024)
        .await
        .expect("detail body");
    let detail_payload: serde_json::Value =
        serde_json::from_slice(&detail_bytes).expect("detail json");
    assert_eq!(
        detail_payload["ticket"]["ticket_ref"]["workspace"],
        child_workspace.as_str()
    );
    assert_eq!(detail_payload["ticket"]["ticket_ref"]["id"], id.to_string());

    let description = get_ticket_description(
        State(state.clone()),
        Extension(RequestIdExt("rid-nested-description".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(description.status(), StatusCode::OK);
    let description_bytes = to_bytes(description.into_body(), 1024 * 1024)
        .await
        .expect("description body");
    let description_payload: serde_json::Value =
        serde_json::from_slice(&description_bytes).expect("description json");
    assert_eq!(
        description_payload["ticket_ref"]["workspace"],
        child_workspace.as_str()
    );
    assert_eq!(
        description_payload["description"],
        "mixed-workspace child description"
    );

    let history = get_ticket_history(
        State(state.clone()),
        Extension(RequestIdExt("rid-nested-history".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(history.status(), StatusCode::OK);
    let history_bytes = to_bytes(history.into_body(), 1024 * 1024)
        .await
        .expect("history body");
    let history_payload: serde_json::Value =
        serde_json::from_slice(&history_bytes).expect("history json");
    assert_eq!(
        history_payload["ticket_ref"]["workspace"],
        child_workspace.as_str()
    );
    assert_eq!(history_payload["ticket_ref"]["id"], id.to_string());

    let files = list_ticket_files(
        State(state.clone()),
        Extension(RequestIdExt("rid-nested-files".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(files.status(), StatusCode::OK);
    let files_bytes = to_bytes(files.into_body(), 1024 * 1024)
        .await
        .expect("files body");
    let files_payload: serde_json::Value =
        serde_json::from_slice(&files_bytes).expect("files json");
    assert_eq!(
        files_payload["ticket_ref"]["workspace"],
        child_workspace.as_str()
    );

    let asset = get_ticket_asset(
        State(state),
        Extension(RequestIdExt("rid-nested-asset".to_string())),
        Path(id),
        Query(TicketAssetParam {
            workspace: workspace.clone(),
            path: "assets/plan.md".to_string(),
        }),
    )
    .await;
    assert_eq!(asset.status(), StatusCode::OK);
    let asset_bytes = to_bytes(asset.into_body(), 1024 * 1024)
        .await
        .expect("asset body");
    let asset_payload: serde_json::Value =
        serde_json::from_slice(&asset_bytes).expect("asset json");
    assert_eq!(
        asset_payload["ticket_ref"]["workspace"],
        child_workspace.as_str()
    );
    assert_eq!(asset_payload["content"], "nested child asset");
}

#[tokio::test]
async fn legacy_workspace_label_collision_returns_typed_bad_request() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(root.path())
            .expect("open parent store"),
    );
    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
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

    let left_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(&left_index_root)
            .expect("open left store"),
    );
    let right_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(&right_index_root)
            .expect("open right store"),
    );
    left_store
        .create(
            None,
            "tracker-improvement",
            Some("left shared ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create left ticket");
    right_store
        .create(
            None,
            "tracker-improvement",
            Some("right shared ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create right ticket");

    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: left_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add left scan root");
    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: right_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add right scan root");
    parent_store.scan(true).expect("scan parent store");

    let state = make_state(parent_store);
    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-ambiguous-label".to_string())),
        Query(WorkspaceParam {
            workspace: "shared".to_string(),
            state: None,
            query: None,
            limit: Some(10),
            cursor: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json body");

    assert_eq!(payload["code"], "workspace.ambiguous_label");
    assert_eq!(payload["details"]["requested"], "shared");
    assert_eq!(
        payload["details"]["matches"]
            .as_array()
            .expect("matches array")
            .len(),
        2
    );
}

#[tokio::test]
async fn unique_display_workspace_label_returns_typed_bad_request() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(root.path())
            .expect("open parent store"),
    );
    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: root.path().join("tickets"),
            label: "default".to_string(),
        })
        .expect("add parent scan root");

    let child_index_root = root.path().join("child").join(".ticket");
    std::fs::create_dir_all(child_index_root.join("tickets"))
        .expect("mkdir child store");
    let child_store = Arc::new(
        ticket_api::storage::store::TicketStore::init(&child_index_root)
            .expect("open child store"),
    );
    child_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root");

    let _id = child_store
        .create(
            None,
            "tracker-improvement",
            Some("legacy workspace alias ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create child ticket");

    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root to parent");
    parent_store.scan(true).expect("scan parent store");

    let state = make_state(parent_store);
    let canonical_workspace = state
        .registry
        .workspace_infos()
        .into_iter()
        .find(|info| info.label == "child")
        .expect("child workspace info")
        .name;

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-legacy-alias".to_string())),
        Query(WorkspaceParam {
            workspace: "child".to_string(),
            state: None,
            query: Some("legacy workspace alias".to_string()),
            limit: Some(10),
            cursor: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json body");

    assert_eq!(payload["code"], "workspace.display_label_not_allowed");
    assert_eq!(payload["details"]["requested"], "child");
    assert_eq!(
        payload["details"]["canonical"],
        canonical_workspace.as_str()
    );
    assert!(
        payload["message"]
            .as_str()
            .expect("message")
            .contains(canonical_workspace.as_str())
    );
}

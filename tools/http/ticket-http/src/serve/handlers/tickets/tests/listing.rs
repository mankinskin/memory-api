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
use viewer_api::error::RequestIdExt;

use super::{
    super::{
        TicketAssetParam,
        TicketIdParam,
        get_ticket_description,
        get_ticket_asset,
        get_ticket,
        get_ticket_history,
        WorkspaceParam,
        list_ticket_files,
        list_tickets,
    },
    make_state,
    make_store,
};

#[tokio::test]
async fn search_list_uses_persisted_updated_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("search-updated-at regression"),
            Some("open"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket");

    let expected_updated_at = store
        .get_indexed(&id)
        .expect("indexed get")
        .expect("indexed ticket exists")
        .updated_at;

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-test".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("search-updated-at".to_string()),
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

    let got = payload["items"][0]["updated_at"]
        .as_str()
        .expect("updated_at string");
    let got = chrono::DateTime::parse_from_rfc3339(got)
        .expect("parse updated_at")
        .with_timezone(&chrono::Utc);

    assert_eq!(got, expected_updated_at);
    assert_eq!(payload["active_workspace"], workspace.clone());
    assert_eq!(payload["items"][0]["ticket_ref"]["workspace"], workspace);
    assert_eq!(payload["items"][0]["ticket_ref"]["id"], id.to_string());
}

#[tokio::test]
async fn search_list_matches_description_body_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let matching_id = store
        .create(
            None,
            "tracker-improvement",
            Some("title-only decoy"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("body-only-needle search phrase lives in description"),
        )
        .expect("create matching ticket");

    store
        .create(
            None,
            "tracker-improvement",
            Some("another ticket"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("different body content"),
        )
        .expect("create non-matching ticket");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-body-search".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("body-only-needle".to_string()),
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
    let items = payload["items"].as_array().expect("items array");
    let matching_id = matching_id.to_string();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some(matching_id.as_str()));
}

#[tokio::test]
async fn search_list_matches_substring_partial_terms() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let matching_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Firecracker control plane foundation"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create matching ticket");

    store
        .create(
            None,
            "tracker-improvement",
            Some("Crackle runtime notes"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create non-matching ticket");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-substring-search".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("cracker".to_string()),
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
    let items = payload["items"].as_array().expect("items array");
    let matching_id = matching_id.to_string();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some(matching_id.as_str()));
}

#[tokio::test]
async fn search_list_supports_id_field_predicates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let matching_id = store
        .create(
            None,
            "tracker-improvement",
            Some("field predicate target"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create matching ticket");

    store
        .create(
            None,
            "tracker-improvement",
            Some("another ticket"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create non-matching ticket");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-id-search".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some(format!("id:{matching_id}")),
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
    let items = payload["items"].as_array().expect("items array");
    let matching_id = matching_id.to_string();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some(matching_id.as_str()));
}

#[tokio::test]
async fn search_list_supports_title_field_substring_predicates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let matching_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Firecracker control plane foundation"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create matching ticket");

    store
        .create(
            None,
            "tracker-improvement",
            Some("Sandbox notes"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("firecracker only appears in the description"),
        )
        .expect("create body-only decoy");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-title-substring-search".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("title:cracker".to_string()),
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
    let items = payload["items"].as_array().expect("items array");
    let matching_id = matching_id.to_string();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some(matching_id.as_str()));
}

#[tokio::test]
async fn state_only_list_filters_items() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let ready_id = store
        .create(
            None,
            "tracker-improvement",
            Some("state-only ready ticket"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ready ticket");

    store
        .create(
            None,
            "tracker-improvement",
            Some("state-only new ticket"),
            Some("new"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create new ticket");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-test".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: Some("ready".to_string()),
            query: None,
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
    let items = payload["items"].as_array().expect("items array");
    let ready_id = ready_id.to_string();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some(ready_id.as_str()));
    assert_eq!(items[0]["state"].as_str(), Some("ready"));
}

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
            Some("ready"),
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
    assert_eq!(detail_payload["ticket"]["ticket_ref"]["workspace"], workspace.clone());
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
async fn nested_child_workspace_ticket_uses_child_ref_and_reads_from_default() {
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
            Some("nested child-owned ticket"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("nested child description"),
        )
        .expect("create child ticket");
    let ticket_dir = child_store
        .get_indexed(&id)
        .expect("get child indexed ticket")
        .expect("child indexed ticket");
    std::fs::create_dir_all(ticket_dir.path.join("assets")).expect("mkdir assets");
    std::fs::write(ticket_dir.path.join("assets").join("plan.md"), "nested child asset")
        .expect("write child asset");

    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: child_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add child scan root to parent");
    parent_store.scan(true).expect("scan parent store");

    let state = make_state(Arc::clone(&parent_store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let list = list_tickets(
        State(state.clone()),
        Extension(RequestIdExt("rid-nested-list".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("nested child-owned".to_string()),
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
    assert_eq!(list_payload["items"][0]["ticket_ref"]["workspace"], "child");
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
    assert_eq!(detail_payload["ticket"]["ticket_ref"]["workspace"], "child");
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
    let description_payload: serde_json::Value = serde_json::from_slice(&description_bytes)
        .expect("description json");
    assert_eq!(description_payload["ticket_ref"]["workspace"], "child");
    assert_eq!(description_payload["description"], "nested child description");

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
    assert_eq!(history_payload["ticket_ref"]["workspace"], "child");
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
    assert_eq!(files_payload["ticket_ref"]["workspace"], "child");

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
    assert_eq!(asset_payload["ticket_ref"]["workspace"], "child");
    assert_eq!(asset_payload["content"], "nested child asset");
}

#[tokio::test]
async fn search_list_combines_query_and_state_before_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    store
        .create(
            None,
            "tracker-improvement",
            Some("needle needle needle wrong-state"),
            Some("new"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create higher-ranked wrong-state ticket");

    let ready_id = store
        .create(
            None,
            "tracker-improvement",
            Some("needle right-state"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create matching ready ticket");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-test".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: Some("ready".to_string()),
            query: Some("needle".to_string()),
            limit: Some(1),
            cursor: None,
        }),
    )
    .await;

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json body");
    let items = payload["items"].as_array().expect("items array");
    let ready_id = ready_id.to_string();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some(ready_id.as_str()));
    assert_eq!(items[0]["state"].as_str(), Some("ready"));
}

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
async fn duplicate_basename_workspaces_keep_followups_distinct() {
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
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: left_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add left scan root");
    right_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: right_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add right scan root");

    let left_id = left_store
        .create(
            None,
            "tracker-improvement",
            Some("left shared ticket"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("left shared description"),
        )
        .expect("create left ticket");
    let right_id = right_store
        .create(
            None,
            "tracker-improvement",
            Some("right shared ticket"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("right shared description"),
        )
        .expect("create right ticket");

    for (store, id, asset_content) in [
        (Arc::clone(&left_store), left_id, "left shared asset"),
        (Arc::clone(&right_store), right_id, "right shared asset"),
    ] {
        let ticket_dir = store
            .get_indexed(&id)
            .expect("get indexed ticket")
            .expect("indexed ticket");
        std::fs::create_dir_all(ticket_dir.path.join("assets"))
            .expect("mkdir assets");
        std::fs::write(
            ticket_dir.path.join("assets").join("plan.md"),
            asset_content,
        )
        .expect("write asset");
    }

    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: left_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add left scan root to parent");
    parent_store
        .add_scan_root(ticket_api::model::filesystem::ScanRoot {
            path: right_index_root.join("tickets"),
            label: "tickets".to_string(),
        })
        .expect("add right scan root to parent");
    parent_store.scan(true).expect("scan parent store");

    let state = make_state(parent_store);
    let mut shared_workspaces = state
        .registry
        .workspace_infos()
        .into_iter()
        .filter(|info| info.label == "shared")
        .map(|info| info.name)
        .collect::<Vec<_>>();
    shared_workspaces.sort();
    assert_eq!(shared_workspaces.len(), 2);
    assert_ne!(shared_workspaces[0], shared_workspaces[1]);

    for workspace in shared_workspaces {
        let list = list_tickets(
            State(state.clone()),
            Extension(RequestIdExt(format!("rid-list-{workspace}"))),
            Query(WorkspaceParam {
                workspace: workspace.to_string(),
                state: None,
                query: Some("shared ticket".to_string()),
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
        let title = list_payload["items"][0]["title"].as_str().expect("title");
        let (expected_description, expected_asset) = match title {
            "left shared ticket" =>
                ("left shared description", "left shared asset"),
            "right shared ticket" =>
                ("right shared description", "right shared asset"),
            other => panic!("unexpected shared workspace title: {other}"),
        };
        let id = Uuid::parse_str(
            list_payload["items"][0]["id"].as_str().expect("ticket id"),
        )
        .expect("valid ticket id");
        assert_eq!(list_payload["active_workspace"], workspace);
        assert_eq!(list_payload["workspace"], workspace);
        assert_eq!(
            list_payload["items"][0]["ticket_ref"]["workspace"],
            workspace
        );
        assert_eq!(
            list_payload["items"][0]["ticket_ref"]["id"],
            id.to_string()
        );
        assert_eq!(list_payload["items"][0]["title"], title);

        let detail = get_ticket(
            State(state.clone()),
            Extension(RequestIdExt(format!("rid-detail-{workspace}"))),
            Path(id),
            Query(TicketIdParam {
                workspace: workspace.to_string(),
            }),
        )
        .await;
        assert_eq!(detail.status(), StatusCode::OK);
        let detail_bytes = to_bytes(detail.into_body(), 1024 * 1024)
            .await
            .expect("detail body");
        let detail_payload: serde_json::Value =
            serde_json::from_slice(&detail_bytes).expect("detail json");
        assert_eq!(detail_payload["active_workspace"], workspace);
        assert_eq!(
            detail_payload["ticket"]["ticket_ref"]["workspace"],
            workspace
        );

        let description_response = get_ticket_description(
            State(state.clone()),
            Extension(RequestIdExt(format!("rid-description-{workspace}"))),
            Path(id),
            Query(TicketIdParam {
                workspace: workspace.to_string(),
            }),
        )
        .await;
        assert_eq!(description_response.status(), StatusCode::OK);
        let description_bytes =
            to_bytes(description_response.into_body(), 1024 * 1024)
                .await
                .expect("description body");
        let description_payload: serde_json::Value =
            serde_json::from_slice(&description_bytes)
                .expect("description json");
        assert_eq!(description_payload["active_workspace"], workspace);
        assert_eq!(description_payload["ticket_ref"]["workspace"], workspace);
        assert_eq!(description_payload["description"], expected_description);

        let history = get_ticket_history(
            State(state.clone()),
            Extension(RequestIdExt(format!("rid-history-{workspace}"))),
            Path(id),
            Query(TicketIdParam {
                workspace: workspace.to_string(),
            }),
        )
        .await;
        assert_eq!(history.status(), StatusCode::OK);
        let history_bytes = to_bytes(history.into_body(), 1024 * 1024)
            .await
            .expect("history body");
        let history_payload: serde_json::Value =
            serde_json::from_slice(&history_bytes).expect("history json");
        assert_eq!(history_payload["active_workspace"], workspace);
        assert_eq!(history_payload["ticket_ref"]["workspace"], workspace);

        let files = list_ticket_files(
            State(state.clone()),
            Extension(RequestIdExt(format!("rid-files-{workspace}"))),
            Path(id),
            Query(TicketIdParam {
                workspace: workspace.to_string(),
            }),
        )
        .await;
        assert_eq!(files.status(), StatusCode::OK);
        let files_bytes = to_bytes(files.into_body(), 1024 * 1024)
            .await
            .expect("files body");
        let files_payload: serde_json::Value =
            serde_json::from_slice(&files_bytes).expect("files json");
        assert_eq!(files_payload["active_workspace"], workspace);
        assert_eq!(files_payload["ticket_ref"]["workspace"], workspace);

        let asset = get_ticket_asset(
            State(state.clone()),
            Extension(RequestIdExt(format!("rid-asset-{workspace}"))),
            Path(id),
            Query(TicketAssetParam {
                workspace: workspace.to_string(),
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
        assert_eq!(asset_payload["active_workspace"], workspace);
        assert_eq!(asset_payload["ticket_ref"]["workspace"], workspace);
        assert_eq!(asset_payload["content"], expected_asset);
    }
}

#[tokio::test]
async fn search_list_excludes_stale_search_hits_and_followups() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("deleted-hit regression ticket"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("deleted-hit regression description"),
        )
        .expect("create ticket");
    let ticket_dir = store
        .get_indexed(&id)
        .expect("get indexed ticket")
        .expect("indexed ticket");
    std::fs::create_dir_all(ticket_dir.path.join("assets"))
        .expect("mkdir assets");
    std::fs::write(
        ticket_dir.path.join("assets").join("plan.md"),
        "deleted hit asset",
    )
    .expect("write asset");

    store.delete(&id).expect("delete ticket");
    TantivySearchIndex::open_or_create(&store.index_root.join("search_index"))
        .expect("open search index")
        .upsert(
            &id,
            Some("deleted-hit regression ticket"),
            Some("deleted-hit regression description"),
            Some("ready"),
            Some("tracker-improvement"),
            Some(&chrono::Utc::now().to_rfc3339()),
            None,
        )
        .expect("upsert deleted residual search doc");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let list = list_tickets(
        State(state.clone()),
        Extension(RequestIdExt("rid-deleted-list".to_string())),
        Query(WorkspaceParam {
            workspace: workspace.clone(),
            state: None,
            query: Some("deleted-hit regression".to_string()),
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
    assert!(
        list_payload["items"]
            .as_array()
            .expect("items array")
            .is_empty(),
        "deleted residual search hits must be dropped"
    );

    let detail = get_ticket(
        State(state.clone()),
        Extension(RequestIdExt("rid-deleted-detail".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::NOT_FOUND);

    let description = get_ticket_description(
        State(state.clone()),
        Extension(RequestIdExt("rid-deleted-description".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(description.status(), StatusCode::NOT_FOUND);

    let history = get_ticket_history(
        State(state.clone()),
        Extension(RequestIdExt("rid-deleted-history".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(history.status(), StatusCode::NOT_FOUND);

    let files = list_ticket_files(
        State(state.clone()),
        Extension(RequestIdExt("rid-deleted-files".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace: workspace.clone(),
        }),
    )
    .await;
    assert_eq!(files.status(), StatusCode::NOT_FOUND);

    let asset = get_ticket_asset(
        State(state),
        Extension(RequestIdExt("rid-deleted-asset".to_string())),
        Path(id),
        Query(TicketAssetParam {
            workspace,
            path: "assets/plan.md".to_string(),
        }),
    )
    .await;
    assert_eq!(asset.status(), StatusCode::NOT_FOUND);
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

#[tokio::test]
async fn list_rejects_synthetic_or_unknown_public_workspace_identifiers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());
    store
        .create(
            None,
            "tracker-improvement",
            Some("workspace alias rejection regression"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket");

    let state = make_state(Arc::clone(&store));

    for workspace in ["default", "..", "../..", "missing-workspace"] {
        let response = list_tickets(
            State(state.clone()),
            Extension(RequestIdExt(format!("rid-{workspace}"))),
            Query(WorkspaceParam {
                workspace: workspace.to_string(),
                state: None,
                query: None,
                limit: Some(10),
                cursor: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let payload: serde_json::Value =
            serde_json::from_slice(&bytes).expect("json body");

        assert_eq!(payload["code"], "not_found");
        assert!(
            payload["message"]
                .as_str()
                .expect("message")
                .contains("workspace")
        );
        assert!(payload.get("request_id").is_some());
    }
}

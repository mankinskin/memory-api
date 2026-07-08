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
async fn search_list_drops_unresolved_tantivy_only_hits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());
    let ghost_id = Uuid::new_v4();
    TantivySearchIndex::open_or_create(&store.index_root.join("search_index"))
        .expect("open search index")
        .upsert(
            &ghost_id,
            Some("ghost-only unresolved search hit"),
            Some("ghost-only unresolved search hit body"),
            Some("ready"),
            Some("tracker-improvement"),
            Some(&chrono::Utc::now().to_rfc3339()),
            None,
        )
        .expect("upsert ghost search doc");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-ghost-search".to_string())),
        Query(WorkspaceParam {
            workspace,
            state: None,
            query: Some("ghost-only unresolved".to_string()),
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

    assert!(items.is_empty(), "unresolved search-only hits must be dropped");
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


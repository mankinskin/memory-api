use axum::{
    body::to_bytes,
    extract::{
        Extension,
        Query,
        State,
    },
};
use std::{
    collections::BTreeMap,
    sync::Arc,
};
use viewer_api::error::RequestIdExt;

use super::{
    super::{
        WorkspaceParam,
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

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-test".to_string())),
        Query(WorkspaceParam {
            workspace: "default".to_string(),
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

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-test".to_string())),
        Query(WorkspaceParam {
            workspace: "default".to_string(),
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

    let response = list_tickets(
        State(state),
        Extension(RequestIdExt("rid-test".to_string())),
        Query(WorkspaceParam {
            workspace: "default".to_string(),
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

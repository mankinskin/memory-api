use axum::{
    body::to_bytes,
    extract::{Extension, Query, State},
};
use std::{collections::BTreeMap, sync::Arc};
use viewer_api::error::RequestIdExt;

use super::{make_state, make_store};
use super::super::{WorkspaceParam, list_tickets};

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
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json body");

    let got = payload["items"][0]["updated_at"]
        .as_str()
        .expect("updated_at string");
    let got = chrono::DateTime::parse_from_rfc3339(got)
        .expect("parse updated_at")
        .with_timezone(&chrono::Utc);

    assert_eq!(got, expected_updated_at);
}
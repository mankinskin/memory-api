use std::sync::Arc;

use axum::{
    body::{
        Body,
        to_bytes,
    },
    http::{
        Method,
        Request,
        StatusCode,
        header,
    },
};
use ticket_api::{
    model::filesystem::ScanRoot,
    storage::store::TicketStore,
};
use tower::ServiceExt;

use crate::serve::{
    AppState,
    StreamBroker,
    WorkspaceRegistry,
};

use super::*;

fn make_router(dir: &std::path::Path) -> axum::Router {
    let store = Arc::new(TicketStore::open(dir).expect("open store"));
    store
        .add_scan_root(ScanRoot {
            path: dir.join("tickets"),
            label: "default".into(),
        })
        .expect("add scan root");
    let state = AppState::new(
        Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
        Arc::new(StreamBroker::new()),
    );
    crate::serve::routes::build_router(state)
}

async fn post_batch(
    app: axum::Router,
    body: serde_json::Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/batch")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn batch_create_returns_ok() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_router(dir.path());

    let (status, resp) = post_batch(
        app,
        json!({
            "workspace": "default",
            "commands": [
                {"op": "create", "type": "tracker-improvement", "title": "Batch A"},
                {"op": "create", "type": "tracker-improvement", "title": "Batch B"},
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200, got: {resp}");
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["count"], 2);
    let results = resp["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["op"], "create");
    assert_eq!(results[0]["index"], 0);
    assert_eq!(results[1]["op"], "create");
    assert_eq!(results[1]["index"], 1);
}

#[tokio::test]
async fn batch_rolls_back_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_router(dir.path());

    let (status, resp) = post_batch(
        app,
        json!({
            "workspace": "default",
            "commands": [
                {"op": "create", "type": "tracker-improvement", "title": "Should be rolled back"},
                {"op": "close", "id": "00000000-0000-0000-0000-000000000000"},
            ]
        }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "expected 422, got: {resp}"
    );
    assert_eq!(resp["status"], "error");
    assert_eq!(resp["failed_at"], 1);
    assert_eq!(
        resp["rolled_back"].as_bool().unwrap_or(false),
        true,
        "rollback must succeed"
    );
}

#[tokio::test]
async fn batch_link_and_unlink() {
    let dir = tempfile::tempdir().unwrap();

    let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));
    store
        .add_scan_root(ScanRoot {
            path: dir.path().join("tickets"),
            label: "default".into(),
        })
        .expect("add scan root");
    let id_a = store
        .create(
            None,
            "tracker-improvement",
            Some("A"),
            None,
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let id_b = store
        .create(
            None,
            "tracker-improvement",
            Some("B"),
            None,
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let state = AppState::new(
        Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
        Arc::new(StreamBroker::new()),
    );
    let app = crate::serve::routes::build_router(state);

    let (status, resp) = post_batch(
        app,
        json!({
            "workspace": "default",
            "commands": [
                {"op": "link", "from": id_a.to_string(), "to": id_b.to_string(), "kind": "depends_on"},
                {"op": "unlink", "from": id_a.to_string(), "to": id_b.to_string(), "kind": "depends_on"},
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200, got: {resp}");
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["count"], 2);
}

#[tokio::test]
async fn batch_unknown_workspace_returns_404() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_router(dir.path());

    let (status, _) = post_batch(
        app,
        json!({
            "workspace": "nonexistent",
            "commands": [
                {"op": "create", "type": "tracker-improvement", "title": "x"},
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn batch_empty_commands_returns_ok() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_router(dir.path());

    let (status, resp) = post_batch(
        app,
        json!({
            "workspace": "default",
            "commands": []
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["count"], 0);
}

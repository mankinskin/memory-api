use std::sync::Arc;

use axum::{
    body::{
        Body,
        to_bytes,
    },
    http::{
        Request,
        StatusCode,
    },
};
use serde_json::Value;
use tower::ServiceExt;

use ticket_http::serve::{
    AppState,
    StreamBroker,
    WorkspaceRegistry,
    routes::build_router,
};

async fn get_json(
    app: axum::Router,
    uri: &str,
) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("request should succeed");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body");
    let json = serde_json::from_slice::<Value>(&bytes).expect("json response");
    (status, json)
}

#[tokio::test]
async fn read_probes_do_not_create_missing_ticket_store_root() {
    let repo = tempfile::tempdir().expect("tempdir");
    let missing_store_root = repo.path().join(".ticket");
    assert!(
        !missing_store_root.exists(),
        "fixture should start without .ticket"
    );

    let registry = Arc::new(WorkspaceRegistry::single(missing_store_root.clone()));
    let workspace = registry.primary_workspace_name().to_string();
    let app = build_router(AppState::new(registry, Arc::new(StreamBroker::new())));

    let (workspaces_status, workspaces_payload) =
        get_json(app.clone(), "/api/workspaces").await;
    assert_eq!(workspaces_status, StatusCode::OK);
    assert_eq!(
        workspaces_payload["active_workspace"].as_str(),
        Some(workspace.as_str())
    );
    assert!(
        !missing_store_root.exists(),
        "/api/workspaces must not create a missing .ticket root"
    );

    let (tickets_status, _tickets_payload) =
        get_json(app, &format!("/api/tickets?workspace={workspace}&limit=10")).await;
    assert_eq!(
        tickets_status,
        StatusCode::NOT_FOUND,
        "missing workspace probes should fail without auto-init"
    );
    assert!(
        !missing_store_root.exists(),
        "/api/tickets probe must not create a missing .ticket root"
    );
}

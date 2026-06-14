use axum::{
    Json,
    body::to_bytes,
    extract::{
        Extension,
        Path,
        Query,
        State,
    },
    http::{
        HeaderMap,
        StatusCode,
    },
};
use std::{
    collections::BTreeMap,
    sync::Arc,
};
use uuid::Uuid;
use viewer_api::error::RequestIdExt;

use super::{
    super::{
        CancelTicketBody,
        CloseTicketBody,
        MutationWorkspaceParam,
        RevertTicketBody,
        cancel_ticket,
        close_ticket,
        delete_ticket,
        revert_ticket,
    },
    make_state,
    make_store,
};

#[tokio::test]
async fn close_ticket_fast_forwards_to_done() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Close me"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = close_ticket(
        State(state),
        Extension(RequestIdExt("rid-close".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        HeaderMap::new(),
        Json(CloseTicketBody { target_state: None }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["state"], "done");
}

#[tokio::test]
async fn revert_ticket_restores_historical_revision() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Revert me"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    store
        .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
        .expect("update to ready");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = revert_ticket(
        State(state),
        Extension(RequestIdExt("rid-revert".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        HeaderMap::new(),
        Json(RevertTicketBody { revision: 1 }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["request_id"], "rid-revert");
    assert_eq!(payload["workspace"], workspace);
    assert_eq!(payload["active_workspace"], workspace.clone());
    assert_eq!(payload["ticket"]["ticket_ref"]["workspace"], workspace);
    assert_eq!(payload["ticket"]["fields"]["state"], "new");
    assert_eq!(payload["ticket"]["fields"]["title"], "Revert me");
}

#[tokio::test]
async fn revert_ticket_returns_400_for_unknown_revision() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("T"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = revert_ticket(
        State(state),
        Extension(RequestIdExt("rid".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        HeaderMap::new(),
        Json(RevertTicketBody { revision: 999 }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["code"], "revision_not_found");
}

#[tokio::test]
async fn cancel_ticket_transitions_to_cancelled_with_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Cancel me"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = cancel_ticket(
        State(state),
        Extension(RequestIdExt("rid-cancel".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        HeaderMap::new(),
        Json(CancelTicketBody {
            reason: Some("No longer needed".to_string()),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["state"], "cancelled");
    assert_eq!(
        payload["ticket"]["fields"]["cancel_reason"],
        "No longer needed"
    );
}

#[tokio::test]
async fn delete_ticket_removes_folder() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Delete me"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    let ticket_path = store.get_indexed(&id).unwrap().unwrap().path.clone();
    assert!(ticket_path.exists(), "folder must exist before delete");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = delete_ticket(
        State(state.clone()),
        Extension(RequestIdExt("rid-delete".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["id"], id.to_string());
    assert_eq!(payload["ticket_ref"]["workspace"], workspace);
    assert_eq!(payload["ticket_ref"]["id"], id.to_string());

    assert!(!ticket_path.exists(), "folder must be removed after delete");
    assert!(
        store.get_indexed(&id).expect("indexed ok").is_none(),
        "ticket must be absent from index after delete"
    );
}

#[tokio::test]
async fn delete_nonexistent_ticket_returns_404_envelope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = make_state(make_store(dir.path()));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = delete_ticket(
        State(state),
        Extension(RequestIdExt("rid".to_string())),
        Path(Uuid::new_v4()),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["code"], "ticket.not_found");
    assert!(payload.get("request_id").is_some());
}

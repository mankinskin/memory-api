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
use viewer_api::error::RequestIdExt;

use super::{
    super::{
        CreateTicketBody,
        MutationWorkspaceParam,
        UpdateTicketBody,
        create_ticket,
        update_ticket,
    },
    make_state,
    make_store,
};

#[tokio::test]
async fn create_ticket_returns_201_with_new_ticket() {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = make_state(make_store(dir.path()));

    let response = create_ticket(
        State(state),
        Extension(RequestIdExt("rid-create".to_string())),
        Query(MutationWorkspaceParam {
            workspace: "default".to_string(),
        }),
        Json(CreateTicketBody {
            type_id: "tracker-improvement".to_string(),
            title: Some("My new ticket".to_string()),
            fields: None,
            description: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");

    assert_eq!(payload["workspace"], "default");
    assert_eq!(payload["active_workspace"], "default");
    assert_eq!(payload["request_id"], "rid-create");
    assert_eq!(payload["ticket"]["fields"]["title"], "My new ticket");
    assert_eq!(payload["ticket"]["fields"]["state"], "new");
    assert_eq!(payload["ticket"]["ticket_ref"]["workspace"], "default");
}

#[tokio::test]
async fn create_ticket_with_extra_fields_and_description() {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = make_state(make_store(dir.path()));

    let mut fields = BTreeMap::new();
    fields.insert(
        "priority".to_string(),
        serde_json::Value::String("high".to_string()),
    );

    let response = create_ticket(
        State(state),
        Extension(RequestIdExt("rid".to_string())),
        Query(MutationWorkspaceParam {
            workspace: "default".to_string(),
        }),
        Json(CreateTicketBody {
            type_id: "tracker-improvement".to_string(),
            title: Some("Ticket with fields".to_string()),
            fields: Some(fields),
            description: Some("## Overview\n\nSome description.".to_string()),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["priority"], "high");
}

#[tokio::test]
async fn update_ticket_patches_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Original"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    let state = make_state(Arc::clone(&store));

    let mut patch = BTreeMap::new();
    patch.insert(
        "title".to_string(),
        serde_json::Value::String("Updated title".to_string()),
    );

    let response = update_ticket(
        State(state),
        Extension(RequestIdExt("rid-update".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: "default".to_string(),
        }),
        HeaderMap::new(),
        Json(UpdateTicketBody {
            fields: Some(patch),
            state: None,
            from_state: None,
            description: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["title"], "Updated title");
}

#[tokio::test]
async fn update_ticket_transitions_state() {
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

    let response = update_ticket(
        State(state),
        Extension(RequestIdExt("rid".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: "default".to_string(),
        }),
        HeaderMap::new(),
        Json(UpdateTicketBody {
            fields: None,
            state: Some("ready".to_string()),
            from_state: None,
            description: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["state"], "ready");
}

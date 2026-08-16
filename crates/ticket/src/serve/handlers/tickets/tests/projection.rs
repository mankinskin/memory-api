//! HTTP transport reachability for read projections (ticket 4c7b884e, AC7):
//! `GET /api/tickets/{id}?view=summary` and `?parts=...` reach
//! `TicketStore::project` rather than returning the raw manifest.

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
use std::collections::BTreeMap;
use viewer_api::error::RequestIdExt;

use super::{
    super::{
        TicketIdParam,
        get_ticket,
    },
    make_state,
    make_store,
};

#[tokio::test]
async fn get_ticket_with_view_summary_projects_to_objective_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("http projection"),
            Some("open"),
            BTreeMap::new(),
            None,
            Some("objective body"),
        )
        .expect("create ticket");

    let state = make_state(store);
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = get_ticket(
        State(state),
        Extension(RequestIdExt("rid-projection-summary".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace,
            view: Some("summary".to_string()),
            parts: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json body");

    let parts = payload["ticket"]["parts"].as_array().expect("parts array");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0]["kind"], "objective");
}

#[tokio::test]
async fn get_ticket_with_explicit_parts_returns_exactly_those_kinds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("http projection parts"),
            Some("open"),
            BTreeMap::new(),
            None,
            Some("objective body"),
        )
        .expect("create ticket");
    store
        .write_part(
            &id,
            uuid::Uuid::new_v4(),
            "objective",
            "objective body",
            None,
        )
        .expect("write objective part");
    store
        .write_part(
            &id,
            uuid::Uuid::new_v4(),
            "acceptance_criteria",
            "AC text",
            None,
        )
        .expect("write acceptance_criteria part");

    let state = make_state(store);
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = get_ticket(
        State(state),
        Extension(RequestIdExt("rid-projection-parts".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace,
            view: None,
            parts: Some("objective,acceptance_criteria".to_string()),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json body");

    let kinds: Vec<String> = payload["ticket"]["parts"]
        .as_array()
        .expect("parts array")
        .iter()
        .map(|part| part["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        kinds,
        vec!["objective".to_string(), "acceptance_criteria".to_string()]
    );
}

#[tokio::test]
async fn get_ticket_with_both_view_and_parts_is_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("http projection conflict"),
            Some("open"),
            BTreeMap::new(),
            None,
            Some("objective body"),
        )
        .expect("create ticket");

    let state = make_state(store);
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = get_ticket(
        State(state),
        Extension(RequestIdExt("rid-projection-conflict".to_string())),
        Path(id),
        Query(TicketIdParam {
            workspace,
            view: Some("summary".to_string()),
            parts: Some("objective".to_string()),
        }),
    )
    .await;

    assert_ne!(response.status(), StatusCode::OK);
}

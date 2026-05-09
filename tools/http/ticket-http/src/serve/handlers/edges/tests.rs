use axum::{
    Json,
    body::to_bytes,
    extract::{Extension, Query, State},
    http::StatusCode,
};
use std::{collections::BTreeMap, sync::Arc};
use uuid::Uuid;
use viewer_api::error::RequestIdExt;

use ticket_api::{model::filesystem::ScanRoot, storage::store::TicketStore};

use crate::serve::{AppState, StreamBroker, WorkspaceRegistry};

use super::{EdgeBody, EdgeMutationQuery, add_edge, remove_edge};

fn make_state_with_store(dir: &std::path::Path) -> (AppState, Arc<TicketStore>) {
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
    (state, store)
}

fn create_ticket(store: &TicketStore) -> Uuid {
    store
        .create(
            None,
            "tracker-improvement",
            Some("edge test ticket"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket")
}

#[tokio::test]
async fn add_edge_returns_201_with_edge_detail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = make_state_with_store(dir.path());

    let from_id = create_ticket(&store);
    let to_id = create_ticket(&store);

    let response = add_edge(
        State(state),
        Extension(RequestIdExt("rid-add".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id,
            to_id,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    assert_eq!(payload["workspace"], "default");
    assert_eq!(payload["edge"]["from"], from_id.to_string());
    assert_eq!(payload["edge"]["to"], to_id.to_string());
    assert_eq!(payload["edge"]["kind"], "depends_on");
}

#[tokio::test]
async fn add_edge_self_referential_depends_on_returns_422() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = make_state_with_store(dir.path());

    let id = create_ticket(&store);

    let response = add_edge(
        State(state),
        Extension(RequestIdExt("rid-cycle".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id: id,
            to_id: id,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["code"], "edge.cycle_detected");
}

#[tokio::test]
async fn remove_edge_returns_200_with_edge_detail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = make_state_with_store(dir.path());

    let from_id = create_ticket(&store);
    let to_id = create_ticket(&store);

    add_edge(
        State(state.clone()),
        Extension(RequestIdExt("rid-setup".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id,
            to_id,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    let response = remove_edge(
        State(state),
        Extension(RequestIdExt("rid-remove".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id,
            to_id,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["edge"]["from"], from_id.to_string());
    assert_eq!(payload["edge"]["to"], to_id.to_string());
    assert_eq!(payload["edge"]["kind"], "depends_on");
}

#[tokio::test]
async fn add_edge_cycle_detection_indirect() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = make_state_with_store(dir.path());

    let a = create_ticket(&store);
    let b = create_ticket(&store);

    add_edge(
        State(state.clone()),
        Extension(RequestIdExt("rid-1".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id: b,
            to_id: a,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    let response = add_edge(
        State(state),
        Extension(RequestIdExt("rid-2".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id: a,
            to_id: b,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn sse_edge_events_emitted_on_add_and_remove() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (state, store) = make_state_with_store(dir.path());

    let mut rx = state.broker.subscribe("default");
    let from_id = create_ticket(&store);
    let to_id = create_ticket(&store);

    add_edge(
        State(state.clone()),
        Extension(RequestIdExt("rid-sse-add".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id,
            to_id,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match rx.recv().await {
                Ok((_id, ev)) => {
                    if ev.event_name() == "edge.upsert" {
                        return ev;
                    }
                }
                Err(_) => panic!("channel closed before edge.upsert received"),
            }
        }
    })
    .await
    .expect("edge.upsert event within timeout");

    assert_eq!(event.event_name(), "edge.upsert");

    remove_edge(
        State(state),
        Extension(RequestIdExt("rid-sse-rm".to_string())),
        Query(EdgeMutationQuery {
            workspace: "default".to_string(),
        }),
        Json(EdgeBody {
            from_id,
            to_id,
            kind: "depends_on".to_string(),
            reason: None,
        }),
    )
    .await;

    let del_event = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            match rx.recv().await {
                Ok((_id, ev)) => {
                    if ev.event_name() == "edge.delete" {
                        return ev;
                    }
                }
                Err(_) => panic!("channel closed before edge.delete received"),
            }
        }
    })
    .await
    .expect("edge.delete event within timeout");

    assert_eq!(del_event.event_name(), "edge.delete");
}
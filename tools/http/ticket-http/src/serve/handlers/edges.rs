use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use viewer_api::error::RequestIdExt;
use crate::serve::{error::storage_err, AppState};
use ticket_api::model::edge::EdgeRecord;

#[derive(Deserialize)]
pub struct EdgesQuery {
    pub workspace: String,
    pub kind: Option<String>,
}

#[derive(Serialize)]
pub struct EdgeItem {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct EdgesResponse {
    pub request_id: String,
    pub workspace: String,
    pub items: Vec<EdgeItem>,
}

pub async fn list_edges(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<EdgesQuery>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || match store.list_all_edges() {
        Ok(edges) => {
            let items: Vec<EdgeItem> = edges
                .into_iter()
                .filter(|e| {
                    if let Some(k) = &params.kind {
                        k == "all" || &e.kind == k
                    } else {
                        true
                    }
                })
                .map(|e| EdgeItem {
                    from: e.from.to_string(),
                    to: e.to.to_string(),
                    kind: e.kind,
                })
                .collect();

            Json(EdgesResponse {
                request_id: rid.0.clone(),
                workspace: params.workspace.clone(),
                items,
            })
            .into_response()
        }
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ── Edge mutation types ───────────────────────────────────────────────────────

/// Query-string parameter shared by edge mutation endpoints.
#[derive(Deserialize)]
pub struct EdgeMutationQuery {
    pub workspace: String,
}

/// Request body for `POST /api/edges` and `DELETE /api/edges`.
#[derive(Deserialize)]
pub struct EdgeBody {
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub kind: String,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct EdgeMutationResponse {
    pub request_id: String,
    pub workspace: String,
    pub edge: EdgeItem,
}

// ── Mutation handlers ─────────────────────────────────────────────────────────

/// `POST /api/edges?workspace=<name>`
///
/// Add an edge between two tickets.  For `depends_on` edges, cycle detection
/// is enforced by ticket-api and returns 422 on a detected cycle.
///
/// SSE `edge.upsert` events are emitted to subscribed clients on success.
pub async fn add_edge(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<EdgeMutationQuery>,
    Json(body): Json<EdgeBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let edge = EdgeRecord {
        from: body.from_id,
        to: body.to_id,
        kind: body.kind.clone(),
        created_at: Utc::now(),
    };

    tokio::task::spawn_blocking(move || match store.add_edge(edge) {
        Ok(()) => (
            StatusCode::CREATED,
            Json(EdgeMutationResponse {
                request_id: rid.0,
                workspace: params.workspace,
                edge: EdgeItem {
                    from: body.from_id.to_string(),
                    to: body.to_id.to_string(),
                    kind: body.kind,
                },
            }),
        )
            .into_response(),
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `DELETE /api/edges?workspace=<name>`
///
/// Remove an edge between two tickets.  Missing edges are treated as a no-op
/// (idempotent DELETE).
///
/// SSE `edge.delete` events are emitted to subscribed clients on success.
pub async fn remove_edge(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<EdgeMutationQuery>,
    Json(body): Json<EdgeBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let edge = EdgeRecord {
        from: body.from_id,
        to: body.to_id,
        kind: body.kind.clone(),
        created_at: Utc::now(),
    };

    tokio::task::spawn_blocking(move || match store.remove_edge(edge) {
        Ok(()) => Json(EdgeMutationResponse {
            request_id: rid.0,
            workspace: params.workspace,
            edge: EdgeItem {
                from: body.from_id.to_string(),
                to: body.to_id.to_string(),
                kind: body.kind,
            },
        })
        .into_response(),
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{add_edge, remove_edge, EdgeBody, EdgeMutationQuery};
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

    fn make_state(dir: &std::path::Path) -> AppState {
        let store = Arc::new(TicketStore::open(dir).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
            Arc::new(StreamBroker::new()),
        )
    }

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

        // Add the edge first.
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

        // Now remove it.
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
        // A -> B -> A should be rejected when adding A -> B (after B -> A exists).
        let dir = tempfile::tempdir().expect("tempdir");
        let (state, store) = make_state_with_store(dir.path());

        let a = create_ticket(&store);
        let b = create_ticket(&store);

        // Add B -> A first so there is a path from B to A.
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

        // Now adding A -> B would create a cycle — should be rejected.
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
        // Verify that broker receives edge events after add and remove.
        let dir = tempfile::tempdir().expect("tempdir");
        let (state, store) = make_state_with_store(dir.path());

        // Subscribe before any mutations so we catch events.
        let mut rx = state.broker.subscribe("default");

        let from_id = create_ticket(&store);
        let to_id = create_ticket(&store);

        // Trigger add — this goes through ensure_workspace_runtime which wires the hook.
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

        // Drain until we see an edge.upsert event.
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

        // Now remove and expect edge.delete.
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
}

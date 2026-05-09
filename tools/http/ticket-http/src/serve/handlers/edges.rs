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

#[cfg(test)]
#[path = "edges/tests.rs"]
mod tests;

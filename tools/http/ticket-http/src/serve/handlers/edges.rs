use axum::{
    extract::{
        Extension,
        Query,
        State,
    },
    http::StatusCode,
    response::{
        IntoResponse,
        Json,
        Response,
    },
};
use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use crate::serve::{
    AppState,
    error::{
        storage_err,
        task_join_err,
    },
    handlers::tickets::{
        TicketRef,
        ticket_ref_from_indexed,
    },
    registry::ResolvedIndexedTicket,
};
use ticket_api::model::edge::EdgeRecord;
use viewer_api::error::RequestIdExt;

#[derive(Deserialize)]
pub struct EdgesQuery {
    pub workspace: String,
    pub kind: Option<String>,
}

#[derive(Serialize)]
pub struct EdgeItem {
    pub from: String,
    pub to: String,
    pub from_ref: TicketRef,
    pub to_ref: TicketRef,
    pub kind: String,
}

#[derive(Serialize)]
pub struct EdgesResponse {
    pub request_id: String,
    pub active_workspace: String,
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
        },
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();
    let active_workspace = params.workspace.clone();
    let kind = params.kind.clone();

    tokio::task::spawn_blocking(move || match store.list_all_edges() {
        Ok(edges) => {
            let filtered: Vec<EdgeRecord> = edges
                .into_iter()
                .filter(|e| {
                    if let Some(k) = &kind {
                        k == "all" || &e.kind == k
                    } else {
                        true
                    }
                })
                .collect();
            let mut edge_ids = Vec::with_capacity(filtered.len() * 2);
            for edge in &filtered {
                edge_ids.push(edge.from);
                edge_ids.push(edge.to);
            }
            edge_ids.sort();
            edge_ids.dedup();

            let resolved = match state
                .registry
                .resolve_indexed_many(&active_workspace, &edge_ids)
            {
                Ok(resolved) => resolved,
                Err(error) => return storage_err(error, &task_request_id),
            };
            let items = match filtered
                .into_iter()
                .map(|edge| {
                    edge_item_from_record(
                        &resolved,
                        &active_workspace,
                        edge,
                        &task_request_id,
                    )
                })
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(items) => items,
                Err(response) => return response,
            };

            Json(EdgesResponse {
                request_id: task_request_id.clone(),
                active_workspace: active_workspace.clone(),
                workspace: active_workspace.clone(),
                items,
            })
            .into_response()
        },
        Err(e) => storage_err(e, &task_request_id),
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "edge request"))
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
    pub active_workspace: String,
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
        },
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();
    let active_workspace = params.workspace.clone();

    let edge = EdgeRecord {
        from: body.from_id,
        to: body.to_id,
        kind: body.kind.clone(),
        created_at: Utc::now(),
    };
    let from_id = body.from_id;
    let to_id = body.to_id;
    let edge_kind = body.kind;

    tokio::task::spawn_blocking(move || match store.add_edge(edge) {
        Ok(()) => {
            let resolved = match state
                .registry
                .resolve_indexed_many(&active_workspace, &[from_id, to_id])
            {
                Ok(resolved) => resolved,
                Err(error) => return storage_err(error, &task_request_id),
            };
            let edge = match edge_item(
                &resolved,
                &active_workspace,
                from_id,
                to_id,
                edge_kind,
                &task_request_id,
            ) {
                Ok(edge) => edge,
                Err(response) => return response,
            };

            (
                StatusCode::CREATED,
                Json(EdgeMutationResponse {
                    request_id: task_request_id.clone(),
                    active_workspace: active_workspace.clone(),
                    workspace: active_workspace.clone(),
                    edge,
                }),
            )
                .into_response()
        },
        Err(e) => storage_err(e, &task_request_id),
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "edge request"))
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
        },
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();
    let active_workspace = params.workspace.clone();

    let edge = EdgeRecord {
        from: body.from_id,
        to: body.to_id,
        kind: body.kind.clone(),
        created_at: Utc::now(),
    };
    let from_id = body.from_id;
    let to_id = body.to_id;
    let edge_kind = body.kind;

    tokio::task::spawn_blocking(move || match store.remove_edge(edge) {
        Ok(()) => {
            let resolved = match state
                .registry
                .resolve_indexed_many(&active_workspace, &[from_id, to_id])
            {
                Ok(resolved) => resolved,
                Err(error) => return storage_err(error, &task_request_id),
            };
            let edge = match edge_item(
                &resolved,
                &active_workspace,
                from_id,
                to_id,
                edge_kind,
                &task_request_id,
            ) {
                Ok(edge) => edge,
                Err(response) => return response,
            };

            Json(EdgeMutationResponse {
                request_id: task_request_id.clone(),
                active_workspace: active_workspace.clone(),
                workspace: active_workspace.clone(),
                edge,
            })
            .into_response()
        },
        Err(e) => storage_err(e, &task_request_id),
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "edge request"))
}

fn edge_item_from_record(
    resolved: &std::collections::HashMap<Uuid, ResolvedIndexedTicket>,
    active_workspace: &str,
    edge: EdgeRecord,
    request_id: &str,
) -> Result<EdgeItem, Response> {
    edge_item(
        resolved,
        active_workspace,
        edge.from,
        edge.to,
        edge.kind,
        request_id,
    )
}

fn edge_item(
    resolved: &std::collections::HashMap<Uuid, ResolvedIndexedTicket>,
    active_workspace: &str,
    from_id: Uuid,
    to_id: Uuid,
    kind: String,
    request_id: &str,
) -> Result<EdgeItem, Response> {
    Ok(EdgeItem {
        from: from_id.to_string(),
        to: to_id.to_string(),
        from_ref: resolve_edge_ref(resolved, active_workspace, from_id, request_id)?,
        to_ref: resolve_edge_ref(resolved, active_workspace, to_id, request_id)?,
        kind,
    })
}

fn resolve_edge_ref(
    resolved: &std::collections::HashMap<Uuid, ResolvedIndexedTicket>,
    active_workspace: &str,
    id: Uuid,
    request_id: &str,
) -> Result<TicketRef, Response> {
    match resolved.get(&id) {
        Some(ticket) => ticket_ref_from_indexed(
            &ticket.store,
            &ticket.workspace,
            &ticket.ticket,
        )
        .map_err(|error| storage_err(error, request_id)),
        None => Ok(TicketRef {
            workspace: active_workspace.to_string(),
            id: id.to_string(),
        }),
    }
}

#[cfg(test)]
#[path = "edges/tests.rs"]
mod tests;

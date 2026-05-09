use std::collections::BTreeMap;

use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde_json::Value;
use uuid::Uuid;

use viewer_api::auth::extract_bearer_token;
use viewer_api::error::{ApiError, RequestIdExt};

use crate::serve::{AppState, error::storage_err};

use super::types::{
    CancelTicketBody, CloseTicketBody, CreateTicketBody, DeleteResponse, MutationResponse,
    MutationWorkspaceParam, RevertTicketBody, TicketDetail, UpdateTicketBody,
};

/// `POST /api/tickets?workspace=<name>`
///
/// Create a new ticket. Returns `201 Created` with the new ticket detail.
pub async fn create_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<MutationWorkspaceParam>,
    Json(body): Json<CreateTicketBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let extra = body.fields.unwrap_or_default();
    let type_id = body.type_id;
    let title = body.title;
    let description = body.description;

    tokio::task::spawn_blocking(move || {
        let id = match store.create(
            None,
            &type_id,
            title.as_deref(),
            None,
            extra,
            None,
            description.as_deref(),
        ) {
            Ok(id) => id,
            Err(e) => return storage_err(e, &rid.0),
        };

        let manifest = match store.get(&id) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &rid.0),
        };

        let created_at = indexed_created_at(&store, &id);

        (
            StatusCode::CREATED,
            Json(MutationResponse {
                request_id: rid.0,
                workspace: params.workspace,
                ticket: TicketDetail {
                    id: manifest.id.to_string(),
                    created_at,
                    fields: manifest.extra,
                },
            }),
        )
            .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `PATCH /api/tickets/{id}?workspace=<name>`
///
/// Update fields, state, or description of an existing ticket.
pub async fn update_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<UpdateTicketBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let patch = body.fields.unwrap_or_default();
    let from_state = body.from_state;
    let to_state = body.state;
    let description = body.description;
    let author = author_from_headers(&headers);

    tokio::task::spawn_blocking(move || {
        let manifest = match store.update(
            &id,
            patch,
            from_state.as_deref(),
            to_state.as_deref(),
            description.as_deref(),
            author.as_deref(),
        ) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &rid.0),
        };

        Json(MutationResponse {
            request_id: rid.0,
            workspace: params.workspace,
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `POST /api/tickets/{id}/close?workspace=<name>`
///
/// Fast-forward a ticket through all intermediate states to the target terminal
/// state. `target_state` defaults to `"done"`.
pub async fn close_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<CloseTicketBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let target = body.target_state.as_deref().unwrap_or("done").to_string();
    let author = author_from_headers(&headers);

    tokio::task::spawn_blocking(move || {
        let (manifest, _path) = match store.close(&id, &target, author.as_deref()) {
            Ok(result) => result,
            Err(e) => return storage_err(e, &rid.0),
        };

        Json(MutationResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `POST /api/tickets/{id}/cancel?workspace=<name>`
///
/// Transition a ticket to the `cancelled` state. Optional `reason` field is
/// stored as a ticket field update.
pub async fn cancel_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<CancelTicketBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let author = author_from_headers(&headers);
    let mut patch = BTreeMap::new();
    if let Some(reason) = body.reason {
        patch.insert("cancel_reason".to_string(), Value::String(reason));
    }

    tokio::task::spawn_blocking(move || {
        let manifest = match store.update(&id, patch, None, Some("cancelled"), None, author.as_deref()) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &rid.0),
        };

        Json(MutationResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `POST /api/tickets/{id}/revert?workspace=<name>`
///
/// Revert a ticket to a specific historical revision, identified by its
/// 1-based `revision` number. The revert is forward-only: a new history entry
/// is appended; no history is erased.
pub async fn revert_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<RevertTicketBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let revision = body.revision;
    let author = author_from_headers(&headers);

    tokio::task::spawn_blocking(move || {
        let revisions = match store.get_history(&id) {
            Ok(revisions) => revisions,
            Err(e) => return storage_err(e, &rid.0),
        };

        let target_rev = match revisions.iter().find(|revision_entry| revision_entry.rev == revision) {
            Some(revision_entry) => revision_entry.clone(),
            None => {
                return ApiError::bad_request(
                    "revision_not_found",
                    &format!("revision {} does not exist for this ticket", revision),
                    &rid.0,
                )
                .into_response_with_status(StatusCode::BAD_REQUEST);
            }
        };

        match store.apply_revert(&id, target_rev.fields, author.as_deref()) {
            Ok(_new_rev) => current_ticket_response(&store, &rid.0, &params.workspace, &id),
            Err(e) => storage_err(e, &rid.0),
        }
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `POST /api/tickets/{id}/undo?workspace=<name>`
///
/// Undo the last state/field transition on a ticket by reverting to the
/// second-to-last history revision, bypassing state-machine validation.
pub async fn undo_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let author = author_from_headers(&headers);

    tokio::task::spawn_blocking(move || {
        let revisions = match store.get_history(&id) {
            Ok(revisions) => revisions,
            Err(e) => return storage_err(e, &rid.0),
        };

        if revisions.len() < 2 {
            return ApiError::bad_request(
                "no_previous_revision",
                "ticket has no previous revision to undo",
                &rid.0,
            )
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY);
        }

        let prev_fields = revisions[revisions.len() - 2].fields.clone();

        match store.apply_revert(&id, prev_fields, author.as_deref()) {
            Ok(_new_rev) => current_ticket_response(&store, &rid.0, &params.workspace, &id),
            Err(e) => storage_err(e, &rid.0),
        }
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `DELETE /api/tickets/{id}?workspace=<name>`
///
/// Soft-delete (mark deleted) a ticket. Emits a `ticket.delete` SSE event.
pub async fn delete_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || match store.delete(&id) {
        Ok(()) => Json(DeleteResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
        })
        .into_response(),
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn author_from_headers(headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers).map(str::to_string)
}

fn indexed_created_at(
    store: &ticket_api::storage::store::TicketStore,
    id: &Uuid,
) -> chrono::DateTime<chrono::Utc> {
    store
        .get_indexed(id)
        .ok()
        .flatten()
        .map(|ticket| ticket.created_at)
        .unwrap_or_else(chrono::Utc::now)
}

fn current_ticket_response(
    store: &ticket_api::storage::store::TicketStore,
    request_id: &str,
    workspace: &str,
    id: &Uuid,
) -> Response {
    let manifest = match store.get(id) {
        Ok(manifest) => manifest,
        Err(e) => return storage_err(e, request_id),
    };

    Json(MutationResponse {
        request_id: request_id.to_string(),
        workspace: workspace.to_string(),
        ticket: TicketDetail {
            id: manifest.id.to_string(),
            created_at: indexed_created_at(store, id),
            fields: manifest.extra,
        },
    })
    .into_response()
}
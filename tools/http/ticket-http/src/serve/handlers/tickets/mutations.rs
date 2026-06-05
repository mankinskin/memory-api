use std::collections::BTreeMap;

use axum::{
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
    response::{
        IntoResponse,
        Json,
        Response,
    },
};
use serde_json::Value;
use uuid::Uuid;

use viewer_api::{
    auth::extract_bearer_token,
    error::{
        ApiError,
        RequestIdExt,
    },
};

use crate::serve::{
    AppState,
    error::{
        storage_err,
        task_join_err,
    },
};

use super::types::{
    CancelTicketBody,
    CloseTicketBody,
    CreateTicketBody,
    DeleteResponse,
    MutationResponse,
    MutationWorkspaceParam,
    RevertTicketBody,
    TicketDetail,
    UpdateTicketBody,
    ticket_ref_for_id,
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
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let extra = body.fields.unwrap_or_default();
    let type_id = body.type_id;
    let title = body.title;
    let description = body.description;
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let workspace = workspace.clone();
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
            Err(e) => return storage_err(e, &request_id),
        };

        let manifest = match store.get(&id) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };

        let created_at = indexed_created_at(&store, &id);
        let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &request_id),
        };

        (
            StatusCode::CREATED,
            Json(MutationResponse {
                request_id,
                active_workspace: workspace.clone(),
                workspace,
                ticket: TicketDetail {
                    id: manifest.id.to_string(),
                    ticket_ref,
                    created_at,
                    fields: manifest.extra,
                },
            }),
        )
            .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket create request"))
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
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let patch = body.fields.unwrap_or_default();
    let transition_states = body.transition_states;
    let to_state = body.state;
    let description = body.description;
    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let workspace = workspace.clone();
        let manifest = match store.update(
            &id,
            patch,
            Some(transition_states.as_slice()),
            to_state.as_deref(),
            description.as_deref(),
            author.as_deref(),
        ) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };

        let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &request_id),
        };

        Json(MutationResponse {
            request_id,
            active_workspace: workspace.clone(),
            workspace,
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                ticket_ref,
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket update request"))
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
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let target = body.target_state.as_deref().unwrap_or("done").to_string();
    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let (manifest, _path) =
            match store.close(&id, &target, author.as_deref()) {
                Ok(result) => result,
                Err(e) => return storage_err(e, &request_id),
            };

        Json(MutationResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                ticket_ref: match ticket_ref_for_id(&store, &workspace, &id) {
                    Ok(ticket_ref) => ticket_ref,
                    Err(e) => return storage_err(e, &request_id),
                },
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket close request"))
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
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let author = author_from_headers(&headers);
    let mut patch = BTreeMap::new();
    if let Some(reason) = body.reason {
        patch.insert("cancel_reason".to_string(), Value::String(reason));
    }
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let manifest = match store.update(
            &id,
            patch,
            Some(&[]),
            Some("cancelled"),
            None,
            author.as_deref(),
        ) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };

        Json(MutationResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                ticket_ref: match ticket_ref_for_id(&store, &workspace, &id) {
                    Ok(ticket_ref) => ticket_ref,
                    Err(e) => return storage_err(e, &request_id),
                },
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket cancel request"))
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
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let revision = body.revision;
    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let revisions = match store.get_history(&id) {
            Ok(revisions) => revisions,
            Err(e) => return storage_err(e, &request_id),
        };

        let target_rev = match revisions
            .iter()
            .find(|revision_entry| revision_entry.rev == revision)
        {
            Some(revision_entry) => revision_entry.clone(),
            None => {
                return ApiError::bad_request(
                    "revision_not_found",
                    &format!(
                        "revision {} does not exist for this ticket",
                        revision
                    ),
                    &request_id,
                )
                .into_response_with_status(StatusCode::BAD_REQUEST);
            },
        };

        match store.apply_revert(&id, target_rev.fields, author.as_deref()) {
            Ok(_new_rev) => current_ticket_response(
                &store,
                &request_id,
                &workspace,
                &id,
            ),
            Err(e) => storage_err(e, &request_id),
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket revert request"))
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
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let revisions = match store.get_history(&id) {
            Ok(revisions) => revisions,
            Err(e) => return storage_err(e, &request_id),
        };

        if revisions.len() < 2 {
            return ApiError::bad_request(
                "no_previous_revision",
                "ticket has no previous revision to undo",
                &request_id,
            )
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY);
        }

        let prev_fields = revisions[revisions.len() - 2].fields.clone();

        match store.apply_revert(&id, prev_fields, author.as_deref()) {
            Ok(_new_rev) => current_ticket_response(
                &store,
                &request_id,
                &workspace,
                &id,
            ),
            Err(e) => storage_err(e, &request_id),
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket undo request"))
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
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || match store.delete(&id) {
        Ok(()) => {
            let request_id = task_request_id.clone();
            let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
                Ok(ticket_ref) => ticket_ref,
                Err(e) => return storage_err(e, &request_id),
            };

            Json(DeleteResponse {
                request_id: request_id.clone(),
                active_workspace: workspace.clone(),
                workspace: workspace.clone(),
                id: id.to_string(),
                ticket_ref,
            })
            .into_response()
        },
        Err(e) => storage_err(e, &task_request_id),
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket delete request"))
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
        active_workspace: workspace.to_string(),
        workspace: workspace.to_string(),
        ticket: TicketDetail {
            id: manifest.id.to_string(),
            ticket_ref: match ticket_ref_for_id(store, workspace, id) {
                Ok(ticket_ref) => ticket_ref,
                Err(e) => return storage_err(e, request_id),
            },
            created_at: indexed_created_at(store, id),
            fields: manifest.extra,
        },
    })
    .into_response()
}

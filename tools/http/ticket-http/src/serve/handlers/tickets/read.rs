use std::{
    collections::BTreeMap,
    time::SystemTime,
};

use axum::{
    extract::{
        Extension,
        Path,
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
use uuid::Uuid;

use viewer_api::error::{
    ApiError,
    RequestIdExt,
};

use crate::serve::{
    AppState,
    error::storage_err,
};

use ticket_api::storage::ticket_fs::TicketFs;

use super::types::{
    TicketDescriptionResponse,
    TicketDetail,
    TicketDetailResponse,
    TicketIdParam,
    TicketSummary,
    TicketsResponse,
    WorkspaceParam,
};

pub async fn list_tickets(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<WorkspaceParam>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };

    tokio::task::spawn_blocking(move || {
        let requested_limit = params.limit.unwrap_or(100).min(1000);
        let state_filter = params.state.as_deref();
        let tickets: Vec<TicketSummary> = if let Some(query) = &params.query {
            let search_limit = if state_filter.is_some() {
                match store.count_tickets() {
                    Ok(count) => count.max(requested_limit),
                    Err(e) => return storage_err(e, &rid.0),
                }
            } else {
                requested_limit
            };

            match store.search_tickets(query, search_limit) {
                Ok(results) => {
                    let mut items = Vec::with_capacity(results.len().min(requested_limit));
                    for result in results
                        .into_iter()
                        .filter(|result| {
                            state_filter.map_or(true, |state| {
                                result.state.as_deref() == Some(state)
                            })
                        })
                        .take(requested_limit)
                    {
                        let (created_at, updated_at) =
                            match store.get_indexed(&result.id) {
                                Ok(Some(indexed)) =>
                                    (indexed.created_at, indexed.updated_at),
                                Ok(None) => epoch_timestamps(),
                                Err(e) => return storage_err(e, &rid.0),
                            };

                        items.push(TicketSummary {
                            id: result.id.to_string(),
                            type_id: result.ticket_type.unwrap_or_default(),
                            title: result.title,
                            state: result.state,
                            created_at,
                            updated_at,
                            fields: BTreeMap::new(),
                        });
                    }
                    items
                },
                Err(e) => return storage_err(e, &rid.0),
            }
        } else {
            match store.list(state_filter, None, Some(requested_limit)) {
                Ok(items) => items
                    .into_iter()
                    .map(|ticket| TicketSummary {
                        id: ticket.id.to_string(),
                        type_id: ticket.type_id,
                        title: ticket.title,
                        state: ticket.state,
                        created_at: ticket.created_at,
                        updated_at: ticket.updated_at,
                        fields: BTreeMap::new(),
                    })
                    .collect(),
                Err(e) => return storage_err(e, &rid.0),
            }
        };

        Json(TicketsResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            items: tickets,
            next_cursor: None,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub async fn get_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<TicketIdParam>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };

    tokio::task::spawn_blocking(move || match store.get(&id) {
        Ok(manifest) => Json(TicketDetailResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at: manifest.created_at,
                fields: manifest.extra.into_iter().collect(),
            },
        })
        .into_response(),
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `GET /api/tickets/{id}/description?workspace=<name>`
///
/// Returns the raw Markdown content of `description.md` for a ticket, if it
/// exists. Returns `{ "description": null }` when no description has been
/// written, rather than 404, so the UI can show a placeholder without special-
/// casing the status code.
pub async fn get_ticket_description(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<TicketIdParam>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };

    tokio::task::spawn_blocking(move || {
        let indexed = match store.get_indexed(&id) {
            Ok(Some(ticket)) => ticket,
            Ok(None) => {
                return ApiError::not_found("ticket", &rid.0)
                    .into_response_with_status(StatusCode::NOT_FOUND);
            },
            Err(e) => return storage_err(e, &rid.0),
        };

        if indexed.deleted {
            return ApiError::not_found("ticket", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }

        Json(TicketDescriptionResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
            description: TicketFs::read_description(&indexed.path),
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `GET /api/tickets/{id}/history?workspace=<name>`
///
/// Return all history revisions for a ticket, oldest first.
pub async fn get_ticket_history(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<TicketIdParam>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };

    tokio::task::spawn_blocking(move || match store.get_history(&id) {
        Ok(revisions) => {
            let entries: Vec<serde_json::Value> = revisions
                .into_iter()
                .map(|revision| {
                    serde_json::json!({
                        "rev": revision.rev,
                        "ts": revision.ts,
                        "author": revision.author,
                        "fields": revision.fields,
                    })
                })
                .collect();
            Json(serde_json::json!({
                "request_id": &rid.0,
                "workspace": &params.workspace,
                "id": id.to_string(),
                "count": entries.len(),
                "entries": entries,
            }))
            .into_response()
        },
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn epoch_timestamps()
-> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let epoch = chrono::DateTime::<chrono::Utc>::from(SystemTime::UNIX_EPOCH);
    (epoch, epoch)
}

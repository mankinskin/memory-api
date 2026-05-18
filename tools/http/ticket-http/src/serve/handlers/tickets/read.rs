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
    HistoryEntry,
    TicketDescriptionResponse,
    TicketDetail,
    TicketDetailResponse,
    TicketHistoryResponse,
    TicketIdParam,
    TicketSummary,
    TicketsResponse,
    WorkspaceParam,
    ticket_ref_for_id,
    ticket_ref_from_indexed,
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
                            ticket_ref: match ticket_ref_for_id(
                                &store,
                                &params.workspace,
                                &result.id,
                            ) {
                                Ok(ticket_ref) => ticket_ref,
                                Err(e) => return storage_err(e, &rid.0),
                            },
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
                Ok(items) => {
                    let mut summaries = Vec::with_capacity(items.len());
                    for ticket in items {
                        let ticket_ref = match ticket_ref_from_indexed(
                            &store,
                            &params.workspace,
                            &ticket,
                        ) {
                            Ok(ticket_ref) => ticket_ref,
                            Err(e) => return storage_err(e, &rid.0),
                        };
                        summaries.push(TicketSummary {
                            id: ticket.id.to_string(),
                            ticket_ref,
                            type_id: ticket.type_id,
                            title: ticket.title,
                            state: ticket.state,
                            created_at: ticket.created_at,
                            updated_at: ticket.updated_at,
                            fields: BTreeMap::new(),
                        });
                    }
                    summaries
                },
                Err(e) => return storage_err(e, &rid.0),
            }
        };

        Json(TicketsResponse {
            request_id: rid.0.clone(),
            active_workspace: params.workspace.clone(),
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
        Ok(manifest) => {
            let ticket_ref = match ticket_ref_for_id(
                &store,
                &params.workspace,
                &id,
            ) {
                Ok(ticket_ref) => ticket_ref,
                Err(e) => return storage_err(e, &rid.0),
            };

            Json(TicketDetailResponse {
                request_id: rid.0.clone(),
                active_workspace: params.workspace.clone(),
                workspace: params.workspace.clone(),
                ticket: TicketDetail {
                    id: manifest.id.to_string(),
                    ticket_ref,
                    created_at: manifest.created_at,
                    fields: manifest.extra.into_iter().collect(),
                },
            })
            .into_response()
        },
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

        let ticket_ref = match ticket_ref_from_indexed(
            &store,
            &params.workspace,
            &indexed,
        ) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &rid.0),
        };

        Json(TicketDescriptionResponse {
            request_id: rid.0.clone(),
            active_workspace: params.workspace.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
            ticket_ref,
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
            let ticket_ref = match ticket_ref_for_id(
                &store,
                &params.workspace,
                &id,
            ) {
                Ok(ticket_ref) => ticket_ref,
                Err(e) => return storage_err(e, &rid.0),
            };

            let entries = revisions
                .into_iter()
                .map(|revision| HistoryEntry {
                    rev: revision.rev,
                    ts: revision.ts,
                    author: revision.author,
                    fields: revision.fields,
                })
                .collect::<Vec<_>>();
            Json(TicketHistoryResponse {
                request_id: rid.0.clone(),
                active_workspace: params.workspace.clone(),
                workspace: params.workspace.clone(),
                id: id.to_string(),
                ticket_ref,
                count: entries.len() as u64,
                entries,
            })
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

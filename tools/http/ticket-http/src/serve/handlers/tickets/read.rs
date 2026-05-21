use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
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
    error::{
        storage_err,
        task_join_err,
    },
    registry::ResolvedIndexedTicket,
};

use ticket_api::storage::ticket_fs::TicketFs;

use super::types::{
    HistoryEntry,
    TicketDescriptionResponse,
    TicketDetail,
    TicketDetailResponse,
    TicketHistoryResponse,
    TicketIdParam,
    TicketRef,
    TicketSummary,
    TicketsResponse,
    WorkspaceParam,
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
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let requested_limit = params.limit.unwrap_or(100).min(1000);
        let state_filter = params.state.as_deref();
        let tickets: Vec<TicketSummary> = if let Some(query) = &params.query {
            let search_limit = if state_filter.is_some() {
                match store.count_tickets() {
                    Ok(count) => count.max(requested_limit),
                    Err(e) => return storage_err(e, &request_id),
                }
            } else {
                requested_limit
            };

            match store.search_tickets(query, search_limit) {
                Ok(results) => {
                    let results = results
                        .into_iter()
                        .filter(|result| {
                            state_filter.map_or(true, |state| {
                                result.state.as_deref() == Some(state)
                            })
                        })
                        .take(requested_limit)
                        .collect::<Vec<_>>();
                    let ids = results
                        .iter()
                        .map(|result| result.id)
                        .collect::<Vec<_>>();
                    let resolved = match resolve_tickets(
                        &state,
                        &params.workspace,
                        &ids,
                        &request_id,
                    ) {
                        Ok(resolved) => resolved,
                        Err(response) => return response,
                    };

                    let mut items = Vec::with_capacity(results.len());
                    for result in results {
                        let local_ticket = match store.get_indexed(&result.id) {
                            Ok(ticket) => ticket,
                            Err(e) => return storage_err(e, &request_id),
                        };
                        let local_ticket_ref = match local_ticket
                            .as_ref()
                            .map(|indexed| {
                                ticket_ref_from_indexed(
                                    &store,
                                    &params.workspace,
                                    indexed,
                                )
                            })
                            .transpose()
                        {
                            Ok(ticket_ref) => ticket_ref,
                            Err(e) => return storage_err(e, &request_id),
                        };

                        let prefer_local = local_ticket
                            .as_ref()
                            .zip(local_ticket_ref.as_ref())
                            .map(|(ticket, ticket_ref)| {
                                should_use_local_ticket(
                                    &params.workspace,
                                    ticket,
                                    ticket_ref,
                                )
                            })
                            .unwrap_or(false);

                        let (created_at, updated_at, ticket_ref, type_id) =
                            if prefer_local {
                                let ticket = local_ticket
                                    .as_ref()
                                    .expect("local ticket");
                                (
                                    ticket.created_at,
                                    ticket.updated_at,
                                    local_ticket_ref.expect("local ticket ref"),
                                    result.ticket_type.clone().unwrap_or_else(
                                        || ticket.type_id.clone(),
                                    ),
                                )
                            } else if let Some(ticket) =
                                resolved.get(&result.id)
                            {
                                (
                                    ticket.ticket.created_at,
                                    ticket.ticket.updated_at,
                                    ticket_ref_from_resolved(ticket),
                                    result.ticket_type.clone().unwrap_or_else(
                                        || ticket.ticket.type_id.clone(),
                                    ),
                                )
                            } else if let Some(ticket) = local_ticket {
                                (
                                    ticket.created_at,
                                    ticket.updated_at,
                                    local_ticket_ref.unwrap_or(TicketRef {
                                        workspace: params.workspace.clone(),
                                        id: result.id.to_string(),
                                    }),
                                    result
                                        .ticket_type
                                        .clone()
                                        .unwrap_or(ticket.type_id),
                                )
                            } else {
                                let (created_at, updated_at) =
                                    epoch_timestamps();
                                (
                                    created_at,
                                    updated_at,
                                    TicketRef {
                                        workspace: params.workspace.clone(),
                                        id: result.id.to_string(),
                                    },
                                    result
                                        .ticket_type
                                        .clone()
                                        .unwrap_or_default(),
                                )
                            };

                        items.push(TicketSummary {
                            id: result.id.to_string(),
                            ticket_ref,
                            type_id,
                            title: result.title,
                            state: result.state,
                            created_at,
                            updated_at,
                            fields: BTreeMap::new(),
                        });
                    }
                    items
                },
                Err(e) => return storage_err(e, &request_id),
            }
        } else {
            match store.list(state_filter, None, Some(requested_limit)) {
                Ok(items) => {
                    let ids = items
                        .iter()
                        .map(|ticket| ticket.id)
                        .collect::<Vec<_>>();
                    let resolved = match resolve_tickets(
                        &state,
                        &params.workspace,
                        &ids,
                        &request_id,
                    ) {
                        Ok(resolved) => resolved,
                        Err(response) => return response,
                    };
                    let mut summaries = Vec::with_capacity(items.len());
                    for ticket in items {
                        let resolved_ticket = resolved.get(&ticket.id);
                        let local_ticket_ref = match ticket_ref_from_indexed(
                            &store,
                            &params.workspace,
                            &ticket,
                        ) {
                            Ok(ticket_ref) => ticket_ref,
                            Err(e) => return storage_err(e, &request_id),
                        };
                        let prefer_local = should_use_local_ticket(
                            &params.workspace,
                            &ticket,
                            &local_ticket_ref,
                        );
                        let ticket_ref = if prefer_local {
                            local_ticket_ref
                        } else {
                            resolved_ticket
                                .map(ticket_ref_from_resolved)
                                .unwrap_or(local_ticket_ref)
                        };
                        let ticket_meta = if prefer_local {
                            None
                        } else {
                            resolved_ticket.map(|ticket| &ticket.ticket)
                        };
                        summaries.push(TicketSummary {
                            id: ticket.id.to_string(),
                            ticket_ref,
                            type_id: ticket_meta
                                .map(|ticket| ticket.type_id.clone())
                                .unwrap_or(ticket.type_id),
                            title: ticket_meta
                                .and_then(|ticket| ticket.title.clone())
                                .or(ticket.title),
                            state: ticket_meta
                                .and_then(|ticket| ticket.state.clone())
                                .or(ticket.state),
                            created_at: ticket_meta
                                .map(|ticket| ticket.created_at)
                                .unwrap_or(ticket.created_at),
                            updated_at: ticket_meta
                                .map(|ticket| ticket.updated_at)
                                .unwrap_or(ticket.updated_at),
                            fields: BTreeMap::new(),
                        });
                    }
                    summaries
                },
                Err(e) => return storage_err(e, &request_id),
            }
        };

        Json(TicketsResponse {
            request_id: request_id.clone(),
            active_workspace: params.workspace.clone(),
            workspace: params.workspace.clone(),
            items: tickets,
            next_cursor: None,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket list request"))
}

pub async fn get_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<TicketIdParam>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(store) => store,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &params.workspace,
            id,
            &request_id,
        ) {
            Ok(ticket) => ticket,
            Err(response) => return response,
        };
        match TicketFs::read(&resolved.path) {
            Ok(manifest) => {
                let ticket_ref = resolved.ticket_ref;

                Json(TicketDetailResponse {
                    request_id: request_id.clone(),
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
            Err(e) => storage_err(e, &request_id),
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket detail request"))
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
        Some(store) => store,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &params.workspace,
            id,
            &request_id,
        ) {
            Ok(ticket) => ticket,
            Err(response) => return response,
        };

        Json(TicketDescriptionResponse {
            request_id: request_id.clone(),
            active_workspace: params.workspace.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
            ticket_ref: resolved.ticket_ref,
            description: TicketFs::read_description(&resolved.path),
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| {
        task_join_err(&request_id, "ticket description request")
    })
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
        Some(store) => store,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &params.workspace,
            id,
            &request_id,
        ) {
            Ok(ticket) => ticket,
            Err(response) => return response,
        };
        match TicketFs::read_history(&resolved.path) {
            Ok(revisions) => {
                let ticket_ref = resolved.ticket_ref;

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
                    request_id: request_id.clone(),
                    active_workspace: params.workspace.clone(),
                    workspace: params.workspace.clone(),
                    id: id.to_string(),
                    ticket_ref,
                    count: entries.len() as u64,
                    entries,
                })
                .into_response()
            },
            Err(e) => storage_err(e, &request_id),
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket history request"))
}

fn resolve_tickets(
    state: &AppState,
    active_workspace: &str,
    ids: &[Uuid],
    request_id: &str,
) -> Result<HashMap<Uuid, ResolvedIndexedTicket>, Response> {
    state
        .registry
        .resolve_indexed_many(active_workspace, ids)
        .map_err(|error| storage_err(error, request_id))
}

fn resolve_ticket(
    state: &AppState,
    active_workspace: &str,
    id: Uuid,
    request_id: &str,
) -> Result<ResolvedIndexedTicket, Response> {
    let mut resolved =
        resolve_tickets(state, active_workspace, &[id], request_id)?;
    resolved.remove(&id).ok_or_else(|| {
        ApiError::not_found("ticket", request_id)
            .into_response_with_status(StatusCode::NOT_FOUND)
    })
}

fn ticket_ref_from_resolved(ticket: &ResolvedIndexedTicket) -> TicketRef {
    TicketRef {
        workspace: ticket.workspace.clone(),
        id: ticket.ticket.id.to_string(),
    }
}

struct PreferredResolvedTicket {
    path: std::path::PathBuf,
    ticket_ref: TicketRef,
}

fn resolve_ticket_with_preferred_source(
    store: &ticket_api::storage::store::TicketStore,
    state: &AppState,
    active_workspace: &str,
    id: Uuid,
    request_id: &str,
) -> Result<PreferredResolvedTicket, Response> {
    let local_ticket = match store.get_indexed(&id) {
        Ok(ticket) => ticket,
        Err(error) => return Err(storage_err(error, request_id)),
    };
    let local_ticket_ref = match local_ticket
        .as_ref()
        .map(|ticket| ticket_ref_from_indexed(store, active_workspace, ticket))
        .transpose()
    {
        Ok(ticket_ref) => ticket_ref,
        Err(error) => return Err(storage_err(error, request_id)),
    };

    if let Some((ticket, ticket_ref)) = local_ticket
        .as_ref()
        .zip(local_ticket_ref.as_ref())
        .filter(|(ticket, ticket_ref)| {
            should_use_local_ticket(active_workspace, ticket, ticket_ref)
        })
    {
        return Ok(PreferredResolvedTicket {
            path: ticket.path.clone(),
            ticket_ref: ticket_ref.clone(),
        });
    }

    let resolved = resolve_ticket(state, active_workspace, id, request_id)?;
    let ticket_ref = ticket_ref_from_resolved(&resolved);
    Ok(PreferredResolvedTicket {
        path: resolved.ticket.path,
        ticket_ref,
    })
}

fn should_use_local_ticket(
    active_workspace: &str,
    ticket: &ticket_api::storage::indexed::IndexedTicket,
    ticket_ref: &TicketRef,
) -> bool {
    ticket_ref.workspace != active_workspace
        && ticket.path.join("ticket.toml").is_file()
}

fn epoch_timestamps()
-> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
    let epoch = chrono::DateTime::<chrono::Utc>::from(SystemTime::UNIX_EPOCH);
    (epoch, epoch)
}

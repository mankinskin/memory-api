use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
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
use serde_json::json;
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
    let (workspace, store) = match resolve_workspace_request(
        &state,
        &params.workspace,
        &rid.0,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let requested_limit = params.limit.unwrap_or(100).min(1000);
        let state_filter = params.state.as_deref();
        let tickets: Vec<TicketSummary> = if let Some(query) = &params.query {
            let search_limit = match store.count_tickets() {
                Ok(count) => count.max(requested_limit),
                Err(e) => return storage_err(e, &request_id),
            };

            match store.search_tickets(query, search_limit) {
                Ok(results) => {
                    let ids = results
                        .iter()
                        .map(|result| result.id)
                        .collect::<Vec<_>>();
                    let resolved = match resolve_tickets(
                        &state,
                        &workspace,
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
                                    &workspace,
                                    indexed,
                                )
                            })
                            .transpose()
                        {
                            Ok(ticket_ref) => ticket_ref,
                            Err(e) => return storage_err(e, &request_id),
                        };
                        let resolved_ticket = resolved.get(&result.id);
                        let summary = if should_prefer_local_ticket(
                            &store,
                            &workspace,
                            local_ticket.as_ref(),
                            local_ticket_ref.as_ref(),
                            resolved_ticket,
                        ) {
                            let ticket =
                                local_ticket.as_ref().expect("local ticket");
                            let ticket_ref = local_ticket_ref
                                .expect("local ticket ref");
                            Some(ticket_summary_from_indexed(ticket_ref, ticket))
                        } else {
                            resolved_ticket.map(ticket_summary_from_resolved)
                        };

                        let Some(summary) = summary else {
                            tracing::debug!(
                                ticket_id = %result.id,
                                active_workspace = %workspace,
                                has_local = local_ticket.is_some(),
                                local_deleted = local_ticket
                                    .as_ref()
                                    .map(|ticket| ticket.deleted)
                                    .unwrap_or(false),
                                has_resolved = resolved_ticket.is_some(),
                                "dropping unresolved search hit"
                            );
                            continue;
                        };
                        if state_filter.map_or(true, |state| {
                            summary.state.as_deref() == Some(state)
                        }) {
                            items.push(summary);
                        }
                        if items.len() >= requested_limit {
                            break;
                        }
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
                        &workspace,
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
                            &workspace,
                            &ticket,
                        ) {
                            Ok(ticket_ref) => ticket_ref,
                            Err(e) => return storage_err(e, &request_id),
                        };
                        let summary = if should_prefer_local_ticket(
                            &store,
                            &workspace,
                            Some(&ticket),
                            Some(&local_ticket_ref),
                            resolved_ticket,
                        ) {
                            ticket_summary_from_indexed(local_ticket_ref, &ticket)
                        } else if let Some(resolved_ticket) = resolved_ticket {
                            ticket_summary_from_resolved(resolved_ticket)
                        } else {
                            continue;
                        };
                        summaries.push(summary);
                    }
                    summaries
                },
                Err(e) => return storage_err(e, &request_id),
            }
        };

        Json(TicketsResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
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
    let (workspace, store) = match resolve_workspace_request(
        &state,
        &params.workspace,
        &rid.0,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &workspace,
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
                    active_workspace: workspace.clone(),
                    workspace: workspace.clone(),
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
    let (workspace, store) = match resolve_workspace_request(
        &state,
        &params.workspace,
        &rid.0,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &workspace,
            id,
            &request_id,
        ) {
            Ok(ticket) => ticket,
            Err(response) => return response,
        };

        Json(TicketDescriptionResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
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
    let (workspace, store) = match resolve_workspace_request(
        &state,
        &params.workspace,
        &rid.0,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let state = state.clone();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &workspace,
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
                    active_workspace: workspace.clone(),
                    workspace: workspace.clone(),
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

fn resolve_workspace_request(
    state: &AppState,
    requested_workspace: &str,
    request_id: &str,
) -> Result<
    (
        String,
        std::sync::Arc<ticket_api::storage::store::TicketStore>,
    ),
    Response,
> {
    match state.resolve_workspace_runtime(requested_workspace) {
        Ok(Some((workspace, store))) => Ok((workspace, store)),
        Ok(None) => Err(
            ApiError::not_found("workspace", request_id)
                .into_response_with_status(StatusCode::NOT_FOUND),
        ),
        Err(crate::serve::registry::WorkspaceResolveError::AmbiguousLegacyLabel {
            requested,
            matches,
        }) => Err(
            ApiError::bad_request(
                "workspace.ambiguous_label",
                format!("workspace label '{requested}' matches multiple workspaces"),
                request_id,
            )
            .with_details(json!({
                "requested": requested,
                "matches": matches,
            }))
            .into_response_with_status(StatusCode::BAD_REQUEST),
        ),
    }
}

fn ticket_ref_from_resolved(ticket: &ResolvedIndexedTicket) -> TicketRef {
    TicketRef {
        workspace: ticket.workspace.clone(),
        id: ticket.ticket.id.to_string(),
    }
}

fn ticket_summary_from_indexed(
    ticket_ref: TicketRef,
    ticket: &ticket_api::storage::indexed::IndexedTicket,
) -> TicketSummary {
    TicketSummary {
        id: ticket.id.to_string(),
        ticket_ref,
        type_id: ticket.type_id.clone(),
        title: ticket.title.clone(),
        state: ticket.state.clone(),
        created_at: ticket.created_at,
        updated_at: ticket.updated_at,
        fields: BTreeMap::new(),
    }
}

fn ticket_summary_from_resolved(ticket: &ResolvedIndexedTicket) -> TicketSummary {
    TicketSummary {
        id: ticket.ticket.id.to_string(),
        ticket_ref: ticket_ref_from_resolved(ticket),
        type_id: ticket.ticket.type_id.clone(),
        title: ticket.ticket.title.clone(),
        state: ticket.ticket.state.clone(),
        created_at: ticket.ticket.created_at,
        updated_at: ticket.ticket.updated_at,
        fields: BTreeMap::new(),
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

    let resolved = resolve_ticket(state, active_workspace, id, request_id)?;

    if should_prefer_local_ticket(
        store,
        active_workspace,
        local_ticket.as_ref(),
        local_ticket_ref.as_ref(),
        Some(&resolved),
    ) {
        let ticket = local_ticket.as_ref().expect("local ticket");
        let ticket_ref = local_ticket_ref.expect("local ticket ref");
        return Ok(PreferredResolvedTicket {
            path: ticket.path.clone(),
            ticket_ref,
        });
    }

    let ticket_ref = ticket_ref_from_resolved(&resolved);
    Ok(PreferredResolvedTicket {
        path: resolved.ticket.path,
        ticket_ref,
    })
}

fn should_prefer_local_ticket(
    store: &ticket_api::storage::store::TicketStore,
    active_workspace: &str,
    local_ticket: Option<&ticket_api::storage::indexed::IndexedTicket>,
    local_ticket_ref: Option<&TicketRef>,
    resolved_ticket: Option<&ResolvedIndexedTicket>,
) -> bool {
    let (Some(ticket), Some(ticket_ref), Some(resolved_ticket)) =
        (local_ticket, local_ticket_ref, resolved_ticket)
    else {
        return false;
    };

    should_use_local_ticket(active_workspace, ticket, ticket_ref)
        && resolved_ticket.store.index_root == store.index_root
}

fn should_use_local_ticket(
    active_workspace: &str,
    ticket: &ticket_api::storage::indexed::IndexedTicket,
    ticket_ref: &TicketRef,
) -> bool {
    !ticket.deleted
        && ticket_ref.workspace != active_workspace
        && ticket.path.join("ticket.toml").is_file()
}

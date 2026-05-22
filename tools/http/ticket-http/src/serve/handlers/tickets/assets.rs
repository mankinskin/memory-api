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

use super::types::{
    TicketAssetParam,
    TicketAssetResponse,
    TicketFileEntry,
    TicketFilesResponse,
    TicketIdParam,
    TicketRef,
};

/// `GET /api/tickets/{id}/files?workspace=<name>`
///
/// Returns the list of user-visible files for a ticket:
/// - `description.md` (if present) — always first
/// - Every `*.md` file under `assets/` (recursively), sorted by path
pub async fn list_ticket_files(
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
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &workspace,
            id,
            &task_request_id,
        ) {
            Ok(ticket) => ticket,
            Err(response) => return response,
        };
        let ticket_dir = resolved.path;
        let ticket_ref = resolved.ticket_ref;

        let mut files = Vec::new();
        if ticket_dir.join("description.md").is_file() {
            files.push(TicketFileEntry {
                path: "description.md".to_string(),
                name: "description.md".to_string(),
            });
        }

        let assets_dir = ticket_dir.join("assets");
        if assets_dir.is_dir() {
            collect_ticket_files(&assets_dir, &ticket_dir, &mut files);
        }

        Json(TicketFilesResponse {
            request_id: task_request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            id: id.to_string(),
            ticket_ref,
            files,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket files request"))
}

/// `GET /api/tickets/{id}/asset?workspace=<name>&path=<relative-path>`
///
/// Returns the raw UTF-8 content of a single ticket asset file.
/// Only files within the ticket's own directory tree are accessible;
/// path traversal attempts are rejected with `403 Forbidden`.
pub async fn get_ticket_asset(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<TicketAssetParam>,
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
        let resolved = match resolve_ticket_with_preferred_source(
            &store,
            &state,
            &workspace,
            id,
            &task_request_id,
        ) {
            Ok(ticket) => ticket,
            Err(response) => return response,
        };
        let ticket_dir = resolved.path;
        let ticket_ref = resolved.ticket_ref;
        let asset_path = match resolve_asset_path(&ticket_dir, &params.path) {
            Ok(path) => path,
            Err(response) => return response,
        };
        let content = match read_asset_content(&asset_path) {
            Ok(content) => content,
            Err(response) => return response,
        };

        Json(TicketAssetResponse {
            request_id: task_request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            id: id.to_string(),
            ticket_ref,
            path: params.path.clone(),
            content,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket asset request"))
}

fn resolve_ticket(
    state: &AppState,
    active_workspace: &str,
    id: Uuid,
    request_id: &str,
) -> Result<ResolvedIndexedTicket, Response> {
    let mut resolved = state
        .registry
        .resolve_indexed_many(active_workspace, &[id])
        .map_err(|error| storage_err(error, request_id))?;
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
    state.resolve_public_workspace_request(requested_workspace, request_id)
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
        .map(|ticket| {
            super::types::ticket_ref_from_indexed(
                store,
                active_workspace,
                ticket,
            )
        })
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
        &resolved,
    ) {
        let ticket = local_ticket.as_ref().expect("local ticket");
        let ticket_ref = local_ticket_ref.expect("local ticket ref");
        return Ok(PreferredResolvedTicket {
            path: ticket.path.clone(),
            ticket_ref,
        });
    }

    Ok(PreferredResolvedTicket {
        path: resolved.ticket.path.clone(),
        ticket_ref: TicketRef {
            workspace: resolved.workspace,
            id: resolved.ticket.id.to_string(),
        },
    })
}

fn should_prefer_local_ticket(
    store: &ticket_api::storage::store::TicketStore,
    active_workspace: &str,
    local_ticket: Option<&ticket_api::storage::indexed::IndexedTicket>,
    local_ticket_ref: Option<&TicketRef>,
    resolved_ticket: &ResolvedIndexedTicket,
) -> bool {
    let (Some(ticket), Some(ticket_ref)) = (local_ticket, local_ticket_ref)
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

fn resolve_asset_path(
    ticket_dir: &std::path::Path,
    requested_path: &str,
) -> Result<std::path::PathBuf, Response> {
    let canonical_dir = match ticket_dir.canonicalize() {
        Ok(path) => path,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
    };

    let canonical_file = match ticket_dir.join(requested_path).canonicalize() {
        Ok(path) => path,
        Err(_) =>
            return Err(
                (StatusCode::NOT_FOUND, "file not found").into_response()
            ),
    };

    if !canonical_file.starts_with(&canonical_dir) {
        return Err((StatusCode::FORBIDDEN, "access denied").into_response());
    }

    Ok(canonical_file)
}

fn read_asset_content(
    asset_path: &std::path::Path
) -> Result<String, Response> {
    std::fs::read_to_string(asset_path)
        .map_err(|_| (StatusCode::NOT_FOUND, "file not found").into_response())
}

/// Recursively collect all files under `dir`, appending `TicketFileEntry`
/// items with paths relative to `ticket_dir`.
fn collect_ticket_files(
    dir: &std::path::Path,
    ticket_dir: &std::path::Path,
    files: &mut Vec<TicketFileEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<std::path::PathBuf> =
        entries.flatten().map(|entry| entry.path()).collect();
    children.sort();

    for child in children {
        if child.is_dir() {
            collect_ticket_files(&child, ticket_dir, files);
            continue;
        }

        let Some(ext) = child.extension() else {
            continue;
        };
        if !ext.eq_ignore_ascii_case("md") {
            continue;
        }

        if let Ok(relative) = child.strip_prefix(ticket_dir) {
            let path = relative.to_string_lossy().replace('\\', "/");
            let name = child
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            files.push(TicketFileEntry { path, name });
        }
    }
}

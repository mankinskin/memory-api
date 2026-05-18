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

use super::types::{
    TicketAssetParam,
    TicketAssetResponse,
    TicketFileEntry,
    TicketFilesResponse,
    TicketIdParam,
    ticket_ref_for_id,
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
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };

    tokio::task::spawn_blocking(move || {
        let ticket_dir = match resolve_ticket_dir(&store, id, &rid.0) {
            Ok(path) => path,
            Err(response) => return response,
        };

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
            request_id: rid.0.clone(),
            active_workspace: params.workspace.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
            ticket_ref: match ticket_ref_for_id(
                &store,
                &params.workspace,
                &id,
            ) {
                Ok(ticket_ref) => ticket_ref,
                Err(e) => return storage_err(e, &rid.0),
            },
            files,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        },
    };

    tokio::task::spawn_blocking(move || {
        let ticket_dir = match resolve_ticket_dir(&store, id, &rid.0) {
            Ok(path) => path,
            Err(response) => return response,
        };
        let asset_path = match resolve_asset_path(&ticket_dir, &params.path) {
            Ok(path) => path,
            Err(response) => return response,
        };
        let content = match read_asset_content(&asset_path) {
            Ok(content) => content,
            Err(response) => return response,
        };

        Json(TicketAssetResponse {
            request_id: rid.0.clone(),
            active_workspace: params.workspace.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
            ticket_ref: match ticket_ref_for_id(
                &store,
                &params.workspace,
                &id,
            ) {
                Ok(ticket_ref) => ticket_ref,
                Err(e) => return storage_err(e, &rid.0),
            },
            path: params.path.clone(),
            content,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn resolve_ticket_dir(
    store: &ticket_api::storage::store::TicketStore,
    id: Uuid,
    request_id: &str,
) -> Result<std::path::PathBuf, Response> {
    let indexed = match store.get_indexed(&id) {
        Ok(Some(ticket)) => ticket,
        Ok(None) => {
            return Err(ApiError::not_found("ticket", request_id)
                .into_response_with_status(StatusCode::NOT_FOUND));
        },
        Err(e) => return Err(storage_err(e, request_id)),
    };

    if indexed.deleted {
        return Err(ApiError::not_found("ticket", request_id)
            .into_response_with_status(StatusCode::NOT_FOUND));
    }

    Ok(indexed.path)
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

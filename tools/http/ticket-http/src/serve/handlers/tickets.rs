use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::SystemTime;
use uuid::Uuid;

use viewer_api::auth::extract_bearer_token;
use viewer_api::error::{ApiError, RequestIdExt};
use crate::serve::{error::storage_err, AppState};
use ticket_api::storage::ticket_fs::TicketFs;

#[derive(Deserialize)]
pub struct WorkspaceParam {
    pub workspace: String,
    pub state: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
    /// Pagination cursor — not yet implemented, accepted to keep the API forward-compatible.
    #[allow(dead_code)]
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct TicketIdParam {
    pub workspace: String,
}

#[derive(Serialize)]
pub struct TicketSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub type_id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Serialize)]
pub struct TicketsResponse {
    pub request_id: String,
    pub workspace: String,
    pub items: Vec<TicketSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct TicketDetailResponse {
    pub request_id: String,
    pub workspace: String,
    pub ticket: TicketDetail,
}

#[derive(Serialize)]
pub struct TicketDetail {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
}

pub async fn list_tickets(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<WorkspaceParam>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || {
        // Use query search if provided, otherwise plain list
        let tickets = if let Some(q) = &params.query {
            let limit = params.limit.unwrap_or(100).min(1000);
            match store.search_tickets(q, limit) {
                Ok(results) => {
                    let mut items = Vec::with_capacity(results.len());
                    for r in results {
                        let (created_at, updated_at) = match store.get_indexed(&r.id) {
                            Ok(Some(indexed)) => (indexed.created_at, indexed.updated_at),
                            Ok(None) => {
                                let epoch = chrono::DateTime::<chrono::Utc>::from(SystemTime::UNIX_EPOCH);
                                (epoch, epoch)
                            }
                            Err(e) => return storage_err(e, &rid.0),
                        };

                        items.push(TicketSummary {
                            id: r.id.to_string(),
                            type_id: r.ticket_type.unwrap_or_default(),
                            title: r.title,
                            state: r.state,
                            created_at,
                            updated_at,
                            fields: BTreeMap::new(),
                        });
                    }
                    items
                }
                Err(e) => return storage_err(e, &rid.0),
            }
        } else {
            let limit = params.limit.map(|l| l.min(1000));
            match store.list(params.state.as_deref(), None, limit) {
                Ok(items) => items
                    .into_iter()
                    .map(|t| TicketSummary {
                        id: t.id.to_string(),
                        type_id: t.type_id,
                        title: t.title,
                        state: t.state,
                        created_at: t.created_at,
                        updated_at: t.updated_at,
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
            next_cursor: None, // cursor pagination deferred to later iteration
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
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || match store.get(&id) {
        Ok(manifest) => Json(TicketDetailResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at: manifest.created_at,
                fields: manifest.extra.into_iter().map(|(k, v)| (k, v)).collect(),
            },
        })
        .into_response(),
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Serialize)]
pub struct TicketDescriptionResponse {
    pub request_id: String,
    pub workspace: String,
    pub id: String,
    pub description: Option<String>,
}

/// `GET /api/tickets/{id}/description?workspace=<name>`
///
/// Returns the raw Markdown content of `description.md` for a ticket, if it
/// exists.  Returns `{ "description": null }` when no description has been
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
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || {
        let indexed = match store.get_indexed(&id) {
            Ok(Some(t)) => t,
            Ok(None) => {
                return viewer_api::error::ApiError::not_found("ticket", &rid.0)
                    .into_response_with_status(StatusCode::NOT_FOUND);
            }
            Err(e) => return storage_err(e, &rid.0),
        };

        if indexed.deleted {
            return viewer_api::error::ApiError::not_found("ticket", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }

        let description = TicketFs::read_description(&indexed.path);

        Json(TicketDescriptionResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
            description,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Extract the bearer token from request headers as an author string.
fn author_from_headers(headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers).map(str::to_string)
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
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || match store.get_history(&id) {
        Ok(revisions) => {
            let entries: Vec<serde_json::Value> = revisions
                .into_iter()
                .map(|r| serde_json::json!({
                    "rev": r.rev,
                    "ts": r.ts,
                    "author": r.author,
                    "fields": r.fields,
                }))
                .collect();
            Json(serde_json::json!({
                "request_id": &rid.0,
                "workspace": &params.workspace,
                "id": id.to_string(),
                "count": entries.len(),
                "entries": entries,
            }))
            .into_response()
        }
        Err(e) => storage_err(e, &rid.0),
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ── Mutation request / response types ────────────────────────────────────────

/// Query-string params shared by all mutation endpoints.
#[derive(Deserialize)]
pub struct MutationWorkspaceParam {
    pub workspace: String,
}

/// `POST /api/tickets` request body.
#[derive(Deserialize)]
pub struct CreateTicketBody {
    #[serde(rename = "type")]
    pub type_id: String,
    pub title: Option<String>,
    pub fields: Option<BTreeMap<String, Value>>,
    pub description: Option<String>,
}

/// `PATCH /api/tickets/{id}` request body.
#[derive(Deserialize)]
pub struct UpdateTicketBody {
    pub fields: Option<BTreeMap<String, Value>>,
    pub state: Option<String>,
    pub from_state: Option<String>,
    pub description: Option<String>,
}

/// `POST /api/tickets/{id}/close` request body.
#[derive(Deserialize)]
pub struct CloseTicketBody {
    /// Target terminal state.  Defaults to `"done"`.
    pub target_state: Option<String>,
}

/// `POST /api/tickets/{id}/cancel` request body.
#[derive(Deserialize)]
pub struct CancelTicketBody {
    pub reason: Option<String>,
}

/// `POST /api/tickets/{id}/revert` request body.
#[derive(Deserialize)]
pub struct RevertTicketBody {
    pub revision: u64,
}

#[derive(Serialize)]
pub struct MutationResponse {
    pub request_id: String,
    pub workspace: String,
    pub ticket: TicketDetail,
}

#[derive(Serialize)]
pub struct DeleteResponse {
    pub request_id: String,
    pub workspace: String,
    pub id: String,
}

// ── Mutation handlers ─────────────────────────────────────────────────────────

/// `POST /api/tickets?workspace=<name>`
///
/// Create a new ticket.  Returns `201 Created` with the new ticket detail.
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
            Ok(m) => m,
            Err(e) => return storage_err(e, &rid.0),
        };

        let created_at = store
            .get_indexed(&id)
            .ok()
            .flatten()
            .map(|t| t.created_at)
            .unwrap_or_else(chrono::Utc::now);

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
            Ok(m) => m,
            Err(e) => return storage_err(e, &rid.0),
        };

        let created_at = store
            .get_indexed(&id)
            .ok()
            .flatten()
            .map(|t| t.created_at)
            .unwrap_or_else(chrono::Utc::now);

        Json(MutationResponse {
            request_id: rid.0,
            workspace: params.workspace,
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at,
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
/// state.  `target_state` defaults to `"done"`.
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

        let created_at = store
            .get_indexed(&id)
            .ok()
            .flatten()
            .map(|t| t.created_at)
            .unwrap_or_else(chrono::Utc::now);

        Json(MutationResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at,
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
/// Transition a ticket to the `cancelled` state.  Optional `reason` field is
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
            Ok(m) => m,
            Err(e) => return storage_err(e, &rid.0),
        };

        let created_at = store
            .get_indexed(&id)
            .ok()
            .flatten()
            .map(|t| t.created_at)
            .unwrap_or_else(chrono::Utc::now);

        Json(MutationResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at,
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
/// 1-based `revision` number.  The revert is forward-only: a new history
/// entry is appended; no history is erased.
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
            Ok(r) => r,
            Err(e) => return storage_err(e, &rid.0),
        };

        let target_rev = match revisions.iter().find(|r| r.rev == revision) {
            Some(r) => r.clone(),
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
            Ok(_new_rev) => {
                let manifest = match store.get(&id) {
                    Ok(m) => m,
                    Err(e) => return storage_err(e, &rid.0),
                };
                let created_at = store
                    .get_indexed(&id)
                    .ok()
                    .flatten()
                    .map(|t| t.created_at)
                    .unwrap_or_else(chrono::Utc::now);
                Json(MutationResponse {
                    request_id: rid.0.clone(),
                    workspace: params.workspace.clone(),
                    ticket: TicketDetail {
                        id: manifest.id.to_string(),
                        created_at,
                        fields: manifest.extra,
                    },
                })
                .into_response()
            }
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
            Ok(r) => r,
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

        // Second-to-last revision — the state before the most recent change.
        let prev_fields = revisions[revisions.len() - 2].fields.clone();

        match store.apply_revert(&id, prev_fields, author.as_deref()) {
            Ok(_new_rev) => {
                let manifest = match store.get(&id) {
                    Ok(m) => m,
                    Err(e) => return storage_err(e, &rid.0),
                };
                let created_at = store
                    .get_indexed(&id)
                    .ok()
                    .flatten()
                    .map(|t| t.created_at)
                    .unwrap_or_else(chrono::Utc::now);
                Json(MutationResponse {
                    request_id: rid.0.clone(),
                    workspace: params.workspace.clone(),
                    ticket: TicketDetail {
                        id: manifest.id.to_string(),
                        created_at,
                        fields: manifest.extra,
                    },
                })
                .into_response()
            }
            Err(e) => storage_err(e, &rid.0),
        }
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `DELETE /api/tickets/{id}?workspace=<name>`
///
/// Soft-delete (mark deleted) a ticket.  Emits a `ticket.delete` SSE event.
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

// ── Ticket file listing / asset serving ──────────────────────────────────────

#[derive(Serialize)]
pub struct TicketFileEntry {
    /// Relative path within the ticket folder (e.g. `"description.md"` or
    /// `"assets/design/plan.md"`).
    pub path: String,
    /// Display name — just the file's stem+extension (e.g. `"plan.md"`).
    pub name: String,
}

#[derive(Serialize)]
pub struct TicketFilesResponse {
    pub request_id: String,
    pub workspace: String,
    pub id: String,
    pub files: Vec<TicketFileEntry>,
}

#[derive(Deserialize)]
pub struct TicketAssetParam {
    pub workspace: String,
    /// Relative path within the ticket folder, e.g. `"assets/plan.md"`.
    pub path: String,
}

#[derive(Serialize)]
pub struct TicketAssetResponse {
    pub request_id: String,
    pub workspace: String,
    pub id: String,
    pub path: String,
    pub content: String,
}

/// Recursively collect all files under `dir`, appending `TicketFileEntry`
/// items with paths relative to `ticket_dir`.
fn collect_ticket_files(
    dir: &std::path::Path,
    ticket_dir: &std::path::Path,
    files: &mut Vec<TicketFileEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut children: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .collect();
    children.sort();
    for child in children {
        if child.is_dir() {
            collect_ticket_files(&child, ticket_dir, files);
        } else if let Some(ext) = child.extension() {
            // Only expose Markdown files from the assets tree.
            if ext.eq_ignore_ascii_case("md") {
                if let Ok(rel) = child.strip_prefix(ticket_dir) {
                    let path_str = rel.to_string_lossy().replace('\\', "/");
                    let name = child
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    files.push(TicketFileEntry { path: path_str, name });
                }
            }
        }
    }
}

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
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || {
        let indexed = match store.get_indexed(&id) {
            Ok(Some(t)) => t,
            Ok(None) => {
                return viewer_api::error::ApiError::not_found("ticket", &rid.0)
                    .into_response_with_status(StatusCode::NOT_FOUND);
            }
            Err(e) => return storage_err(e, &rid.0),
        };

        if indexed.deleted {
            return viewer_api::error::ApiError::not_found("ticket", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }

        let ticket_dir = match indexed.path.parent() {
            Some(p) => p.to_path_buf(),
            None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        let mut files: Vec<TicketFileEntry> = Vec::new();

        // description.md always comes first.
        if ticket_dir.join("description.md").is_file() {
            files.push(TicketFileEntry {
                path: "description.md".to_string(),
                name: "description.md".to_string(),
            });
        }

        // All *.md files under assets/ (recursively, sorted).
        let assets_dir = ticket_dir.join("assets");
        if assets_dir.is_dir() {
            collect_ticket_files(&assets_dir, &ticket_dir, &mut files);
        }

        Json(TicketFilesResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
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
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    tokio::task::spawn_blocking(move || {
        let indexed = match store.get_indexed(&id) {
            Ok(Some(t)) => t,
            Ok(None) => {
                return viewer_api::error::ApiError::not_found("ticket", &rid.0)
                    .into_response_with_status(StatusCode::NOT_FOUND);
            }
            Err(e) => return storage_err(e, &rid.0),
        };

        if indexed.deleted {
            return viewer_api::error::ApiError::not_found("ticket", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }

        let ticket_dir = match indexed.path.parent() {
            Some(p) => p.to_path_buf(),
            None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        // Prevent path traversal: canonicalize both the ticket dir and the
        // requested file path, then assert the file is inside the ticket dir.
        let canonical_dir = match ticket_dir.canonicalize() {
            Ok(p) => p,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let requested = ticket_dir.join(&params.path);
        let canonical_file = match requested.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                return (StatusCode::NOT_FOUND, "file not found").into_response();
            }
        };
        if !canonical_file.starts_with(&canonical_dir) {
            return (StatusCode::FORBIDDEN, "access denied").into_response();
        }

        let content = match std::fs::read_to_string(&canonical_file) {
            Ok(s) => s,
            Err(_) => return (StatusCode::NOT_FOUND, "file not found").into_response(),
        };

        Json(TicketAssetResponse {
            request_id: rid.0.clone(),
            workspace: params.workspace.clone(),
            id: id.to_string(),
            path: params.path.clone(),
            content,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::{
        cancel_ticket, close_ticket, create_ticket, delete_ticket, list_tickets, revert_ticket,
        update_ticket, CancelTicketBody, CloseTicketBody, CreateTicketBody, MutationWorkspaceParam,
        RevertTicketBody, UpdateTicketBody, WorkspaceParam,
    };
    use axum::{
        Json,
        body::to_bytes,
        extract::{Extension, Path, Query, State},
        http::{HeaderMap, StatusCode},
    };
    use std::{collections::BTreeMap, sync::Arc};
    use uuid::Uuid;
    use viewer_api::error::RequestIdExt;

    use ticket_api::{
        model::filesystem::ScanRoot,
        storage::store::TicketStore,
    };
    use crate::serve::{AppState, StreamBroker, WorkspaceRegistry};

    fn make_store(dir: &std::path::Path) -> Arc<TicketStore> {
        let store = Arc::new(TicketStore::open(dir).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        store
    }

    fn make_state(store: Arc<TicketStore>) -> AppState {
        AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
            Arc::new(StreamBroker::new()),
        )
    }

    #[tokio::test]
    async fn search_list_uses_persisted_updated_at() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("search-updated-at regression"),
                Some("open"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create ticket");

        let expected_updated_at = store
            .get_indexed(&id)
            .expect("indexed get")
            .expect("indexed ticket exists")
            .updated_at;

        let state = make_state(Arc::clone(&store));

        let response = list_tickets(
            State(state),
            Extension(RequestIdExt("rid-test".to_string())),
            Query(WorkspaceParam {
                workspace: "default".to_string(),
                state: None,
                query: Some("search-updated-at".to_string()),
                limit: Some(10),
                cursor: None,
            }),
        )
        .await;

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json body");

        let got = payload["items"][0]["updated_at"]
            .as_str()
            .expect("updated_at string");
        let got = chrono::DateTime::parse_from_rfc3339(got)
            .expect("parse updated_at")
            .with_timezone(&chrono::Utc);

        assert_eq!(got, expected_updated_at);
    }

    #[tokio::test]
    async fn create_ticket_returns_201_with_new_ticket() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = make_state(make_store(dir.path()));

        let response = create_ticket(
            State(state),
            Extension(RequestIdExt("rid-create".to_string())),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            Json(CreateTicketBody {
                type_id: "tracker-improvement".to_string(),
                title: Some("My new ticket".to_string()),
                fields: None,
                description: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::CREATED);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

        assert_eq!(payload["workspace"], "default");
        assert_eq!(payload["request_id"], "rid-create");
        assert_eq!(payload["ticket"]["fields"]["title"], "My new ticket");
        assert_eq!(payload["ticket"]["fields"]["state"], "new");
    }

    #[tokio::test]
    async fn create_ticket_with_extra_fields_and_description() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = make_state(make_store(dir.path()));

        let mut fields = BTreeMap::new();
        fields.insert("priority".to_string(), serde_json::Value::String("high".to_string()));

        let response = create_ticket(
            State(state),
            Extension(RequestIdExt("rid".to_string())),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            Json(CreateTicketBody {
                type_id: "tracker-improvement".to_string(),
                title: Some("Ticket with fields".to_string()),
                fields: Some(fields),
                description: Some("## Overview\n\nSome description.".to_string()),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["ticket"]["fields"]["priority"], "high");
    }

    #[tokio::test]
    async fn update_ticket_patches_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        let id = store
            .create(None, "tracker-improvement", Some("Original"), None, BTreeMap::new(), None, None)
            .expect("create");

        let state = make_state(Arc::clone(&store));

        let mut patch = BTreeMap::new();
        patch.insert("title".to_string(), serde_json::Value::String("Updated title".to_string()));

        let response = update_ticket(
            State(state),
            Extension(RequestIdExt("rid-update".to_string())),
            Path(id),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            HeaderMap::new(),
            Json(UpdateTicketBody {
                fields: Some(patch),
                state: None,
                from_state: None,
                description: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["ticket"]["fields"]["title"], "Updated title");
    }

    #[tokio::test]
    async fn update_ticket_transitions_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        let id = store
            .create(None, "tracker-improvement", Some("T"), None, BTreeMap::new(), None, None)
            .expect("create");

        let state = make_state(Arc::clone(&store));

        let response = update_ticket(
            State(state),
            Extension(RequestIdExt("rid".to_string())),
            Path(id),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            HeaderMap::new(),
            Json(UpdateTicketBody {
                fields: None,
                state: Some("ready".to_string()),
                from_state: None,
                description: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["ticket"]["fields"]["state"], "ready");
    }

    #[tokio::test]
    async fn close_ticket_fast_forwards_to_done() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        let id = store
            .create(None, "tracker-improvement", Some("Close me"), None, BTreeMap::new(), None, None)
            .expect("create");

        let state = make_state(Arc::clone(&store));

        let response = close_ticket(
            State(state),
            Extension(RequestIdExt("rid-close".to_string())),
            Path(id),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            HeaderMap::new(),
            Json(CloseTicketBody { target_state: None }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["ticket"]["fields"]["state"], "done");
    }

    #[tokio::test]
    async fn revert_ticket_restores_historical_revision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        // Create ticket (revision 1: state="new")
        let id = store
            .create(None, "tracker-improvement", Some("Revert me"), None, BTreeMap::new(), None, None)
            .expect("create");

        // Advance to "ready" (revision 2)
        store
            .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
            .expect("update to ready");

        let state = make_state(Arc::clone(&store));

        // Revert back to revision 1 (state="new")
        let response = revert_ticket(
            State(state),
            Extension(RequestIdExt("rid-revert".to_string())),
            Path(id),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            HeaderMap::new(),
            Json(RevertTicketBody { revision: 1 }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["request_id"], "rid-revert");
        assert_eq!(payload["workspace"], "default");
        assert_eq!(payload["ticket"]["fields"]["state"], "new");
        assert_eq!(payload["ticket"]["fields"]["title"], "Revert me");
    }

    #[tokio::test]
    async fn revert_ticket_returns_400_for_unknown_revision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        let id = store
            .create(None, "tracker-improvement", Some("T"), None, BTreeMap::new(), None, None)
            .expect("create");

        let state = make_state(Arc::clone(&store));

        let response = revert_ticket(
            State(state),
            Extension(RequestIdExt("rid".to_string())),
            Path(id),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            HeaderMap::new(),
            Json(RevertTicketBody { revision: 999 }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["code"], "revision_not_found");
    }

    #[tokio::test]
    async fn cancel_ticket_transitions_to_cancelled_with_reason() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        let id = store
            .create(None, "tracker-improvement", Some("Cancel me"), None, BTreeMap::new(), None, None)
            .expect("create");

        let state = make_state(Arc::clone(&store));

        let response = cancel_ticket(
            State(state),
            Extension(RequestIdExt("rid-cancel".to_string())),
            Path(id),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
            HeaderMap::new(),
            Json(CancelTicketBody { reason: Some("No longer needed".to_string()) }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["ticket"]["fields"]["state"], "cancelled");
        assert_eq!(payload["ticket"]["fields"]["cancel_reason"], "No longer needed");
    }

    #[tokio::test]
    async fn delete_ticket_marks_as_deleted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = make_store(dir.path());

        let id = store
            .create(None, "tracker-improvement", Some("Delete me"), None, BTreeMap::new(), None, None)
            .expect("create");

        let state = make_state(Arc::clone(&store));

        let response = delete_ticket(
            State(state.clone()),
            Extension(RequestIdExt("rid-delete".to_string())),
            Path(id),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["id"], id.to_string());

        let indexed = store.get_indexed(&id).expect("indexed ok").expect("indexed exists");
        assert!(indexed.deleted, "ticket should be marked deleted");
    }

    #[tokio::test]
    async fn delete_nonexistent_ticket_returns_404_envelope() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = make_state(make_store(dir.path()));

        let response = delete_ticket(
            State(state),
            Extension(RequestIdExt("rid".to_string())),
            Path(Uuid::new_v4()),
            Query(MutationWorkspaceParam { workspace: "default".to_string() }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["code"], "not_found");
        assert!(payload.get("request_id").is_some());
    }
}

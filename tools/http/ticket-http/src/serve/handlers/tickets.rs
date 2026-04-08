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
        request_id: rid.0,
        workspace: params.workspace,
        items: tickets,
        next_cursor: None, // cursor pagination deferred to later iteration
    })
    .into_response()
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

    match store.get(&id) {
        Ok(manifest) => Json(TicketDetailResponse {
            request_id: rid.0,
            workspace: params.workspace,
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                created_at: manifest.created_at,
                fields: manifest.extra.into_iter().map(|(k, v)| (k, v)).collect(),
            },
        })
        .into_response(),
        Err(e) => storage_err(e, &rid.0),
    }
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
        request_id: rid.0,
        workspace: params.workspace,
        id: id.to_string(),
        description,
    })
    .into_response()
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

    match store.get_history(&id) {
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
                "request_id": rid.0,
                "workspace": params.workspace,
                "id": id.to_string(),
                "count": entries.len(),
                "entries": entries,
            }))
            .into_response()
        }
        Err(e) => storage_err(e, &rid.0),
    }
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

    let id = match store.create(
        None,
        &body.type_id,
        body.title.as_deref(),
        None,
        extra,
        None,
        body.description.as_deref(),
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
    let author = author_from_headers(&headers);

    let manifest = match store.update(
        &id,
        patch,
        body.from_state.as_deref(),
        body.state.as_deref(),
        body.description.as_deref(),
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

    let target = body.target_state.as_deref().unwrap_or("done");
    let author = author_from_headers(&headers);

    let (manifest, _path) = match store.close(&id, target, author.as_deref()) {
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
        request_id: rid.0,
        workspace: params.workspace,
        ticket: TicketDetail {
            id: manifest.id.to_string(),
            created_at,
            fields: manifest.extra,
        },
    })
    .into_response()
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
        request_id: rid.0,
        workspace: params.workspace,
        ticket: TicketDetail {
            id: manifest.id.to_string(),
            created_at,
            fields: manifest.extra,
        },
    })
    .into_response()
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

    match store.delete(&id) {
        Ok(()) => Json(DeleteResponse {
            request_id: rid.0,
            workspace: params.workspace,
            id: id.to_string(),
        })
        .into_response(),
        Err(e) => storage_err(e, &rid.0),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cancel_ticket, close_ticket, create_ticket, delete_ticket, list_tickets, update_ticket,
        CancelTicketBody, CloseTicketBody, CreateTicketBody, MutationWorkspaceParam,
        UpdateTicketBody, WorkspaceParam,
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
                state: Some("in-refinement".to_string()),
                from_state: None,
                description: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.expect("body");
        let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["ticket"]["fields"]["state"], "in-refinement");
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

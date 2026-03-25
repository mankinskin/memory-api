use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::SystemTime;
use uuid::Uuid;

use viewer_api::error::RequestIdExt;
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

#[cfg(test)]
mod tests {
    use super::{list_tickets, WorkspaceParam};
    use axum::{
        body::to_bytes,
        extract::{Extension, Query, State},
    };
    use std::{collections::BTreeMap, sync::Arc};
    use viewer_api::error::RequestIdExt;

    use ticket_api::{
        model::filesystem::ScanRoot,
        storage::store::TicketStore,
    };
    use crate::serve::{AppState, StreamBroker, WorkspaceRegistry};

    #[tokio::test]
    async fn search_list_uses_persisted_updated_at() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.path().join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");

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

        let state = AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
            Arc::new(StreamBroker::new()),
        );

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
}

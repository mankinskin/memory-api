use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};

use viewer_api::error::RequestIdExt;
use crate::serve::{error::storage_err, AppState};

#[derive(Deserialize)]
pub struct EdgesQuery {
    pub workspace: String,
    pub kind: Option<String>,
}

#[derive(Serialize)]
pub struct EdgeItem {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct EdgesResponse {
    pub request_id: String,
    pub workspace: String,
    pub items: Vec<EdgeItem>,
}

pub async fn list_edges(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<EdgesQuery>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    match store.list_all_edges() {
        Ok(edges) => {
            let items: Vec<EdgeItem> = edges
                .into_iter()
                .filter(|e| {
                    if let Some(k) = &params.kind {
                        k == "all" || &e.kind == k
                    } else {
                        true
                    }
                })
                .map(|e| EdgeItem {
                    from: e.from.to_string(),
                    to: e.to.to_string(),
                    kind: e.kind,
                })
                .collect();

            Json(EdgesResponse {
                request_id: rid.0,
                workspace: params.workspace,
                items,
            })
            .into_response()
        }
        Err(e) => storage_err(e, &rid.0),
    }
}

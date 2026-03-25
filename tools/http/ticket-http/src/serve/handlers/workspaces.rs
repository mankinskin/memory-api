use axum::{
    extract::{Extension, State},
    response::Json,
};
use serde::Serialize;

use viewer_api::error::RequestIdExt;
use crate::serve::AppState;

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub name: String,
}

#[derive(Serialize)]
pub struct WorkspacesResponse {
    pub request_id: String,
    pub workspaces: Vec<WorkspaceInfo>,
}

pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
) -> Json<WorkspacesResponse> {
    let workspaces = state
        .registry
        .workspace_names()
        .into_iter()
        .map(|name| WorkspaceInfo { name })
        .collect();

    Json(WorkspacesResponse {
        request_id: rid.0,
        workspaces,
    })
}

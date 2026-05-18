use axum::{
    extract::{
        Extension,
        State,
    },
    response::Json,
};
use serde::Serialize;

use crate::serve::AppState;
use viewer_api::error::RequestIdExt;

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub name: String,
}

#[derive(Serialize)]
pub struct WorkspacesResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspaces: Vec<WorkspaceInfo>,
}

fn preferred_active_workspace(workspace_names: &[String]) -> String {
    workspace_names
        .iter()
        .find(|name| name.as_str() == "default")
        .cloned()
        .or_else(|| workspace_names.first().cloned())
        .unwrap_or_else(|| "default".to_string())
}

pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
) -> Json<WorkspacesResponse> {
    let workspace_names = state.registry.workspace_names();
    let active_workspace = preferred_active_workspace(&workspace_names);
    let workspaces = workspace_names
        .into_iter()
        .map(|name| WorkspaceInfo { name })
        .collect();

    Json(WorkspacesResponse {
        request_id: rid.0,
        active_workspace,
        workspaces,
    })
}

#[cfg(test)]
mod tests {
    use super::preferred_active_workspace;

    #[test]
    fn preferred_active_workspace_prefers_default() {
        let workspaces = vec!["child".to_string(), "default".to_string()];
        assert_eq!(preferred_active_workspace(&workspaces), "default");
    }

    #[test]
    fn preferred_active_workspace_falls_back_to_first() {
        let workspaces = vec!["child".to_string(), "../..".to_string()];
        assert_eq!(preferred_active_workspace(&workspaces), "child");
    }
}

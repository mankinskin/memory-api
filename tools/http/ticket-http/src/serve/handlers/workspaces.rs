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

fn preferred_active_workspace(
    primary_workspace: &str,
    workspace_names: &[String],
) -> String {
    if workspace_names
        .iter()
        .any(|name| name == primary_workspace)
    {
        return primary_workspace.to_string();
    }

    workspace_names
        .first()
        .cloned()
        .unwrap_or_else(|| primary_workspace.to_string())
}

pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
) -> Json<WorkspacesResponse> {
    let workspace_names = state.registry.workspace_names();
    let active_workspace = preferred_active_workspace(
        state.registry.primary_workspace_name(),
        &workspace_names,
    );
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
    use std::sync::Arc;

    use axum::extract::{
        Extension,
        State,
    };
    use ticket_api::storage::store::TicketStore;
    use viewer_api::error::RequestIdExt;

    use super::*;
    use crate::serve::{
        StreamBroker,
        WorkspaceRegistry,
    };

    #[test]
    fn preferred_active_workspace_prefers_primary_workspace() {
        let workspaces = vec!["child".to_string(), "context-engine".to_string()];
        assert_eq!(
            preferred_active_workspace("context-engine", &workspaces),
            "context-engine"
        );
    }

    #[test]
    fn preferred_active_workspace_falls_back_to_first() {
        let workspaces = vec!["child".to_string(), "memory-api".to_string()];
        assert_eq!(
            preferred_active_workspace("context-engine", &workspaces),
            "child"
        );
    }

    #[tokio::test]
    async fn list_workspaces_uses_concrete_folder_names_for_child_and_ancestor() {
        let root = tempfile::tempdir().expect("tempdir");
        let child_dir = root.path().join("child");
        std::fs::create_dir_all(&child_dir).expect("create child dir");

        let _parent_store = TicketStore::init(root.path()).expect("open parent store");
        let child_store = Arc::new(
            TicketStore::init(&child_dir).expect("open child store"),
        );

        let state = AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&child_store))),
            Arc::new(StreamBroker::new()),
        );

        let response = list_workspaces(
            State(state),
            Extension(RequestIdExt("rid-workspaces".to_string())),
        )
        .await;
        let payload = response.0;
        let workspace_names = payload
            .workspaces
            .iter()
            .map(|workspace| workspace.name.as_str())
            .collect::<Vec<_>>();
        let parent_name = root
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("parent workspace name");

        assert_eq!(payload.active_workspace, "child");
        assert!(workspace_names.contains(&"child"));
        assert!(workspace_names.contains(&parent_name));
        assert!(!workspace_names.contains(&"default"));
        assert!(!workspace_names.contains(&".."));
        assert!(!workspace_names.contains(&"../.."));
    }
}

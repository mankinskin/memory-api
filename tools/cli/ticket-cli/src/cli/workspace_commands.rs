use std::path::PathBuf;

use serde_json::{
    Value,
    json,
};

use ticket_api::storage::TicketStore;
use ticket_api::workspace;

use super::{
    WorkspaceArgs,
    WorkspaceInitArgs,
    WorkspaceSubCommand,
};

pub(super) fn workspace_command_mutates(command: &WorkspaceSubCommand) -> bool {
    matches!(command, WorkspaceSubCommand::Init(_))
}

pub(super) fn cmd_workspace(args: WorkspaceArgs) -> Value {
    match args.command {
        WorkspaceSubCommand::Init(args) => cmd_workspace_init(args),
        WorkspaceSubCommand::Current => cmd_workspace_current(),
    }
}

fn cmd_workspace_init(args: WorkspaceInitArgs) -> Value {
    let path = args.path.unwrap_or_else(default_workspace_path);
    if let Err(error) = TicketStore::open(&path) {
        return error_response("workspace_init", error.to_string());
    }

    json!({
        "command": "workspace_init",
        "status": "ok",
        "path": path.to_string_lossy(),
    })
}

fn cmd_workspace_current() -> Value {
    let (path, source) = workspace::resolve_workspace();
    json!({
        "command": "workspace_current",
        "status": "ok",
        "path": path.to_string_lossy(),
        "source": source.description(),
    })
}

fn default_workspace_path() -> PathBuf {
    workspace::resolve_workspace().0
}

fn error_response(
    command: &str,
    message: impl Into<String>,
) -> Value {
    json!({
        "command": command,
        "status": "error",
        "message": message.into(),
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn workspace_init_creates_default_local_ticket_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let path = repo.join(".ticket");

        let response = cmd_workspace(WorkspaceArgs {
            command: WorkspaceSubCommand::Init(WorkspaceInitArgs {
                path: Some(path.clone()),
            }),
        });

        assert_eq!(response["status"], "ok");
        assert!(path.join(".gitignore").is_file());
    }
}

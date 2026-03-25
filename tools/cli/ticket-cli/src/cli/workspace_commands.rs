use std::path::PathBuf;

use serde_json::{Value, json};

use ticket_api::workspace::{self, WorkspaceConfig};

use super::{WorkspaceArgs, WorkspaceSubCommand};

pub(super) fn workspace_command_mutates(command: &WorkspaceSubCommand) -> bool {
    matches!(
        command,
        WorkspaceSubCommand::New(_) | WorkspaceSubCommand::Use(_) | WorkspaceSubCommand::Remove(_)
    )
}

pub(super) fn cmd_workspace(args: WorkspaceArgs) -> Value {
    match args.command {
        WorkspaceSubCommand::List => {
            let config = WorkspaceConfig::load();
            let active = config.active.as_deref().unwrap_or("");
            let workspaces: Vec<Value> = config
                .workspaces
                .iter()
                .map(|(name, path)| {
                    json!({
                        "name": name,
                        "path": path,
                        "active": name == active,
                    })
                })
                .collect();
            json!({
                "command": "workspace_list",
                "status": "ok",
                "active": if active.is_empty() { Value::Null } else { Value::String(active.to_string()) },
                "workspaces": workspaces,
            })
        }
        WorkspaceSubCommand::New(args) => {
            let path = args.path.unwrap_or_else(|| {
                // Default: .ticket/ inside the current directory (repo-local)
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".ticket")
            });
            let mut config = WorkspaceConfig::load();
            match config.add(&args.name, path.clone()) {
                Err(e) => json!({ "command": "workspace_new", "status": "error", "message": e }),
                Ok(()) => {
                    if let Err(e) = config.save() {
                        return json!({ "command": "workspace_new", "status": "error", "message": e.to_string() });
                    }
                    json!({
                        "command": "workspace_new",
                        "status": "ok",
                        "name": args.name,
                        "path": path.to_string_lossy(),
                    })
                }
            }
        }
        WorkspaceSubCommand::Use(use_args) => {
            if use_args.local {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let local_path = cwd.join(ticket_api::workspace::LOCAL_WORKSPACE_FILE);

                // Resolve the index path: registry lookup first, then treat name as path.
                let config = WorkspaceConfig::load();
                let index_path = config
                    .workspaces
                    .get(&use_args.name)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(&use_args.name));

                // Write a repo-relative path so the file is self-contained
                // (no dependency on the user-level workspace registry).
                let rel = ticket_api::workspace::make_relative_path(&cwd, &index_path);
                let content = rel.to_string_lossy().replace('\\', "/");

                match std::fs::write(&local_path, &content) {
                    Err(e) => json!({ "command": "workspace_use", "status": "error", "message": e.to_string() }),
                    Ok(()) => json!({
                        "command": "workspace_use",
                        "status": "ok",
                        "name": use_args.name,
                        "scope": "local",
                        "path": content,
                        "file": local_path.to_string_lossy(),
                    }),
                }
            } else {
                let mut config = WorkspaceConfig::load();
                match config.set_active(&use_args.name) {
                    Err(e) => json!({ "command": "workspace_use", "status": "error", "message": e }),
                    Ok(()) => {
                        if let Err(e) = config.save() {
                            return json!({ "command": "workspace_use", "status": "error", "message": e.to_string() });
                        }
                        json!({
                            "command": "workspace_use",
                            "status": "ok",
                            "name": use_args.name,
                            "scope": "global",
                        })
                    }
                }
            }
        }
        WorkspaceSubCommand::Current => {
            // Reproduce the full resolution chain with source annotation.
            let (path, source) = workspace::resolve_workspace();
            json!({
                "command": "workspace_current",
                "status": "ok",
                "path": path.to_string_lossy(),
                "source": source.description(),
            })
        }
        WorkspaceSubCommand::Remove(args) => {
            let mut config = WorkspaceConfig::load();
            match config.remove(&args.name) {
                Err(e) => json!({ "command": "workspace_remove", "status": "error", "message": e }),
                Ok(()) => {
                    if let Err(e) = config.save() {
                        return json!({ "command": "workspace_remove", "status": "error", "message": e.to_string() });
                    }
                    json!({
                        "command": "workspace_remove",
                        "status": "ok",
                        "name": args.name,
                    })
                }
            }
        }
    }
}

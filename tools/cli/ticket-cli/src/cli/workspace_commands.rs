use std::path::PathBuf;

use serde_json::{
    Value,
    json,
};

use ticket_api::workspace::{
    self,
    WorkspaceConfig,
};

use super::{
    WorkspaceArgs,
    WorkspaceNewArgs,
    WorkspaceRemoveArgs,
    WorkspaceSubCommand,
    WorkspaceUseArgs,
};

pub(super) fn workspace_command_mutates(command: &WorkspaceSubCommand) -> bool {
    matches!(
        command,
        WorkspaceSubCommand::New(_)
            | WorkspaceSubCommand::Use(_)
            | WorkspaceSubCommand::Remove(_)
    )
}

pub(super) fn cmd_workspace(args: WorkspaceArgs) -> Value {
    match args.command {
        WorkspaceSubCommand::List => cmd_workspace_list(),
        WorkspaceSubCommand::New(args) => cmd_workspace_new(args),
        WorkspaceSubCommand::Use(args) => cmd_workspace_use(args),
        WorkspaceSubCommand::Current => cmd_workspace_current(),
        WorkspaceSubCommand::Remove(args) => cmd_workspace_remove(args),
    }
}

fn cmd_workspace_list() -> Value {
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
        "active": active_value(active),
        "workspaces": workspaces,
    })
}

fn cmd_workspace_new(args: WorkspaceNewArgs) -> Value {
    let path = args.path.unwrap_or_else(default_workspace_path);
    let mut config = WorkspaceConfig::load();
    match config.add(&args.name, path.clone()) {
        Err(error) => error_response("workspace_new", error),
        Ok(()) => match save_config(&mut config, "workspace_new") {
            Some(response) => response,
            None => json!({
                "command": "workspace_new",
                "status": "ok",
                "name": args.name,
                "path": path.to_string_lossy(),
            }),
        },
    }
}

fn cmd_workspace_use(args: WorkspaceUseArgs) -> Value {
    if args.local {
        cmd_workspace_use_local(args)
    } else {
        cmd_workspace_use_global(args)
    }
}

fn cmd_workspace_use_local(args: WorkspaceUseArgs) -> Value {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let local_path = cwd.join(ticket_api::workspace::LOCAL_WORKSPACE_FILE);
    let index_path = resolve_workspace_path(&args.name);
    let rel = ticket_api::workspace::make_relative_path(&cwd, &index_path);
    let content = rel.to_string_lossy().replace('\\', "/");

    match std::fs::write(&local_path, &content) {
        Err(error) => error_response("workspace_use", error.to_string()),
        Ok(()) => json!({
            "command": "workspace_use",
            "status": "ok",
            "name": args.name,
            "scope": "local",
            "path": content,
            "file": local_path.to_string_lossy(),
        }),
    }
}

fn cmd_workspace_use_global(args: WorkspaceUseArgs) -> Value {
    let mut config = WorkspaceConfig::load();
    match config.set_active(&args.name) {
        Err(error) => error_response("workspace_use", error),
        Ok(()) => match save_config(&mut config, "workspace_use") {
            Some(response) => response,
            None => json!({
                "command": "workspace_use",
                "status": "ok",
                "name": args.name,
                "scope": "global",
            }),
        },
    }
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

fn cmd_workspace_remove(args: WorkspaceRemoveArgs) -> Value {
    let mut config = WorkspaceConfig::load();
    match config.remove(&args.name) {
        Err(error) => error_response("workspace_remove", error),
        Ok(()) => match save_config(&mut config, "workspace_remove") {
            Some(response) => response,
            None => json!({
                "command": "workspace_remove",
                "status": "ok",
                "name": args.name,
            }),
        },
    }
}

fn default_workspace_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".ticket")
}

fn resolve_workspace_path(name: &str) -> PathBuf {
    WorkspaceConfig::load()
        .workspaces
        .get(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(name))
}

fn save_config(
    config: &mut WorkspaceConfig,
    command: &str,
) -> Option<Value> {
    config
        .save()
        .err()
        .map(|error| error_response(command, error.to_string()))
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

fn active_value(active: &str) -> Value {
    if active.is_empty() {
        Value::Null
    } else {
        Value::String(active.to_string())
    }
}

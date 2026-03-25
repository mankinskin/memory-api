use std::path::Path;
use std::path::PathBuf;

use serde_json::{Value, json};

use ticket_api::contracts::command_schema::{
    export_command_schema, export_command_schema_json,
};
use ticket_api::model::schema_registry::SchemaRegistry;
use ticket_api::storage::TicketStore;

use super::{
    CliRunError, TicketCommandCli, commands, batch,
    workspace_commands::{cmd_workspace, workspace_command_mutates},
};

pub(super) fn dispatch(
    command: TicketCommandCli,
    index_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
    _as_json: bool,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    // Commands that don't need storage.
    match &command {
        TicketCommandCli::ExportCommandSchema => {
            let schema_json = export_command_schema_json()?;
            let schema: Value = serde_json::from_str(&schema_json)?;
            return Ok(json!({
                "command": "export_command_schema",
                "status": "ok",
                "schema": schema,
                "known_commands": export_command_schema().commands,
            }));
        }
        TicketCommandCli::Workspace(_) => {}
        _ => {}
    }
    if let TicketCommandCli::Workspace(args) = command {
        if dry_run && workspace_command_mutates(&args.command) {
            return Ok(dry_run_payload("workspace", "workspace config writes"));
        }
        return Ok(cmd_workspace(args));
    }

    if dry_run {
        if let Some(payload) = dry_run_command_payload(&command) {
            return Ok(payload);
        }
    }

    // All other commands need the store.
    let index_root = resolve_index_root(index_root_override);
    let mut registry = SchemaRegistry::with_builtins();
    if let Some(schema_dir) = schema_dir_override {
        registry.load_dir(schema_dir)?;
    }
    let store = TicketStore::open_with(&index_root, registry)?;

    match command {
        TicketCommandCli::Create(args) => commands::cmd_create(args, &store),
        TicketCommandCli::Get(args) => commands::cmd_get(args, &store),
        TicketCommandCli::Describe(args) => commands::cmd_describe(args, &store),
        TicketCommandCli::Update(args) => commands::cmd_update(args, &store),
        TicketCommandCli::Repro(args) => commands::cmd_repro(args, &store),
        TicketCommandCli::List(args) => commands::cmd_list(args, &store),
        TicketCommandCli::Delete(args) => commands::cmd_delete(args, &store),
        TicketCommandCli::Scan(args) => commands::cmd_scan(args, &store),
        TicketCommandCli::Claim(args) => commands::cmd_claim(args, &store),
        TicketCommandCli::Unclaim(args) => commands::cmd_unclaim(args, &store),
        TicketCommandCli::Leases => commands::cmd_leases(&store),
        TicketCommandCli::Search(args) => commands::cmd_search(args, &store),
        TicketCommandCli::Query(args) => commands::cmd_search(args, &store),
        TicketCommandCli::AddRoot(args) => commands::cmd_add_root(args, &store),
        TicketCommandCli::Batch(args) => batch::cmd_batch(args, &store),
        TicketCommandCli::History(args) => commands::cmd_history(args, &store),
        TicketCommandCli::Diff(args) => commands::cmd_diff(args, &store),
        TicketCommandCli::Revert(args) => commands::cmd_revert(args, &store),
        TicketCommandCli::FinalizeMerge(args) => {
            let id = commands::resolve_uuid_prefix(&args.id, &store)?;
            Ok(json!({
                "command": "finalize_merge",
                "status": "phase2_stub",
                "id": id,
                "merge_commit": args.merge_commit
            }))
        },
        TicketCommandCli::Link(args) => commands::cmd_link(args, &store),
        TicketCommandCli::Unlink(args) => commands::cmd_unlink(args, &store),
        TicketCommandCli::Links(args) => commands::cmd_links(args, &store),
        TicketCommandCli::Subgraph(args) => commands::cmd_subgraph(args, &store),
        TicketCommandCli::Topgraph(args) => commands::cmd_topgraph(args, &store),
        TicketCommandCli::Watch(args) => commands::cmd_watch(args, &store),
        TicketCommandCli::Status(args) => commands::cmd_status(args, &store),
        TicketCommandCli::ReadyOverview(args) => commands::cmd_ready_overview(args, &store),
        TicketCommandCli::Serve(args) => commands::cmd_serve(args, store),
        TicketCommandCli::Close(args) => commands::cmd_close(args, &store),
        TicketCommandCli::Cancel(args) => commands::cmd_cancel(args, &store),
        TicketCommandCli::Attach(args) => commands::cmd_attach(args, &store),
        TicketCommandCli::Assets(args) => commands::cmd_assets(args, &store),
        TicketCommandCli::Health(args) => commands::cmd_health(args, &store),
        TicketCommandCli::Audit => commands::cmd_audit(&store),
        TicketCommandCli::ExportCommandSchema => unreachable!("handled above"),
        TicketCommandCli::Workspace(_) => unreachable!("handled above"),
    }
}

fn dry_run_command_payload(command: &TicketCommandCli) -> Option<Value> {
    match command {
        TicketCommandCli::Create(_) => Some(dry_run_payload("create", "create ticket")),
        TicketCommandCli::Update(_) => Some(dry_run_payload("update", "update ticket")),
        TicketCommandCli::Repro(_) => Some(dry_run_payload("repro", "record repro metadata")),
        TicketCommandCli::Delete(_) => Some(dry_run_payload("delete", "soft-delete ticket")),
        TicketCommandCli::Scan(_) => Some(dry_run_payload("scan", "scan/reindex ticket roots")),
        TicketCommandCli::Claim(_) => Some(dry_run_payload("claim", "claim ticket lease")),
        TicketCommandCli::Unclaim(_) => Some(dry_run_payload("unclaim", "release ticket lease")),
        TicketCommandCli::AddRoot(_) => Some(dry_run_payload("add_root", "register scan root")),
        TicketCommandCli::Batch(_) => Some(dry_run_payload("batch", "execute CLI batch commands")),
        TicketCommandCli::Revert(_) => Some(dry_run_payload("revert", "apply historical snapshot")),
        TicketCommandCli::FinalizeMerge(_) => {
            Some(dry_run_payload("finalize_merge", "record merge metadata"))
        }
        TicketCommandCli::Link(_) => Some(dry_run_payload("link", "add directed edge")),
        TicketCommandCli::Unlink(_) => Some(dry_run_payload("unlink", "remove directed edge")),
        TicketCommandCli::Watch(_) => Some(dry_run_payload("watch", "start watcher/reconcile loop")),
        TicketCommandCli::Serve(_) => Some(dry_run_payload("serve", "start HTTP server")),
        TicketCommandCli::Close(_) => Some(dry_run_payload("close", "fast-forward ticket state")),
        TicketCommandCli::Cancel(_) => {
            Some(dry_run_payload("cancel", "cancel ticket via state transition"))
        }
        TicketCommandCli::Attach(_) => Some(dry_run_payload("attach", "attach asset to ticket")),
        TicketCommandCli::Workspace(_) => Some(dry_run_payload("workspace", "workspace config writes")),
        TicketCommandCli::Get(_)
        | TicketCommandCli::List(_)
        | TicketCommandCli::Leases
        | TicketCommandCli::Search(_)
        | TicketCommandCli::Query(_)
        | TicketCommandCli::History(_)
        | TicketCommandCli::Diff(_)
        | TicketCommandCli::Describe(_)
        | TicketCommandCli::Links(_)
        | TicketCommandCli::Subgraph(_)
        | TicketCommandCli::Topgraph(_)
        | TicketCommandCli::Status(_)
        | TicketCommandCli::ReadyOverview(_)
        | TicketCommandCli::Assets(_)
        | TicketCommandCli::Health(_)
        | TicketCommandCli::Audit
        | TicketCommandCli::ExportCommandSchema => None,
    }
}

fn dry_run_payload(command: &str, action: &str) -> Value {
    json!({
        "command": command,
        "status": "ok",
        "dry_run": true,
        "would_execute": action,
    })
}

fn resolve_index_root(override_path: Option<&Path>) -> PathBuf {
    // Layer 1: explicit --index-root flag
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    // Layer 1b: TICKET_INDEX_ROOT env var
    if let Ok(env_val) = std::env::var("TICKET_INDEX_ROOT") {
        return PathBuf::from(env_val);
    }
    // Layers 2-4: workspace resolution chain (.ticket-workspace -> active workspace -> default)
    let (path, _source) = ticket_api::workspace::resolve_workspace();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::IdArgs;
    use uuid::Uuid;

    #[test]
    fn dry_run_payload_is_returned_for_mutating_command() {
        let payload = dry_run_command_payload(&TicketCommandCli::Delete(IdArgs {
            id: Uuid::new_v4().to_string(),
        }))
        .expect("delete should be dry-runnable");
        assert_eq!(payload["dry_run"], json!(true));
        assert_eq!(payload["command"], json!("delete"));
    }

    #[test]
    fn dry_run_payload_is_none_for_read_only_command() {
        let payload = dry_run_command_payload(&TicketCommandCli::Leases);
        assert!(payload.is_none());
    }
}

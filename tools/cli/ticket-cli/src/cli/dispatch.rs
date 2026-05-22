use std::path::{
    Path,
    PathBuf,
};

use serde_json::{
    Value,
    json,
};

use ticket_api::{
    contracts::command_schema::{
        export_command_schema,
        export_command_schema_json,
    },
    model::schema_registry::SchemaRegistry,
    storage::TicketStore,
};

use super::{
    CliRunError,
    TicketCommandCli,
    batch,
    commands,
};

pub(super) fn dispatch(
    command: TicketCommandCli,
    index_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
    _as_json: bool,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::ExportCommandSchema =>
            export_command_schema_payload(),
        TicketCommandCli::Init =>
            cmd_init(index_root_override, schema_dir_override),
        other => dispatch_store_backed(
            other,
            index_root_override,
            schema_dir_override,
            dry_run,
        ),
    }
}

fn cmd_init(
    index_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
) -> Result<Value, CliRunError> {
    let index_root = resolve_index_root(index_root_override);
    let mut registry = SchemaRegistry::with_builtins();
    if let Some(schema_dir) = schema_dir_override {
        registry.load_dir(schema_dir)?;
    }
    let store = TicketStore::init_with(&index_root, registry)?;
    Ok(json!({
        "command": "init",
        "status": "ok",
        "workspace": store.index_root.display().to_string(),
        "message": "workspace initialized",
    }))
}

fn dry_run_command_payload(command: &TicketCommandCli) -> Option<Value> {
    dry_run_payload_core(command)
        .or_else(|| dry_run_payload_history(command))
        .or_else(|| dry_run_payload_runtime(command))
}

fn dry_run_payload_core(command: &TicketCommandCli) -> Option<Value> {
    match command {
        TicketCommandCli::Init =>
            Some(dry_run_payload("init", "initialize ticket workspace")),
        TicketCommandCli::Create(_) =>
            Some(dry_run_payload("create", "create ticket")),
        TicketCommandCli::Update(_) =>
            Some(dry_run_payload("update", "update ticket")),
        TicketCommandCli::Repro(_) =>
            Some(dry_run_payload("repro", "record repro metadata")),
        TicketCommandCli::Delete(_) =>
            Some(dry_run_payload("delete", "soft-delete ticket")),
        TicketCommandCli::Scan(_) =>
            Some(dry_run_payload("scan", "scan/reindex ticket roots")),
        TicketCommandCli::Claim(_) =>
            Some(dry_run_payload("claim", "claim ticket lease")),
        TicketCommandCli::Unclaim(_) =>
            Some(dry_run_payload("unclaim", "release ticket lease")),
        TicketCommandCli::AddRoot(_) =>
            Some(dry_run_payload("add_root", "register scan root")),
        TicketCommandCli::Batch(_) =>
            Some(dry_run_payload("batch", "execute CLI batch commands")),
        _ => None,
    }
}

fn dry_run_payload_history(command: &TicketCommandCli) -> Option<Value> {
    match command {
        TicketCommandCli::Revert(_) =>
            Some(dry_run_payload("revert", "apply historical snapshot")),
        TicketCommandCli::FinalizeMerge(_) =>
            Some(dry_run_payload("finalize_merge", "record merge metadata")),
        TicketCommandCli::Link(_) =>
            Some(dry_run_payload("link", "add directed edge")),
        TicketCommandCli::Unlink(_) =>
            Some(dry_run_payload("unlink", "remove directed edge")),
        TicketCommandCli::Close(_) =>
            Some(dry_run_payload("close", "fast-forward ticket state")),
        TicketCommandCli::Cancel(_) => Some(dry_run_payload(
            "cancel",
            "cancel ticket via state transition",
        )),
        TicketCommandCli::Attach(_) =>
            Some(dry_run_payload("attach", "attach asset to ticket")),
        _ => None,
    }
}

fn dry_run_payload_runtime(command: &TicketCommandCli) -> Option<Value> {
    match command {
        TicketCommandCli::Watch(_) =>
            Some(dry_run_payload("watch", "start watcher/reconcile loop")),
        TicketCommandCli::Serve(_) =>
            Some(dry_run_payload("serve", "start HTTP server")),
        TicketCommandCli::Fmt(_) =>
            Some(dry_run_payload("fmt", "reformat ticket.toml files")),
        TicketCommandCli::Board(_) =>
            Some(dry_run_payload("board", "board state mutation")),
        _ => None,
    }
}

fn dry_run_payload(
    command: &str,
    action: &str,
) -> Value {
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
    // Local discovery: nearest .ticket/ walking upward, else ./ .ticket
    let (path, _source) = ticket_api::workspace::resolve_workspace();
    path
}

fn export_command_schema_payload() -> Result<Value, CliRunError> {
    let schema_json = export_command_schema_json()?;
    let schema: Value = serde_json::from_str(&schema_json)?;
    Ok(json!({
        "command": "export_command_schema",
        "status": "ok",
        "schema": schema,
        "known_commands": export_command_schema().commands,
    }))
}

fn dispatch_store_backed(
    command: TicketCommandCli,
    index_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    if dry_run {
        if let Some(payload) = dry_run_command_payload(&command) {
            return Ok(payload);
        }
    }

    let store = open_store(index_root_override, schema_dir_override)?;
    dispatch_store_command(command, store)
}

fn open_store(
    index_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
) -> Result<TicketStore, CliRunError> {
    let index_root = resolve_index_root(index_root_override);
    let mut registry = SchemaRegistry::with_builtins();
    if let Some(schema_dir) = schema_dir_override {
        registry.load_dir(schema_dir)?;
    }
    TicketStore::open_with(&index_root, registry).map_err(CliRunError::from)
}

fn dispatch_store_command(
    command: TicketCommandCli,
    store: TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Create(_)
        | TicketCommandCli::Get(_)
        | TicketCommandCli::Describe(_)
        | TicketCommandCli::Update(_)
        | TicketCommandCli::Repro(_)
        | TicketCommandCli::List(_)
        | TicketCommandCli::Delete(_)
        | TicketCommandCli::Scan(_)
        | TicketCommandCli::Claim(_)
        | TicketCommandCli::Unclaim(_) =>
            dispatch_store_command_core(command, &store),
        TicketCommandCli::Leases
        | TicketCommandCli::Search(_)
        | TicketCommandCli::Query(_)
        | TicketCommandCli::AddRoot(_)
        | TicketCommandCli::Batch(_)
        | TicketCommandCli::History(_)
        | TicketCommandCli::Diff(_)
        | TicketCommandCli::Revert(_)
        | TicketCommandCli::FinalizeMerge(_) =>
            dispatch_store_command_history(command, &store),
        TicketCommandCli::Link(_)
        | TicketCommandCli::Unlink(_)
        | TicketCommandCli::Links(_)
        | TicketCommandCli::Subgraph(_)
        | TicketCommandCli::Topgraph(_)
        | TicketCommandCli::Watch(_)
        | TicketCommandCli::Status(_)
        | TicketCommandCli::ReadyOverview(_)
        | TicketCommandCli::Next(_)
        | TicketCommandCli::UnblockedBy(_) =>
            dispatch_store_command_graph(command, &store),
        TicketCommandCli::Serve(_)
        | TicketCommandCli::Close(_)
        | TicketCommandCli::Cancel(_)
        | TicketCommandCli::Attach(_)
        | TicketCommandCli::Assets(_)
        | TicketCommandCli::Health(_)
        | TicketCommandCli::Audit
        | TicketCommandCli::Fmt(_)
        | TicketCommandCli::Board(_) =>
            dispatch_store_command_ops(command, store),
        TicketCommandCli::ExportCommandSchema | TicketCommandCli::Init => {
            unreachable!("handled before store dispatch")
        },
    }
}

fn dispatch_store_command_core(
    command: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Create(args) => commands::cmd_create(args, store),
        TicketCommandCli::Get(args) => commands::cmd_get(args, store),
        TicketCommandCli::Describe(args) => commands::cmd_describe(args, store),
        TicketCommandCli::Update(args) => commands::cmd_update(args, store),
        TicketCommandCli::Repro(args) => commands::cmd_repro(args, store),
        TicketCommandCli::List(args) => commands::cmd_list(args, store),
        TicketCommandCli::Delete(args) => commands::cmd_delete(args, store),
        TicketCommandCli::Scan(args) => commands::cmd_scan(args, store),
        TicketCommandCli::Claim(args) => commands::cmd_claim(args, store),
        TicketCommandCli::Unclaim(args) => commands::cmd_unclaim(args, store),
        _ => unreachable!("handled in core store dispatch"),
    }
}

fn dispatch_store_command_history(
    command: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Leases => commands::cmd_leases(store),
        TicketCommandCli::Search(args) => commands::cmd_search(args, store),
        TicketCommandCli::Query(args) => commands::cmd_search(args, store),
        TicketCommandCli::AddRoot(args) => commands::cmd_add_root(args, store),
        TicketCommandCli::Batch(args) => batch::cmd_batch(args, store),
        TicketCommandCli::History(args) => commands::cmd_history(args, store),
        TicketCommandCli::Diff(args) => commands::cmd_diff(args, store),
        TicketCommandCli::Revert(args) => commands::cmd_revert(args, store),
        TicketCommandCli::FinalizeMerge(args) => {
            let id = commands::resolve_uuid_prefix(&args.id, store)?;
            Ok(json!({
                "command": "finalize_merge",
                "status": "phase2_stub",
                "id": id,
                "merge_commit": args.merge_commit
            }))
        },
        _ => unreachable!("handled in history store dispatch"),
    }
}

fn dispatch_store_command_graph(
    command: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Link(args) => commands::cmd_link(args, store),
        TicketCommandCli::Unlink(args) => commands::cmd_unlink(args, store),
        TicketCommandCli::Links(args) => commands::cmd_links(args, store),
        TicketCommandCli::Subgraph(args) => commands::cmd_subgraph(args, store),
        TicketCommandCli::Topgraph(args) => commands::cmd_topgraph(args, store),
        TicketCommandCli::Watch(args) => commands::cmd_watch(args, store),
        TicketCommandCli::Status(args) => commands::cmd_status(args, store),
        TicketCommandCli::ReadyOverview(args) =>
            commands::cmd_ready_overview(args, store),
        TicketCommandCli::Next(args) => commands::cmd_next(args, store),
        TicketCommandCli::UnblockedBy(args) =>
            commands::cmd_unblocked_by(args, store),
        _ => unreachable!("handled in graph store dispatch"),
    }
}

fn dispatch_store_command_ops(
    command: TicketCommandCli,
    store: TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Serve(args) => commands::cmd_serve(args, store),
        TicketCommandCli::Close(args) => commands::cmd_close(args, &store),
        TicketCommandCli::Cancel(args) => commands::cmd_cancel(args, &store),
        TicketCommandCli::Attach(args) => commands::cmd_attach(args, &store),
        TicketCommandCli::Assets(args) => commands::cmd_assets(args, &store),
        TicketCommandCli::Health(args) => commands::cmd_health(args, &store),
        TicketCommandCli::Audit => commands::cmd_audit(&store),
        TicketCommandCli::Fmt(args) => commands::cmd_fmt(args, &store),
        TicketCommandCli::Board(args) => commands::cmd_board(args, &store),
        _ => unreachable!("handled in ops store dispatch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::IdArgs;
    use uuid::Uuid;

    #[test]
    fn dry_run_payload_is_returned_for_mutating_command() {
        let payload =
            dry_run_command_payload(&TicketCommandCli::Delete(IdArgs {
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

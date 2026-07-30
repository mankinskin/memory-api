use std::{
    collections::BTreeSet,
    path::{
        Path,
        PathBuf,
    },
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
    workspace_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
    _as_json: bool,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::ExportCommandSchema =>
            export_command_schema_payload(),
        TicketCommandCli::Catalog => capability_catalog_payload(),
        TicketCommandCli::Init => cmd_init(
            index_root_override,
            workspace_root_override,
            schema_dir_override,
        ),
        other => dispatch_store_backed(
            other,
            index_root_override,
            workspace_root_override,
            schema_dir_override,
            dry_run,
        ),
    }
}

fn cmd_init(
    index_root_override: Option<&Path>,
    workspace_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
) -> Result<Value, CliRunError> {
    let index_root =
        resolve_index_root(index_root_override, workspace_root_override);
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
            Some(dry_run_payload("delete", "permanently delete ticket")),
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
        TicketCommandCli::WritePart(_) =>
            Some(dry_run_payload("write-part", "write ticket content part")),
        TicketCommandCli::WriteAmendment(_) => Some(dry_run_payload(
            "write-amendment",
            "write amendment part superseding another part",
        )),
        TicketCommandCli::UndoPart(_) =>
            Some(dry_run_payload("undo-part", "restore prior part content")),
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
        TicketCommandCli::PruneDangling(_) =>
            Some(dry_run_payload("prune-dangling", "remove dangling edges")),
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
        TicketCommandCli::StoreIndex(_) => Some(dry_run_payload(
            "store-index",
            "generate/check ticket catalog",
        )),
        TicketCommandCli::Fmt(_) =>
            Some(dry_run_payload("fmt", "reformat ticket.toml files")),
        TicketCommandCli::Board(_) =>
            Some(dry_run_payload("board", "board state mutation")),
        TicketCommandCli::Workspace(_) =>
            Some(dry_run_payload("workspace", "workspace policy mutation")),
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

fn resolve_index_root(
    override_path: Option<&Path>,
    workspace_root_override: Option<&Path>,
) -> PathBuf {
    let cwd = ticket_api::workspace::working_dir();
    let env_root = std::env::var_os("TICKET_INDEX_ROOT").map(PathBuf::from);
    resolve_index_root_from(
        override_path,
        workspace_root_override,
        env_root.as_deref(),
        cwd.as_deref(),
    )
}

fn resolve_index_root_from(
    override_path: Option<&Path>,
    workspace_root_override: Option<&Path>,
    env_root: Option<&Path>,
    cwd: Option<&Path>,
) -> PathBuf {
    ticket_api::workspace::resolve_requested_store_root_from(
        override_path,
        workspace_root_override,
        env_root,
        cwd,
        ticket_api::workspace::TICKET_INDEX_DIR,
    )
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

fn capability_catalog_payload() -> Result<Value, CliRunError> {
    let mut payload = ticket_api::contracts::capability_catalog::capability_catalog();
    if let Value::Object(map) = &mut payload {
        map.insert("command".to_string(), json!("catalog"));
        map.insert("status".to_string(), json!("ok"));
    }
    Ok(payload)
}

fn dispatch_store_backed(
    command: TicketCommandCli,
    index_root_override: Option<&Path>,
    workspace_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    if dry_run {
        if let Some(payload) = dry_run_command_payload(&command) {
            return Ok(payload);
        }
    }

    require_explicit_workspace_for_create(
        &command,
        index_root_override,
        workspace_root_override,
    )?;

    let index_root =
        resolve_index_root(index_root_override, workspace_root_override);
    let workspace_root =
        resolve_workspace_root(&index_root, workspace_root_override);
    let store = open_store(&index_root, schema_dir_override)?;
    if command_uses_descendant_scan_roots(&command) {
        let reindex = register_descendant_scan_roots(&store, &workspace_root)?;
        if reindex {
            store.scan(true)?;
        }
    }

    dispatch_store_command(command, store, dry_run)
}

fn require_explicit_workspace_for_create(
    command: &TicketCommandCli,
    index_root_override: Option<&Path>,
    workspace_root_override: Option<&Path>,
) -> Result<(), CliRunError> {
    if matches!(
        command,
        TicketCommandCli::Create(_) | TicketCommandCli::Batch(_)
    ) && index_root_override.is_none()
        && workspace_root_override.is_none()
    {
        return Err(CliRunError::BadRequest(
            "entity creation requires explicit --workspace <path> or --index-root <path>".to_string(),
        ));
    }
    Ok(())
}

fn command_uses_descendant_scan_roots(command: &TicketCommandCli) -> bool {
    matches!(
        command,
        TicketCommandCli::Describe(_)
            | TicketCommandCli::List(_)
            | TicketCommandCli::Scan(_)
            | TicketCommandCli::Leases
            | TicketCommandCli::Search(_)
            | TicketCommandCli::Query(_)
            | TicketCommandCli::History(_)
            | TicketCommandCli::Diff(_)
            | TicketCommandCli::Links(_)
            | TicketCommandCli::PruneDangling(_)
            | TicketCommandCli::Subgraph(_)
            | TicketCommandCli::Topgraph(_)
            | TicketCommandCli::Status(_)
            | TicketCommandCli::ReadyOverview(_)
            | TicketCommandCli::Next(_)
            | TicketCommandCli::Blockers(_)
            | TicketCommandCli::UnblockedBy(_)
            | TicketCommandCli::Move(_)
            | TicketCommandCli::Assets(_)
            | TicketCommandCli::Transitions(_)
            | TicketCommandCli::Health(_)
            | TicketCommandCli::StoreIndex(_)
            | TicketCommandCli::Audit
            | TicketCommandCli::ValidateLinks
            | TicketCommandCli::ListParts(_)
            | TicketCommandCli::GetPart(_)
    )
}

fn resolve_workspace_root(
    index_root: &Path,
    workspace_root_override: Option<&Path>,
) -> PathBuf {
    if let Some(path) = workspace_root_override {
        let store_root = ticket_api::workspace::resolve_store_root_from(
            path,
            ticket_api::workspace::TICKET_INDEX_DIR,
        );
        return ticket_api::workspace::resolve_workspace_root_from_store_root(
            &store_root,
            ticket_api::workspace::TICKET_INDEX_DIR,
        );
    }

    ticket_api::workspace::resolve_workspace_root_from_store_root(
        index_root,
        ticket_api::workspace::TICKET_INDEX_DIR,
    )
}

fn open_store(
    index_root: &Path,
    schema_dir_override: Option<&Path>,
) -> Result<TicketStore, CliRunError> {
    let mut registry = SchemaRegistry::with_builtins();
    if let Some(schema_dir) = schema_dir_override {
        registry.load_dir(schema_dir)?;
    }
    TicketStore::open_with(index_root, registry).map_err(CliRunError::from)
}

fn register_descendant_scan_roots(
    store: &TicketStore,
    workspace_root: &Path,
) -> Result<bool, CliRunError> {
    let mut known_scan_roots = store
        .list_scan_roots()?
        .into_iter()
        .map(|root| root.path)
        .collect::<BTreeSet<_>>();
    let mut reindex = false;

    for root in ticket_api::workspace::discover_workspace_scan_roots(
        workspace_root,
        ticket_api::workspace::TICKET_INDEX_DIR,
        "tickets",
    ) {
        if known_scan_roots.insert(root.path.clone()) {
            reindex = true;
        }
        store.add_scan_root(root)?;
    }

    Ok(reindex)
}

fn dispatch_store_command(
    command: TicketCommandCli,
    store: TicketStore,
    dry_run: bool,
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
        | TicketCommandCli::Unclaim(_)
        | TicketCommandCli::ListParts(_)
        | TicketCommandCli::GetPart(_)
        | TicketCommandCli::WritePart(_)
        | TicketCommandCli::WriteAmendment(_)
        | TicketCommandCli::UndoPart(_) =>
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
        | TicketCommandCli::PruneDangling(_)
        | TicketCommandCli::Subgraph(_)
        | TicketCommandCli::Topgraph(_)
        | TicketCommandCli::Watch(_)
        | TicketCommandCli::Status(_)
        | TicketCommandCli::ReadyOverview(_)
        | TicketCommandCli::Next(_)
        | TicketCommandCli::Blockers(_)
        | TicketCommandCli::UnblockedBy(_) =>
            dispatch_store_command_graph(command, &store),
        TicketCommandCli::Serve(_)
        | TicketCommandCli::Close(_)
        | TicketCommandCli::Cancel(_)
        | TicketCommandCli::Move(_)
        | TicketCommandCli::Attach(_)
        | TicketCommandCli::Assets(_)
        | TicketCommandCli::Transitions(_)
        | TicketCommandCli::Health(_)
        | TicketCommandCli::StoreIndex(_)
        | TicketCommandCli::Audit
        | TicketCommandCli::Fmt(_)
        | TicketCommandCli::Board(_)
        | TicketCommandCli::Workspace(_)
        | TicketCommandCli::ValidateLinks =>
            dispatch_store_command_ops(command, store, dry_run),
        TicketCommandCli::ExportCommandSchema | TicketCommandCli::Init => {
            unreachable!("handled before store dispatch")
        },
        TicketCommandCli::Catalog => {
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
        TicketCommandCli::ListParts(args) =>
            commands::cmd_list_parts(args, store),
        TicketCommandCli::GetPart(args) => commands::cmd_get_part(args, store),
        TicketCommandCli::WritePart(args) =>
            commands::cmd_write_part(args, store),
        TicketCommandCli::WriteAmendment(args) =>
            commands::cmd_write_amendment(args, store),
        TicketCommandCli::UndoPart(args) =>
            commands::cmd_undo_part(args, store),
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
        TicketCommandCli::Link(_)
        | TicketCommandCli::Unlink(_)
        | TicketCommandCli::Links(_)
        | TicketCommandCli::PruneDangling(_)
        | TicketCommandCli::Subgraph(_)
        | TicketCommandCli::Topgraph(_) =>
            dispatch_store_command_graph_edges(command, store),
        TicketCommandCli::Watch(_)
        | TicketCommandCli::Status(_)
        | TicketCommandCli::ReadyOverview(_)
        | TicketCommandCli::Next(_)
        | TicketCommandCli::Blockers(_)
        | TicketCommandCli::UnblockedBy(_) =>
            dispatch_store_command_graph_workflow(command, store),
        _ => unreachable!("handled in graph store dispatch"),
    }
}

fn dispatch_store_command_graph_edges(
    command: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Link(args) => commands::cmd_link(args, store),
        TicketCommandCli::Unlink(args) => commands::cmd_unlink(args, store),
        TicketCommandCli::Links(args) => commands::cmd_links(args, store),
        TicketCommandCli::PruneDangling(args) =>
            commands::cmd_prune_dangling(args, store),
        TicketCommandCli::Subgraph(args) => commands::cmd_subgraph(args, store),
        TicketCommandCli::Topgraph(args) => commands::cmd_topgraph(args, store),
        _ => unreachable!("handled in graph edge dispatch"),
    }
}

fn dispatch_store_command_graph_workflow(
    command: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Watch(args) => commands::cmd_watch(args, store),
        TicketCommandCli::Status(args) => commands::cmd_status(args, store),
        TicketCommandCli::ReadyOverview(args) =>
            commands::cmd_ready_overview(args, store),
        TicketCommandCli::Next(args) => commands::cmd_next(args, store),
        TicketCommandCli::Blockers(args) => commands::cmd_blockers(args, store),
        TicketCommandCli::UnblockedBy(args) =>
            commands::cmd_unblocked_by(args, store),
        _ => unreachable!("handled in graph workflow dispatch"),
    }
}

fn dispatch_store_command_ops(
    command: TicketCommandCli,
    store: TicketStore,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Serve(_)
        | TicketCommandCli::Close(_)
        | TicketCommandCli::Cancel(_)
        | TicketCommandCli::Move(_)
        | TicketCommandCli::Attach(_)
        | TicketCommandCli::Assets(_)
        | TicketCommandCli::Transitions(_) =>
            dispatch_store_command_ops_runtime(command, store, dry_run),
        TicketCommandCli::Health(_)
        | TicketCommandCli::StoreIndex(_)
        | TicketCommandCli::Audit
        | TicketCommandCli::Fmt(_)
        | TicketCommandCli::Board(_)
        | TicketCommandCli::Workspace(_)
        | TicketCommandCli::ValidateLinks =>
            dispatch_store_command_ops_admin(command, store),
        _ => unreachable!("handled in ops store dispatch"),
    }
}

fn dispatch_store_command_ops_runtime(
    command: TicketCommandCli,
    store: TicketStore,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Serve(args) => commands::cmd_serve(args, store),
        TicketCommandCli::Close(args) => commands::cmd_close(args, &store),
        TicketCommandCli::Cancel(args) => commands::cmd_cancel(args, &store),
        TicketCommandCli::Move(args) =>
            commands::cmd_move(args, &store, dry_run),
        TicketCommandCli::Attach(args) => commands::cmd_attach(args, &store),
        TicketCommandCli::Assets(args) => commands::cmd_assets(args, &store),
        TicketCommandCli::Transitions(args) =>
            commands::cmd_transitions(args, &store),
        _ => unreachable!("handled in ops runtime dispatch"),
    }
}

fn dispatch_store_command_ops_admin(
    command: TicketCommandCli,
    store: TicketStore,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::Health(args) => commands::cmd_health(args, &store),
        TicketCommandCli::StoreIndex(args) =>
            commands::cmd_store_index(args, &store),
        TicketCommandCli::Audit => commands::cmd_audit(&store),
        TicketCommandCli::Fmt(args) => commands::cmd_fmt(args, &store),
        TicketCommandCli::Board(args) => commands::cmd_board(args, &store),
        TicketCommandCli::Workspace(args) =>
            commands::cmd_workspace(args, &store),
        TicketCommandCli::ValidateLinks => commands::cmd_validate_links(&store),
        _ => unreachable!("handled in ops admin dispatch"),
    }
}

#[cfg(test)]
#[path = "dispatch/tests.rs"]
mod tests;

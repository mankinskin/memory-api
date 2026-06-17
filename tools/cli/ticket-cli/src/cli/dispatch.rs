use std::collections::BTreeSet;
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
    workspace_root_override: Option<&Path>,
    schema_dir_override: Option<&Path>,
    _as_json: bool,
    dry_run: bool,
) -> Result<Value, CliRunError> {
    match command {
        TicketCommandCli::ExportCommandSchema =>
            export_command_schema_payload(),
        TicketCommandCli::Init =>
            cmd_init(
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
    let index_root = resolve_index_root(
        index_root_override,
        workspace_root_override,
    );
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
        TicketCommandCli::StoreIndex(_) =>
            Some(dry_run_payload("store-index", "generate/check ticket catalog")),
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

    let index_root = resolve_index_root(
        index_root_override,
        workspace_root_override,
    );
    let workspace_root = resolve_workspace_root(
        &index_root,
        workspace_root_override,
    );
    let store = open_store(&index_root, schema_dir_override)?;
    if command_uses_descendant_scan_roots(&command) {
        let reindex = register_descendant_scan_roots(&store, &workspace_root)?;
        if reindex {
            store.scan(true)?;
        }
    }

    dispatch_store_command(command, store)
}

fn command_uses_descendant_scan_roots(command: &TicketCommandCli) -> bool {
    matches!(
        command,
        TicketCommandCli::Get(_)
            | TicketCommandCli::Describe(_)
            | TicketCommandCli::List(_)
            | TicketCommandCli::Scan(_)
            | TicketCommandCli::Leases
            | TicketCommandCli::Search(_)
            | TicketCommandCli::Query(_)
            | TicketCommandCli::History(_)
            | TicketCommandCli::Diff(_)
            | TicketCommandCli::Links(_)
            | TicketCommandCli::Subgraph(_)
            | TicketCommandCli::Topgraph(_)
            | TicketCommandCli::Status(_)
            | TicketCommandCli::ReadyOverview(_)
            | TicketCommandCli::Next(_)
            | TicketCommandCli::Blockers(_)
            | TicketCommandCli::UnblockedBy(_)
            | TicketCommandCli::Assets(_)
            | TicketCommandCli::Health(_)
            | TicketCommandCli::StoreIndex(_)
            | TicketCommandCli::Audit
    )
}

fn resolve_workspace_root(
    index_root: &Path,
    workspace_root_override: Option<&Path>,
) -> PathBuf {
    if let Some(path) = workspace_root_override {
        let store_root =
            ticket_api::workspace::resolve_store_root_from(
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
        | TicketCommandCli::Blockers(_)
        | TicketCommandCli::UnblockedBy(_) =>
            dispatch_store_command_graph(command, &store),
        TicketCommandCli::Serve(_)
        | TicketCommandCli::Close(_)
        | TicketCommandCli::Cancel(_)
        | TicketCommandCli::Attach(_)
        | TicketCommandCli::Assets(_)
        | TicketCommandCli::Health(_)
        | TicketCommandCli::StoreIndex(_)
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
        TicketCommandCli::Blockers(args) => commands::cmd_blockers(args, store),
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
        TicketCommandCli::StoreIndex(args) => commands::cmd_store_index(args, &store),
        TicketCommandCli::Audit => commands::cmd_audit(&store),
        TicketCommandCli::Fmt(args) => commands::cmd_fmt(args, &store),
        TicketCommandCli::Board(args) => commands::cmd_board(args, &store),
        _ => unreachable!("handled in ops store dispatch"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use ticket_api::storage::index::RedbIndexStore;
    use crate::cli::{
        IdArgs,
        ListArgs,
        ScanArgs,
        TextArgs,
    };
    use tempfile::tempdir;
    use uuid::Uuid;

    fn create_nested_ticket_fixture(
    ) -> (tempfile::TempDir, PathBuf, PathBuf, String) {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(&child).unwrap();

        let _root_store = TicketStore::init(&repo.join(".ticket")).unwrap();
        let child_store = TicketStore::init(&child.join(".ticket")).unwrap();
        let ticket_id = child_store
            .create(
                None,
                "tracker-improvement",
                Some("Nested workspace ticket"),
                None,
                BTreeMap::<String, serde_json::Value>::new(),
                None,
                Some("Nested workspace ticket body"),
            )
            .unwrap();

        (dir, repo, child, ticket_id.to_string())
    }

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

    #[test]
    fn resolve_index_root_prefers_explicit_workspace_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".ticket")).unwrap();
        std::fs::create_dir_all(child.join(".ticket")).unwrap();

        let resolved = resolve_index_root_from(
            None,
            Some(&child),
            None,
            Some(&repo),
        );

        assert_eq!(resolved, child.join(".ticket"));
    }

    #[test]
    fn resolve_index_root_prefers_explicit_index_root_over_workspace_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".ticket")).unwrap();
        std::fs::create_dir_all(child.join(".ticket")).unwrap();

        let resolved = resolve_index_root_from(
            Some(&repo.join(".ticket")),
            Some(&child),
            None,
            Some(&repo),
        );

        assert_eq!(resolved, repo.join(".ticket"));
    }

    #[test]
    fn dispatch_get_reads_child_ticket_from_explicit_workspace_root() {
        let (_dir, _repo, child, ticket_id) = create_nested_ticket_fixture();

        let payload = dispatch(
            TicketCommandCli::Get(IdArgs {
                id: ticket_id.clone(),
            }),
            None,
            Some(&child),
            None,
            true,
            false,
        )
        .unwrap();

        assert_eq!(payload["command"], "get");
        assert_eq!(payload["ticket"]["id"], ticket_id);
        assert_eq!(
            payload["ticket"]["fields"]["title"],
            "Nested workspace ticket"
        );
    }

    #[test]
    fn dispatch_search_reads_child_ticket_from_explicit_workspace_root() {
        let (_dir, _repo, child, ticket_id) = create_nested_ticket_fixture();

        let payload = dispatch(
            TicketCommandCli::Search(TextArgs {
                expression: "Nested workspace ticket".to_string(),
                limit: 10,
            }),
            None,
            Some(&child),
            None,
            true,
            false,
        )
        .unwrap();

        assert_eq!(payload["command"], "search");
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["results"][0]["id"], ticket_id);
    }

    #[test]
    fn dispatch_list_reads_child_ticket_from_explicit_workspace_root() {
        let (_dir, _repo, child, ticket_id) = create_nested_ticket_fixture();

        let payload = dispatch(
            TicketCommandCli::List(ListArgs {
                state: None,
                ticket_type: None,
                limit: Some(10),
                with_repro: false,
                where_clauses: Vec::new(),
            }),
            None,
            Some(&child),
            None,
            true,
            false,
        )
        .unwrap();

        assert_eq!(payload["command"], "list");
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["items"][0]["id"], ticket_id);
    }

    #[test]
    fn dispatch_list_repairs_existing_empty_root_index() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let store = TicketStore::init(&repo).unwrap();
        let ticket_id = store
            .create(
                None,
                "tracker-improvement",
                Some("Root workspace ticket"),
                None,
                BTreeMap::<String, serde_json::Value>::new(),
                None,
                Some("Root workspace ticket body"),
            )
            .unwrap();

        let index_root = store.index_root.clone();
        drop(store);

        std::fs::remove_file(index_root.join("tickets.db")).unwrap();
        let _ = std::fs::remove_file(index_root.join("tickets.db-shm"));
        let _ = std::fs::remove_file(index_root.join("tickets.db-wal"));
        let _ = std::fs::remove_dir_all(index_root.join("search_index"));
        RedbIndexStore::open(&index_root.join("tickets.db")).unwrap();

        let payload = dispatch(
            TicketCommandCli::List(ListArgs {
                state: None,
                ticket_type: None,
                limit: Some(10),
                with_repro: false,
                where_clauses: Vec::new(),
            }),
            None,
            Some(&repo),
            None,
            true,
            false,
        )
        .unwrap();

        assert_eq!(payload["command"], "list");
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["items"][0]["id"], ticket_id.to_string());
    }

    #[test]
    fn dispatch_scan_registers_child_ticket_from_explicit_workspace_root() {
        let (_dir, repo, _child, ticket_id) = create_nested_ticket_fixture();

        let payload = dispatch(
            TicketCommandCli::Scan(ScanArgs {
                reindex: false,
                force: false,
            }),
            None,
            Some(&repo),
            None,
            true,
            false,
        )
        .unwrap();

        assert_eq!(payload["command"], "scan");

        let root_store = TicketStore::open(&repo.join(".ticket")).unwrap();
        let search_payload = dispatch_store_command(
            TicketCommandCli::Search(TextArgs {
                expression: "Nested workspace ticket".to_string(),
                limit: 10,
            }),
            root_store,
        )
        .unwrap();

        assert_eq!(search_payload["command"], "search");
        assert_eq!(search_payload["count"], 1);
        assert_eq!(search_payload["results"][0]["id"], ticket_id);
    }

    #[test]
    fn dispatch_get_reads_child_ticket_after_scan_root_augmentation() {
        let (_dir, repo, _child, ticket_id) = create_nested_ticket_fixture();
        let root_store = TicketStore::open(&repo.join(".ticket")).unwrap();

        let reindex = register_descendant_scan_roots(&root_store, &repo).unwrap();
        assert!(reindex);
        root_store.scan(true).unwrap();

        let payload = dispatch_store_command(
            TicketCommandCli::Get(IdArgs {
                id: ticket_id.clone(),
            }),
            root_store,
        )
        .unwrap();

        assert_eq!(payload["command"], "get");
        assert_eq!(payload["ticket"]["id"], ticket_id);
    }

    #[test]
    fn dispatch_search_reads_child_ticket_after_scan_root_augmentation() {
        let (_dir, repo, _child, ticket_id) = create_nested_ticket_fixture();
        let root_store = TicketStore::open(&repo.join(".ticket")).unwrap();

        let reindex = register_descendant_scan_roots(&root_store, &repo).unwrap();
        assert!(reindex);
        root_store.scan(true).unwrap();

        let payload = dispatch_store_command(
            TicketCommandCli::Search(TextArgs {
                expression: "Nested workspace ticket".to_string(),
                limit: 10,
            }),
            root_store,
        )
        .unwrap();

        assert_eq!(payload["command"], "search");
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["results"][0]["id"], ticket_id);
    }
}

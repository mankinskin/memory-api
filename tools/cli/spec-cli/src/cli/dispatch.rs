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

use spec_api::SpecStore;

use crate::cli::{
    CliRunError,
    SpecCommandCli,
    commands,
};

pub(super) fn dispatch(
    command: SpecCommandCli,
    index_root_override: Option<&Path>,
    workspace_root_override: Option<&Path>,
    _as_json: bool,
) -> Result<Value, CliRunError> {
    require_explicit_workspace_for_create(
        &command,
        index_root_override,
        workspace_root_override,
    )?;

    let index_root =
        resolve_index_root(index_root_override, workspace_root_override);
    let default_workspace_root =
        resolve_workspace_root(&index_root, workspace_root_override);

    if matches!(command, SpecCommandCli::Init) {
        let store = SpecStore::init(&index_root)?;
        return Ok(json!({
            "command": "init",
            "status": "ok",
            "workspace": store.entity_store().index_root.display().to_string(),
            "message": "workspace initialized",
        }));
    }

    let mut store = SpecStore::open(&index_root)?;

    let reindex = if command_uses_descendant_scan_roots(&command) {
        register_descendant_scan_roots(&store, &default_workspace_root)?
    } else {
        false
    };

    // Auto-scan to pick up any new spec folders and keep search in sync when
    // descendant workspace roots are added.
    store.scan(reindex)?;

    if command_mutates(&command) {
        dispatch_mutating(command, &mut store, &default_workspace_root)
    } else {
        dispatch_read_only(command, &store, &default_workspace_root)
    }
}

fn require_explicit_workspace_for_create(
    command: &SpecCommandCli,
    index_root_override: Option<&Path>,
    workspace_root_override: Option<&Path>,
) -> Result<(), CliRunError> {
    if matches!(
        command,
        SpecCommandCli::Create(_) | SpecCommandCli::Bootstrap(_)
    ) && index_root_override.is_none()
        && workspace_root_override.is_none()
    {
        return Err(CliRunError::BadRequest(
            "entity creation requires explicit --workspace <path> or --index-root <path>".to_string(),
        ));
    }
    Ok(())
}

fn command_uses_descendant_scan_roots(command: &SpecCommandCli) -> bool {
    matches!(
        command,
        SpecCommandCli::Get(_)
            | SpecCommandCli::List(_)
            | SpecCommandCli::Search(_)
            | SpecCommandCli::Tree(_)
            | SpecCommandCli::Refs(_)
            | SpecCommandCli::SyncGenerated(_)
            | SpecCommandCli::Health(_)
            | SpecCommandCli::StoreIndex(_)
            | SpecCommandCli::Scan(_)
            | SpecCommandCli::ValidateLinks
    )
}

fn command_mutates(command: &SpecCommandCli) -> bool {
    matches!(
        command,
        SpecCommandCli::Create(_)
            | SpecCommandCli::Update(_)
            | SpecCommandCli::Delete(_)
            | SpecCommandCli::Scan(_)
            | SpecCommandCli::SyncGenerated(_)
            | SpecCommandCli::Section(_)
            | SpecCommandCli::Bootstrap(_)
    )
}

fn dispatch_mutating(
    command: SpecCommandCli,
    store: &mut SpecStore,
    default_workspace_root: &Path,
) -> Result<Value, CliRunError> {
    match command {
        SpecCommandCli::Create(args) => commands::cmd_create(args, store),
        SpecCommandCli::Update(args) => commands::cmd_update(args, store),
        SpecCommandCli::Delete(args) => commands::cmd_delete(args, store),
        SpecCommandCli::Scan(args) => commands::cmd_scan(args, store),
        SpecCommandCli::SyncGenerated(args) =>
            commands::cmd_sync_generated(args, store, default_workspace_root),
        SpecCommandCli::Section(args) => commands::cmd_section(args, store),
        SpecCommandCli::Bootstrap(args) => commands::cmd_bootstrap(args, store),
        SpecCommandCli::Init => unreachable!("Init handled before store open"),
        _ => unreachable!(
            "command_mutates keeps non-mutating commands out of this path"
        ),
    }
}

fn dispatch_read_only(
    command: SpecCommandCli,
    store: &SpecStore,
    default_workspace_root: &Path,
) -> Result<Value, CliRunError> {
    match command {
        SpecCommandCli::Get(args) => commands::cmd_get(args, store),
        SpecCommandCli::List(args) => commands::cmd_list(args, store),
        SpecCommandCli::Search(args) => commands::cmd_search(args, store),
        SpecCommandCli::AddRoot(args) => commands::cmd_add_root(args, store),
        SpecCommandCli::Tree(args) => commands::cmd_tree(args, store),
        SpecCommandCli::Refs(args) =>
            commands::cmd_refs(args, store, default_workspace_root),
        SpecCommandCli::Health(args) => commands::cmd_health(args, store),
        SpecCommandCli::Move(args) => commands::cmd_move(args, store),
        SpecCommandCli::StoreIndex(args) =>
            commands::cmd_store_index(args, store, default_workspace_root),
        SpecCommandCli::ValidateLinks =>
            commands::cmd_validate_links(store, default_workspace_root),
        SpecCommandCli::Init => unreachable!("Init handled before store open"),
        _ => unreachable!(
            "command_mutates keeps mutating commands out of this path"
        ),
    }
}

fn resolve_index_root(
    override_path: Option<&Path>,
    workspace_root_override: Option<&Path>,
) -> PathBuf {
    let cwd = memory_kernel::workspace::working_dir();
    let env_root = std::env::var_os("SPEC_INDEX_ROOT").map(PathBuf::from);
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
    memory_kernel::workspace::resolve_requested_store_root_from(
        override_path,
        workspace_root_override,
        env_root,
        cwd,
        ".spec",
    )
}

fn resolve_workspace_root(
    index_root: &Path,
    workspace_root_override: Option<&Path>,
) -> PathBuf {
    if let Some(path) = workspace_root_override {
        let store_root =
            memory_kernel::workspace::resolve_store_root_from(path, ".spec");
        return memory_kernel::workspace::resolve_workspace_root_from_store_root(
            &store_root,
            ".spec",
        );
    }

    memory_kernel::workspace::resolve_workspace_root_from_store_root(
        index_root, ".spec",
    )
}

fn register_descendant_scan_roots(
    store: &SpecStore,
    workspace_root: &Path,
) -> Result<bool, CliRunError> {
    let mut known_scan_roots = store
        .entity_store()
        .list_scan_roots()?
        .into_iter()
        .map(|root| root.path)
        .collect::<BTreeSet<_>>();
    let mut reindex = false;

    for root in memory_kernel::workspace::discover_workspace_scan_roots(
        workspace_root,
        ".spec",
        "specs",
    ) {
        if known_scan_roots.insert(root.path.clone()) {
            reindex = true;
        }
        store.entity_store().add_scan_root(root)?;
    }

    Ok(reindex)
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

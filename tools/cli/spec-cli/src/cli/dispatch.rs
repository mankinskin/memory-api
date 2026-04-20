use std::path::{Path, PathBuf};

use serde_json::Value;

use spec_api::SpecStore;

use crate::cli::{CliRunError, SpecCommandCli, commands};

pub(super) fn dispatch(
    command: SpecCommandCli,
    index_root_override: Option<&Path>,
    _as_json: bool,
) -> Result<Value, CliRunError> {
    let index_root = resolve_index_root(index_root_override);
    let mut store = SpecStore::open(&index_root)?;

    // Auto-scan to pick up any new spec folders
    store.scan(false)?;

    match command {
        SpecCommandCli::Create(args) => commands::cmd_create(args, &mut store),
        SpecCommandCli::Get(args) => commands::cmd_get(args, &store),
        SpecCommandCli::Update(args) => commands::cmd_update(args, &mut store),
        SpecCommandCli::Delete(args) => commands::cmd_delete(args, &mut store),
        SpecCommandCli::List(args) => commands::cmd_list(args, &store),
        SpecCommandCli::Search(args) => commands::cmd_search(args, &store),
        SpecCommandCli::Scan(args) => commands::cmd_scan(args, &mut store),
        SpecCommandCli::AddRoot(args) => commands::cmd_add_root(args, &store),
        SpecCommandCli::Tree(args) => commands::cmd_tree(args, &store),
        SpecCommandCli::Refs(args) => commands::cmd_refs(args, &store),
        SpecCommandCli::Section(args) => commands::cmd_section(args, &mut store),
        SpecCommandCli::Health(args) => commands::cmd_health(args, &store),
    }
}

fn resolve_index_root(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    if let Ok(env_val) = std::env::var("SPEC_INDEX_ROOT") {
        return PathBuf::from(env_val);
    }
    // Default: .spec/ in current working directory
    let cwd_spec = std::env::current_dir()
        .ok()
        .map(|d| d.join(".spec"));
    if let Some(p) = cwd_spec.filter(|p| p.exists()) {
        return p;
    }
    // Fallback: ~/.spec-index via HOME env var
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(home).join(".spec-index");
    }
    PathBuf::from(".spec")
}

use std::collections::BTreeSet;
use std::path::{
    Path,
    PathBuf,
};

use serde_json::{
    Value,
    json,
};

use spec_api::{
    SpecStore,
};

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
    let index_root =
        resolve_index_root(index_root_override, workspace_root_override);
    let default_workspace_root = resolve_workspace_root(
        &index_root,
        workspace_root_override,
    );

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
        dispatch_mutating(command, &mut store)
    } else {
        dispatch_read_only(
            command,
            &store,
            &default_workspace_root,
        )
    }
}

fn command_uses_descendant_scan_roots(command: &SpecCommandCli) -> bool {
    matches!(
        command,
        SpecCommandCli::Get(_)
            | SpecCommandCli::List(_)
            | SpecCommandCli::Search(_)
            | SpecCommandCli::Tree(_)
            | SpecCommandCli::Refs(_)
            | SpecCommandCli::Health(_)
            | SpecCommandCli::Scan(_)
    )
}

fn command_mutates(command: &SpecCommandCli) -> bool {
    matches!(
        command,
        SpecCommandCli::Create(_)
            | SpecCommandCli::Update(_)
            | SpecCommandCli::Delete(_)
            | SpecCommandCli::Scan(_)
            | SpecCommandCli::Section(_)
            | SpecCommandCli::Bootstrap(_)
    )
}

fn dispatch_mutating(
    command: SpecCommandCli,
    store: &mut SpecStore,
) -> Result<Value, CliRunError> {
    match command {
        SpecCommandCli::Create(args) => commands::cmd_create(args, store),
        SpecCommandCli::Update(args) => commands::cmd_update(args, store),
        SpecCommandCli::Delete(args) => commands::cmd_delete(args, store),
        SpecCommandCli::Scan(args) => commands::cmd_scan(args, store),
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
    let cwd = memory_api::workspace::working_dir();
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
    memory_api::workspace::resolve_requested_store_root_from(
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
            memory_api::workspace::resolve_store_root_from(path, ".spec");
        return memory_api::workspace::resolve_workspace_root_from_store_root(
            &store_root,
            ".spec",
        );
    }

    memory_api::workspace::resolve_workspace_root_from_store_root(
        index_root,
        ".spec",
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

    for root in memory_api::workspace::discover_workspace_scan_roots(
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
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn create_nested_spec_fixture(
    ) -> (tempfile::TempDir, PathBuf, PathBuf, String) {
        use spec_api::{
            SpecManifest,
            code_ref::{
                CodeRef,
                SymbolKind,
            },
        };

        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".spec")).unwrap();
        std::fs::create_dir_all(child.join(".spec")).unwrap();
        std::fs::create_dir_all(child.join("src")).unwrap();
        std::fs::write(child.join("src/lib.rs"), "pub fn nested() {}\n")
            .unwrap();

        let _root_store = SpecStore::init(&repo.join(".spec")).unwrap();
        let mut child_store = SpecStore::init(&child.join(".spec")).unwrap();
        let mut manifest = SpecManifest::new(
            "memory-api/nested-spec",
            "Nested spec",
            "memory-api",
        );
        manifest.code_refs = vec![CodeRef {
            file: "src/lib.rs".to_string(),
            symbol: "nested".to_string(),
            kind: SymbolKind::Function,
            line_start: 1,
            line_end: 1,
            description: None,
        }];
        let spec_id = child_store
            .create(&manifest, "Nested spec body", None)
            .unwrap();

        (dir, repo, child, spec_id.to_string())
    }

    #[test]
    fn resolve_index_root_prefers_nearest_parent_spec_dir() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let nested = repo.join("src").join("api");
        std::fs::create_dir_all(repo.join(".spec")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_index_root_from(None, None, None, Some(&nested));

        assert_eq!(resolved, repo.join(".spec"));
    }

    #[test]
    fn resolve_index_root_defaults_to_current_directory_spec_dir() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let resolved = resolve_index_root_from(None, None, None, Some(&repo));

        assert_eq!(resolved, repo.join(".spec"));
    }

    #[test]
    fn resolve_index_root_prefers_explicit_workspace_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".spec")).unwrap();
        std::fs::create_dir_all(child.join(".spec")).unwrap();

        let resolved =
            resolve_index_root_from(None, Some(&child), None, Some(&repo));

        assert_eq!(resolved, child.join(".spec"));
    }

    #[test]
    fn resolve_workspace_root_prefers_explicit_workspace_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(child.join(".spec")).unwrap();

        let resolved =
            resolve_workspace_root(&child.join(".spec"), Some(&child));

        assert_eq!(resolved, child);
    }

    #[test]
    fn resolve_workspace_root_defaults_to_parent_of_hidden_store() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".spec")).unwrap();

        let resolved =
            resolve_workspace_root(&repo.join(".spec"), None);

        assert_eq!(resolved, repo);
    }

    #[test]
    fn dispatch_get_reads_child_spec_from_explicit_workspace_root() {
        let (_dir, _repo, child, spec_id) = create_nested_spec_fixture();

        let payload = dispatch(
            SpecCommandCli::Get(crate::cli::GetArgs {
                id: spec_id.clone(),
                full: false,
            }),
            None,
            Some(&child),
            true,
        )
        .unwrap();

        assert_eq!(payload["command"], "get");
        assert_eq!(payload["spec"]["id"], spec_id);
        assert_eq!(payload["spec"]["fields"]["title"], "Nested spec");
    }

    #[test]
    fn dispatch_search_reads_child_spec_from_explicit_workspace_root() {
        let (_dir, _repo, child, spec_id) = create_nested_spec_fixture();

        let payload = dispatch(
            SpecCommandCli::Search(crate::cli::SearchArgs {
                query: "Nested spec".to_string(),
                limit: 10,
            }),
            None,
            Some(&child),
            true,
        )
        .unwrap();

        assert_eq!(payload["command"], "search");
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["items"][0]["id"], spec_id);
    }

    #[test]
    fn dispatch_scan_registers_child_spec_from_explicit_workspace_root() {
        let (_dir, repo, _child, spec_id) = create_nested_spec_fixture();

        let payload = dispatch(
            SpecCommandCli::Scan(crate::cli::ScanArgs { force: false }),
            None,
            Some(&repo),
            true,
        )
        .unwrap();

        assert_eq!(payload["command"], "scan");

        let root_store = SpecStore::open(&repo.join(".spec")).unwrap();
        let search_payload = dispatch_read_only(
            SpecCommandCli::Search(crate::cli::SearchArgs {
                query: "Nested spec".to_string(),
                limit: 10,
            }),
            &root_store,
            &repo,
        )
        .unwrap();

        assert_eq!(search_payload["command"], "search");
        assert_eq!(search_payload["count"], 1);
        assert_eq!(search_payload["items"][0]["id"], spec_id);
    }

    #[test]
    fn dispatch_refs_reads_child_spec_after_scan_root_augmentation() {
        let (_dir, repo, child, spec_id) = create_nested_spec_fixture();
        let mut root_store = SpecStore::init(&repo.join(".spec")).unwrap();

        let reindex = register_descendant_scan_roots(&root_store, &repo).unwrap();
        root_store.scan(reindex).unwrap();

        let payload = dispatch_read_only(
            SpecCommandCli::Refs(crate::cli::RefsArgs {
                id: spec_id.clone(),
                subcommand: Some(crate::cli::RefsSubcommand::Validate {
                    code_workspace_root: None,
                }),
            }),
            &root_store,
            &repo,
        )
        .unwrap();

        assert_eq!(payload["command"], "refs_validate");
        assert_eq!(payload["valid"], true);
        assert_eq!(
            payload["workspace_root"],
            child.to_string_lossy().replace('\\', "/")
        );
    }

    #[test]
    fn dispatch_search_reads_child_spec_after_scan_root_augmentation() {
        let (_dir, repo, _child, spec_id) = create_nested_spec_fixture();
        let mut root_store = SpecStore::init(&repo.join(".spec")).unwrap();

        let reindex = register_descendant_scan_roots(&root_store, &repo).unwrap();
        assert!(reindex);
        root_store.scan(reindex).unwrap();

        let payload = dispatch_read_only(
            SpecCommandCli::Search(crate::cli::SearchArgs {
                query: "Nested spec".to_string(),
                limit: 10,
            }),
            &root_store,
            &repo,
        )
        .unwrap();

        assert_eq!(payload["command"], "search");
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["items"][0]["id"], spec_id);
    }
}

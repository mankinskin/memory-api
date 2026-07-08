use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use rule_api::{
    RuleFilter,
    RuleManifest,
    RuleStore,
};
use spec_api::{
    SpecManifest,
    SpecStore,
};
use tempfile::tempdir;

use super::*;

fn sample_rule(
    slug: &str,
    title: &str,
    section: &str,
    body: &str,
    order_key: i64,
) -> RuleManifest {
    let mut manifest = RuleManifest::new(slug, title, "AGENTS", section, body);
    manifest.set_repo_scopes(["context-engine"]);
    manifest.set_order_key(order_key);
    manifest
}

fn run_git(
    repo_root: &Path,
    args: &[&str],
) {
    let status = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()
        .expect("git command");
    assert!(status.success(), "git {args:?} failed: {status}");
}

fn empty_filter_args() -> FilterArgs {
    FilterArgs {
        state: None,
        file_kind: None,
        section: None,
        repo_scope: None,
        path_scope: None,
        slug: None,
        low_rated_only: false,
        unresolved_only: false,
    }
}

fn create_nested_rule_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let dir = tempdir().unwrap();
    let repo_root = dir.path().join("repo");
    let parent_index_root = repo_root.join(".rule");
    let child_workspace = repo_root.join("memory-viewers").join("memory-api");
    let child_index_root = child_workspace.join(".rule");
    fs::create_dir_all(&child_workspace).unwrap();

    let mut parent_store = RuleStore::init(&parent_index_root).unwrap();
    parent_store
        .create(
            &sample_rule(
                "shared/agents/opening",
                "Opening",
                "opening",
                "Start with the concrete anchor.",
                10,
            ),
            None,
        )
        .unwrap();

    let mut child_store = RuleStore::init(&child_index_root).unwrap();
    let mut child_rule = RuleManifest::new(
        "memory-api/agents/overview",
        "Overview",
        "AGENTS",
        "overview",
        "Document memory-api specifics.",
    );
    child_rule.set_repo_scopes(["memory-api"]);
    child_rule.set_path_scopes(["AGENTS.md"]);
    child_rule.set_order_key(20);
    let child_id = child_store.create(&child_rule, None).unwrap();

    (dir, parent_index_root, child_index_root, child_id.to_string())
}

fn scan_nested_rule_fixture(parent_index_root: &Path) {
    let payload = dispatch::dispatch(
        RuleCommandCli::Scan(ScanArgs { force: false }),
        parent_index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
}


#[path = "tests/tests_commands.rs"]
mod tests_commands;
#[path = "tests/tests_generate_targets.rs"]
mod tests_generate_targets;
#[path = "tests/tests_workspace.rs"]
mod tests_workspace;

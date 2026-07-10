use super::*;

use std::{
    collections::BTreeMap,
    fs,
};

use serde::Deserialize;
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct ContractParityFixture {
    fields: BTreeMap<String, Value>,
    fulfillment_update: BTreeMap<String, Value>,
    search_query: String,
    expected_health_issue: String,
}

fn create_nested_spec_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, String)
{
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
    std::fs::write(child.join("src/lib.rs"), "pub fn nested() {}\n").unwrap();

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

fn create_cli_spec_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join(".spec")).unwrap();
    SpecStore::init(&repo.join(".spec")).unwrap();
    (dir, repo)
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

fn load_contract_parity_fixture() -> ContractParityFixture {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../test-fixtures/spec-contract-parity.json");
    serde_json::from_str(&fs::read_to_string(fixture_path).unwrap()).unwrap()
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

    let resolved = resolve_workspace_root(&child.join(".spec"), Some(&child));

    assert_eq!(resolved, child);
}

#[test]
fn resolve_workspace_root_defaults_to_parent_of_hidden_store() {
    let dir = tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join(".spec")).unwrap();

    let resolved = resolve_workspace_root(&repo.join(".spec"), None);

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
fn dispatch_move_dry_run_returns_supported_preflight_plan() {
    let (_dir, repo) = create_cli_spec_fixture();
    run_git(&repo, &["init"]);

    let target_workspace = repo.join("target");
    std::fs::create_dir_all(target_workspace.join(".spec")).unwrap();
    SpecStore::init(&target_workspace.join(".spec")).unwrap();

    let mut store = SpecStore::open(&repo.join(".spec")).unwrap();
    let manifest =
        spec_api::SpecManifest::new("sample/spec", "Sample spec", "spec-cli");
    let spec_id = store.create(&manifest, "body", None).unwrap();
    store.scan(true).unwrap();

    let payload = dispatch(
        SpecCommandCli::Move(crate::cli::MoveArgs {
            id: Some(spec_id.to_string()),
            to_workspace_root: Some(target_workspace),
            dry_run: true,
            resume: None,
            rollback: None,
        }),
        Some(&repo.join(".spec")),
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(payload["command"], "move");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["mode"], "plan");
    assert_eq!(payload["plan"]["supported"], true);
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
        memory_api::workspace::normalize_path_for_display(&child)
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

#[test]
fn dispatch_authoring_contract_supports_legacy_current_format_specs() {
    let (_dir, repo) = create_cli_spec_fixture();
    let body_path = repo.join("legacy-body.md");
    fs::write(
            &body_path,
            concat!(
                "# Summary\n\n",
                "Document the legacy current-format authoring path.\n\n",
                "## Motivation\n\n",
                "Keep existing authored specs valid during the first migration slice.\n\n",
                "## Current State\n\n",
                "Legacy current-format authored specs still exist in the store.\n\n",
                "## Acceptance Criteria\n\n",
                "- Legacy authored specs remain readable and searchable.\n",
            ),
        )
        .unwrap();

    let created = dispatch(
        SpecCommandCli::Create(crate::cli::CreateArgs {
            title: "Legacy current format spec".to_string(),
            slug: "contract/legacy-current-format".to_string(),
            component: "context-engine".to_string(),
            parent: None,
            scope: Some("public".to_string()),
            body_file: Some(body_path.clone()),
            fields_file: None,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let spec_id = created["id"].as_str().unwrap().to_string();

    let updated_body_path = repo.join("legacy-body-updated.md");
    fs::write(
            &updated_body_path,
            concat!(
                "# Summary\n\n",
                "Document the legacy current-format authoring path.\n\n",
                "## Motivation\n\n",
                "Keep existing authored specs valid during the first migration slice.\n\n",
                "## Current State\n\n",
                "Legacy current-format authored specs still exist in the store.\n\n",
                "## Acceptance Criteria\n\n",
                "- Legacy authored specs remain readable and searchable after updates.\n",
            ),
        )
        .unwrap();

    dispatch(
        SpecCommandCli::Update(crate::cli::UpdateArgs {
            id: spec_id.clone(),
            fields: vec![
                "title=Legacy current format spec updated".to_string(),
            ],
            to_state: None,
            body_file: Some(updated_body_path),
            fields_file: None,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let fetched = dispatch(
        SpecCommandCli::Get(crate::cli::GetArgs {
            id: spec_id.clone(),
            full: true,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(
        fetched["spec"]["fields"]["title"],
        "Legacy current format spec updated"
    );
    assert!(fetched["body"].as_str().unwrap().contains(
        "Legacy current-format authored specs still exist in the store."
    ));

    let searched = dispatch(
        SpecCommandCli::Search(crate::cli::SearchArgs {
            query: "legacy current-format authored specs still exist"
                .to_string(),
            limit: 10,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(searched["count"], 1);
    assert_eq!(searched["items"][0]["id"], spec_id);

    let health = dispatch(
        SpecCommandCli::Health(crate::cli::HealthArgs {
            id: Some(spec_id),
            all: false,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(health["issues_count"], 0);
}

#[test]
fn dispatch_authoring_contract_supports_expectation_oriented_specs() {
    let (_dir, repo) = create_cli_spec_fixture();
    let body_path = repo.join("expectation-body.md");
    fs::write(
        &body_path,
        concat!(
            "# Summary\n\n",
            "Describe an expectation-oriented authored spec.\n\n",
            "## Intended Properties\n\n",
            "- Specs document intended system properties.\n",
            "- Tickets carry rollout sequencing and current-state notes.\n\n",
            "## Acceptance Criteria\n\n",
            "- The expected property is observable through the store.\n\n",
            "## Evidence\n\n",
            "- Store-owned evidence can satisfy or block implementation.\n",
        ),
    )
    .unwrap();

    let created = dispatch(
        SpecCommandCli::Create(crate::cli::CreateArgs {
            title: "Expectation-oriented spec".to_string(),
            slug: "contract/expectation-oriented".to_string(),
            component: "context-engine".to_string(),
            parent: None,
            scope: Some("public".to_string()),
            body_file: Some(body_path.clone()),
            fields_file: None,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let spec_id = created["id"].as_str().unwrap().to_string();

    let updated_body_path = repo.join("expectation-body-updated.md");
    fs::write(
        &updated_body_path,
        concat!(
            "# Summary\n\n",
            "Describe an expectation-oriented authored spec.\n\n",
            "## Intended Properties\n\n",
            "- Specs document intended system properties.\n",
            "- Tickets carry rollout sequencing and current-state notes.\n\n",
            "## Acceptance Criteria\n\n",
            "- The expected property remains observable after updates.\n\n",
            "## Evidence\n\n",
            "- Store-owned evidence can satisfy or block implementation.\n",
        ),
    )
    .unwrap();

    dispatch(
        SpecCommandCli::Update(crate::cli::UpdateArgs {
            id: spec_id.clone(),
            fields: vec!["title=Expectation-oriented spec updated".to_string()],
            to_state: None,
            body_file: Some(updated_body_path),
            fields_file: None,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let fetched = dispatch(
        SpecCommandCli::Get(crate::cli::GetArgs {
            id: spec_id.clone(),
            full: true,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(
        fetched["spec"]["fields"]["title"],
        "Expectation-oriented spec updated"
    );
    assert!(
        fetched["body"].as_str().unwrap().contains(
            "Store-owned evidence can satisfy or block implementation."
        )
    );

    let searched = dispatch(
        SpecCommandCli::Search(crate::cli::SearchArgs {
            query: "store-owned evidence can satisfy or block implementation"
                .to_string(),
            limit: 10,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(searched["count"], 1);
    assert_eq!(searched["items"][0]["id"], spec_id);

    let health = dispatch(
        SpecCommandCli::Health(crate::cli::HealthArgs {
            id: Some(spec_id),
            all: false,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(health["issues_count"], 0);
}

#[test]
fn dispatch_structured_contract_fields_round_trip_across_cli_surfaces() {
    let (_dir, repo) = create_cli_spec_fixture();
    let fixture = load_contract_parity_fixture();
    let create_fields_path = repo.join("contract-fields.json");
    let update_fields_path = repo.join("contract-update.json");

    fs::write(
        &create_fields_path,
        serde_json::to_string_pretty(&fixture.fields).unwrap(),
    )
    .unwrap();
    fs::write(
        &update_fields_path,
        serde_json::to_string_pretty(&fixture.fulfillment_update).unwrap(),
    )
    .unwrap();

    let created = dispatch(
        SpecCommandCli::Create(crate::cli::CreateArgs {
            title: "Structured contract parity spec".to_string(),
            slug: "contract/structured-parity".to_string(),
            component: "context-engine".to_string(),
            parent: None,
            scope: Some("public".to_string()),
            body_file: None,
            fields_file: Some(create_fields_path),
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let spec_id = created["id"].as_str().unwrap().to_string();

    let health_before = dispatch(
        SpecCommandCli::Health(crate::cli::HealthArgs {
            id: Some(spec_id.clone()),
            all: false,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(health_before["issues_count"], 1);
    assert_eq!(
        health_before["issues"][0]["issue"],
        fixture.expected_health_issue
    );

    dispatch(
        SpecCommandCli::Update(crate::cli::UpdateArgs {
            id: spec_id.clone(),
            fields: Vec::new(),
            to_state: None,
            body_file: None,
            fields_file: Some(update_fields_path),
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let fetched = dispatch(
        SpecCommandCli::Get(crate::cli::GetArgs {
            id: spec_id.clone(),
            full: false,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(
        fetched["spec"]["fields"]["contract_mode"],
        "expectation-oriented"
    );
    assert_eq!(
        fetched["spec"]["fields"]["fulfillment_summaries"][0]["status"],
        "satisfied"
    );

    let searched = dispatch(
        SpecCommandCli::Search(crate::cli::SearchArgs {
            query: fixture.search_query,
            limit: 10,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(searched["count"], 1);
    assert_eq!(searched["items"][0]["id"], spec_id);

    let health_after = dispatch(
        SpecCommandCli::Health(crate::cli::HealthArgs {
            id: Some(spec_id),
            all: false,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(health_after["issues_count"], 0);
}

#[test]
fn dispatch_structured_contract_fields_accept_toon_files() {
    let (_dir, repo) = create_cli_spec_fixture();
    let fixture = load_contract_parity_fixture();
    let create_fields_path = repo.join("contract-fields.toon");
    let update_fields_path = repo.join("contract-update.toon");

    std::fs::write(
        &create_fields_path,
        toon_format::encode_default(&fixture.fields).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &update_fields_path,
        toon_format::encode_default(&fixture.fulfillment_update).unwrap(),
    )
    .unwrap();

    let created = dispatch(
        SpecCommandCli::Create(crate::cli::CreateArgs {
            title: "Structured contract TOON parity spec".to_string(),
            slug: "contract/structured-toon-parity".to_string(),
            component: "context-engine".to_string(),
            parent: None,
            scope: Some("public".to_string()),
            body_file: None,
            fields_file: Some(create_fields_path),
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let spec_id = created["id"].as_str().unwrap().to_string();

    dispatch(
        SpecCommandCli::Update(crate::cli::UpdateArgs {
            id: spec_id.clone(),
            fields: Vec::new(),
            to_state: None,
            body_file: None,
            fields_file: Some(update_fields_path),
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    let fetched = dispatch(
        SpecCommandCli::Get(crate::cli::GetArgs {
            id: spec_id,
            full: false,
        }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();

    assert_eq!(
        fetched["spec"]["fields"]["contract_mode"],
        "expectation-oriented"
    );
    assert_eq!(
        fetched["spec"]["fields"]["fulfillment_summaries"][0]["status"],
        "satisfied"
    );
}

#[test]
fn dispatch_store_index_writes_catalog_with_hierarchy_then_check_detects_drift()
{
    use spec_api::SpecManifest;

    let (_dir, repo) = create_cli_spec_fixture();
    let mut store = SpecStore::open(&repo.join(".spec")).unwrap();

    let parent = SpecManifest::new("root", "Root Spec", "comp-a");
    let parent_id = store.create(&parent, "Root body.", None).unwrap();

    let mut child = SpecManifest::new("root/child", "Child Spec", "comp-a");
    child.set_parent(&parent_id.to_string());
    store.create(&child, "Child body.", None).unwrap();

    // Write the catalog artifacts.
    let payload = dispatch(
        SpecCommandCli::StoreIndex(crate::cli::StoreIndexArgs { check: false }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["specs"], 2);
    assert_eq!(payload["roots"], 1);

    let readme = repo.join(".spec/README.md");
    let sidecar = repo.join(".spec/index.toon");
    let agent_hook = repo.join(".agents/spec-catalog.md");
    let tree_root = repo.join(".spec/tree");
    assert!(readme.is_file());
    assert!(sidecar.is_file());
    assert!(agent_hook.is_file());
    assert!(tree_root.is_dir());

    let tree_files = collect_files_recursive(&tree_root);
    assert_eq!(tree_files.len(), 2);
    let child_tree = tree_files
        .iter()
        .find(|p| p.to_string_lossy().contains("child"))
        .unwrap();
    let child_tree_text = fs::read_to_string(child_tree).unwrap();
    assert!(child_tree_text.contains("## Navigation"));
    assert!(child_tree_text.contains("Parent:"));

    let readme_text = fs::read_to_string(&readme).unwrap();
    assert!(readme_text.starts_with("<!-- spec-index:file generated=true -->"));
    assert!(readme_text.contains("## comp-a"));
    assert!(readme_text.contains("- [root](./tree/root/"));
    assert!(readme_text.contains("- [root/child](./tree/root/"));

    // --check is clean immediately after a write (idempotent).
    let check = dispatch(
        SpecCommandCli::StoreIndex(crate::cli::StoreIndexArgs { check: true }),
        None,
        Some(&repo),
        true,
    )
    .unwrap();
    assert_eq!(check["drift"], false);

    // Mutating a generated artifact makes --check fail (drift detected).
    fs::write(&readme, "tampered\n").unwrap();
    let drift = dispatch(
        SpecCommandCli::StoreIndex(crate::cli::StoreIndexArgs { check: true }),
        None,
        Some(&repo),
        true,
    );
    assert!(drift.is_err(), "check must fail on drift");
    assert!(drift.unwrap_err().to_string().contains("out of date"));
}

fn collect_files_recursive(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    collect_files_into(root, &mut out);
    out
}

fn collect_files_into(
    dir: &Path,
    out: &mut Vec<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_into(&path, out);
        } else {
            out.push(path);
        }
    }
}

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

#[test]
fn scan_command_reports_diagnostics_and_explains_counts() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let mut store = RuleStore::init(&index_root).unwrap();
    store
        .create(
            &sample_rule(
                "shared/agents/scan-root",
                "Scan Root",
                "scan-root",
                "A valid rule so scan integrates one entity.",
                10,
            ),
            None,
        )
        .unwrap();

    let rules_root = store
        .entity_store()
        .list_scan_roots()
        .unwrap()
        .into_iter()
        .find(|root| root.label == "rules")
        .unwrap()
        .path;
    let broken_rule_dir =
        rules_root.join("123e4567-e89b-12d3-a456-426614174000");
    fs::create_dir_all(&broken_rule_dir).unwrap();
    fs::write(
        broken_rule_dir.join("rule.toml"),
        "this is not valid = [toml",
    )
    .unwrap();
    drop(store);

    let payload = dispatch::dispatch(
        RuleCommandCli::Scan(ScanArgs { force: false }),
        &index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
    assert!(payload["integrated"].is_number());
    assert_eq!(payload["integrated"], payload["integrated_entities"]);
    assert!(payload["integrated_description"]
        .as_str()
        .unwrap()
        .contains("integrated"));
    assert!(payload["pruned"].is_number());
    assert_eq!(payload["pruned"], payload["pruned_entities"]);
    assert!(payload["pruned_description"]
        .as_str()
        .unwrap()
        .contains("reindex"));
    assert_eq!(payload["diagnostics_count"], 1);
    assert!(payload["diagnostics_description"]
        .as_str()
        .unwrap()
        .contains("path"));
    assert_eq!(payload["diagnostics"].as_array().unwrap().len(), 1);
    assert!(payload["diagnostics"][0]["path"]
        .as_str()
        .unwrap()
        .replace('\\', "/")
        .ends_with("/rule.toml"));
    assert!(!payload["diagnostics"][0]["reason"]
        .as_str()
        .unwrap()
        .is_empty());
    assert!(payload["scan_root_count"].as_u64().unwrap() >= 1);
    assert!(payload["active_scan_roots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|root| root["kind"] == "default"));
    assert!(payload["active_scan_roots"]
        .as_array()
        .unwrap()
        .iter()
        .any(|root| root["kind"] == "registered"));
}

#[test]
fn parse_search_command_with_filter_flags() {
    let cli = parse_cli_from([
        "rule",
        "search",
        "discovery",
        "--repo",
        "context-engine",
        "--limit",
        "5",
    ])
    .unwrap();

    match cli.command {
        RuleCommandCli::Search(args) => {
            assert_eq!(args.query, "discovery");
            assert_eq!(
                args.filter.repo_scope.as_deref(),
                Some("context-engine")
            );
            assert_eq!(args.limit, 5);
        },
        _ => panic!("expected search command"),
    }
}

#[test]
fn parse_sync_targets_command() {
    let cli = parse_cli_from([
        "rule",
        "sync-targets",
        "--config",
        "rule-targets.yaml",
        "--dry-run",
    ])
    .unwrap();

    match cli.command {
        RuleCommandCli::SyncTargets(args) => {
            assert_eq!(args.config, PathBuf::from("rule-targets.yaml"));
            assert!(args.dry_run);
            assert!(!args.check);
        },
        _ => panic!("expected sync-targets command"),
    }
}

#[test]
fn parse_global_workspace_root() {
    let cli = parse_cli_from([
        "rule",
        "--workspace-root",
        "memory-viewers/memory-api",
        "search",
        "discovery",
    ])
    .unwrap();

    assert_eq!(
        cli.workspace_root,
        Some(PathBuf::from("memory-viewers/memory-api"))
    );
}

#[test]
fn generate_target_respects_explicit_workspace_root_over_config_path() {
    let (_dir, parent_index_root, child_index_root, _child_id) =
        create_nested_rule_fixture();
    let repo_root = parent_index_root.parent().unwrap().to_path_buf();
    let child_workspace = child_index_root.parent().unwrap().to_path_buf();
    let config_path = repo_root.join("rule-targets.yaml");

    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: root-only\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    output_path: generated/AGENTS.md\n",
            "    nodes:\n",
            "      - name: opening\n",
            "        section: opening\n",
        ),
    )
    .unwrap();

    let result = run(RuleCli {
        json: true,
        index_root: None,
        workspace_root: Some(child_workspace),
        command: RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: "root-only".to_string(),
            dry_run: true,
            check: false,
        }),
    })
    .unwrap();

    match result {
        CliOutput::Json(payload) => {
            assert_eq!(payload["count"], 0);
            assert_eq!(payload["target"], "root-only");
        },
        CliOutput::Text(text) => {
            panic!("expected json output, got text: {text}");
        },
    }
}

#[test]
fn delete_command_removes_rule_by_slug() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let rule = sample_rule(
        "shared/agents/delete-me",
        "Delete Me",
        "delete-me",
        "Delete this rule via the CLI.",
        10,
    );
    store.create(&rule, None).unwrap();
    drop(store);

    dispatch::dispatch(
        RuleCommandCli::Delete(IdArgs {
            id: "shared/agents/delete-me".to_string(),
        }),
        dir.path(),
    )
    .unwrap();

    let reopened = RuleStore::init(dir.path()).unwrap();
    assert!(matches!(
        reopened.get("shared/agents/delete-me"),
        Err(rule_api::error::RuleError::NotFound(_))
    ));
}

#[test]
fn get_command_requires_explicit_scan_for_nested_workspaces() {
    let (_dir, parent_index_root, _child_index_root, child_id) =
        create_nested_rule_fixture();

    let result = dispatch::dispatch(
        RuleCommandCli::Get(IdArgs {
            id: child_id.clone(),
        }),
        &parent_index_root,
    );

    assert!(matches!(
        result,
        Err(crate::cli::CliRunError::Rule(rule_api::error::RuleError::NotFound(_)))
    ));

    scan_nested_rule_fixture(&parent_index_root);

    let payload = dispatch::dispatch(
        RuleCommandCli::Get(IdArgs {
            id: child_id.clone(),
        }),
        &parent_index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["rule"]["id"], child_id);
    assert_eq!(
        payload["rule"]["fields"]["slug"],
        "memory-api/agents/overview"
    );
}

#[test]
fn list_command_requires_explicit_scan_for_nested_workspaces() {
    let (_dir, parent_index_root, _child_index_root, child_id) =
        create_nested_rule_fixture();

    let payload = dispatch::dispatch(
        RuleCommandCli::List(ListArgs {
            filter: empty_filter_args(),
            limit: Some(10),
        }),
        &parent_index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["count"], 1);
    assert!(!payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == child_id));

    scan_nested_rule_fixture(&parent_index_root);

    let payload = dispatch::dispatch(
        RuleCommandCli::List(ListArgs {
            filter: empty_filter_args(),
            limit: Some(10),
        }),
        &parent_index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["count"], 2);
    assert!(payload["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == child_id));
}

#[test]
fn search_command_requires_explicit_scan_for_nested_workspaces() {
    let (_dir, parent_index_root, _child_index_root, child_id) =
        create_nested_rule_fixture();

    let payload = dispatch::dispatch(
        RuleCommandCli::Search(SearchArgs {
            query: "overview".to_string(),
            filter: empty_filter_args(),
            limit: 10,
        }),
        &parent_index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["count"], 0);

    scan_nested_rule_fixture(&parent_index_root);

    let payload = dispatch::dispatch(
        RuleCommandCli::Search(SearchArgs {
            query: "overview".to_string(),
            filter: empty_filter_args(),
            limit: 10,
        }),
        &parent_index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["items"][0]["id"], child_id);
}

#[test]
fn delete_command_from_ancestor_root_does_not_remove_child_rule() {
    let (_dir, parent_index_root, child_index_root, child_id) =
        create_nested_rule_fixture();

    let result = dispatch::dispatch(
        RuleCommandCli::Delete(IdArgs {
            id: child_id.clone(),
        }),
        &parent_index_root,
    );

    assert!(matches!(
        result,
        Err(crate::cli::CliRunError::Rule(rule_api::error::RuleError::NotFound(_)))
    ));

    let child_store = RuleStore::open(&child_index_root).unwrap();
    let child_rule = child_store.get(&child_id).unwrap();
    assert_eq!(child_rule.slug(), Some("memory-api/agents/overview"));
}

#[test]
fn generate_file_writes_deterministic_markdown_with_provenance() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let first = sample_rule(
        "shared/agents/validation",
        "Validation",
        "validation",
        "Run the focused check next.",
        20,
    );
    let second = sample_rule(
        "shared/agents/opening",
        "Opening",
        "opening",
        "Start with the concrete anchor.",
        10,
    );
    store.create(&first, None).unwrap();
    store.create(&second, None).unwrap();

    let output = dir.path().join("generated").join("AGENTS.md");
    dispatch::dispatch(
        RuleCommandCli::GenerateFile(GenerateFileArgs {
            file_kind: "AGENTS".to_string(),
            repo_scope: "context-engine".to_string(),
            path_scope: None,
            section: None,
            state: None,
            output: Some(output.clone()),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    let rendered = fs::read_to_string(&output).unwrap();

    assert!(rendered.starts_with("<!-- rule-api:file generated=true -->\n\n"));
    let opening_idx = rendered.find("slug=shared/agents/opening").unwrap();
    let validation_idx =
        rendered.find("slug=shared/agents/validation").unwrap();
    assert!(opening_idx < validation_idx);
}

#[test]
fn generate_file_omits_provenance_for_frontmatter_prompt_output() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let mut prompt = RuleManifest::new(
        "context-engine/prompts/spec",
        "Spec Prompt",
        ".prompt",
        "spec-prompt",
        "---\nname: spec\ndescription: Create a spec entry\n---\nCreate a new spec entry.\n",
    );
    prompt.set_repo_scopes(["context-engine"]);
    prompt.set_order_key(10);
    store.create(&prompt, None).unwrap();

    let output = dir.path().join("generated").join("spec.prompt.md");
    dispatch::dispatch(
        RuleCommandCli::GenerateFile(GenerateFileArgs {
            file_kind: ".prompt".to_string(),
            repo_scope: "context-engine".to_string(),
            path_scope: None,
            section: None,
            state: None,
            output: Some(output.clone()),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    let rendered = fs::read_to_string(&output).unwrap();

    assert!(rendered.starts_with("---\nname: spec\n"));
    assert!(!rendered.contains("rule-api:file generated=true"));
    assert!(!rendered.contains("rule-api:entry id="));
}

#[test]
fn import_file_creates_rules_from_markdown_blocks() {
    let dir = tempdir().unwrap();
    let markdown = dir.path().join("AGENTS.md");
    fs::write(
        &markdown,
        "# Opening\n\nStart with the concrete anchor.\n\n## Validation\n\nRun the focused check next.",
    )
    .unwrap();

    let mut store = RuleStore::init(dir.path()).unwrap();
    let items = importing::import_file(
        &mut store,
        &ImportFileArgs {
            path: markdown,
            file_kind: "AGENTS".to_string(),
            repo_scope: vec![
                "context-engine".to_string(),
                "memory-viewers".to_string(),
            ],
            slug_prefix: "shared/agents".to_string(),
            default_section: None,
            path_scope: vec!["AGENTS.md".to_string()],
            source_repo: Some("context-engine".to_string()),
            dry_run: false,
        },
    )
    .unwrap();

    assert_eq!(items.len(), 2);
    let imported = store
        .list(
            &RuleFilter {
                repo_scope: Some("context-engine".to_string()),
                ..RuleFilter::default()
            },
            None,
        )
        .unwrap();
    let imported_memory_viewers = store
        .list(
            &RuleFilter {
                repo_scope: Some("memory-viewers".to_string()),
                ..RuleFilter::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(imported.len(), 2);
    assert_eq!(imported_memory_viewers.len(), 2);
    assert_eq!(imported[0].slug(), Some("shared/agents/opening/l1"));
    assert_eq!(
        imported[1].slug(),
        Some("shared/agents/opening/validation/l5")
    );
}

#[test]
fn generate_target_uses_config_output_path() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let mut first = sample_rule(
        "shared/agents/opening",
        "Opening",
        "opening",
        "Start with the concrete anchor.",
        10,
    );
    first.set_path_scopes(["AGENTS.md"]);
    let mut second = sample_rule(
        "shared/agents/other",
        "Other",
        "other",
        "Different file target.",
        20,
    );
    second.set_path_scopes([".github/copilot-instructions.md"]);
    store.create(&first, None).unwrap();
    store.create(&second, None).unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: generated/AGENTS.md\n",
        ),
    )
    .unwrap();

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path.clone(),
            target: "context-engine-agents".to_string(),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    let rendered =
        fs::read_to_string(dir.path().join("generated").join("AGENTS.md"))
            .unwrap();
    assert!(rendered.contains("slug=shared/agents/opening"));
    assert!(!rendered.contains("slug=shared/agents/other"));
    assert!(rendered.starts_with("<!-- rule-api:file generated=true -->"));
}

#[test]
fn generate_target_accepts_output_path_selector() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let mut rule = RuleManifest::new(
        "shared/copilot/rtk",
        "RTK",
        "copilot-instructions",
        "rtk-token-optimized-cli",
        "Always prefix shell commands with rtk.",
    );
    rule.set_repo_scopes(["context-engine"]);
    rule.set_path_scopes([".github/copilot-instructions.md"]);
    rule.set_order_key(10);
    store.create(&rule, None).unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: context-engine-copilot-instructions\n",
            "    repo_scope: context-engine\n",
            "    file_kind: copilot-instructions\n",
            "    path_scope: .github/copilot-instructions.md\n",
            "    output_path: .github/copilot-instructions.md\n",
        ),
    )
    .unwrap();

    let payload = dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: ".github/copilot-instructions.md".to_string(),
            dry_run: true,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    assert_eq!(payload["target"], "context-engine-copilot-instructions");
    assert!(payload["output"]
        .as_str()
        .unwrap()
        .replace('\\', "/")
        .ends_with("/.github/copilot-instructions.md"));
    assert_eq!(payload["count"], 1);
    assert!(payload["content"]
        .as_str()
        .unwrap()
        .contains("slug=shared/copilot/rtk"));
}

#[test]
fn generate_target_supports_directory_config() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let config_dir = dir.path().join("rule-targets");
    fs::create_dir_all(&config_dir).unwrap();
    let mut rule = RuleManifest::new(
        "shared/copilot/rtk",
        "RTK",
        "copilot-instructions",
        "rtk-token-optimized-cli",
        "Always prefix shell commands with rtk.",
    );
    rule.set_repo_scopes(["context-engine"]);
    rule.set_path_scopes([".github/copilot-instructions.md"]);
    rule.set_order_key(10);
    store.create(&rule, None).unwrap();
    fs::write(
        config_dir.join("20-github-copilot.yaml"),
        concat!(
            "targets:\n",
            "  - name: context-engine-copilot-instructions\n",
            "    repo_scope: context-engine\n",
            "    file_kind: copilot-instructions\n",
            "    path_scope: .github/copilot-instructions.md\n",
            "    output_path: .github/copilot-instructions.md\n",
        ),
    )
    .unwrap();

    let payload = dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_dir,
            target: ".github/copilot-instructions.md".to_string(),
            dry_run: true,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    assert_eq!(payload["target"], "context-engine-copilot-instructions");
    assert!(payload["output"]
        .as_str()
        .unwrap()
        .replace('\\', "/")
        .ends_with("/.github/copilot-instructions.md"));
    assert_eq!(payload["count"], 1);
    assert!(payload["content"]
        .as_str()
        .unwrap()
        .contains("slug=shared/copilot/rtk"));
}

#[test]
fn sync_targets_writes_spec_doc_targets_into_spec_entries() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path().join("repo");
    fs::create_dir_all(&workspace_root).unwrap();

    let mut rule_store = RuleStore::init(&workspace_root).unwrap();
    let mut spec_store = SpecStore::init(&workspace_root).unwrap();
    let spec = SpecManifest::new(
        "memory-api/recurring-principles",
        "Recurring Principles",
        "memory-api",
    );
    let spec_id = spec_store.create(&spec, "placeholder", None).unwrap();
    let spec_path = spec_store
        .entity_store()
        .get_indexed(&spec_id)
        .unwrap()
        .unwrap()
        .path;
    let path_scope = format!(".spec/specs/{spec_id}/body.md");

    let mut rule = RuleManifest::new(
        "memory-api/recurring-principles/summary",
        "Recurring summary",
        "spec-doc",
        "summary",
        "## Summary\nGenerate through spec-api.\n",
    );
    rule.set_repo_scopes(["memory-api"]);
    rule.set_path_scopes([path_scope.as_str()]);
    rule_store.create(&rule, None).unwrap();
    drop(rule_store);
    drop(spec_store);

    let config_path = workspace_root.join("rule-targets.yaml");
    fs::write(
        &config_path,
        format!(
            concat!(
                "targets:\n",
                "  - name: recurring-principles-body\n",
                "    repo_scope: memory-api\n",
                "    file_kind: spec-doc\n",
                "    path_scope: {path_scope}\n",
                "    output_path: {path_scope}\n",
            ),
            path_scope = path_scope,
        ),
    )
    .unwrap();

    dispatch::dispatch(
        RuleCommandCli::SyncTargets(SyncTargetsArgs {
            config: config_path,
            dry_run: false,
            check: false,
        }),
        &workspace_root,
    )
    .unwrap();

    let spec_body = fs::read_to_string(spec_path.join("body.md")).unwrap();
    assert!(spec_body.starts_with("<!-- spec-api:file generated=true -->"));
    assert!(spec_body.contains("slug=memory-api/recurring-principles/summary"));
    assert!(!workspace_root.join("generated").exists());
}

#[test]
fn generate_target_preserves_existing_crlf_output() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let mut rule = sample_rule(
        "shared/agents/opening",
        "Opening",
        "opening",
        "Start with the concrete anchor.",
        10,
    );
    rule.set_path_scopes(["AGENTS.md"]);
    store.create(&rule, None).unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: generated/AGENTS.md\n",
        ),
    )
    .unwrap();

    let output = dir.path().join("generated").join("AGENTS.md");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(&output, "legacy\r\ncontent\r\n").unwrap();

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path.clone(),
            target: "context-engine-agents".to_string(),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    let rendered_bytes = fs::read(&output).unwrap();
    let rendered = String::from_utf8(rendered_bytes.clone()).unwrap();
    assert!(rendered.contains("slug=shared/agents/opening"));
    assert!(rendered_bytes.windows(2).any(|window| window == b"\r\n"));
    for (index, byte) in rendered_bytes.iter().enumerate() {
        if *byte == b'\n' {
            assert!(index > 0 && rendered_bytes[index - 1] == b'\r');
        }
    }

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: "context-engine-agents".to_string(),
            dry_run: false,
            check: true,
        }),
        dir.path(),
    )
    .unwrap();
}

#[test]
fn generate_target_supports_folder_tree_config_output() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let mut rule = sample_rule(
        "shared/agents/opening",
        "Opening",
        "opening",
        "Start with the concrete anchor.",
        10,
    );
    rule.set_path_scopes(["AGENTS.md"]);
    store.create(&rule, None).unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "folders:\n",
            "  - name: generated\n",
            "    folders:\n",
            "      - name: docs\n",
            "        files:\n",
            "          - name: AGENTS.md\n",
            "            target:\n",
            "              name: context-engine-agents\n",
            "              repo_scope: context-engine\n",
            "              file_kind: AGENTS\n",
            "              path_scope: AGENTS.md\n",
        ),
    )
    .unwrap();

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path.clone(),
            target: "context-engine-agents".to_string(),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    let rendered = fs::read_to_string(
        dir.path().join("generated").join("docs").join("AGENTS.md"),
    )
    .unwrap();
    assert!(rendered.contains("slug=shared/agents/opening"));

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: "context-engine-agents".to_string(),
            dry_run: false,
            check: true,
        }),
        dir.path(),
    )
    .unwrap();
}

#[test]
fn generate_target_supports_dot_prefixed_prompt_tree_output() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let mut prompt = RuleManifest::new(
        "context-engine/prompts/spec",
        "Spec Prompt",
        ".prompt",
        "spec-prompt",
        "---\nname: spec\n---\nCreate a new spec entry.\n",
    );
    prompt.set_repo_scopes(["context-engine"]);
    prompt.set_path_scopes([".github/prompts/spec.prompt.md"]);
    prompt.set_order_key(10);
    store.create(&prompt, None).unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "folders:\n",
            "  - name: .github\n",
            "    folders:\n",
            "      - name: prompts\n",
            "        files:\n",
            "          - name: spec.prompt.md\n",
            "            target:\n",
            "              name: context-engine-prompt-spec\n",
            "              repo_scope: context-engine\n",
            "              file_kind: .prompt\n",
            "              path_scope: .github/prompts/spec.prompt.md\n",
            "              nodes:\n",
            "                - name: spec-prompt\n",
            "                  section: spec-prompt\n",
        ),
    )
    .unwrap();

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path.clone(),
            target: "context-engine-prompt-spec".to_string(),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    let rendered = fs::read_to_string(
        dir.path()
            .join(".github")
            .join("prompts")
            .join("spec.prompt.md"),
    )
    .unwrap();
    assert!(rendered.starts_with("---\nname: spec\n"));
    assert!(!rendered.contains("rule-api:file generated=true"));

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: "context-engine-prompt-spec".to_string(),
            dry_run: false,
            check: true,
        }),
        dir.path(),
    )
    .unwrap();
}

#[test]
fn repo_spec_prompt_target_matches_expectation_oriented_contract() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(5)
        .expect("context-engine repo root")
        .to_path_buf();

    let prompt_path = repo_root.join(".github/prompts/spec.prompt.md");
    let rendered = fs::read_to_string(&prompt_path).unwrap();

    assert!(rendered.contains("intended system properties"));
    assert!(rendered.contains("explicit acceptance criteria"));
    assert!(rendered.contains(
        "Keep problem statements, current-state analysis, rollout sequencing, blockers, and implementation notes in related tickets"
    ));
    assert!(!rendered.contains(
        "captures motivation, intended behavior or scope"
    ));

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: repo_root.join("rule-targets.yaml"),
            target: "context-engine-prompt-spec".to_string(),
            dry_run: false,
            check: true,
        }),
        &repo_root,
    )
    .unwrap();
}

#[test]
fn add_root_command_creates_missing_directory() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let root = index_root.join("rules");

    dispatch::dispatch(RuleCommandCli::Init, &index_root).unwrap();
    dispatch::dispatch(
        RuleCommandCli::AddRoot(AddRootArgs {
            path: root.clone(),
            label: None,
        }),
        &index_root,
    )
    .unwrap();

    assert!(root.is_dir());
}

#[test]
fn feedback_command_self_heals_after_missing_rule_folder() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let stale = sample_rule(
        "shared/agents/stale-rule",
        "Stale Rule",
        "stale-rule",
        "This folder will be deleted before feedback runs.",
        10,
    );
    let healthy = sample_rule(
        "shared/agents/healthy-rule",
        "Healthy Rule",
        "healthy-rule",
        "This rule should still accept feedback.",
        20,
    );

    let stale_id = store.create(&stale, None).unwrap();
    store.create(&healthy, None).unwrap();
    let stale_path = store
        .entity_store()
        .get_indexed(&stale_id)
        .unwrap()
        .unwrap()
        .path;
    fs::remove_dir_all(&stale_path).unwrap();
    drop(store);

    let result = dispatch::dispatch(
        RuleCommandCli::Feedback(FeedbackArgs {
            id: "shared/agents/healthy-rule".to_string(),
            rating: "helpful".to_string(),
            note: Some("Still accurate after pruning stale rows.".to_string()),
            note_kind: Some("note".to_string()),
            session_id: None,
            agent_or_user_id: None,
        }),
        dir.path(),
    )
    .unwrap();

    assert_eq!(result["status"], "ok");

    let reopened = RuleStore::init(dir.path()).unwrap();
    let healthy_rule = reopened.get("shared/agents/healthy-rule").unwrap();
    assert_eq!(healthy_rule.feedback_helpful_count(), Some(1));
    assert!(
        reopened
            .entity_store()
            .get_indexed(&stale_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn generate_target_requires_explicit_scan_for_nested_workspaces() {
    let dir = tempdir().unwrap();
    let repo_root = dir.path().join("repo");
    let parent_index_root = repo_root.join(".rule");
    let child_workspace = repo_root.join("memory-viewers").join("memory-api");
    let child_index_root = child_workspace.join(".rule");
    fs::create_dir_all(&child_workspace).unwrap();

    let mut parent_store = RuleStore::init(&parent_index_root).unwrap();
    let mut parent_rule = sample_rule(
        "shared/agents/opening",
        "Opening",
        "opening",
        "Start with the concrete anchor.",
        10,
    );
    parent_rule.set_path_scopes(["AGENTS.md"]);
    parent_store.create(&parent_rule, None).unwrap();

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
    child_store.create(&child_rule, None).unwrap();

    let config_path = repo_root.join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: combined-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: generated/AGENTS.md\n",
            "    nodes:\n",
            "      - name: opening\n",
            "        section: opening\n",
            "      - name: child-overview\n",
            "        repo_scope: memory-api\n",
            "        section: overview\n",
        ),
    )
    .unwrap();

    let payload = dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: "combined-agents".to_string(),
            dry_run: true,
            check: false,
        }),
        &parent_index_root,
    )
    .unwrap();

    let rendered = payload["content"].as_str().unwrap();
    assert!(rendered.contains("slug=shared/agents/opening"));
    assert!(!rendered.contains("slug=memory-api/agents/overview"));

    scan_nested_rule_fixture(&parent_index_root);

    let payload = dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: repo_root.join("rule-targets.yaml"),
            target: "combined-agents".to_string(),
            dry_run: true,
            check: false,
        }),
        &parent_index_root,
    )
    .unwrap();

    let rendered = payload["content"].as_str().unwrap();
    assert!(rendered.contains("slug=shared/agents/opening"));
    assert!(rendered.contains("slug=memory-api/agents/overview"));
}

#[test]
fn sync_targets_prunes_removed_outputs_from_previous_sync() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();

    store
        .create(
            &sample_rule(
                "shared/agents/root-readme",
                "Root README",
                "root-readme",
                "Root body.",
                10,
            ),
            None,
        )
        .unwrap();
    store
        .create(
            &sample_rule(
                "shared/agents/nested-readme",
                "Nested README",
                "nested-readme",
                "Nested body.",
                20,
            ),
            None,
        )
        .unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: root-readme\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    section: root-readme\n",
            "    output_path: .github/README.md\n",
            "  - name: nested-readme\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    section: nested-readme\n",
            "    output_path: memory-viewers/.github/README.md\n",
        ),
    )
    .unwrap();

    rendering::sync_targets_payload(&mut store, &config_path, false, false)
        .unwrap();
    assert!(dir.path().join("memory-viewers/.github/README.md").exists());

    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: root-readme\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    section: root-readme\n",
            "    output_path: .github/README.md\n",
        ),
    )
    .unwrap();

    let payload =
        rendering::sync_targets_payload(&mut store, &config_path, false, false)
            .unwrap();
    assert_eq!(payload.generated.len(), 1);
    assert_eq!(payload.removed.len(), 1);
    assert!(!dir.path().join("memory-viewers/.github/README.md").exists());
    assert!(!dir.path().join("memory-viewers/.github").exists());
}

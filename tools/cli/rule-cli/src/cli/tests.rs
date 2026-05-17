use std::{
    fs,
    path::PathBuf,
};

use rule_api::{
    RuleFilter,
    RuleManifest,
    RuleStore,
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
fn delete_command_removes_rule_by_slug() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
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

    let reopened = RuleStore::open(dir.path()).unwrap();
    assert!(matches!(
        reopened.get("shared/agents/delete-me"),
        Err(rule_api::error::RuleError::NotFound(_))
    ));
}

#[test]
fn generate_file_writes_deterministic_markdown_with_provenance() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
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
    let mut store = RuleStore::open(dir.path()).unwrap();
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

    let mut store = RuleStore::open(dir.path()).unwrap();
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
            target_root: None,
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
    let mut store = RuleStore::open(dir.path()).unwrap();
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
fn generate_target_preserves_existing_crlf_output() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
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
    let mut store = RuleStore::open(dir.path()).unwrap();
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
    let mut store = RuleStore::open(dir.path()).unwrap();
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
        dir.path().join(".github").join("prompts").join("spec.prompt.md"),
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
fn add_root_command_creates_missing_directory() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let root = index_root.join("rules");

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
    let mut store = RuleStore::open(dir.path()).unwrap();
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

    let reopened = RuleStore::open(dir.path()).unwrap();
    let healthy_rule = reopened.get("shared/agents/healthy-rule").unwrap();
    assert_eq!(healthy_rule.feedback_helpful_count(), Some(1));
    assert!(reopened.entity_store().get_indexed(&stale_id).unwrap().is_none());
}

#[test]
fn generate_target_collects_rules_from_nested_workspaces() {
    let dir = tempdir().unwrap();
    let repo_root = dir.path().join("repo");
    let parent_index_root = repo_root.join(".rule");
    let child_workspace = repo_root.join("memory-viewers").join("memory-api");
    let child_index_root = child_workspace.join(".rule");
    fs::create_dir_all(&child_workspace).unwrap();

    let mut parent_store = RuleStore::open(&parent_index_root).unwrap();
    let mut parent_rule = sample_rule(
        "shared/agents/opening",
        "Opening",
        "opening",
        "Start with the concrete anchor.",
        10,
    );
    parent_rule.set_path_scopes(["AGENTS.md"]);
    parent_store.create(&parent_rule, None).unwrap();

    let mut child_store = RuleStore::open(&child_index_root).unwrap();
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

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: "combined-agents".to_string(),
            dry_run: false,
            check: false,
        }),
        &parent_index_root,
    )
    .unwrap();

    let rendered =
        fs::read_to_string(repo_root.join("generated").join("AGENTS.md"))
            .unwrap();
    assert!(rendered.contains("slug=shared/agents/opening"));
    assert!(rendered.contains("slug=memory-api/agents/overview"));
}

#[test]
fn sync_targets_prunes_removed_outputs_from_previous_sync() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();

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

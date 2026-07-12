use super::*;

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
    assert!(
        payload["output"]
            .as_str()
            .unwrap()
            .replace('\\', "/")
            .ends_with("/.github/copilot-instructions.md")
    );
    assert_eq!(payload["count"], 1);
    assert!(
        payload["content"]
            .as_str()
            .unwrap()
            .contains("slug=shared/copilot/rtk")
    );
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

    let rendered = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
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
    prompt.set_path_scopes([".agents/prompts/spec.prompt.md"]);
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
            "              path_scope: .agents/prompts/spec.prompt.md\n",
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
            .join(".agents")
            .join("prompts")
            .join("spec.prompt.md"),
    )
    .unwrap();
    assert!(rendered.starts_with("---\nname: spec\n"));
    assert!(rendered.contains("rule-api:file generated=true"));

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
fn sync_rules_round_trip_preserves_frontmatter_for_generated_target() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let mut rule = RuleManifest::new(
        "context-engine/agents/roast/roast-agent",
        "Roast Agent",
        "AGENTS",
        "roast-agent",
        "---\nname: Roast Agent\nuser-invocable: true\n---\nOriginal roast body.\n",
    );
    rule.set_repo_scopes(["context-engine"]);
    rule.set_path_scopes([".agents/agents/roast.agent.md"]);
    rule.set_order_key(10);
    let id = store.create(&rule, None).unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: roast-agent\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: .agents/agents/roast.agent.md\n",
            "    output_path: .agents/agents/roast.agent.md\n",
        ),
    )
    .unwrap();

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path.clone(),
            target: "roast-agent".to_string(),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    let output = dir.path().join(".agents").join("agents").join("roast.agent.md");
    let edited = fs::read_to_string(&output)
        .unwrap()
        .replace("Original roast body.", "Edited roast body from generated artifact.");
    fs::write(&output, edited).unwrap();

    dispatch::dispatch(
        RuleCommandCli::SyncRules(SyncRulesArgs {
            file: output.clone(),
            dry_run: false,
            check: false,
        }),
        dir.path(),
    )
    .unwrap();

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: config_path,
            target: "roast-agent".to_string(),
            dry_run: false,
            check: true,
        }),
        dir.path(),
    )
    .unwrap();

    let reopened = RuleStore::open(dir.path()).unwrap();
    let synced = reopened.get(&id.to_string()).unwrap();
    let body = synced.body().unwrap();
    assert!(body.starts_with("---\nname: Roast Agent\nuser-invocable: true\n---\n"));
    assert!(body.contains("Edited roast body from generated artifact."));
}

#[test]
fn repo_spec_prompt_target_matches_expectation_oriented_contract() {
    let prompt_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|path| {
            let candidate = path.join(".agents/prompts/spec.prompt.md");
            candidate.is_file().then_some(candidate)
        })
        .expect("context-engine prompt target path");
    let prompts_dir = prompt_path.parent().expect("prompt directory");
    assert_eq!(
        prompts_dir.file_name().and_then(|name| name.to_str()),
        Some("prompts")
    );
    let agents_dir = prompts_dir.parent().expect(".agents directory");
    assert_eq!(
        agents_dir.file_name().and_then(|name| name.to_str()),
        Some(".agents")
    );
    let repo_root = agents_dir
        .parent()
        .expect("context-engine repo root")
        .to_path_buf();
    let rendered = fs::read_to_string(&prompt_path).unwrap();

    assert!(rendered.contains("intended system properties"));
    assert!(rendered.contains("explicit acceptance criteria"));
    assert!(rendered.contains(
        "Keep problem statements, current-state analysis, rollout sequencing, blockers, and implementation notes in related tickets"
    ));
    assert!(
        !rendered.contains("captures motivation, intended behavior or scope")
    );

    dispatch::dispatch(
        RuleCommandCli::GenerateTarget(GenerateTargetArgs {
            config: repo_root.join("rule-targets.yaml"),
            target: "context-engine-prompt-spec".to_string(),
            dry_run: true,
            check: false,
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
fn move_command_dry_run_returns_supported_preflight_plan() {
    let dir = tempdir().unwrap();
    let repo_root = dir.path().join("repo");
    let source_index_root = repo_root.join(".rule");
    let target_workspace = repo_root.join("target");
    let target_index_root = target_workspace.join(".rule");
    fs::create_dir_all(&source_index_root).unwrap();
    fs::create_dir_all(&target_index_root).unwrap();
    run_git(&repo_root, &["init"]);

    let mut source_store = RuleStore::init(&source_index_root).unwrap();
    RuleStore::init(&target_index_root).unwrap();
    let manifest = sample_rule(
        "shared/tests/movable-rule",
        "Movable Rule",
        "tests",
        "body",
        10,
    );
    let rule_id = source_store.create(&manifest, None).unwrap();
    source_store.scan(true).unwrap();

    let payload = dispatch::dispatch(
        RuleCommandCli::Move(MoveArgs {
            id: Some(rule_id.to_string()),
            to_workspace_root: Some(target_workspace),
            dry_run: true,
            resume: None,
            rollback: None,
        }),
        &source_index_root,
    )
    .unwrap();

    assert_eq!(payload["command"], "move");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["mode"], "plan");
    assert_eq!(payload["supported"], true);
}

#[test]
fn sync_targets_emits_forward_slash_path_fields() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path().join("repo");
    fs::create_dir_all(&workspace_root).unwrap();

    let mut store = RuleStore::init(&workspace_root).unwrap();
    let mut rule = RuleManifest::new(
        "context-engine/agents/overview",
        "Overview",
        "AGENTS",
        "overview",
        "Document repository conventions.",
    );
    rule.set_repo_scopes(["context-engine"]);
    rule.set_path_scopes(["docs/AGENTS.md"]);
    rule.set_order_key(10);
    store.create(&rule, None).unwrap();
    drop(store);

    let config_path = workspace_root.join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: docs/AGENTS.md\n",
            "    output_path: docs/AGENTS.md\n",
        ),
    )
    .unwrap();

    let payload = dispatch::dispatch(
        RuleCommandCli::SyncTargets(SyncTargetsArgs {
            config: config_path,
            dry_run: false,
            check: false,
        }),
        &workspace_root,
    )
    .unwrap();

    let config_field = payload["config"].as_str().unwrap();
    assert!(
        !config_field.contains('\\'),
        "config path must use forward slashes: {config_field}"
    );

    let outputs = payload["generated"].as_array().unwrap();
    assert!(!outputs.is_empty());
    for entry in outputs {
        let output = entry["output"].as_str().unwrap();
        assert!(
            !output.contains('\\'),
            "generated output path must use forward slashes: {output}"
        );
    }

    for entry in payload["removed"].as_array().unwrap() {
        let output = entry["output"].as_str().unwrap();
        assert!(
            !output.contains('\\'),
            "removed output path must use forward slashes: {output}"
        );
    }
}

#[test]
fn sync_targets_reports_changed_flag_and_skips_unchanged_writes() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path().join("repo");
    fs::create_dir_all(&workspace_root).unwrap();

    let mut store = RuleStore::init(&workspace_root).unwrap();
    let mut rule = RuleManifest::new(
        "context-engine/agents/overview",
        "Overview",
        "AGENTS",
        "overview",
        "Document repository conventions.",
    );
    rule.set_repo_scopes(["context-engine"]);
    rule.set_path_scopes(["docs/AGENTS.md"]);
    rule.set_order_key(10);
    store.create(&rule, None).unwrap();
    drop(store);

    let config_path = workspace_root.join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: docs/AGENTS.md\n",
            "    output_path: docs/AGENTS.md\n",
        ),
    )
    .unwrap();

    let first = dispatch::dispatch(
        RuleCommandCli::SyncTargets(SyncTargetsArgs {
            config: config_path.clone(),
            dry_run: false,
            check: false,
        }),
        &workspace_root,
    )
    .unwrap();
    assert_eq!(first["generated"][0]["changed"], true);

    let output_path = workspace_root.join("docs").join("AGENTS.md");
    let mtime_after_first =
        fs::metadata(&output_path).unwrap().modified().unwrap();

    // Second sync with identical inputs must not rewrite the file.
    let second = dispatch::dispatch(
        RuleCommandCli::SyncTargets(SyncTargetsArgs {
            config: config_path,
            dry_run: false,
            check: false,
        }),
        &workspace_root,
    )
    .unwrap();
    assert_eq!(second["generated"][0]["changed"], false);

    let mtime_after_second =
        fs::metadata(&output_path).unwrap().modified().unwrap();
    assert_eq!(
        mtime_after_first, mtime_after_second,
        "unchanged target must not be rewritten"
    );
}


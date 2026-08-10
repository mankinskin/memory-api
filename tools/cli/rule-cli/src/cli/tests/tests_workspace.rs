use super::*;

#[test]
fn generate_target_bootstraps_nested_workspaces_automatically() {
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
fn generate_target_from_child_workspace_bootstraps_empty_local_index() {
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
    drop(child_store);

    fs::remove_file(child_index_root.join("entities.db")).unwrap();
    let _ = fs::remove_file(child_index_root.join("entities.db-shm"));
    let _ = fs::remove_file(child_index_root.join("entities.db-wal"));
    let _ = fs::remove_dir_all(child_index_root.join("search_index"));
    RuleStore::init(&child_index_root).unwrap();

    let config_path = child_workspace.join("rule-targets.yaml");
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
        &child_index_root,
    )
    .unwrap();

    let rendered = payload["content"].as_str().unwrap();
    assert!(rendered.contains("slug=shared/agents/opening"));
    assert!(rendered.contains("slug=memory-api/agents/overview"));
}

#[test]
fn sync_targets_prunes_removed_outputs_from_previous_sync() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(&dir.path().join(".rule")).unwrap();

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

#[test]
fn sync_targets_prunes_decoupled_hand_owned_outputs_without_deleting_them() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(&dir.path().join(".rule")).unwrap();

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
                "shared/agents/instruction",
                "Instruction",
                "instruction",
                "Instruction body.",
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
            "  - name: instruction\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    section: instruction\n",
            "    output_path: .agents/instructions/demo.instructions.md\n",
        ),
    )
    .unwrap();

    rendering::sync_targets_payload(&mut store, &config_path, false, false)
        .unwrap();
    let instruction_path =
        dir.path().join(".agents/instructions/demo.instructions.md");
    assert!(instruction_path.exists());

    // Migrate the generated file to a hand-owned file: strip the generated
    // marker so it no longer starts with `GENERATED_FILE_COMMENT`.
    let hand_owned = "---\ndescription: \"Hand-owned.\"\n---\n\nHand body.\n";
    fs::write(&instruction_path, hand_owned).unwrap();

    // Remove the instruction target from config, leaving it decoupled.
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

    // `--check` must not flag the decoupled hand-owned output.
    rendering::sync_targets_payload(&mut store, &config_path, false, true)
        .expect("check tolerates decoupled hand-owned outputs");

    // A real sync prunes the tracking record but keeps the hand-owned file.
    let payload =
        rendering::sync_targets_payload(&mut store, &config_path, false, false)
            .unwrap();
    assert_eq!(payload.generated.len(), 1);
    assert_eq!(payload.removed.len(), 1);
    assert_eq!(payload.removed[0]["decoupled"], serde_json::json!(true));
    assert!(instruction_path.exists());
    assert_eq!(fs::read_to_string(&instruction_path).unwrap(), hand_owned);

    // State is now clean: a follow-up sync reports nothing to prune.
    let payload =
        rendering::sync_targets_payload(&mut store, &config_path, false, false)
            .unwrap();
    assert!(payload.removed.is_empty());
}

#[test]
fn sync_targets_refuses_zero_match_writes_before_touching_outputs() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(&dir.path().join(".rule")).unwrap();

    let mut rule = sample_rule(
        "shared/agents/root-readme",
        "Root README",
        "root-readme",
        "Root body.",
        10,
    );
    rule.set_path_scopes(["AGENTS.md"]);
    store.create(&rule, None).unwrap();

    let config_path = dir.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: valid-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    section: root-readme\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: generated/AGENTS.md\n",
            "  - name: missing-agents\n",
            "    repo_scope: memory-api\n",
            "    file_kind: AGENTS\n",
            "    section: does-not-exist\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: generated/memory-api-AGENTS.md\n",
        ),
    )
    .unwrap();

    let err =
        rendering::sync_targets_payload(&mut store, &config_path, false, false)
            .unwrap_err();

    assert!(err.to_string().contains("matched zero rules"));
    assert!(!dir.path().join("generated/AGENTS.md").exists());
    assert!(!dir.path().join("generated/memory-api-AGENTS.md").exists());
}

#[test]
fn store_index_writes_catalog_then_check_is_clean_and_detects_drift() {
    let dir = empty_workspace().unwrap();
    let index_root = dir.path().join(".rule");
    let workspace_root = dir.path();

    let mut store = RuleStore::init(&index_root).unwrap();
    store
        .create(
            &sample_rule(
                "shared/agent-rules/opening",
                "Opening",
                "opening",
                "Start with the concrete anchor.",
                10,
            ),
            None,
        )
        .unwrap();
    store
        .create(
            &sample_rule(
                "shared/agent-rules/closing",
                "Closing",
                "closing",
                "Finish with the focused validation.",
                20,
            ),
            None,
        )
        .unwrap();

    // Write the catalog artifacts.
    let payload = dispatch::dispatch(
        RuleCommandCli::StoreIndex(StoreIndexArgs { check: false }),
        &index_root,
    )
    .unwrap();
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["rules"], 2);

    let readme = workspace_root.join(".rule/README.md");
    let sidecar = workspace_root.join(".rule/index.toon");
    let agent_hook = workspace_root.join(".agents/rules-catalog.md");
    assert!(readme.exists());
    assert!(sidecar.exists());
    assert!(agent_hook.exists());

    let readme_text = fs::read_to_string(&readme).unwrap();
    assert!(
        readme_text.starts_with("<!-- rule-catalog:file generated=true -->")
    );
    assert!(readme_text.contains("## shared/agent-rules"));
    assert!(readme_text.contains("<!-- rule-catalog:entry id="));
    assert!(readme_text.contains("digest="));

    // --check is clean immediately after a write (idempotent).
    let check = dispatch::dispatch(
        RuleCommandCli::StoreIndex(StoreIndexArgs { check: true }),
        &index_root,
    )
    .unwrap();
    assert_eq!(check["drift"], false);

    // Mutating a generated artifact makes --check fail (drift detected).
    fs::write(&readme, "tampered\n").unwrap();
    let drift = dispatch::dispatch(
        RuleCommandCli::StoreIndex(StoreIndexArgs { check: true }),
        &index_root,
    );
    assert!(drift.is_err(), "check must fail on drift");
    assert!(drift.unwrap_err().to_string().contains("out of date"));
}

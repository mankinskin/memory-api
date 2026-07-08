use super::*;

#[test]
fn load_render_target_config_parses_targets_and_rejects_duplicates() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &config_path,
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&config_path).unwrap();

    assert_eq!(config.targets.len(), 1);
    assert_eq!(config.targets[0].name, "context-engine-agents");
    assert_eq!(config.targets[0].path_scope.as_deref(), Some("AGENTS.md"));
    assert_eq!(config.targets[0].ordered_nodes().len(), 1);

    let path = tmp.path().join("duplicate-rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: dup\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: AGENTS.md\n",
            "  - name: dup\n",
            "    repo_scope: memory-api\n",
            "    file_kind: AGENTS\n",
            "    path_scope: memory-api/AGENTS.md\n",
            "    output_path: memory-api/AGENTS.md\n",
        ),
    )
    .unwrap();

    let err = load_render_target_config(&path).unwrap_err();
    assert!(
        matches!(err, TargetConfigError::DuplicateName(name) if name == "dup")
    );
}

#[test]
fn load_render_target_config_supports_file_folder_tree_structure() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "files:\n",
            "  - name: AGENTS.md\n",
            "    target:\n",
            "      name: context-engine-agents\n",
            "      repo_scope: context-engine\n",
            "      file_kind: AGENTS\n",
            "      path_scope: AGENTS.md\n",
            "folders:\n",
            "  - name: .github\n",
            "    files:\n",
            "      - name: copilot-instructions.md\n",
            "        target:\n",
            "          name: context-engine-copilot-instructions\n",
            "          repo_scope: context-engine\n",
            "          file_kind: copilot-instructions\n",
            "          path_scope: .github/copilot-instructions.md\n",
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

    let config = load_render_target_config(&path).unwrap();

    assert_eq!(config.targets.len(), 3);
    assert_eq!(config.targets[0].name, "context-engine-agents");
    assert_eq!(config.targets[0].output_path, "AGENTS.md");
    assert_eq!(
        config.targets[1].output_path,
        ".github/copilot-instructions.md"
    );
    assert_eq!(
        config.targets[2].output_path,
        ".agents/prompts/spec.prompt.md"
    );
    assert_eq!(config.targets[2].nodes.len(), 1);
    assert_eq!(config.targets[2].nodes[0].name, "spec-prompt");
}

#[test]
fn load_render_target_config_preserves_domain_tree_target_order() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "files:\n",
            "  - name: AGENTS.md\n",
            "    target:\n",
            "      name: context-engine-agents\n",
            "      repo_scope: context-engine\n",
            "      file_kind: AGENTS\n",
            "      path_scope: AGENTS.md\n",
            "folders:\n",
            "  - name: memory-viewers\n",
            "    files:\n",
            "      - name: AGENTS.md\n",
            "        target:\n",
            "          name: memory-viewers-agents\n",
            "          repo_scope: memory-viewers\n",
            "          file_kind: AGENTS\n",
            "          path_scope: memory-viewers/AGENTS.md\n",
            "    folders:\n",
            "      - name: memory-api\n",
            "        files:\n",
            "          - name: AGENTS.md\n",
            "            target:\n",
            "              name: memory-api-agents\n",
            "              repo_scope: memory-api\n",
            "              file_kind: AGENTS\n",
            "              path_scope: memory-api/AGENTS.md\n",
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
            "  - name: .agents\n",
            "    folders:\n",
            "      - name: instructions\n",
            "        files:\n",
            "          - name: audit.instructions.md\n",
            "            target:\n",
            "              name: context-engine-instruction-audit\n",
            "              repo_scope: context-engine\n",
            "              file_kind: .instructions\n",
            "              path_scope: .agents/instructions/audit.instructions.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    let names = config
        .targets
        .iter()
        .map(|target| target.name.as_str())
        .collect::<Vec<_>>();
    let outputs = config
        .targets
        .iter()
        .map(|target| target.output_path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "context-engine-agents",
            "memory-viewers-agents",
            "memory-api-agents",
            "context-engine-prompt-spec",
            "context-engine-instruction-audit",
        ]
    );
    assert_eq!(
        outputs,
        vec![
            "AGENTS.md",
            "memory-viewers/AGENTS.md",
            "memory-api/AGENTS.md",
            ".agents/prompts/spec.prompt.md",
            ".agents/instructions/audit.instructions.md",
        ]
    );
}

#[test]
fn load_render_target_config_rejects_duplicate_names_across_tree_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: dup\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
            "folders:\n",
            "  - name: .github\n",
            "    files:\n",
            "      - name: README.md\n",
            "        target:\n",
            "          name: dup\n",
            "          repo_scope: context-engine\n",
            "          file_kind: README\n",
        ),
    )
    .unwrap();

    let err = load_render_target_config(&path).unwrap_err();
    assert!(
        matches!(err, TargetConfigError::DuplicateName(name) if name == "dup")
    );
}

#[test]
fn load_render_target_config_imports_child_targets_with_source_config_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let child_dir = repo_root.join("memory-viewers");
    fs::create_dir_all(&child_dir).unwrap();

    let child_path = child_dir.join("rule-targets.yaml");
    fs::write(
        &child_path,
        concat!(
            "files:\n",
            "  - name: AGENTS.md\n",
            "    target:\n",
            "      name: memory-viewers-agents\n",
            "      repo_scope: memory-viewers\n",
            "      file_kind: AGENTS\n",
            "      path_scope: AGENTS.md\n",
        ),
    )
    .unwrap();

    let root_path = repo_root.join("rule-targets.yaml");
    fs::write(
        &root_path,
        concat!(
            "imports:\n",
            "  - memory-viewers/rule-targets.yaml\n",
            "files:\n",
            "  - name: AGENTS.md\n",
            "    target:\n",
            "      name: context-engine-agents\n",
            "      repo_scope: context-engine\n",
            "      file_kind: AGENTS\n",
            "      path_scope: AGENTS.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&root_path).unwrap();
    assert_eq!(config.targets.len(), 2);

    let imported =
        render_target_by_name(&config, "memory-viewers-agents").unwrap();
    assert_eq!(
        imported.source_config_path.as_deref(),
        Some(child_path.as_path())
    );
    assert_eq!(
        imported.source_output_root.as_deref(),
        Some(child_dir.as_path())
    );
    assert_eq!(
        resolve_render_target_output(&root_path, imported),
        child_dir.join("AGENTS.md")
    );

    let local =
        render_target_by_name(&config, "context-engine-agents").unwrap();
    assert_eq!(
        local.source_config_path.as_deref(),
        Some(root_path.as_path())
    );
    assert_eq!(
        local.source_output_root.as_deref(),
        Some(repo_root.as_path())
    );
    assert_eq!(
        resolve_render_target_output(&root_path, local),
        repo_root.join("AGENTS.md")
    );
}

#[test]
fn load_render_target_config_imports_directory_fragments_in_sorted_order() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let child_dir = repo_root.join("memory-viewers");
    let child_targets_dir = child_dir.join("rule-targets");
    fs::create_dir_all(&child_targets_dir).unwrap();

    fs::write(
        child_targets_dir.join("20-agents.yaml"),
        concat!(
            "targets:\n",
            "  - name: memory-viewers-agents\n",
            "    repo_scope: memory-viewers\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();
    fs::write(
        child_targets_dir.join("10-readme.yaml"),
        concat!(
            "targets:\n",
            "  - name: memory-viewers-readme\n",
            "    repo_scope: memory-viewers\n",
            "    file_kind: README\n",
            "    output_path: README.md\n",
        ),
    )
    .unwrap();
    fs::write(child_targets_dir.join("notes.txt"), "ignore me\n").unwrap();

    let root_path = repo_root.join("rule-targets.yaml");
    fs::write(
        &root_path,
        concat!(
            "imports:\n",
            "  - memory-viewers/rule-targets\n",
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&root_path).unwrap();
    let names = config
        .targets
        .iter()
        .map(|target| target.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "memory-viewers-readme",
            "memory-viewers-agents",
            "context-engine-agents",
        ]
    );

    let readme =
        render_target_by_name(&config, "memory-viewers-readme").unwrap();
    assert_eq!(
        readme.source_config_path.as_deref(),
        Some(child_targets_dir.join("10-readme.yaml").as_path())
    );
    assert_eq!(
        readme.source_output_root.as_deref(),
        Some(child_dir.as_path())
    );
    assert_eq!(
        resolve_render_target_output(&root_path, readme),
        child_dir.join("README.md")
    );

    let agents =
        render_target_by_name(&config, "memory-viewers-agents").unwrap();
    assert_eq!(
        agents.source_config_path.as_deref(),
        Some(child_targets_dir.join("20-agents.yaml").as_path())
    );
    assert_eq!(
        agents.source_output_root.as_deref(),
        Some(child_dir.as_path())
    );
    assert_eq!(
        resolve_render_target_output(&root_path, agents),
        child_dir.join("AGENTS.md")
    );
}

#[test]
fn load_render_target_config_accepts_top_level_directory_configs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let targets_dir = repo_root.join("rule-targets");
    fs::create_dir_all(&targets_dir).unwrap();

    fs::write(
        targets_dir.join("20-agents.yaml"),
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();
    fs::write(
        targets_dir.join("10-readme.yaml"),
        concat!(
            "targets:\n",
            "  - name: context-engine-readme\n",
            "    repo_scope: context-engine\n",
            "    file_kind: README\n",
            "    output_path: README.md\n",
        ),
    )
    .unwrap();
    fs::write(targets_dir.join("notes.txt"), "ignore me\n").unwrap();

    let config = load_render_target_config(&targets_dir).unwrap();
    let names = config
        .targets
        .iter()
        .map(|target| target.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec!["context-engine-readme", "context-engine-agents",]
    );

    let readme =
        render_target_by_name(&config, "context-engine-readme").unwrap();
    assert_eq!(
        readme.source_config_path.as_deref(),
        Some(targets_dir.join("10-readme.yaml").as_path())
    );
    assert_eq!(
        readme.source_output_root.as_deref(),
        Some(repo_root.as_path())
    );
    assert_eq!(
        resolve_render_target_output(&targets_dir, readme),
        repo_root.join("README.md")
    );

    let agents =
        render_target_by_name(&config, "context-engine-agents").unwrap();
    assert_eq!(
        agents.source_config_path.as_deref(),
        Some(targets_dir.join("20-agents.yaml").as_path())
    );
    assert_eq!(
        agents.source_output_root.as_deref(),
        Some(repo_root.as_path())
    );
    assert_eq!(
        resolve_render_target_output(&targets_dir, agents),
        repo_root.join("AGENTS.md")
    );
}

#[test]
fn load_render_target_config_rejects_duplicate_names_across_imports() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let child_dir = repo_root.join("memory-viewers");
    fs::create_dir_all(&child_dir).unwrap();

    fs::write(
        child_dir.join("rule-targets.yaml"),
        concat!(
            "targets:\n",
            "  - name: dup\n",
            "    repo_scope: memory-viewers\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let root_path = repo_root.join("rule-targets.yaml");
    fs::write(
        &root_path,
        concat!(
            "imports:\n",
            "  - memory-viewers/rule-targets.yaml\n",
            "targets:\n",
            "  - name: dup\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let err = load_render_target_config(&root_path).unwrap_err();
    assert!(
        matches!(err, TargetConfigError::DuplicateName(name) if name == "dup")
    );
}

#[test]
fn load_render_target_config_rejects_import_cycles() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let child_dir = repo_root.join("memory-viewers");
    fs::create_dir_all(&child_dir).unwrap();

    let root_path = repo_root.join("rule-targets.yaml");
    let child_path = child_dir.join("rule-targets.yaml");
    fs::write(
        &root_path,
        concat!("imports:\n", "  - memory-viewers/rule-targets.yaml\n",),
    )
    .unwrap();
    fs::write(
        &child_path,
        concat!("imports:\n", "  - ../rule-targets.yaml\n",),
    )
    .unwrap();

    let err = load_render_target_config(&root_path).unwrap_err();
    assert!(matches!(err, TargetConfigError::ImportCycle { .. }));
}

#[test]
fn load_render_target_config_supports_legacy_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.toml");
    fs::write(
        &path,
        r#"
            [[targets]]
            name = "context-engine-agents"
            repo_scope = "context-engine"
            file_kind = "AGENTS"
            path_scope = "AGENTS.md"
            output_path = "AGENTS.md"

            [[targets.nodes]]
            name = "agent-rules"
            title = "Agent Rules"
            section = "agent-rules"

            [[targets.nodes.nodes]]
            name = "operating-principles"
            title = "Operating Principles"
            section = "agent-rules/operating-principles"

            [[targets.nodes.nodes]]
            name = "task-routing"
            title = "Task Routing"
            section = "agent-rules/task-routing"
        "#,
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    let target = &config.targets[0];

    assert_eq!(target.name, "context-engine-agents");
    assert_eq!(target.nodes.len(), 1);
    assert_eq!(target.nodes[0].name, "agent-rules");
    assert_eq!(target.nodes[0].nodes.len(), 2);
    assert_eq!(target.nodes[0].nodes[0].name, "operating-principles");
    assert_eq!(target.nodes[0].nodes[1].name, "task-routing");
}

#[test]
fn load_render_target_config_parses_hierarchical_outline_nodes_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("hierarchical-rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
            "    nodes:\n",
            "      - name: opening\n",
            "        title: Opening\n",
            "        section: opening\n",
            "        nodes:\n",
            "          - name: validation\n",
            "            title: Validation\n",
            "            section: opening/validation\n",
            "      - name: quality-gates\n",
            "        title: Quality Gates\n",
            "        section: quality-gates\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();

    let target = &config.targets[0];
    let nodes = target.ordered_nodes();

    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].name, "opening");
    assert_eq!(nodes[1].name, "quality-gates");
    assert_eq!(nodes[0].nodes.len(), 1);
    assert_eq!(nodes[0].nodes[0].name, "validation");

    let inherited = target.flat_filter();
    assert_eq!(
        nodes[0].effective_filter(&inherited).repo_scope.as_deref(),
        Some("context-engine")
    );
    assert_eq!(
        nodes[0].effective_filter(&inherited).file_kind.as_deref(),
        Some("AGENTS")
    );
    assert_eq!(
        nodes[0].effective_filter(&inherited).section.as_deref(),
        Some("opening")
    );
    assert_eq!(
        nodes[0].nodes[0]
            .effective_filter(&nodes[0].effective_filter(&inherited))
            .section
            .as_deref(),
        Some("opening/validation")
    );
}

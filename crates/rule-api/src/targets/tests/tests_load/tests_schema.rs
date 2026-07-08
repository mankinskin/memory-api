use super::*;

#[test]
fn readme_schema_inherits_shared_outline_for_multiple_targets() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "schemas:\n",
            "  - name: repository-readme-v1\n",
            "    nodes:\n",
            "      - name: summary\n",
            "        title: Summary\n",
            "      - name: installable-content\n",
            "        title: Installable Content\n",
            "      - name: command-docs\n",
            "        title: Command Docs\n",
            "targets:\n",
            "  - name: memory-api-readme\n",
            "    repo_scope: memory-api\n",
            "    file_kind: README\n",
            "    output_path: README.md\n",
            "    schema: repository-readme-v1\n",
            "  - name: viewer-api-readme\n",
            "    repo_scope: viewer-api\n",
            "    file_kind: README\n",
            "    output_path: README.md\n",
            "    schema: repository-readme-v1\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    let memory_api =
        render_target_by_name(&config, "memory-api-readme").unwrap();
    let viewer_api =
        render_target_by_name(&config, "viewer-api-readme").unwrap();
    let expected = vec![
        "summary".to_string(),
        "installable-content".to_string(),
        "command-docs".to_string(),
    ];

    assert_eq!(target_node_names(memory_api), expected);
    assert_eq!(target_node_names(viewer_api), expected);
}

#[test]
fn readme_schema_appends_explicit_nodes_without_redeclaring_outline() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "schemas:\n",
            "  - name: repository-readme-v1\n",
            "    nodes:\n",
            "      - name: summary\n",
            "        title: Summary\n",
            "      - name: installable-content\n",
            "        title: Installable Content\n",
            "      - name: child-readmes\n",
            "        title: Child READMEs\n",
            "      - name: command-docs\n",
            "        title: Command Docs\n",
            "targets:\n",
            "  - name: memory-viewers-readme\n",
            "    repo_scope: memory-viewers\n",
            "    file_kind: README\n",
            "    output_path: README.md\n",
            "    schema: repository-readme-v1\n",
            "    node_mode: append\n",
            "    nodes:\n",
            "      - name: screenshots\n",
            "        title: Screenshots\n",
            "        section: screenshots\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    let target =
        render_target_by_name(&config, "memory-viewers-readme").unwrap();

    assert_eq!(
        target_node_names(target),
        vec![
            "summary".to_string(),
            "installable-content".to_string(),
            "child-readmes".to_string(),
            "command-docs".to_string(),
            "screenshots".to_string(),
        ]
    );
}

#[test]
fn readme_schema_rejects_child_targets_missing_required_parent_block() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "schemas:\n",
            "  - name: repository-readme-v1\n",
            "    required_blocks:\n",
            "      child:\n",
            "        - parent-readme\n",
            "        - command-docs\n",
            "targets:\n",
            "  - name: rule-cli-readme\n",
            "    repo_scope: memory-api\n",
            "    file_kind: README\n",
            "    output_path: tools/cli/rule-cli/README.md\n",
            "    schema: repository-readme-v1\n",
            "    target_kind: child\n",
            "    nodes:\n",
            "      - name: summary\n",
            "        title: Summary\n",
            "      - name: command-docs\n",
            "        title: Command Docs\n",
        ),
    )
    .unwrap();

    load_render_target_config(&path).expect_err(
        "child README targets should fail when the shared schema requires a parent-readme block",
    );
}

#[test]
fn load_render_target_config_allows_identical_schema_imports_across_fragments()
{
    let tmp = tempdir().unwrap();
    let shared = tmp.path().join("shared-schema.yaml");
    fs::write(
        &shared,
        concat!(
            "schemas:\n",
            "  - name: repository-readme-v1\n",
            "    nodes:\n",
            "      - name: summary\n",
            "        title: Summary\n",
        ),
    )
    .unwrap();

    let config_dir = tmp.path().join("rule-targets");
    fs::create_dir(&config_dir).unwrap();
    fs::write(
        config_dir.join("10-root.yaml"),
        concat!(
            "imports:\n",
            "- ../shared-schema.yaml\n",
            "targets:\n",
            "  - name: root-readme\n",
            "    repo_scope: memory-api\n",
            "    file_kind: README\n",
            "    output_path: README.md\n",
            "    schema: repository-readme-v1\n",
        ),
    )
    .unwrap();
    fs::write(
        config_dir.join("20-child.yaml"),
        concat!(
            "imports:\n",
            "- ../shared-schema.yaml\n",
            "targets:\n",
            "  - name: child-readme\n",
            "    repo_scope: memory-api\n",
            "    file_kind: README\n",
            "    output_path: tools/cli/rule-cli/README.md\n",
            "    schema: repository-readme-v1\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&config_dir).unwrap();

    assert!(render_target_by_name(&config, "root-readme").is_ok());
    assert!(render_target_by_name(&config, "child-readme").is_ok());
}

#[test]
fn resolve_render_target_output_uses_config_parent_for_relative_paths() {
    let config_path = PathBuf::from("repo/rule-targets.yaml");
    let target = RenderTarget {
        name: "context-engine-agents".to_string(),
        repo_scope: "context-engine".to_string(),
        file_kind: "AGENTS".to_string(),
        path_scope: Some("AGENTS.md".to_string()),
        section: None,
        state: None,
        nodes: Vec::new(),
        output_path: ".github/generated/AGENTS.md".to_string(),
        source_config_path: None,
        source_output_root: None,
    };

    assert_eq!(
        resolve_render_target_output(&config_path, &target),
        PathBuf::from("repo/.github/generated/AGENTS.md")
    );
}

#[test]
fn resolve_render_target_output_uses_rule_targets_directory_parent() {
    let repo_root = PathBuf::from("repo");
    let target = RenderTarget {
        name: "context-engine-agents".to_string(),
        repo_scope: "context-engine".to_string(),
        file_kind: "AGENTS".to_string(),
        path_scope: Some("AGENTS.md".to_string()),
        section: None,
        state: None,
        nodes: Vec::new(),
        output_path: "AGENTS.md".to_string(),
        source_config_path: Some(repo_root.join("rule-targets/20-agents.yaml")),
        source_output_root: Some(repo_root.clone()),
    };

    assert_eq!(
        resolve_render_target_output(
            PathBuf::from("repo/rule-targets.yaml").as_path(),
            &target
        ),
        repo_root.join("AGENTS.md")
    );
}

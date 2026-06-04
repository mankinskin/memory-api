use std::{
    fs,
    path::PathBuf,
};

use tempfile::tempdir;

use super::*;
use crate::{
    manifest::RuleManifest,
    store::RuleStore,
};

fn target_node_names(target: &RenderTarget) -> Vec<String> {
    target
        .ordered_nodes()
        .into_iter()
        .map(|node| node.name)
        .collect()
}

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
            "              path_scope: memory-viewers/memory-api/AGENTS.md\n",
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
            "memory-viewers/memory-api/AGENTS.md",
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

    let imported = render_target_by_name(&config, "memory-viewers-agents")
        .unwrap();
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

    let local = render_target_by_name(&config, "context-engine-agents")
        .unwrap();
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

    let readme = render_target_by_name(&config, "memory-viewers-readme")
        .unwrap();
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

    let agents = render_target_by_name(&config, "memory-viewers-agents")
        .unwrap();
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

    let readme = render_target_by_name(&config, "context-engine-readme")
        .unwrap();
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

    let agents = render_target_by_name(&config, "context-engine-agents")
        .unwrap();
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
        concat!(
            "imports:\n",
            "  - memory-viewers/rule-targets.yaml\n",
        ),
    )
    .unwrap();
    fs::write(
        &child_path,
        concat!(
            "imports:\n",
            "  - ../rule-targets.yaml\n",
        ),
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
    let memory_api = render_target_by_name(&config, "memory-api-readme").unwrap();
    let viewer_api = render_target_by_name(&config, "viewer-api-readme").unwrap();
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
    let target = render_target_by_name(&config, "memory-viewers-readme").unwrap();

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
fn load_render_target_config_allows_identical_schema_imports_across_fragments() {
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
        resolve_render_target_output(PathBuf::from("repo/rule-targets.yaml").as_path(), &target),
        repo_root.join("AGENTS.md")
    );
}

#[test]
fn collect_target_rules_traverses_nodes_in_outline_order() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();

    let mut opening = RuleManifest::new(
        "shared/agents/opening",
        "Opening",
        "AGENTS",
        "opening",
        "Start with the concrete anchor.",
    );
    opening.set_repo_scopes(["context-engine"]);
    opening.set_path_scopes(["AGENTS.md"]);
    opening.set_order_key(20);

    let mut validation = RuleManifest::new(
        "shared/agents/validation",
        "Validation",
        "AGENTS",
        "opening/validation",
        "Run the focused check next.",
    );
    validation.set_repo_scopes(["context-engine"]);
    validation.set_path_scopes(["AGENTS.md"]);
    validation.set_order_key(10);

    let mut quality_gates = RuleManifest::new(
        "shared/agents/quality-gates",
        "Quality Gates",
        "AGENTS",
        "quality-gates",
        "Run relevant tests before completion.",
    );
    quality_gates.set_repo_scopes(["context-engine"]);
    quality_gates.set_path_scopes(["AGENTS.md"]);
    quality_gates.set_order_key(5);

    store.create(&opening, None).unwrap();
    store.create(&validation, None).unwrap();
    store.create(&quality_gates, None).unwrap();

    let target = RenderTarget {
        name: "context-engine-agents".to_string(),
        repo_scope: "context-engine".to_string(),
        file_kind: "AGENTS".to_string(),
        path_scope: Some("AGENTS.md".to_string()),
        section: None,
        state: None,
        nodes: vec![
            RenderTargetNode {
                name: "opening".to_string(),
                title: Some("Opening".to_string()),
                repo_scope: None,
                file_kind: None,
                path_scope: None,
                section: Some("opening".to_string()),
                state: None,
                nodes: vec![RenderTargetNode {
                    name: "validation".to_string(),
                    title: Some("Validation".to_string()),
                    repo_scope: None,
                    file_kind: None,
                    path_scope: None,
                    section: Some("opening/validation".to_string()),
                    state: None,
                    nodes: Vec::new(),
                }],
            },
            RenderTargetNode {
                name: "quality-gates".to_string(),
                title: Some("Quality Gates".to_string()),
                repo_scope: None,
                file_kind: None,
                path_scope: None,
                section: Some("quality-gates".to_string()),
                state: None,
                nodes: Vec::new(),
            },
        ],
        output_path: "AGENTS.md".to_string(),
        source_config_path: None,
        source_output_root: None,
    };

    let rules = collect_target_rules(&store, &target).unwrap();
    let slugs = rules
        .iter()
        .map(|rule| rule.slug().unwrap().to_string())
        .collect::<Vec<_>>();

    assert_eq!(
        slugs,
        vec![
            "shared/agents/opening".to_string(),
            "shared/agents/validation".to_string(),
            "shared/agents/quality-gates".to_string(),
        ]
    );
}

#[test]
fn collect_target_rules_rejects_duplicate_matches_across_nodes() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();

    let mut opening = RuleManifest::new(
        "shared/agents/opening",
        "Opening",
        "AGENTS",
        "opening",
        "Start with the concrete anchor.",
    );
    opening.set_repo_scopes(["context-engine"]);
    opening.set_path_scopes(["AGENTS.md"]);
    store.create(&opening, None).unwrap();

    let target = RenderTarget {
        name: "context-engine-agents".to_string(),
        repo_scope: "context-engine".to_string(),
        file_kind: "AGENTS".to_string(),
        path_scope: Some("AGENTS.md".to_string()),
        section: None,
        state: None,
        nodes: vec![
            RenderTargetNode {
                name: "first".to_string(),
                title: None,
                repo_scope: None,
                file_kind: None,
                path_scope: None,
                section: Some("opening".to_string()),
                state: None,
                nodes: Vec::new(),
            },
            RenderTargetNode {
                name: "second".to_string(),
                title: None,
                repo_scope: None,
                file_kind: None,
                path_scope: None,
                section: Some("opening".to_string()),
                state: None,
                nodes: Vec::new(),
            },
        ],
        output_path: "AGENTS.md".to_string(),
        source_config_path: None,
        source_output_root: None,
    };

    let err = collect_target_rules(&store, &target).unwrap_err();
    assert!(matches!(
        err,
        RuleError::DuplicateRenderRule { target, node, slug }
        if target == "context-engine-agents" && node == "second" && slug == "shared/agents/opening"
    ));
}

#[test]
fn explain_target_reports_node_matches_with_effective_filters() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();

    let mut opening = RuleManifest::new(
        "shared/agents/opening",
        "Opening",
        "AGENTS",
        "agent-rules/operating-principles",
        "Gather context before coding.",
    );
    opening.set_repo_scopes(["context-engine"]);
    opening.set_path_scopes(["AGENTS.md"]);
    opening.set_order_key(10);
    store.create(&opening, None).unwrap();

    let target = RenderTarget {
        name: "context-engine-agents".to_string(),
        repo_scope: "context-engine".to_string(),
        file_kind: "AGENTS".to_string(),
        path_scope: Some("AGENTS.md".to_string()),
        section: None,
        state: None,
        nodes: vec![RenderTargetNode {
            name: "agent-rules".to_string(),
            title: Some("Agent Rules".to_string()),
            repo_scope: None,
            file_kind: None,
            path_scope: None,
            section: Some("agent-rules".to_string()),
            state: None,
            nodes: vec![RenderTargetNode {
                name: "operating-principles".to_string(),
                title: Some("Operating Principles".to_string()),
                repo_scope: None,
                file_kind: None,
                path_scope: None,
                section: Some("agent-rules/operating-principles".to_string()),
                state: None,
                nodes: Vec::new(),
            }],
        }],
        output_path: "AGENTS.md".to_string(),
        source_config_path: None,
        source_output_root: None,
    };

    let explained = explain_target(&store, &target).unwrap();

    assert_eq!(explained.name, "context-engine-agents");
    assert_eq!(explained.matched_rule_count, 1);
    assert_eq!(explained.nodes.len(), 1);
    assert_eq!(explained.nodes[0].nodes.len(), 1);
    assert_eq!(
        explained.nodes[0].nodes[0]
            .effective_filter
            .section
            .as_deref(),
        Some("agent-rules/operating-principles")
    );
    assert_eq!(explained.nodes[0].nodes[0].matched_rules.len(), 1);
    assert_eq!(
        explained.nodes[0].nodes[0].matched_rules[0].slug,
        "shared/agents/opening"
    );
}

// ── infer_file_kind ──────────────────────────────────────────────────────────

#[test]
fn infer_file_kind_recognises_well_known_filenames() {
    assert_eq!(infer_file_kind("AGENTS.md"), Some("AGENTS"));
    assert_eq!(infer_file_kind("README.md"), Some("README"));
    assert_eq!(
        infer_file_kind("copilot-instructions.md"),
        Some("copilot-instructions")
    );
    assert_eq!(infer_file_kind(".agents/agents/interview.agent.md"), Some(".agent"));
    assert_eq!(infer_file_kind(".agents/prompts/spec.prompt.md"), Some(".prompt"));
    assert_eq!(
        infer_file_kind(".agents/instructions/audit.instructions.md"),
        Some(".instructions")
    );
    assert_eq!(infer_file_kind(".spec/specs/uuid/body.md"), Some("spec-doc"));
    assert_eq!(infer_file_kind("some/unknown/file.md"), None);
}

// ── parse_scope ───────────────────────────────────────────────────────────────

#[test]
fn parse_scope_splits_repo_and_path() {
    let (repo, path) =
        parse_scope("t", "context-engine:AGENTS.md").unwrap();
    assert_eq!(repo, "context-engine");
    assert_eq!(path, "AGENTS.md");
}

#[test]
fn parse_scope_rejects_missing_separator() {
    let err = parse_scope("t", "no-colon-here").unwrap_err();
    assert!(matches!(err, TargetConfigError::InvalidScope { .. }));
}

#[test]
fn parse_scope_rejects_empty_repo() {
    let err = parse_scope("t", ":AGENTS.md").unwrap_err();
    assert!(matches!(err, TargetConfigError::InvalidScope { .. }));
}

#[test]
fn parse_scope_rejects_empty_path() {
    let err = parse_scope("t", "context-engine:").unwrap_err();
    assert!(matches!(err, TargetConfigError::InvalidScope { .. }));
}

// ── defaults block ────────────────────────────────────────────────────────────

#[test]
fn defaults_block_fills_repo_scope_and_file_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "defaults:\n",
            "  repo_scope: context-engine\n",
            "  file_kind: .agent\n",
            "targets:\n",
            "  - name: agent-interview\n",
            "    path_scope: .agents/agents/interview.agent.md\n",
            "    output_path: .agents/agents/interview.agent.md\n",
            "  - name: agent-implement\n",
            "    path_scope: .agents/agents/implement.agent.md\n",
            "    output_path: .agents/agents/implement.agent.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    assert_eq!(config.targets.len(), 2);

    for target in &config.targets {
        assert_eq!(target.repo_scope, "context-engine");
        assert_eq!(target.file_kind, ".agent");
    }
    assert_eq!(
        config.targets[0].path_scope.as_deref(),
        Some(".agents/agents/interview.agent.md")
    );
    assert_eq!(
        config.targets[1].path_scope.as_deref(),
        Some(".agents/agents/implement.agent.md")
    );
}

#[test]
fn target_level_fields_override_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "defaults:\n",
            "  repo_scope: context-engine\n",
            "  file_kind: .agent\n",
            "targets:\n",
            "  - name: special-agents\n",
            "    repo_scope: memory-api\n",
            "    file_kind: AGENTS\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    assert_eq!(config.targets.len(), 1);
    assert_eq!(config.targets[0].repo_scope, "memory-api");
    assert_eq!(config.targets[0].file_kind, "AGENTS");
}

#[test]
fn defaults_missing_required_fields_still_produce_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "defaults:\n",
            "  file_kind: .agent\n",
            "targets:\n",
            "  - name: no-repo\n",
            "    path_scope: AGENTS.md\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let err = load_render_target_config(&path).unwrap_err();
    assert!(
        matches!(err, TargetConfigError::MissingRepoScope { ref target } if target == "no-repo")
    );
}

// ── scope shorthand ───────────────────────────────────────────────────────────

#[test]
fn scope_shorthand_expands_repo_path_and_infers_file_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: context-engine-agents\n",
            "    scope: context-engine:AGENTS.md\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    let t = &config.targets[0];
    assert_eq!(t.repo_scope, "context-engine");
    assert_eq!(t.file_kind, "AGENTS");
    assert_eq!(t.path_scope.as_deref(), Some("AGENTS.md"));
}

#[test]
fn scope_shorthand_infers_file_kind_for_agent_files() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: interview-agent\n",
            "    scope: context-engine:.agents/agents/interview.agent.md\n",
            "    output_path: .agents/agents/interview.agent.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    let t = &config.targets[0];
    assert_eq!(t.file_kind, ".agent");
    assert_eq!(t.repo_scope, "context-engine");
    assert_eq!(
        t.path_scope.as_deref(),
        Some(".agents/agents/interview.agent.md")
    );
}

#[test]
fn scope_shorthand_output_path_defaults_to_path_scope() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    // No output_path — should fall back to path_scope from scope
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: interview-agent\n",
            "    scope: context-engine:.agents/agents/interview.agent.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    assert_eq!(
        config.targets[0].output_path,
        ".agents/agents/interview.agent.md"
    );
}

#[test]
fn scope_plus_defaults_compose_correctly() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    // defaults provides repo_scope; scope provides only path (no leading repo:)
    // Actually scope must always have repo: so use defaults for repo and scope for path + kind
    fs::write(
        &path,
        concat!(
            "defaults:\n",
            "  repo_scope: context-engine\n",
            "targets:\n",
            "  - name: audit-instructions\n",
            // scope overrides repo_scope (explicit wins over default)
            "    scope: context-engine:.agents/instructions/audit.instructions.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    let t = &config.targets[0];
    assert_eq!(t.repo_scope, "context-engine");
    assert_eq!(t.file_kind, ".instructions");
    assert_eq!(
        t.path_scope.as_deref(),
        Some(".agents/instructions/audit.instructions.md")
    );
    assert_eq!(
        t.output_path,
        ".agents/instructions/audit.instructions.md"
    );
}

#[test]
fn explicit_file_kind_overrides_scope_inferred_kind() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: special\n",
            "    scope: context-engine:AGENTS.md\n",
            "    file_kind: spec-doc\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let config = load_render_target_config(&path).unwrap();
    // explicit file_kind wins over inferred "AGENTS"
    assert_eq!(config.targets[0].file_kind, "spec-doc");
}

#[test]
fn scope_shorthand_invalid_format_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: bad\n",
            "    scope: no-colon-here\n",
            "    output_path: AGENTS.md\n",
        ),
    )
    .unwrap();

    let err = load_render_target_config(&path).unwrap_err();
    assert!(matches!(err, TargetConfigError::InvalidScope { .. }));
}

#[test]
fn missing_output_path_without_scope_returns_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("rule-targets.yaml");
    fs::write(
        &path,
        concat!(
            "targets:\n",
            "  - name: no-output\n",
            "    repo_scope: context-engine\n",
            "    file_kind: AGENTS\n",
        ),
    )
    .unwrap();

    let err = load_render_target_config(&path).unwrap_err();
    assert!(
        matches!(err, TargetConfigError::MissingOutputPath { ref target } if target == "no-output")
    );
}

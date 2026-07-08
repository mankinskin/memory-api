use super::*;

#[test]
fn infer_file_kind_recognises_well_known_filenames() {
    assert_eq!(infer_file_kind("AGENTS.md"), Some("AGENTS"));
    assert_eq!(infer_file_kind("README.md"), Some("README"));
    assert_eq!(
        infer_file_kind("copilot-instructions.md"),
        Some("copilot-instructions")
    );
    assert_eq!(
        infer_file_kind(".agents/agents/interview.agent.md"),
        Some(".agent")
    );
    assert_eq!(
        infer_file_kind(".agents/prompts/spec.prompt.md"),
        Some(".prompt")
    );
    assert_eq!(
        infer_file_kind(".agents/instructions/audit.instructions.md"),
        Some(".instructions")
    );
    assert_eq!(
        infer_file_kind(".spec/specs/uuid/body.md"),
        Some("spec-doc")
    );
    assert_eq!(infer_file_kind("some/unknown/file.md"), None);
}

// ── parse_scope ───────────────────────────────────────────────────────────────

#[test]
fn parse_scope_splits_repo_and_path() {
    let (repo, path) = parse_scope("t", "context-engine:AGENTS.md").unwrap();
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
    assert_eq!(t.output_path, ".agents/instructions/audit.instructions.md");
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

use super::*;

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


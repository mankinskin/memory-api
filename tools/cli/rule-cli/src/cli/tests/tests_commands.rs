use super::*;

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
    assert!(
        payload["integrated_description"]
            .as_str()
            .unwrap()
            .contains("integrated")
    );
    assert!(payload["pruned"].is_number());
    assert_eq!(payload["pruned"], payload["pruned_entities"]);
    assert!(
        payload["pruned_description"]
            .as_str()
            .unwrap()
            .contains("reindex")
    );
    assert_eq!(payload["diagnostics_count"], 1);
    assert!(
        payload["diagnostics_description"]
            .as_str()
            .unwrap()
            .contains("path")
    );
    assert_eq!(payload["diagnostics"].as_array().unwrap().len(), 1);
    assert!(
        payload["diagnostics"][0]["path"]
            .as_str()
            .unwrap()
            .replace('\\', "/")
            .ends_with("/rule.toml")
    );
    assert!(
        !payload["diagnostics"][0]["reason"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert!(payload["scan_root_count"].as_u64().unwrap() >= 1);
    assert!(
        payload["active_scan_roots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|root| root["kind"] == "default")
    );
    assert!(
        payload["active_scan_roots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|root| root["kind"] == "registered")
    );
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
fn parse_sync_rules_command() {
    let cli = parse_cli_from([
        "rule",
        "sync-rules",
        "--file",
        ".agents/agents/roast.agent.md",
        "--dry-run",
    ])
    .unwrap();

    match cli.command {
        RuleCommandCli::SyncRules(args) => {
            assert_eq!(
                args.file,
                PathBuf::from(".agents/agents/roast.agent.md")
            );
            assert!(args.dry_run);
            assert!(!args.check);
        },
        _ => panic!("expected sync-rules command"),
    }
}

#[test]
fn parse_missing_rule_command() {
    let cli = parse_cli_from([
        "rule",
        "missing-rule",
        "query without coverage",
        "--context-tag",
        "policy",
        "--context-tag",
        "session",
        "--workspace-slug",
        "default",
    ])
    .unwrap();

    match cli.command {
        RuleCommandCli::MissingRule(args) => {
            assert_eq!(args.query, "query without coverage");
            assert_eq!(args.context_tags, vec!["policy", "session"]);
            assert_eq!(args.workspace_slug, "default");
            assert!(!args.has_matching_rule);
        },
        _ => panic!("expected missing-rule command"),
    }
}

#[test]
fn missing_rule_command_relays_signal_to_ticket_and_feedback_stores() {
    let dir = tempdir().unwrap();
    let workspace_root = dir.path().join("workspace");
    let rule_index_root = workspace_root.join(".rule");
    RuleStore::init(&rule_index_root).unwrap();

    let payload = dispatch::dispatch(
        RuleCommandCli::MissingRule(MissingRuleArgs {
            query: "rule coverage gap".to_string(),
            context_tags: vec!["policy".to_string()],
            workspace_slug: "default".to_string(),
            has_matching_rule: false,
        }),
        &rule_index_root,
    )
    .unwrap();

    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["edge"], "missing-rule");
    assert_eq!(payload["signal_emitted"], true);
    assert_eq!(payload["ticket_created"], true);

    let ticket_id = payload["ticket_id"].as_str().unwrap();
    let ticket_store_root =
        memory_api::workspace::resolve_store_root_from(&workspace_root, ".ticket");
    let ticket_store =
        ticket_api::storage::TicketStore::open_or_init(&ticket_store_root).unwrap();
    let parsed_id = uuid::Uuid::parse_str(ticket_id).unwrap();
    assert!(ticket_store.get(&parsed_id).is_ok());

    let feedback_store_root =
        memory_api::workspace::resolve_store_root_from(&workspace_root, ".feedback");
    let feedback_store =
        feedback_api::EntityFeedbackStore::new(feedback_store_root, "default").unwrap();
    let ticket_urn =
        feedback_api::EntityUrn::ticket("default", ticket_id.to_string()).unwrap();
    let entries = feedback_store.entries_for(&ticket_urn).unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn sync_rules_rejects_non_generated_file_input() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    RuleStore::init(&index_root).unwrap();

    let input = dir.path().join("notes.md");
    fs::write(&input, "# plain markdown").unwrap();

    let error = dispatch::dispatch(
        RuleCommandCli::SyncRules(SyncRulesArgs {
            file: input,
            dry_run: false,
            check: false,
        }),
        &index_root,
    )
    .expect_err("sync-rules should reject non-generated file");

    match error {
        CliRunError::BadRequest(message) => {
            assert!(message.contains("not a generated artifact"));
        },
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn sync_rules_uses_marker_id_as_authoritative_and_ignores_slug_edits() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let mut store = RuleStore::init(&index_root).unwrap();
    let rule = sample_rule(
        "shared/agents/reverse-sync",
        "Reverse Sync",
        "reverse-sync",
        "Original body.",
        10,
    );
    let id = store.create(&rule, None).unwrap();

    let input = dir.path().join("generated.md");
    fs::write(
        &input,
        format!(
            concat!(
                "<!-- rule-api:file generated=true -->\n\n",
                "<!-- rule-api:entry id={} slug=edited/slug -->\n",
                "Updated from generated artifact.\n",
            ),
            id
        ),
    )
    .unwrap();

    let payload = dispatch::dispatch(
        RuleCommandCli::SyncRules(SyncRulesArgs {
            file: input,
            dry_run: false,
            check: false,
        }),
        &index_root,
    )
    .unwrap();

    assert_eq!(payload["changed"], 1);
    assert_eq!(payload["items"][0]["slug_mismatch"], true);

    let reopened = RuleStore::open(&index_root).unwrap();
    let updated = reopened.get(&id.to_string()).unwrap();
    assert_eq!(updated.body(), Some("Updated from generated artifact."));
}

#[test]
fn sync_rules_check_reports_drift_without_writing() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let mut store = RuleStore::init(&index_root).unwrap();
    let rule = sample_rule(
        "shared/agents/check-mode",
        "Check Mode",
        "check-mode",
        "Original body.",
        10,
    );
    let id = store.create(&rule, None).unwrap();

    let input = dir.path().join("generated.md");
    fs::write(
        &input,
        format!(
            concat!(
                "<!-- rule-api:file generated=true -->\n\n",
                "<!-- rule-api:entry id={} slug=shared/agents/check-mode -->\n",
                "Changed body.\n",
            ),
            id
        ),
    )
    .unwrap();

    let error = dispatch::dispatch(
        RuleCommandCli::SyncRules(SyncRulesArgs {
            file: input,
            dry_run: false,
            check: true,
        }),
        &index_root,
    )
    .expect_err("check mode should report drift");

    match error {
        CliRunError::BadRequest(message) => {
            assert!(message.contains("drift"));
        },
        other => panic!("unexpected error: {other:?}"),
    }

    let reopened = RuleStore::open(&index_root).unwrap();
    let unchanged = reopened.get(&id.to_string()).unwrap();
    assert_eq!(unchanged.body(), Some("Original body."));
}

#[test]
fn sync_rules_unknown_id_error_keeps_existing_entries_unchanged() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let mut store = RuleStore::init(&index_root).unwrap();
    let rule = sample_rule(
        "shared/agents/atomicity",
        "Atomicity",
        "atomicity",
        "Original body.",
        10,
    );
    let id = store.create(&rule, None).unwrap();

    let input = dir.path().join("generated.md");
    fs::write(
        &input,
        format!(
            concat!(
                "<!-- rule-api:file generated=true -->\n\n",
                "<!-- rule-api:entry id={} slug=shared/agents/atomicity -->\n",
                "Changed body.\n\n",
                "<!-- rule-api:entry id=00000000-0000-0000-0000-000000000999 slug=missing/rule -->\n",
                "Missing body.\n",
            ),
            id
        ),
    )
    .unwrap();

    let error = dispatch::dispatch(
        RuleCommandCli::SyncRules(SyncRulesArgs {
            file: input,
            dry_run: false,
            check: false,
        }),
        &index_root,
    )
    .expect_err("must fail with orphan ids");

    match error {
        CliRunError::BadRequest(message) => {
            assert!(message.contains("orphan generated entry ids"));
        },
        other => panic!("unexpected error: {other:?}"),
    }

    let reopened = RuleStore::open(&index_root).unwrap();
    let unchanged = reopened.get(&id.to_string()).unwrap();
    assert_eq!(unchanged.body(), Some("Original body."));
}

#[test]
fn sync_rules_rejects_spec_doc_entries() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let mut store = RuleStore::init(&index_root).unwrap();
    let mut rule = RuleManifest::new(
        "memory-api/recurring-principles/summary",
        "Summary",
        "spec-doc",
        "summary",
        "## Summary\ntext\n",
    );
    rule.set_repo_scopes(["memory-api"]);
    let id = store.create(&rule, None).unwrap();

    let input = dir.path().join("generated.md");
    fs::write(
        &input,
        format!(
            concat!(
                "<!-- rule-api:file generated=true -->\n\n",
                "<!-- rule-api:entry id={} slug=memory-api/recurring-principles/summary -->\n",
                "## Summary\nchanged\n",
            ),
            id
        ),
    )
    .unwrap();

    let error = dispatch::dispatch(
        RuleCommandCli::SyncRules(SyncRulesArgs {
            file: input,
            dry_run: false,
            check: false,
        }),
        &index_root,
    )
    .expect_err("spec-doc reverse-sync must be rejected");

    match error {
        CliRunError::BadRequest(message) => {
            assert!(message.contains("spec-doc"));
        },
        other => panic!("unexpected error: {other:?}"),
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
        toon: false,
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
        CliOutput::Machine(payload, MachineOutputFormat::Json) => {
            assert_eq!(payload["count"], 1);
            assert_eq!(payload["target"], "root-only");
            assert!(
                payload["content"]
                    .as_str()
                    .unwrap()
                    .contains("slug=shared/agents/opening")
            );
        },
        CliOutput::Text(text) => {
            panic!("expected json output, got text: {text}");
        },
        CliOutput::Machine(_, format) => {
            panic!("expected json machine output, got {format:?}");
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
        Err(crate::cli::CliRunError::Rule(
            rule_api::error::RuleError::NotFound(_)
        ))
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
fn list_command_bootstraps_nested_workspaces_automatically() {
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
    assert_eq!(payload["count"], 2);
    assert!(
        payload["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == child_id)
    );
}

#[test]
fn search_command_bootstraps_nested_workspaces_automatically() {
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
        Err(crate::cli::CliRunError::Rule(
            rule_api::error::RuleError::NotFound(_)
        ))
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
fn generate_file_keeps_frontmatter_first_and_emits_provenance_for_prompt_output()
 {
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
    assert!(rendered.contains("rule-api:file generated=true"));
    assert!(rendered.contains("rule-api:entry id="));
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

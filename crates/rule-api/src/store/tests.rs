use tempfile::tempdir;

use super::*;

#[test]
fn create_and_get_rule_by_slug() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/discovery-protocol",
        "Discovery Protocol",
        "AGENTS",
        "discovery-protocol",
        "Use live sources first.",
    );

    let id = store.create(&manifest, None).unwrap();
    let fetched = store.get("shared/agents/discovery-protocol").unwrap();

    assert_eq!(fetched.id, id);
    assert_eq!(fetched.slug(), manifest.slug());
    assert_eq!(fetched.body(), manifest.body());
}

#[test]
fn open_rebuilds_slug_index_for_fresh_processes() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/reopen-test",
        "Reopen Test",
        "AGENTS",
        "operating-principles",
        "Persist slug lookup across store instances.",
    );
    store.create(&manifest, None).unwrap();
    drop(store);

    let reopened = RuleStore::open(dir.path()).unwrap();
    let fetched = reopened.get("shared/agents/reopen-test").unwrap();

    assert_eq!(fetched.slug(), Some("shared/agents/reopen-test"));
}

#[test]
fn list_filters_and_sorts_rules_by_metadata() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();

    let mut first = RuleManifest::new(
        "shared/agents/discovery-protocol",
        "Discovery Protocol",
        "AGENTS",
        "discovery-protocol",
        "Use live sources first.",
    );
    first.set_order_key(20);
    first.set_repo_scopes(["context-engine", "memory-viewers"]);
    first.set_path_scopes([".agents/instructions/tests.instructions.md"]);
    first.set_feedback_summary(1, 0, 0, 1, 1, Some("2026-05-07T14:00:00Z"));

    let mut second = RuleManifest::new(
        "shared/github/readme/overview",
        "Overview",
        ".github/README",
        "overview",
        "Project overview.",
    );
    second.set_order_key(10);
    second.set_repo_scopes(["memory-api"]);
    second.set_path_scopes([".github/README.md"]);

    let mut third = RuleManifest::new(
        "shared/agents/quality-gates",
        "Quality Gates",
        "AGENTS",
        "quality-gates",
        "Run relevant tests.",
    );
    third.set_order_key(5);
    third.set_repo_scopes(["context-engine"]);
    third.set_path_scopes(["AGENTS.md"]);

    store.create(&first, None).unwrap();
    store.create(&second, None).unwrap();
    store.create(&third, None).unwrap();

    let filtered = store
        .list(
            &RuleFilter {
                file_kind: Some("AGENTS".to_string()),
                repo_scope: Some("context-engine".to_string()),
                path_scope: Some("AGENTS.md".to_string()),
                has_unresolved_feedback: Some(false),
                ..RuleFilter::default()
            },
            None,
        )
        .unwrap();

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].slug(), Some("shared/agents/quality-gates"));
}

#[test]
fn search_can_filter_rule_results_after_full_text_match() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();

    let mut shared = RuleManifest::new(
        "shared/github/readme/overview",
        "Overview",
        ".github/README",
        "overview",
        "Canonical project overview for all repos.",
    );
    shared.set_repo_scopes(["context-engine"]);
    shared.set_path_scopes([".github/README.md"]);

    let mut memory = RuleManifest::new(
        "memory-api/github/readme/overview",
        "Overview",
        ".github/README",
        "overview",
        "Canonical project overview for memory-api only.",
    );
    memory.set_repo_scopes(["memory-api"]);
    memory.set_path_scopes([".github/README.md"]);

    store.create(&shared, None).unwrap();
    store.create(&memory, None).unwrap();

    let filtered = store
        .search(
            "overview",
            &RuleFilter {
                repo_scope: Some("memory-api".to_string()),
                ..RuleFilter::default()
            },
            10,
        )
        .unwrap();

    assert_eq!(filtered.len(), 1);
    assert_eq!(
        filtered[0].slug(),
        Some("memory-api/github/readme/overview")
    );
}

#[test]
fn update_changes_slug_state_and_body() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/update-test",
        "Update Test",
        "AGENTS",
        "update-test",
        "Initial body.",
    );
    store.create(&manifest, None).unwrap();

    store
        .update_body("shared/agents/update-test", "Updated body.")
        .unwrap();
    let updated = store
        .update(
            "shared/agents/update-test",
            BTreeMap::from([
                (
                    "slug".to_string(),
                    Value::String(
                        "shared/agents/update-test-renamed".to_string(),
                    ),
                ),
                (
                    "title".to_string(),
                    Value::String("Updated Test".to_string()),
                ),
            ]),
            Some("reviewed"),
        )
        .unwrap();

    assert_eq!(updated.slug(), Some("shared/agents/update-test-renamed"));
    assert_eq!(updated.title(), Some("Updated Test"));
    assert_eq!(updated.state(), Some("reviewed"));
    assert_eq!(updated.body(), Some("Updated body."));

    let fetched = store.get("shared/agents/update-test-renamed").unwrap();
    assert_eq!(fetched.body(), Some("Updated body."));
}

#[test]
fn generated_target_records_round_trip_and_delete() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
    let config_path = dir.path().join("rule-targets.yaml");
    let output_path = dir.path().join(".github/README.md");

    let record = store
        .upsert_generated_target(
            &config_path,
            "context-engine-github-readme",
            &output_path,
        )
        .unwrap();

    let listed = store.list_generated_targets(&config_path).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], record);

    store.delete_generated_target(&record.slug).unwrap();
    assert!(
        store
            .list_generated_targets(&config_path)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn generated_target_upsert_updates_existing_output_path() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::open(dir.path()).unwrap();
    let config_path = dir.path().join("rule-targets.yaml");
    let first_output = dir.path().join("memory-viewers/.github/README.md");
    let second_output = dir.path().join(".github/README.md");

    let created = store
        .upsert_generated_target(&config_path, "github-readme", &first_output)
        .unwrap();
    let updated = store
        .upsert_generated_target(&config_path, "github-readme", &second_output)
        .unwrap();

    assert_eq!(created.id, updated.id);
    assert_ne!(created.output_path, updated.output_path);
    assert_eq!(
        store.list_generated_targets(&config_path).unwrap()[0].output_path,
        updated.output_path
    );
}

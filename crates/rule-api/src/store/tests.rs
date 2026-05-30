use std::fs;

use memory_api::model::filesystem::ScanRoot;
use tempfile::tempdir;

use super::*;

#[test]
fn create_and_get_rule_by_slug() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
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
fn create_writes_body_md_without_manifest_body_field() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/body-file-contract",
        "Body File Contract",
        "AGENTS",
        "body-file-contract",
        "Canonical prose belongs in body.md.",
    );

    let id = store.create(&manifest, None).unwrap();
    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    let manifest_text = fs::read_to_string(indexed.path.join("rule.toml")).unwrap();

    assert!(indexed.path.join("body.md").is_file());
    assert!(!indexed.path.join("description.md").exists());
    assert!(!manifest_text.contains("body = "));
}

#[test]
fn open_or_init_reindexes_legacy_description_body_content() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/legacy-description-fallback",
        "Legacy Description Fallback",
        "AGENTS",
        "legacy-description-fallback",
        "Legacy description body text.",
    );

    let id = store.create(&manifest, None).unwrap();
    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    fs::rename(
        indexed.path.join("body.md"),
        indexed.path.join("description.md"),
    )
    .unwrap();
    let index_root = store.entity_store().index_root.clone();
    drop(store);

    fs::remove_file(index_root.join("entities.db")).unwrap();
    let _ = fs::remove_file(index_root.join("entities.db-shm"));
    let _ = fs::remove_file(index_root.join("entities.db-wal"));
    let _ = fs::remove_dir_all(index_root.join("search_index"));

    let reopened = RuleStore::open_or_init(dir.path()).unwrap();
    let fetched = reopened.get("shared/agents/legacy-description-fallback").unwrap();
    let matches = reopened
        .search(
            "Legacy description body text",
            &RuleFilter::default(),
            5,
        )
        .unwrap();

    assert_eq!(fetched.body(), Some("Legacy description body text."));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].id, id);
}

#[test]
fn open_creates_gitignore_for_local_rule_artifacts() {
    let dir = tempdir().unwrap();

    RuleStore::init(dir.path()).unwrap();

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("entities.db"));
    assert!(gitignore.contains("entities.db-shm"));
    assert!(gitignore.contains("entities.db-wal"));
    assert!(gitignore.contains("search_index/"));
    assert!(gitignore.contains("entities/"));
}

#[test]
fn open_rebuilds_slug_index_for_fresh_processes() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/reopen-test",
        "Reopen Test",
        "AGENTS",
        "operating-principles",
        "Persist slug lookup across store instances.",
    );
    store.create(&manifest, None).unwrap();
    drop(store);

    let reopened = RuleStore::init(dir.path()).unwrap();
    let fetched = reopened.get("shared/agents/reopen-test").unwrap();

    assert_eq!(fetched.slug(), Some("shared/agents/reopen-test"));
}

#[test]
fn open_prunes_stale_index_rows_for_deleted_rule_folders() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/stale-folder",
        "Stale Folder",
        "AGENTS",
        "stale-folder",
        "This rule folder will be deleted from disk.",
    );

    let id = store.create(&manifest, None).unwrap();
    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    fs::remove_dir_all(&indexed.path).unwrap();
    drop(store);

    let reopened = RuleStore::init(dir.path()).unwrap();
    assert!(reopened.entity_store().get_indexed(&id).unwrap().is_none());
    assert!(matches!(
        reopened.get("shared/agents/stale-folder"),
        Err(RuleError::NotFound(_))
    ));
}

#[test]
fn open_or_init_bootstraps_manifest_only_local_store() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/bootstrap-open",
        "Bootstrap Open",
        "AGENTS",
        "bootstrap-open",
        "Bootstrap local rule stores from manifests.",
    );

    let id = store.create(&manifest, None).unwrap();
    let index_root = store.entity_store().index_root.clone();
    drop(store);

    fs::remove_file(index_root.join("entities.db")).unwrap();
    let _ = fs::remove_file(index_root.join("entities.db-shm"));
    let _ = fs::remove_file(index_root.join("entities.db-wal"));
    let _ = fs::remove_dir_all(index_root.join("search_index"));

    let reopened = RuleStore::open_or_init(dir.path()).unwrap();
    let fetched = reopened.get("shared/agents/bootstrap-open").unwrap();

    assert_eq!(fetched.id, id);
}

#[test]
fn list_filters_and_sorts_rules_by_metadata() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();

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
    let mut store = RuleStore::init(dir.path()).unwrap();

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
    let mut store = RuleStore::init(dir.path()).unwrap();
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
fn delete_rule_entry_removes_it_from_lookups() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let manifest = RuleManifest::new(
        "shared/agents/delete-test",
        "Delete Test",
        "AGENTS",
        "delete-test",
        "Delete this rule entry.",
    );
    store.create(&manifest, None).unwrap();

    store.delete("shared/agents/delete-test").unwrap();

    assert!(matches!(
        store.get("shared/agents/delete-test"),
        Err(RuleError::NotFound(_))
    ));
    assert!(
        store
            .list(
                &RuleFilter {
                    slug: Some("shared/agents/delete-test".to_string()),
                    ..RuleFilter::default()
                },
                None,
            )
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .search("Delete this rule entry.", &RuleFilter::default(), 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn generated_target_records_round_trip_and_delete() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
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
fn delete_can_remove_generated_target_records() {
    let dir = tempdir().unwrap();
    let mut store = RuleStore::init(dir.path()).unwrap();
    let config_path = dir.path().join("rule-targets.yaml");
    let output_path = dir.path().join(".github/README.md");

    let record = store
        .upsert_generated_target(
            &config_path,
            "context-engine-github-readme",
            &output_path,
        )
        .unwrap();

    store.delete(&record.slug).unwrap();

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
    let mut store = RuleStore::init(dir.path()).unwrap();
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

#[test]
fn create_defaults_to_local_rules_root_even_with_extra_scan_roots() {
    let dir = tempdir().unwrap();
    let index_root = dir.path().join(".rule");
    let child_rules_root =
        dir.path().join("nested").join(".rule").join("rules");
    fs::create_dir_all(&child_rules_root).unwrap();

    let mut store = RuleStore::init(&index_root).unwrap();
    store
        .entity_store()
        .add_scan_root(ScanRoot {
            path: child_rules_root.clone(),
            label: "nested".to_string(),
        })
        .unwrap();

    let manifest = RuleManifest::new(
        "shared/agents/local-authoring",
        "Local Authoring",
        "AGENTS",
        "local-authoring",
        "Rules should default to the local workspace.",
    );
    let id = store.create(&manifest, None).unwrap();
    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();

    assert!(indexed.path.starts_with(index_root.join("rules")));
    assert!(!indexed.path.starts_with(&child_rules_root));
}

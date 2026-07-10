use std::{
    collections::BTreeMap,
    fs,
};

use tempfile::tempdir;

use super::*;
use crate::cli::{
    SearchArgs,
    commands::cmd_search,
};
use spec_api::{
    SpecManifest,
    store::GeneratedSpecArtifacts,
};

fn create_sync_fixture()
-> (tempfile::TempDir, PathBuf, PathBuf, SpecStore, String) {
    let dir = tempdir().unwrap();
    let repo_root = dir.path().join("repo");
    let child_root = repo_root.join("memory-api");
    fs::create_dir_all(&child_root).unwrap();
    fs::create_dir_all(child_root.join(".rule")).unwrap();
    fs::create_dir_all(child_root.join(".spec")).unwrap();

    let mut rule_store = RuleStore::init(&child_root).unwrap();

    let mut body_rule = RuleManifest::new(
        "shared/spec/generated/body",
        "Generated Body",
        "spec-doc",
        "body",
        "## Overview\nGenerated body text for search.\n",
    );
    body_rule.set_repo_scopes(["memory-api"]);

    let mut requirements_rule = RuleManifest::new(
        "shared/spec/generated/requirements",
        "Generated Requirements",
        "spec-doc",
        "requirements",
        "## Requirements\nGenerated section content.\n",
    );
    requirements_rule.set_repo_scopes(["memory-api"]);

    rule_store.create(&body_rule, None).unwrap();
    rule_store.create(&requirements_rule, None).unwrap();

    fs::write(
        child_root.join("rule-targets.yaml"),
        concat!(
            "targets:\n",
            "  - name: spec-body\n",
            "    repo_scope: memory-api\n",
            "    file_kind: spec-doc\n",
            "    output_path: generated/body.md\n",
            "    nodes:\n",
            "      - name: body\n",
            "        section: body\n",
            "  - name: spec-requirements\n",
            "    repo_scope: memory-api\n",
            "    file_kind: spec-doc\n",
            "    output_path: generated/requirements.md\n",
            "    nodes:\n",
            "      - name: requirements\n",
            "        section: requirements\n",
        ),
    )
    .unwrap();

    let mut spec_store = SpecStore::init(&child_root).unwrap();
    let spec = SpecManifest::new(
        "spec-cli/generated-sync",
        "Generated Sync",
        "spec-cli",
    );
    let id = spec_store.create(&spec, "placeholder body", None).unwrap();

    let mut sections = BTreeMap::new();
    sections.insert(
        "requirements".to_string(),
        GeneratedSpecArtifactTarget {
            config: "rule-targets.yaml".into(),
            target: "spec-requirements".into(),
        },
    );

    spec_store
        .update_generated_artifacts(
            &id.to_string(),
            &GeneratedSpecArtifacts {
                body: Some(GeneratedSpecArtifactTarget {
                    config: "rule-targets.yaml".into(),
                    target: "spec-body".into(),
                }),
                sections,
            },
        )
        .unwrap();

    (dir, repo_root, child_root, spec_store, id.to_string())
}

#[test]
fn sync_generated_uses_owning_workspace_and_updates_searchable_body() {
    let (_dir, repo_root, child_root, mut store, id) = create_sync_fixture();

    let payload = cmd_sync_generated(
        SyncGeneratedArgs { id: id.clone() },
        &mut store,
        &repo_root,
    )
    .unwrap();

    assert_eq!(payload["command"], "sync_generated");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["count"], 2);
    assert_eq!(
        payload["workspace_root"],
        child_root.to_string_lossy().replace('\\', "/")
    );

    let (_spec, body) = store.get_full(&id).unwrap();
    assert!(body.contains("<!-- spec-api:file generated=true -->"));
    assert!(body.contains("Generated body text for search."));
    assert!(!body.contains("<!-- rule-api:file generated=true -->"));

    let section_path = store
        .entity_store()
        .get_indexed(&store.resolve_id(&id).unwrap())
        .unwrap()
        .unwrap()
        .path
        .join("sections")
        .join("requirements.md");
    let section = fs::read_to_string(section_path).unwrap();
    assert!(section.contains("Generated section content."));

    let search = cmd_search(
        SearchArgs {
            query: "Generated body text for search".into(),
            limit: 10,
        },
        &store,
    )
    .unwrap();
    assert_eq!(search["count"], 1);
    assert_eq!(search["items"][0]["id"], id);
}

#[test]
fn sync_generated_fails_when_declared_target_is_missing() {
    let (_dir, repo_root, _child_root, mut store, id) = create_sync_fixture();

    store
        .update_generated_artifacts(
            &id,
            &GeneratedSpecArtifacts {
                body: Some(GeneratedSpecArtifactTarget {
                    config: "rule-targets.yaml".into(),
                    target: "missing-target".into(),
                }),
                sections: BTreeMap::new(),
            },
        )
        .unwrap();

    let error =
        cmd_sync_generated(SyncGeneratedArgs { id }, &mut store, &repo_root)
            .unwrap_err();

    assert!(error.to_string().contains("missing-target"));
}

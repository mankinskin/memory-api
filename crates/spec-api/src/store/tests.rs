use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
};

use memory_api::generated_markdown::GeneratedMarkdownSnippet;
use memory_api::model::filesystem::ScanRoot;
use serde_json::Value;
use tempfile::TempDir;

use super::*;

fn setup() -> (TempDir, SpecStore) {
    let tmp = TempDir::new().unwrap();
    let store = SpecStore::init(tmp.path()).unwrap();
    let root = tmp.path().join("specs");
    fs::create_dir_all(&root).unwrap();
    store
        .entity_store()
        .add_scan_root(ScanRoot {
            path: root,
            label: "test".into(),
        })
        .unwrap();
    (tmp, store)
}

fn make_spec(
    slug: &str,
    title: &str,
) -> SpecManifest {
    SpecManifest::new(slug, title, "test-component")
}

fn setup_local_store() -> (TempDir, PathBuf, PathBuf, SpecStore) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let store_root = repo.join(".spec");
    fs::create_dir_all(&store_root).unwrap();
    let store = SpecStore::init(&repo).unwrap();
    (tmp, repo, store_root, store)
}

#[test]
fn create_get_update_delete_spec() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/overview", "Overview");
    let id = store.create(&spec, "body v1", None).unwrap();

    let fetched = store.get("root/overview").unwrap();
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.slug(), Some("root/overview"));

    let full = store.get_full(&id.to_string()).unwrap();
    assert_eq!(full.1, "body v1");

    let mut patch = BTreeMap::new();
    patch.insert("title".into(), Value::String("Overview 2".into()));
    let updated = store.update("root/overview", patch, None).unwrap();
    assert_eq!(updated.title(), Some("Overview 2"));

    store.update_body("root/overview", "body v2").unwrap();
    let full2 = store.get_full("root/overview").unwrap();
    assert_eq!(full2.1, "body v2");

    store.delete("root/overview").unwrap();
    assert!(matches!(
        store.get("root/overview"),
        Err(SpecError::NotFound(_))
    ));
}

#[test]
fn update_generated_body_renders_spec_api_provenance_comments() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated", "Generated");
    store.create(&spec, "body v1", None).unwrap();

    let snippets = [
        GeneratedMarkdownSnippet::new(
            "rule-1",
            Some("shared/spec/problem"),
            "## Problem\nReuse canonical snippets.\n",
        ),
        GeneratedMarkdownSnippet::new(
            "rule-2",
            Some("shared/spec/acceptance"),
            "## Acceptance\nKeep generation deterministic.\n",
        ),
    ];

    store
        .update_generated_body("root/generated", &snippets)
        .unwrap();

    let full = store.get_full("root/generated").unwrap();
    assert_eq!(full.1, render_generated_body(&snippets));
}

#[test]
fn update_generated_body_preserves_existing_crlf_style() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated-crlf", "Generated CRLF");
    let id = store.create(&spec, "old\r\nbody\r\n", None).unwrap();
    let snippets = [GeneratedMarkdownSnippet::new(
        "rule-1",
        Some("shared/spec/problem"),
        "## Problem\nReuse canonical snippets.\n",
    )];

    store
        .update_generated_body("root/generated-crlf", &snippets)
        .unwrap();

    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    let body = fs::read_to_string(indexed.path.join("body.md")).unwrap();

    assert_eq!(
        body,
        "<!-- spec-api:file generated=true -->\r\n\r\n<!-- spec-api:entry id=rule-1 slug=shared/spec/problem -->\r\n## Problem\r\nReuse canonical snippets.\r\n"
    );
}

#[test]
fn update_generated_section_creates_and_renders_named_section() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated-section", "Generated Section");
    store.create(&spec, "body v1", None).unwrap();

    let snippets = [
        GeneratedMarkdownSnippet::new(
            "rule-1",
            Some("shared/spec/requirements"),
            "## Requirements\nGenerate named sections.\n",
        ),
        GeneratedMarkdownSnippet::new(
            "rule-2",
            Some("shared/spec/notes"),
            "## Notes\nKeep deterministic ordering.\n",
        ),
    ];

    store
        .update_generated_section(
            "root/generated-section",
            "requirements",
            &snippets,
        )
        .unwrap();

    let sections = store.list_sections("root/generated-section").unwrap();
    assert_eq!(sections, vec!["requirements.md".to_string()]);

    let full_path = store
        .entity_store()
        .get_indexed(&store.resolve_id("root/generated-section").unwrap())
        .unwrap()
        .unwrap()
        .path
        .join("sections")
        .join("requirements.md");
    let content = fs::read_to_string(full_path).unwrap();

    assert_eq!(content, render_generated_document(&snippets));
}

#[test]
fn update_generated_section_preserves_existing_crlf_style() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated-section-crlf", "Generated Section CRLF");
    let id = store
        .create(&spec, "body v1", None)
        .unwrap();
    store
        .add_section(
            "root/generated-section-crlf",
            "requirements",
            "old\r\ncontent\r\n",
        )
        .unwrap();
    let snippets = [GeneratedMarkdownSnippet::new(
        "rule-1",
        Some("shared/spec/requirements"),
        "## Requirements\nPreserve CRLF.\n",
    )];

    store
        .update_generated_section(
            "root/generated-section-crlf",
            "requirements",
            &snippets,
        )
        .unwrap();

    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    let content = fs::read_to_string(
        indexed.path.join("sections").join("requirements.md"),
    )
    .unwrap();

    assert_eq!(
        content,
        "<!-- spec-api:file generated=true -->\r\n\r\n<!-- spec-api:entry id=rule-1 slug=shared/spec/requirements -->\r\n## Requirements\r\nPreserve CRLF.\r\n"
    );
}

#[test]
fn update_generated_artifacts_round_trips_body_and_sections() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated-artifacts", "Generated Artifacts");
    let id = store.create(&spec, "body v1", None).unwrap();

    let mut sections = BTreeMap::new();
    sections.insert(
        "requirements.md".to_string(),
        GeneratedSpecArtifactTarget {
            config: "rule-targets.yaml".into(),
            target: "requirements".into(),
        },
    );
    sections.insert(
        "design".to_string(),
        GeneratedSpecArtifactTarget {
            config: "spec/rule-targets.yaml".into(),
            target: "design".into(),
        },
    );

    let artifacts = GeneratedSpecArtifacts {
        body: Some(GeneratedSpecArtifactTarget {
            config: "rule-targets.yaml".into(),
            target: "body".into(),
        }),
        sections,
    };

    store
        .update_generated_artifacts("root/generated-artifacts", &artifacts)
        .unwrap();

    let stored = store
        .get_generated_artifacts(&id.to_string())
        .unwrap()
        .unwrap();

    let mut expected_sections = BTreeMap::new();
    expected_sections.insert(
        "design".to_string(),
        GeneratedSpecArtifactTarget {
            config: "spec/rule-targets.yaml".into(),
            target: "design".into(),
        },
    );
    expected_sections.insert(
        "requirements".to_string(),
        GeneratedSpecArtifactTarget {
            config: "rule-targets.yaml".into(),
            target: "requirements".into(),
        },
    );

    assert_eq!(
        stored,
        GeneratedSpecArtifacts {
            body: Some(GeneratedSpecArtifactTarget {
                config: "rule-targets.yaml".into(),
                target: "body".into(),
            }),
            sections: expected_sections,
        }
    );

    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    let generated = fs::read_to_string(indexed.path.join("generated.toml")).unwrap();
    assert!(generated.contains("[body]"));
    assert!(generated.contains("[sections.design]"));
    assert!(generated.contains("[sections.requirements]"));
}

#[test]
fn update_generated_artifacts_rejects_duplicate_section_aliases() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated-artifact-duplicates", "Generated Artifact Duplicates");
    store.create(&spec, "body v1", None).unwrap();

    let mut sections = BTreeMap::new();
    sections.insert(
        "requirements".to_string(),
        GeneratedSpecArtifactTarget {
            config: "rule-targets.yaml".into(),
            target: "requirements".into(),
        },
    );
    sections.insert(
        "requirements.md".to_string(),
        GeneratedSpecArtifactTarget {
            config: "rule-targets.yaml".into(),
            target: "requirements-copy".into(),
        },
    );

    let error = store
        .update_generated_artifacts(
            "root/generated-artifact-duplicates",
            &GeneratedSpecArtifacts {
                body: None,
                sections,
            },
        )
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("duplicate generated section mapping")
    );
}

#[test]
fn update_generated_artifacts_rejects_invalid_targets_and_paths() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated-artifact-invalid", "Generated Artifact Invalid");
    store.create(&spec, "body v1", None).unwrap();

    let blank_target = store
        .update_generated_artifacts(
            "root/generated-artifact-invalid",
            &GeneratedSpecArtifacts {
                body: Some(GeneratedSpecArtifactTarget {
                    config: "rule-targets.yaml".into(),
                    target: "   ".into(),
                }),
                sections: BTreeMap::new(),
            },
        )
        .unwrap_err();
    assert!(blank_target.to_string().contains("missing target"));

    let mut sections = BTreeMap::new();
    sections.insert(
        "../escape".to_string(),
        GeneratedSpecArtifactTarget {
            config: "rule-targets.yaml".into(),
            target: "escape".into(),
        },
    );

    let invalid_path = store
        .update_generated_artifacts(
            "root/generated-artifact-invalid",
            &GeneratedSpecArtifacts {
                body: None,
                sections,
            },
        )
        .unwrap_err();
    assert!(
        invalid_path
            .to_string()
            .contains("must stay within sections/*.md")
    );
}

#[test]
fn update_generated_artifacts_deletes_empty_descriptor_file() {
    let (_tmp, mut store) = setup();

    let spec = make_spec("root/generated-artifact-clear", "Generated Artifact Clear");
    let id = store.create(&spec, "body v1", None).unwrap();

    store
        .update_generated_artifacts(
            "root/generated-artifact-clear",
            &GeneratedSpecArtifacts {
                body: Some(GeneratedSpecArtifactTarget {
                    config: "rule-targets.yaml".into(),
                    target: "body".into(),
                }),
                sections: BTreeMap::new(),
            },
        )
        .unwrap();

    store
        .update_generated_artifacts(
            "root/generated-artifact-clear",
            &GeneratedSpecArtifacts::default(),
        )
        .unwrap();

    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    assert!(!indexed.path.join("generated.toml").exists());
    assert_eq!(
        store
            .get_generated_artifacts("root/generated-artifact-clear")
            .unwrap(),
        None
    );
}

#[test]
fn open_creates_gitignore_for_local_spec_artifacts() {
    let tmp = TempDir::new().unwrap();

    SpecStore::init(tmp.path()).unwrap();

    let gitignore = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains("entities.db"));
    assert!(gitignore.contains("entities.db-shm"));
    assert!(gitignore.contains("entities.db-wal"));
    assert!(gitignore.contains("search_index/"));
}

#[test]
fn open_registers_default_specs_scan_root() {
    let tmp = TempDir::new().unwrap();
    let store = SpecStore::init(tmp.path()).unwrap();

    let roots = store.entity_store().list_scan_roots().unwrap();

    assert!(roots.iter().any(|root| {
        root.path == tmp.path().join("specs") && root.label == "specs"
    }));
}

#[test]
fn create_normalizes_workspace_target_root_into_local_store() {
    let (_tmp, repo, _store_root, mut store) = setup_local_store();

    let spec = make_spec("root/overview", "Overview");
    let id = store.create(&spec, "body", Some(&repo)).unwrap();

    let expected = repo.join(".spec").join("specs").join(id.to_string());
    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();

    assert_eq!(indexed.path, expected);
    assert!(expected.join("spec.toml").exists());
    assert!(!repo.join(id.to_string()).exists());
}

#[test]
fn create_normalizes_store_root_into_specs_scan_root() {
    let (_tmp, repo, store_root, mut store) = setup_local_store();

    let spec = make_spec("root/store-root", "Store Root");
    let id = store.create(&spec, "body", Some(&store_root)).unwrap();

    let expected = repo.join(".spec").join("specs").join(id.to_string());
    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();

    assert_eq!(indexed.path, expected);
    assert!(expected.join("spec.toml").exists());
}

#[test]
fn create_rejects_non_workspace_target_root() {
    let (_tmp, _repo, _store_root, mut store) = setup_local_store();
    let outside = TempDir::new().unwrap();
    let invalid_root = outside.path().join("stray-root");
    fs::create_dir_all(&invalid_root).unwrap();

    let spec = make_spec("root/invalid-root", "Invalid Root");
    let error = store
        .create(&spec, "body", Some(&invalid_root))
        .unwrap_err();

    assert!(error.to_string().contains("invalid spec root"));
}

#[test]
fn open_or_init_bootstraps_manifest_only_local_store() {
    let (_tmp, repo, store_root, mut store) = setup_local_store();

    let spec = make_spec("root/bootstrap-open", "Bootstrap Open");
    let id = store.create(&spec, "body", Some(&repo)).unwrap();
    drop(store);

    fs::remove_file(store_root.join("entities.db")).unwrap();
    let _ = fs::remove_file(store_root.join("entities.db-shm"));
    let _ = fs::remove_file(store_root.join("entities.db-wal"));
    let _ = fs::remove_dir_all(store_root.join("search_index"));

    let reopened = SpecStore::open_or_init(&repo).unwrap();
    let fetched = reopened.get("root/bootstrap-open").unwrap();

    assert_eq!(fetched.id, id);
}

#[test]
fn scan_updates_indexed_path_after_spec_folder_moves_between_roots() {
    let tmp = TempDir::new().unwrap();
    let index_root = tmp.path().join("index");
    let original_root = tmp.path().join("original-specs");
    let repaired_root = tmp.path().join("repaired-specs");
    fs::create_dir_all(&original_root).unwrap();
    fs::create_dir_all(&repaired_root).unwrap();

    let mut store = SpecStore::init(&index_root).unwrap();
    store
        .entity_store()
        .add_scan_root(ScanRoot {
            path: original_root.clone(),
            label: "original".into(),
        })
        .unwrap();
    store
        .entity_store()
        .add_scan_root(ScanRoot {
            path: repaired_root.clone(),
            label: "repaired".into(),
        })
        .unwrap();

    let spec = make_spec("root/moved", "Moved");
    let id = store.create(&spec, "body", Some(&original_root)).unwrap();

    let original_folder = original_root.join(id.to_string());
    let repaired_folder = repaired_root.join(id.to_string());
    fs::rename(&original_folder, &repaired_folder).unwrap();

    store.scan(true).unwrap();

    let indexed = store.entity_store().get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.path, repaired_folder);
    assert_eq!(store.get_full(&id.to_string()).unwrap().1, "body");
}

#[test]
fn duplicate_slug_is_rejected() {
    let (_tmp, mut store) = setup();
    let a = make_spec("a/spec", "A");
    let b = make_spec("a/spec", "B");
    store.create(&a, "body", None).unwrap();
    assert!(matches!(
        store.create(&b, "body", None),
        Err(SpecError::DuplicateSlug(_))
    ));
}

#[test]
fn children_ancestors_subtree_and_sections_work() {
    let (_tmp, mut store) = setup();

    let root = make_spec("root", "Root");
    let root_id = store.create(&root, "root body", None).unwrap();
    let root_id_str = root_id.to_string();

    let mut child = make_spec("root/child", "Child");
    child.set_parent(&root_id_str);
    let child_id = store.create(&child, "child body", None).unwrap();
    let child_id_str = child_id.to_string();

    let mut grand = make_spec("root/child/grand", "Grand");
    grand.set_parent(&child_id_str);
    store.create(&grand, "grand body", None).unwrap();

    let children = store.children(&root_id.to_string()).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].slug(), Some("root/child"));

    let ancestors = store.ancestors("root/child/grand").unwrap();
    assert_eq!(ancestors.len(), 2);
    assert_eq!(ancestors[0].slug(), Some("root/child"));
    assert_eq!(ancestors[1].slug(), Some("root"));

    let subtree = store.subtree("root").unwrap();
    assert_eq!(subtree.len(), 2);

    store.add_section("root", "intro", "hello").unwrap();
    store.update_section("root", "intro", "hello2").unwrap();
    let sections = store.list_sections("root").unwrap();
    assert_eq!(sections, vec!["intro.md".to_string()]);
    store.delete_section("root", "intro").unwrap();
    assert!(store.list_sections("root").unwrap().is_empty());
}

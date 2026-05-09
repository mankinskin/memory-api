use std::collections::BTreeMap;
use std::fs;

use memory_api::model::filesystem::ScanRoot;
use serde_json::Value;
use tempfile::TempDir;

use super::*;

fn setup() -> (TempDir, SpecStore) {
    let tmp = TempDir::new().unwrap();
    let store = SpecStore::open(tmp.path()).unwrap();
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

fn make_spec(slug: &str, title: &str) -> SpecManifest {
    SpecManifest::new(slug, title, "test-component")
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
    assert!(matches!(store.get("root/overview"), Err(SpecError::NotFound(_))));
}

#[test]
fn duplicate_slug_is_rejected() {
    let (_tmp, mut store) = setup();
    let a = make_spec("a/spec", "A");
    let b = make_spec("a/spec", "B");
    store.create(&a, "body", None).unwrap();
    assert!(matches!(store.create(&b, "body", None), Err(SpecError::DuplicateSlug(_))));
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
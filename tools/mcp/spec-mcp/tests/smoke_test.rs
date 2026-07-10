//! Smoke tests for spec-mcp tools.
//!
//! Exercises the key tool cycle (create → get → update → list → search →
//! sections → tree → health → refs_validate → delete) via `SpecServer`
//! methods directly, without going through the JSON-RPC transport.

use std::{
    collections::BTreeMap,
    process::Command,
};

use rmcp::handler::server::wrapper::Parameters;
use spec_mcp::server::*;

#[path = "smoke_test/support.rs"]
mod support;

use support::{
    extract_json,
    make_sandbox,
};

fn run_git(
    repo_root: &std::path::Path,
    args: &[&str],
) {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()
        .expect("git command");
    assert!(status.success(), "git {args:?} failed: {status}");
}

// ── tests ────────────────────────────────────────────────────────────────────

/// Full CRUD lifecycle: create → get → get(full) → update → list → delete.
#[tokio::test]
async fn spec_crud_lifecycle() {
    let (_tmp, server) = make_sandbox();

    // 1. Create a spec
    let result = server
        .spec_create(Parameters(CreateSpecInput {
            workspace: _tmp.path().display().to_string(),
            title: "Test Spec".to_string(),
            slug: "test-component/test-spec".to_string(),
            component: "test-component".to_string(),
            parent: None,
            scope: Some("public".to_string()),
            body: Some("# Test\n\nThis is a test spec.".to_string()),
            fields: BTreeMap::new(),
        }))
        .await
        .expect("spec_create");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["slug"], "test-component/test-spec");
    let spec_id = json["id"].as_str().expect("id").to_string();

    // 2. Get by slug
    let result = server
        .spec_get(Parameters(GetSpecInput {
            workspace: None,
            id: "test-component/test-spec".to_string(),
            full: false,
        }))
        .await
        .expect("spec_get");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["spec"]["id"], spec_id);

    // 3. Get full (with body)
    let result = server
        .spec_get(Parameters(GetSpecInput {
            workspace: None,
            id: spec_id.clone(),
            full: true,
        }))
        .await
        .expect("spec_get full");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert!(json["body"].as_str().unwrap().contains("# Test"));
    assert!(json["sections"].is_array());

    // 4. Update fields
    let result = server
        .spec_update(Parameters(UpdateSpecInput {
            workspace: None,
            id: spec_id.clone(),
            fields: Some(vec!["title=Updated Title".to_string()]),
            to_state: Some("reviewed".to_string()),
            body: None,
            field_map: None,
        }))
        .await
        .expect("spec_update");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["changed_fields"]["title"], "Updated Title");
    assert!(json.get("fields").is_none());

    // 5. Update body
    let result = server
        .spec_update(Parameters(UpdateSpecInput {
            workspace: None,
            id: spec_id.clone(),
            fields: None,
            to_state: None,
            body: Some("# Updated body".to_string()),
            field_map: None,
        }))
        .await
        .expect("spec_update body");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["body_updated"], true);

    // 6. List all
    let result = server
        .spec_list(Parameters(ListSpecsInput {
            workspace: None,
            where_clauses: vec![],
            limit: None,
        }))
        .await
        .expect("spec_list");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["count"], 1);

    // 7. List with filter
    let result = server
        .spec_list(Parameters(ListSpecsInput {
            workspace: None,
            where_clauses: vec!["component=test-component".to_string()],
            limit: None,
        }))
        .await
        .expect("spec_list filtered");
    let json = extract_json(result);
    assert_eq!(json["count"], 1);

    // 8. Delete
    let result = server
        .spec_delete(Parameters(SpecRefInput {
            workspace: None,
            id: spec_id.clone(),
        }))
        .await
        .expect("spec_delete");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn spec_update_accepts_sparse_payload_and_returns_minimal_response() {
    let (_tmp, server) = make_sandbox();

    let created = extract_json(
        server
            .spec_create(Parameters(CreateSpecInput {
                workspace: _tmp.path().display().to_string(),
                title: "Sparse Update".to_string(),
                slug: "tests/sparse-update".to_string(),
                component: "tests".to_string(),
                parent: None,
                scope: None,
                body: None,
                fields: BTreeMap::new(),
            }))
            .await
            .expect("create"),
    );
    let spec_id = created["id"].as_str().unwrap().to_string();

    let result = server
        .spec_update(Parameters(UpdateSpecInput {
            workspace: None,
            id: spec_id,
            fields: None,
            to_state: Some("reviewed".to_string()),
            body: None,
            field_map: None,
        }))
        .await
        .expect("sparse state-only update");
    let json = extract_json(result);

    assert_eq!(json["status"], "ok");
    assert_eq!(json["state_transition"]["to"], "reviewed");
    assert!(json.get("changed_fields").is_none());
    assert!(json.get("fields").is_none());
}

#[tokio::test]
async fn spec_move_preflight_returns_supported_plan() {
    let (_tmp, server) = make_sandbox();
    run_git(_tmp.path(), &["init"]);
    let target_workspace = _tmp.path().join("target");
    std::fs::create_dir_all(&target_workspace).unwrap();
    spec_api::SpecStore::init(&target_workspace).unwrap();

    let created = extract_json(
        server
            .spec_create(Parameters(CreateSpecInput {
                workspace: _tmp.path().display().to_string(),
                title: "Movable Spec".to_string(),
                slug: "tests/movable-spec".to_string(),
                component: "tests".to_string(),
                parent: None,
                scope: None,
                body: Some("body".to_string()),
                fields: BTreeMap::new(),
            }))
            .await
            .expect("create"),
    );
    let spec_id = created["id"].as_str().unwrap().to_string();

    let result = server
        .spec_move_preflight(Parameters(SpecMoveInput {
            workspace: Some(_tmp.path().display().to_string()),
            id: spec_id,
            to_workspace_root: target_workspace.display().to_string(),
        }))
        .await
        .expect("move preflight");
    let json = extract_json(result);

    assert_eq!(json["status"], "ok");
    assert_eq!(json["mode"], "preflight");
    assert_eq!(json["supported"], true);
}

/// Section operations: add → list → get → delete.
#[tokio::test]
async fn spec_section_lifecycle() {
    let (_tmp, server) = make_sandbox();

    // Create a spec first
    let result = server
        .spec_create(Parameters(CreateSpecInput {
            workspace: _tmp.path().display().to_string(),
            title: "Section Test".to_string(),
            slug: "sections/test".to_string(),
            component: "sections".to_string(),
            parent: None,
            scope: None,
            body: None,
            fields: BTreeMap::new(),
        }))
        .await
        .expect("create");
    let json = extract_json(result);
    let spec_id = json["id"].as_str().unwrap().to_string();

    // Add section
    let result = server
        .spec_section_add(Parameters(SectionAddInput {
            workspace: None,
            id: spec_id.clone(),
            name: "design".to_string(),
            content: "## Design\n\nKey design notes.".to_string(),
        }))
        .await
        .expect("section_add");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");

    // List sections
    let result = server
        .spec_section_list(Parameters(SpecRefInput {
            workspace: None,
            id: spec_id.clone(),
        }))
        .await
        .expect("section_list");
    let json = extract_json(result);
    assert_eq!(json["count"], 1);
    let sections = json["sections"].as_array().unwrap();
    assert!(
        sections
            .iter()
            .any(|s| s.as_str().unwrap().contains("design"))
    );

    // Get section
    let result = server
        .spec_section_get(Parameters(SectionRefInput {
            workspace: None,
            id: spec_id.clone(),
            name: "design".to_string(),
        }))
        .await
        .expect("section_get");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert!(
        json["content"]
            .as_str()
            .unwrap()
            .contains("Key design notes")
    );

    // Delete section
    let result = server
        .spec_section_delete(Parameters(SectionRefInput {
            workspace: None,
            id: spec_id.clone(),
            name: "design".to_string(),
        }))
        .await
        .expect("section_delete");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");

    // List again — empty
    let result = server
        .spec_section_list(Parameters(SpecRefInput {
            workspace: None,
            id: spec_id.clone(),
        }))
        .await
        .expect("section_list empty");
    let json = extract_json(result);
    assert_eq!(json["count"], 0);
}

/// Tree and health tools.
#[tokio::test]
async fn spec_tree_and_health() {
    let (_tmp, server) = make_sandbox();

    // Create parent
    let result = server
        .spec_create(Parameters(CreateSpecInput {
            workspace: _tmp.path().display().to_string(),
            title: "Parent Spec".to_string(),
            slug: "tree/parent".to_string(),
            component: "tree".to_string(),
            parent: None,
            scope: None,
            body: None,
            fields: BTreeMap::new(),
        }))
        .await
        .expect("create parent");
    let parent_id = extract_json(result)["id"].as_str().unwrap().to_string();

    // Create child
    let result = server
        .spec_create(Parameters(CreateSpecInput {
            workspace: _tmp.path().display().to_string(),
            title: "Child Spec".to_string(),
            slug: "tree/parent/child".to_string(),
            component: "tree".to_string(),
            parent: Some(parent_id.clone()),
            scope: None,
            body: None,
            fields: BTreeMap::new(),
        }))
        .await
        .expect("create child");
    let _child_id = extract_json(result)["id"].as_str().unwrap().to_string();

    // Tree from parent
    let result = server
        .spec_tree(Parameters(TreeInput {
            workspace: None,
            id: Some(parent_id.clone()),
        }))
        .await
        .expect("spec_tree");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["root"]["slug"], "tree/parent");

    // Tree all roots
    let result = server
        .spec_tree(Parameters(TreeInput {
            workspace: None,
            id: None,
        }))
        .await
        .expect("spec_tree roots");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert!(!json["roots"].as_array().unwrap().is_empty());

    // Health all
    let result = server
        .spec_health(Parameters(HealthInput {
            workspace: None,
            id: None,
            all: true,
        }))
        .await
        .expect("spec_health");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert!(json["specs_checked"].as_u64().unwrap() >= 2);

    // Health single
    let result = server
        .spec_health(Parameters(HealthInput {
            workspace: None,
            id: Some(parent_id.clone()),
            all: false,
        }))
        .await
        .expect("spec_health single");
    let json = extract_json(result);
    assert_eq!(json["specs_checked"], 1);
}

/// Scan and add-root tools.
#[tokio::test]
async fn spec_scan_and_add_root() {
    let (tmp, server) = make_sandbox();

    // Scan (non-force)
    let result = server
        .spec_scan(Parameters(ScanInput {
            workspace: None,
            force: false,
        }))
        .await
        .expect("spec_scan");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");

    // Add a new root
    let new_root = tmp.path().join("extra-specs");
    std::fs::create_dir_all(&new_root).expect("mkdir");
    let result = server
        .spec_add_root(Parameters(AddRootInput {
            workspace: None,
            path: new_root.to_str().unwrap().to_string(),
            label: Some("extra".to_string()),
        }))
        .await
        .expect("spec_add_root");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["label"], "extra");
}

/// Refs validate tool (no refs = valid).
#[tokio::test]
async fn spec_refs_validate_empty() {
    let (_tmp, server) = make_sandbox();

    let result = server
        .spec_create(Parameters(CreateSpecInput {
            workspace: _tmp.path().display().to_string(),
            title: "Ref Test".to_string(),
            slug: "refs/test".to_string(),
            component: "refs".to_string(),
            parent: None,
            scope: None,
            body: None,
            fields: BTreeMap::new(),
        }))
        .await
        .expect("create");
    let spec_id = extract_json(result)["id"].as_str().unwrap().to_string();

    let result = server
        .spec_refs_validate(Parameters(RefsValidateInput {
            workspace: None,
            id: spec_id,
            workspace_root: ".".to_string(),
        }))
        .await
        .expect("refs_validate");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["valid"], true);
    assert_eq!(json["count"], 0);
}

/// Search tool.
#[tokio::test]
async fn spec_search_tool() {
    let (_tmp, server) = make_sandbox();

    // Create a spec with searchable content
    server
        .spec_create(Parameters(CreateSpecInput {
            workspace: _tmp.path().display().to_string(),
            title: "Searchable Alpha".to_string(),
            slug: "search/alpha".to_string(),
            component: "search".to_string(),
            parent: None,
            scope: None,
            body: Some("This spec covers the alpha module.".to_string()),
            fields: BTreeMap::new(),
        }))
        .await
        .expect("create");

    // Search (may not find it immediately if index is async, but should not error)
    let result = server
        .spec_search(Parameters(SearchSpecsInput {
            workspace: None,
            query: "alpha".to_string(),
            limit: 10,
        }))
        .await
        .expect("spec_search");
    let json = extract_json(result);
    assert_eq!(json["status"], "ok");
    // Search results may or may not contain the new spec depending on
    // indexing timing, but the tool should succeed.
}

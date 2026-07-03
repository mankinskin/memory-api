use rmcp::handler::server::wrapper::Parameters;
use rule_api::RuleStore;
use rule_mcp::server::{
    CreateRuleInput,
    RuleMoveInput,
    RuleServer,
    UpdateRuleInput,
};
use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

fn make_sandbox() -> (TempDir, RuleServer) {
    let tmp = TempDir::new().expect("tempdir");
    RuleStore::init(tmp.path()).expect("open store");
    let server = RuleServer::new(tmp.path().to_path_buf());
    (tmp, server)
}

fn extract_json(result: rmcp::model::CallToolResult) -> Value {
    let text = result
        .content
        .iter()
        .find_map(|content| {
            if let rmcp::model::RawContent::Text(text) = &content.raw {
                Some(text.text.clone())
            } else {
                None
            }
        })
        .expect("text content");
    serde_json::from_str(&text).expect("parse json")
}

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

#[tokio::test]
async fn rule_update_accepts_sparse_payload_and_returns_minimal_response() {
    let (_tmp, server) = make_sandbox();

    let created = extract_json(
        server
            .rule_create(Parameters(CreateRuleInput {
                workspace: _tmp.path().display().to_string(),
                title: "Sparse Rule".to_string(),
                slug: "shared/tests/sparse-rule".to_string(),
                file_kind: "AGENTS".to_string(),
                section: "tests".to_string(),
                body: Some("initial body".to_string()),
                repo_scope: vec![],
                path_scope: vec![],
                order_key: None,
                source_repo: None,
                source_path: None,
                source_start_line: None,
                source_end_line: None,
            }))
            .await
            .expect("create rule"),
    );
    let rule_id = created["id"].as_str().unwrap().to_string();

    let updated = extract_json(
        server
            .rule_update(Parameters(UpdateRuleInput {
                id: rule_id,
                fields: None,
                field_map: None,
                to_state: Some("reviewed".to_string()),
                body: None,
            }))
            .await
            .expect("update rule"),
    );

    assert_eq!(updated["status"], "ok");
    assert_eq!(updated["state_transition"]["to"], "reviewed");
    assert!(updated.get("rule").is_none());
    assert!(updated.get("changed_fields").is_none());
}

#[tokio::test]
async fn rule_move_preflight_returns_supported_plan() {
    let (_tmp, server) = make_sandbox();
    run_git(_tmp.path(), &["init"]);
    let target_workspace = _tmp.path().join("target");
    std::fs::create_dir_all(&target_workspace).unwrap();
    RuleStore::init(&target_workspace).unwrap();

    let created = extract_json(
        server
            .rule_create(Parameters(CreateRuleInput {
                workspace: _tmp.path().display().to_string(),
                title: "Movable Rule".to_string(),
                slug: "shared/tests/movable-rule".to_string(),
                file_kind: "AGENTS".to_string(),
                section: "tests".to_string(),
                body: Some("body".to_string()),
                repo_scope: vec![],
                path_scope: vec![],
                order_key: None,
                source_repo: None,
                source_path: None,
                source_start_line: None,
                source_end_line: None,
            }))
            .await
            .expect("create rule"),
    );
    let rule_id = created["id"].as_str().unwrap().to_string();

    let result = server
        .rule_move_preflight(Parameters(RuleMoveInput {
            id: rule_id,
            to_workspace_root: target_workspace.display().to_string(),
        }))
        .await
        .expect("move preflight");
    let json = extract_json(result);

    assert_eq!(json["status"], "ok");
    assert_eq!(json["mode"], "preflight");
    assert_eq!(json["supported"], true);
}
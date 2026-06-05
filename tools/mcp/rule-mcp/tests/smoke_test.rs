use rmcp::handler::server::wrapper::Parameters;
use rule_api::RuleStore;
use rule_mcp::server::{
    CreateRuleInput,
    RuleServer,
    UpdateRuleInput,
};
use serde_json::Value;
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

#[tokio::test]
async fn rule_update_accepts_sparse_payload_and_returns_minimal_response() {
    let (_tmp, server) = make_sandbox();

    let created = extract_json(
        server
            .rule_create(Parameters(CreateRuleInput {
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
//! Parity coverage for the self-describing capability catalog.
//!
//! Asserts the ticket-MCP `ticket_capabilities` tool surfaces the same core
//! ticket/spec/rule workflows (including a rule-oriented workflow) that the
//! shared `ticket-api` catalog defines and the CLI `catalog` command emits.

use rmcp::model::CallToolResult;
use serde_json::Value;
use tempfile::TempDir;
use ticket_mcp::server::TicketServer;

fn make_sandbox() -> (TempDir, TicketServer) {
    let tmp = TempDir::new().expect("tempdir");
    let server = TicketServer::new(tmp.path().to_path_buf());
    (tmp, server)
}

fn extract_json(result: CallToolResult) -> Value {
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

fn domain_names(catalog: &Value) -> Vec<String> {
    catalog["domains"]
        .as_array()
        .expect("domains array")
        .iter()
        .filter_map(|d| d["domain"].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn ticket_capabilities_lists_ticket_spec_rule_workflows() {
    let (_tmp, server) = make_sandbox();

    let result =
        server.ticket_capabilities().await.expect("capabilities call");
    let catalog = extract_json(result);

    let domains = domain_names(&catalog);
    assert!(domains.contains(&"ticket".to_string()));
    assert!(domains.contains(&"spec".to_string()));
    assert!(domains.contains(&"rule".to_string()));

    // A rule-oriented workflow must be reachable from the catalog.
    let rule = catalog["domains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["domain"] == "rule")
        .expect("rule domain");
    let rule_workflows: Vec<&str> = rule["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w["name"].as_str())
        .collect();
    assert!(
        rule_workflows.iter().any(|w| w.contains("generate")),
        "catalog must expose a rule authoring/generation workflow: {rule_workflows:?}"
    );

    // Parity gaps must be documented.
    assert!(catalog["parity_gaps"].as_array().is_some_and(|g| !g.is_empty()));
}

#[tokio::test]
async fn mcp_catalog_matches_shared_ticket_api_catalog() {
    let (_tmp, server) = make_sandbox();
    let result =
        server.ticket_capabilities().await.expect("capabilities call");
    let mcp_catalog = extract_json(result);

    // MCP surface must emit the exact shared catalog so CLI and MCP agree.
    let shared =
        ticket_api::contracts::capability_catalog::capability_catalog();
    assert_eq!(mcp_catalog, shared);
}

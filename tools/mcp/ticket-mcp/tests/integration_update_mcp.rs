use std::collections::BTreeMap;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;
use tempfile::TempDir;
use ticket_api::storage::store::TicketStore;
use ticket_mcp::server::{
    TicketServer,
    UpdateTicketInput,
};

fn make_sandbox() -> (TempDir, TicketServer) {
    let tmp = TempDir::new().expect("tempdir");
    let server = TicketServer::new(tmp.path().to_path_buf());
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
async fn update_ticket_accepts_sparse_payload_and_returns_minimal_response() {
    let (tmp, server) = make_sandbox();
    let store = TicketStore::init(tmp.path()).expect("open store");
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some("Sparse Ticket"),
            Some("new"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket");

    let result = server
        .update_ticket(Parameters(UpdateTicketInput {
            workspace: "default".to_string(),
            id: ticket_id.to_string(),
            transition_states: vec![],
            to_state: Some("ready".to_string()),
            fields: None,
            field_map: None,
            undo: false,
            description: None,
            author: None,
        }))
        .await
        .expect("update ticket");
    let json = extract_json(result);

    assert_eq!(json["status"], "ok");
    assert_eq!(json["id"], ticket_id.to_string());
    assert_eq!(json["state_transition"]["to"], "ready");
    assert!(json.get("ticket").is_none());
    assert!(json.get("changed_fields").is_none());
    assert!(json.get("workspace").is_none());
}

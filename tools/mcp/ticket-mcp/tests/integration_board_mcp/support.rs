use std::{
    collections::BTreeMap,
    path::Path,
};

use tempfile::TempDir;
use ticket_api::storage::store::TicketStore;
use ticket_mcp::server::TicketServer;

pub(super) fn make_sandbox() -> (TempDir, TicketServer) {
    let tmp = TempDir::new().expect("tempdir");
    TicketStore::init(tmp.path()).expect("init ticket store");
    let server = TicketServer::new(tmp.path().to_path_buf());
    (tmp, server)
}

pub(super) fn seed_ticket(
    store_root: &Path,
    title: &str,
) -> String {
    let store = TicketStore::init(store_root).expect("open store");
    let ticket_id = store
        .create(
            None,
            "tracker-improvement",
            Some(title),
            Some("open"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket");
    ticket_id.to_string()
}

pub(super) fn ws() -> String {
    "default".to_string()
}

pub(super) fn extract_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .find_map(|content| {
            if let rmcp::model::RawContent::Text(text) = &content.raw {
                Some(text.text.clone())
            } else {
                None
            }
        })
        .expect("text content in result")
}

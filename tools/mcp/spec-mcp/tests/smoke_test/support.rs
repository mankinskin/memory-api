use serde_json::Value;
use tempfile::TempDir;

use spec_api::SpecStore;
use spec_mcp::server::SpecServer;

pub(super) fn make_sandbox() -> (TempDir, SpecServer) {
    let tmp = TempDir::new().expect("tempdir");
    let store = SpecStore::init(tmp.path()).expect("open store");
    store
        .entity_store()
        .add_scan_root(memory_api::model::filesystem::ScanRoot {
            path: tmp.path().join("specs"),
            label: "test-specs".to_string(),
        })
        .expect("add scan root");
    drop(store);

    let server = SpecServer::new(tmp.path().to_path_buf());
    (tmp, server)
}

pub(super) fn extract_json(result: rmcp::model::CallToolResult) -> Value {
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
        .expect("text content in result");
    serde_json::from_str(&text).expect("parse json")
}

use std::{
    collections::BTreeMap,
    process::Command,
};

use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;
use tempfile::TempDir;
use ticket_api::storage::store::TicketStore;
use ticket_mcp::server::{
    MoveApplyInput,
    MovePreflightInput,
    TicketServer,
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

fn make_sandbox() -> (TempDir, TicketServer) {
    let tmp = TempDir::new().expect("tempdir");
    run_git(tmp.path(), &["init"]);
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
async fn move_tools_preflight_and_apply_smoke() {
    let (tmp, server) = make_sandbox();
    let source_store = TicketStore::init(tmp.path()).expect("open store");

    let target_workspace = tmp.path().join("target-workspace");
    std::fs::create_dir_all(&target_workspace)
        .expect("create target workspace");
    let target_store =
        TicketStore::init(&target_workspace).expect("init target store");

    let ticket_id = source_store
        .create(
            None,
            "tracker-improvement",
            Some("Move via MCP"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create ticket");

    let preflight = server
        .move_preflight(Parameters(MovePreflightInput {
            workspace: "default".to_string(),
            id: ticket_id.to_string(),
            to_workspace_root: target_workspace.to_string_lossy().to_string(),
        }))
        .await
        .expect("move preflight");
    let preflight_json = extract_json(preflight);
    assert_eq!(preflight_json["status"], "ok");
    assert_eq!(preflight_json["mode"], "preflight");
    assert!(preflight_json["plan"]["source_ticket_path"].is_string());
    assert!(preflight_json["plan"]["destination_ticket_path"].is_string());

    let apply = server
        .move_apply(Parameters(MoveApplyInput {
            workspace: "default".to_string(),
            id: ticket_id.to_string(),
            to_workspace_root: target_workspace.to_string_lossy().to_string(),
        }))
        .await
        .expect("move apply");
    let apply_json = extract_json(apply);
    assert_eq!(apply_json["status"], "ok");
    assert_eq!(apply_json["mode"], "apply");
    assert!(apply_json["outcome"]["journal"]["id"].is_string());

    assert!(
        source_store
            .get_indexed(&ticket_id)
            .expect("source lookup")
            .is_none()
    );
    assert!(
        target_store
            .get_indexed(&ticket_id)
            .expect("target lookup")
            .is_some()
    );
}

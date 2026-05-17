use std::collections::BTreeMap;

use rmcp::handler::server::wrapper::Parameters;
use serde_json::{
    Value,
    json,
};
use ticket_api::storage::store::TicketStore;
use ticket_mcp::server::{
    BoardShowInput,
    NextTicketsInput,
};

use super::support::{
    extract_text,
    make_sandbox,
    seed_ticket,
    ws,
};

#[tokio::test]
async fn board_show_parity_store_and_mcp() {
    let (tmp, server) = make_sandbox();
    let ticket_id_str = seed_ticket(tmp.path(), "parity test ticket");
    let ticket_uuid: uuid::Uuid = ticket_id_str.parse().expect("valid uuid");

    let store = TicketStore::open(tmp.path()).expect("open store");
    let entry = store
        .board_check_in(
            &ticket_uuid,
            "parity-agent",
            3600,
            "cross-interface work",
            vec!["parity.rs".to_string()],
        )
        .expect("check-in via store");

    let result = server
        .board_show(Parameters(BoardShowInput {
            workspace: ws(),
            agent_id: None,
        }))
        .await
        .expect("board_show via MCP");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");

    assert_eq!(
        json["snapshot"]["active_count"], 1,
        "MCP board_show must reflect store-inserted entry"
    );

    let entries = json["snapshot"]["entries"]
        .as_array()
        .expect("entries array");
    assert_eq!(entries.len(), 1, "exactly one entry in snapshot");
    assert_eq!(
        entries[0]["agent_id"], "parity-agent",
        "agent_id must match"
    );
    assert_eq!(
        entries[0]["entry_id"].as_str().unwrap_or(""),
        entry.entry_id.to_string(),
        "entry_id must match"
    );

    let owned_files = entries[0]["owned_files"]
        .as_array()
        .expect("owned_files array");
    assert!(
        owned_files
            .iter()
            .any(|file| file.as_str() == Some("parity.rs")),
        "parity.rs must appear in owned_files"
    );

    let _ = tmp;
}

#[tokio::test]
async fn next_tickets_excludes_board_active_and_surfaces_wip_warning() {
    let (tmp, server) = make_sandbox();

    let t_active = seed_ticket(tmp.path(), "active board ticket");
    let t_free = seed_ticket(tmp.path(), "free candidate ticket");

    {
        let store = TicketStore::open(tmp.path()).expect("open store");
        for id_str in [&t_active, &t_free] {
            let uid: uuid::Uuid = id_str.parse().expect("uuid");
            store
                .update(
                    &uid,
                    Default::default(),
                    None,
                    Some("ready"),
                    None,
                    None,
                )
                .expect("ready");
        }

        store
            .board_configure(Some(ticket_api::BoardConfig {
                max_wip: 1,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            }))
            .expect("configure wip");

        let uid: uuid::Uuid = t_active.parse().expect("uuid");
        store
            .board_check_in(&uid, "exclusion-agent", 3600, "in flight", vec![])
            .expect("check-in");
    }

    let result = server
        .next_tickets(Parameters(NextTicketsInput {
            workspace: ws(),
            limit: None,
            filter: None,
        }))
        .await
        .expect("next_tickets ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");

    assert!(json.get("board").is_none(), "next_tickets should not return a duplicate board snapshot: {json:#?}");

    let warnings = json["warnings"].as_array().expect("warnings array");
    assert!(
        warnings.iter().any(|warning| warning.as_str().unwrap_or("").contains("WIP limit reached")),
        "wip limit warning must still be surfaced: {warnings:?}"
    );

    let excluded = json["excluded_by_board"]
        .as_array()
        .expect("excluded_by_board array");
    assert!(
        excluded
            .iter()
            .any(|entry| entry["ticket_id"].as_str().unwrap_or("") == t_active),
        "active ticket must be in excluded_by_board: {excluded:?}"
    );
    assert!(
        !excluded
            .iter()
            .any(|entry| entry["ticket_id"].as_str().unwrap_or("") == t_free),
        "free ticket must not be in excluded_by_board: {excluded:?}"
    );

    let items = json["items"].as_array().expect("items array");
    assert!(
        items
            .iter()
            .any(|candidate| candidate["id"].as_str().unwrap_or("") == t_free),
        "free ticket must appear in items: {items:?}"
    );
    assert!(
        !items
            .iter()
            .any(|candidate| candidate["id"].as_str().unwrap_or("") == t_active),
        "board-active ticket must not appear in items: {items:?}"
    );

    let _ = tmp;
}

#[tokio::test]
async fn next_tickets_prefers_newer_candidates_before_older_ones() {
    let (tmp, server) = make_sandbox();

    let older;
    let newer;
    {
        let store = TicketStore::open(tmp.path()).expect("open store");
        let fields = BTreeMap::from([(String::from("priority"), json!("high"))]);

        older = store
            .create(
                None,
                "tracker-improvement",
                Some("Alpha older candidate"),
                Some("ready"),
                fields.clone(),
                None,
                None,
            )
            .expect("create older ticket")
            .to_string();

        newer = store
            .create(
                None,
                "tracker-improvement",
                Some("Zulu newer candidate"),
                Some("ready"),
                fields,
                None,
                None,
            )
            .expect("create newer ticket")
            .to_string();
    }

    let result = server
        .next_tickets(Parameters(NextTicketsInput {
            workspace: ws(),
            limit: None,
            filter: None,
        }))
        .await
        .expect("next_tickets ok");
    let text = extract_text(&result);
    let json: Value = serde_json::from_str(&text).expect("valid json");
    let items = json["items"].as_array().expect("items array");

    assert!(items.len() >= 2, "expected at least two candidates: {items:?}");
    assert_eq!(items[0]["id"].as_str(), Some(newer.as_str()));
    assert_eq!(items[1]["id"].as_str(), Some(older.as_str()));

    let _ = tmp;
}

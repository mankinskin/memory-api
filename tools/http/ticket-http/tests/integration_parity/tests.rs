use rmcp::handler::server::wrapper::Parameters;
use serde_json::{
    Value,
    json,
};
use std::collections::BTreeMap;
use ticket_api::{
    BoardConfig,
    model::edge::EdgeRecord,
    workflow::WorkflowModel,
};
use ticket_mcp::server::NextTicketsInput;

use super::integration_parity_fixture::*;

#[tokio::test]
async fn workflow_next_candidates_parity_across_http_and_mcp() {
    let fx = ParityFixture::build();

    // ── ticket-api (canonical) ────────────────────────────────────────────
    let api_candidates = fx.api_next_candidates();
    assert!(
        api_candidates.contains(&fx.alpha_id),
        "ticket-api: alpha must be an actionable candidate; got {api_candidates:?}"
    );
    assert!(
        api_candidates.contains(&fx.beta_id),
        "ticket-api: beta must be an actionable candidate; got {api_candidates:?}"
    );
    // Newer candidate (beta) should sort before older (alpha) at equal priority.
    let alpha_pos = api_candidates
        .iter()
        .position(|id| id == &fx.alpha_id)
        .expect("alpha in api candidates");
    let beta_pos = api_candidates
        .iter()
        .position(|id| id == &fx.beta_id)
        .expect("beta in api candidates");
    assert!(
        beta_pos < alpha_pos,
        "ticket-api: beta (newer) must rank before alpha (older); \
         beta_pos={beta_pos} alpha_pos={alpha_pos}"
    );

    // ── HTTP ──────────────────────────────────────────────────────────────
    let app = fx.http_router();
    let ws = &fx.workspace;
    let http =
        http_get_json(app, format!("/api/workflow/next?workspace={ws}")).await;

    let http_items = http["items"].as_array().expect("items array in HTTP");
    let http_ids: Vec<String> = http_items
        .iter()
        .map(|item| item["id"].as_str().unwrap_or("").to_owned())
        .collect();

    assert!(
        http_ids.contains(&fx.alpha_id),
        "HTTP: alpha must appear in items; got {http_ids:?}"
    );
    assert!(
        http_ids.contains(&fx.beta_id),
        "HTTP: beta must appear in items; got {http_ids:?}"
    );
    let http_alpha_pos =
        http_ids.iter().position(|id| id == &fx.alpha_id).unwrap();
    let http_beta_pos =
        http_ids.iter().position(|id| id == &fx.beta_id).unwrap();
    assert!(
        http_beta_pos < http_alpha_pos,
        "HTTP: beta (newer) must rank before alpha (older); \
         http_beta_pos={http_beta_pos} http_alpha_pos={http_alpha_pos}"
    );

    // scope metadata must be present
    assert!(
        http["scope"]["active_index_root"].as_str().is_some(),
        "HTTP: scope.active_index_root must be present"
    );
    assert_eq!(http["scope"]["workspace"].as_str().unwrap(), ws.as_str());
    assert_eq!(http["excluded_by_board"], json!([]));
    assert_eq!(http["warnings"], json!([]));

    // ── MCP ───────────────────────────────────────────────────────────────
    let server = fx.mcp_server();
    let mcp_result = server
        .next_tickets(Parameters(NextTicketsInput {
            workspace: mcp_ws(),
            filter: None,
            root: None,
            limit: None,
        }))
        .await
        .expect("MCP next_tickets");
    let mcp_text = extract_text(&mcp_result);
    let mcp: Value =
        serde_json::from_str(&mcp_text).expect("valid JSON from MCP");

    let mcp_items = mcp["items"].as_array().expect("items array in MCP");
    let mcp_ids: Vec<String> = mcp_items
        .iter()
        .map(|item| item["id"].as_str().unwrap_or("").to_owned())
        .collect();

    assert!(
        mcp_ids.contains(&fx.alpha_id),
        "MCP: alpha must appear in items (no board entry); got {mcp_ids:?}"
    );
    assert!(
        mcp_ids.contains(&fx.beta_id),
        "MCP: beta must appear in items; got {mcp_ids:?}"
    );
    let mcp_alpha_pos =
        mcp_ids.iter().position(|id| id == &fx.alpha_id).unwrap();
    let mcp_beta_pos = mcp_ids.iter().position(|id| id == &fx.beta_id).unwrap();
    assert!(
        mcp_beta_pos < mcp_alpha_pos,
        "MCP: beta (newer) must rank before alpha (older); \
         mcp_beta_pos={mcp_beta_pos} mcp_alpha_pos={mcp_alpha_pos}"
    );
    assert_eq!(mcp["excluded_by_board"], json!([]));
    assert_eq!(mcp["warnings"], json!([]));

    // ── Cross-surface ordering agreement ─────────────────────────────────
    // HTTP and MCP must agree on alpha/beta relative order.
    assert_eq!(
        http_beta_pos < http_alpha_pos,
        mcp_beta_pos < mcp_alpha_pos,
        "HTTP and MCP must agree on beta-before-alpha ordering"
    );
    // gamma must not appear in any surface (it is blocked).
    let gamma_id = &fx._gamma_id;
    assert!(
        !http_ids.iter().any(|id| id == gamma_id),
        "HTTP: gamma (blocked) must not appear in items"
    );
    assert!(
        !mcp_ids.iter().any(|id| id == gamma_id),
        "MCP: gamma (blocked) must not appear in items"
    );
}

#[tokio::test]
async fn workflow_next_root_scope_parity_across_http_and_mcp() {
    let fx = ParityFixture::build();

    let root = fx
        .store
        .create(
            None,
            "tracker-improvement",
            Some("[parity] Root unblock target"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create root");
    let direct = fx
        .store
        .create(
            None,
            "tracker-improvement",
            Some("[parity] Direct blocker"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create direct blocker");
    let intermediate = fx
        .store
        .create(
            None,
            "tracker-improvement",
            Some("[parity] Intermediate blocker"),
            Some("open"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create intermediate blocker");
    let nested_leaf = fx
        .store
        .create(
            None,
            "tracker-improvement",
            Some("[parity] Nested leaf blocker"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create nested blocker");
    let unrelated = fx
        .store
        .create(
            None,
            "tracker-improvement",
            Some("[parity] Unrelated ready"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create unrelated");

    for (from, to) in [
        (root, direct),
        (root, intermediate),
        (intermediate, nested_leaf),
    ] {
        fx.store
            .add_edge(EdgeRecord {
                from,
                to,
                kind: String::from("depends_on"),
                created_at: chrono::Utc::now(),
            })
            .expect("add depends_on edge");
    }

    let tickets = fx.store.list(None, None, None).expect("list");
    let edges = fx.store.list_all_edges().expect("edges");
    let model =
        WorkflowModel::build(&fx.store, tickets, edges).expect("build model");
    let scope = model.root_blocker_scope(root).expect("root blocker scope");
    let mut expected_ids =
        model.actionable_candidate_ids(Some(&scope.remaining_blockers));
    model.sort_candidate_ids(&mut expected_ids);
    let expected_ids: Vec<String> =
        expected_ids.into_iter().map(|id| id.to_string()).collect();

    // HTTP
    let app = fx.http_router();
    let ws = &fx.workspace;
    let http = http_get_json(
        app,
        format!("/api/workflow/next?workspace={ws}&root={root}"),
    )
    .await;
    let http_ids: Vec<String> = http["items"]
        .as_array()
        .expect("http items")
        .iter()
        .map(|item| item["id"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(
        http_ids, expected_ids,
        "HTTP root-scoped items must match API model"
    );
    assert_eq!(http["reachable_dependencies"], scope.reachable_dependencies);
    assert_eq!(http["blocked_dependencies"], scope.blocked_dependencies);
    assert_eq!(
        http["remaining_blocker_count"],
        scope.remaining_blockers.len()
    );
    assert_eq!(http["frontier_count"], expected_ids.len());
    assert_eq!(http["blocker_tree"]["id"], root.to_string());

    // MCP
    let server = fx.mcp_server();
    let mcp_result = server
        .next_tickets(Parameters(NextTicketsInput {
            workspace: mcp_ws(),
            filter: None,
            root: Some(root.to_string()),
            limit: None,
        }))
        .await
        .expect("MCP next_tickets root scope");
    let mcp_text = extract_text(&mcp_result);
    let mcp: Value =
        serde_json::from_str(&mcp_text).expect("valid JSON from MCP");
    let mcp_ids: Vec<String> = mcp["items"]
        .as_array()
        .expect("mcp items")
        .iter()
        .map(|item| item["id"].as_str().unwrap_or("").to_owned())
        .collect();

    assert_eq!(
        mcp_ids, expected_ids,
        "MCP root-scoped items must match API model"
    );
    assert_eq!(mcp["reachable_dependencies"], scope.reachable_dependencies);
    assert_eq!(mcp["blocked_dependencies"], scope.blocked_dependencies);
    assert_eq!(
        mcp["remaining_blocker_count"],
        scope.remaining_blockers.len()
    );
    assert_eq!(mcp["frontier_count"], expected_ids.len());
    assert_eq!(mcp["blocker_tree"]["id"], root.to_string());

    assert!(
        !http_ids.contains(&unrelated.to_string()),
        "root scope must exclude unrelated actionable tickets"
    );
    assert_eq!(
        http_ids, mcp_ids,
        "HTTP and MCP root-scoped next must match"
    );
}

// ── health findings parity ────────────────────────────────────────────────────

/// The `missing_description` check and its severity must be identical across
/// ticket-api, HTTP, and MCP when checked against the same fixture store.
#[tokio::test]
async fn health_findings_parity_across_http_and_mcp() {
    let fx = ParityFixture::build();

    // ── ticket-api (canonical) ────────────────────────────────────────────
    let api_findings = fx.api_health_findings();
    // alpha has no description → exactly one missing_description warning
    let api_alpha_missing: Vec<_> = api_findings
        .iter()
        .filter(|(id, check, _sev)| {
            id == &fx.alpha_id && check == "missing_description"
        })
        .collect();
    assert_eq!(
        api_alpha_missing.len(),
        1,
        "ticket-api: alpha must have exactly one missing_description finding; got {api_findings:?}"
    );
    assert_eq!(
        api_alpha_missing[0].2, "warning",
        "ticket-api: missing_description severity must be 'warning'"
    );
    // beta has a good description → no missing_description
    let api_beta_missing: Vec<_> = api_findings
        .iter()
        .filter(|(id, check, _)| {
            id == &fx.beta_id && check == "missing_description"
        })
        .collect();
    assert!(
        api_beta_missing.is_empty(),
        "ticket-api: beta must not have missing_description finding; got {api_findings:?}"
    );

    // ── HTTP ──────────────────────────────────────────────────────────────
    let app = fx.http_router();
    let ws = &fx.workspace;
    let http = http_get_json(
        app,
        format!("/api/graph/health?workspace={ws}&all=true"),
    )
    .await;

    let http_findings = http["findings"].as_array().expect("findings array");
    let http_alpha_missing: Vec<_> = http_findings
        .iter()
        .filter(|f| {
            f["ticket_id"].as_str() == Some(&fx.alpha_id)
                && f["check"].as_str() == Some("missing_description")
        })
        .collect();
    assert_eq!(
        http_alpha_missing.len(),
        1,
        "HTTP: alpha must have exactly one missing_description finding; got {http_findings:?}"
    );
    assert_eq!(
        http_alpha_missing[0]["severity"].as_str().unwrap_or(""),
        "warning",
        "HTTP: missing_description severity must be 'warning'"
    );
    let http_beta_missing: Vec<_> = http_findings
        .iter()
        .filter(|f| {
            f["ticket_id"].as_str() == Some(&fx.beta_id)
                && f["check"].as_str() == Some("missing_description")
        })
        .collect();
    assert!(
        http_beta_missing.is_empty(),
        "HTTP: beta must not have missing_description finding; got {http_findings:?}"
    );

    // summary must count the finding (at least 1; gamma also lacks a description)
    assert!(
        http["summary"]["missing_description"].as_u64().unwrap_or(0) >= 1,
        "HTTP: summary.missing_description must be ≥ 1"
    );

    // ── MCP ───────────────────────────────────────────────────────────────
    let server = fx.mcp_server();
    let mcp_result = server
        .run_health_checks(
            &mcp_ws(),
            None, // root
            true, // all
            &[],  // ids
            None, // depth
            None, // direction
            &[],  // where
        )
        .await
        .expect("MCP run_health_checks");
    let mcp_text = extract_text(&mcp_result);
    let mcp: Value =
        serde_json::from_str(&mcp_text).expect("valid JSON from MCP");

    let mcp_findings =
        mcp["findings"].as_array().expect("findings array in MCP");
    let mcp_alpha_missing: Vec<_> = mcp_findings
        .iter()
        .filter(|f| {
            f["ticket_id"].as_str() == Some(&fx.alpha_id)
                && f["check"].as_str() == Some("missing_description")
        })
        .collect();
    assert_eq!(
        mcp_alpha_missing.len(),
        1,
        "MCP: alpha must have exactly one missing_description finding; got {mcp_findings:?}"
    );
    assert_eq!(
        mcp_alpha_missing[0]["severity"].as_str().unwrap_or(""),
        "warning",
        "MCP: missing_description severity must be 'warning'"
    );
    let mcp_beta_missing: Vec<_> = mcp_findings
        .iter()
        .filter(|f| {
            f["ticket_id"].as_str() == Some(&fx.beta_id)
                && f["check"].as_str() == Some("missing_description")
        })
        .collect();
    assert!(
        mcp_beta_missing.is_empty(),
        "MCP: beta must not have missing_description finding; got {mcp_findings:?}"
    );
    assert!(
        mcp["summary"]["missing_description"].as_u64().unwrap_or(0) >= 1,
        "MCP: summary.missing_description must be ≥ 1"
    );

    // ── Cross-surface finding agreement ───────────────────────────────────
    // Both HTTP and MCP must agree on the summary count.
    assert_eq!(
        http["summary"]["missing_description"],
        mcp["summary"]["missing_description"],
        "HTTP and MCP must agree on missing_description summary count"
    );
    // finding_count must be ≥ 1 on both surfaces.
    assert!(
        http["finding_count"].as_u64().unwrap_or(0) >= 1,
        "HTTP: finding_count must be ≥ 1"
    );
    assert!(
        mcp["finding_count"].as_u64().unwrap_or(0) >= 1,
        "MCP: finding_count must be ≥ 1"
    );
    assert_eq!(
        http["finding_count"], mcp["finding_count"],
        "HTTP and MCP must agree on total finding_count"
    );
}

// ── Board-aware next parity ───────────────────────────────────────────────────

/// HTTP and MCP must both apply the shared board-aware `next` semantics:
/// active board tickets leave `items`, appear in `excluded_by_board`, and
/// still surface board warnings.
#[tokio::test]
async fn board_aware_next_parity_across_http_and_mcp() {
    let fx = ParityFixture::build();

    fx.store
        .board_configure(Some(BoardConfig {
            max_wip: 1,
            stale_after_secs: 3600,
            completed_audit_window_secs: 3600,
        }))
        .expect("configure board");

    let alpha_uuid: uuid::Uuid = fx.alpha_id.parse().expect("uuid");
    fx.store
        .board_check_in(
            &alpha_uuid,
            "parity-agent",
            3600,
            "in-flight work",
            vec!["parity.rs".to_string()],
            None,
            None,
            None,
        )
        .expect("board check-in");

    let (api_ids, api_excluded, api_warnings) =
        fx.api_board_filtered_candidates();
    assert_eq!(api_ids, vec![fx.beta_id.clone()]);
    assert_eq!(api_excluded, vec![fx.alpha_id.clone()]);
    assert!(
        api_warnings
            .iter()
            .any(|warning| warning.contains("WIP limit reached")),
        "ticket-api helper must surface WIP warning; got {api_warnings:?}"
    );

    // ── HTTP ──────────────────────────────────────────────────────────────
    let app = fx.http_router();
    let ws = &fx.workspace;
    let http =
        http_get_json(app, format!("/api/workflow/next?workspace={ws}")).await;
    let http_items = http["items"].as_array().expect("items");
    let http_ids: Vec<String> = http_items
        .iter()
        .map(|item| item["id"].as_str().unwrap_or("").to_owned())
        .collect();
    let http_excluded = http["excluded_by_board"].as_array().expect("excluded");
    let http_warning_strings: Vec<String> = http["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .filter_map(|warning| warning.as_str().map(ToOwned::to_owned))
        .collect();
    assert_eq!(
        http_ids, api_ids,
        "HTTP visible items must match shared helper"
    );
    assert_eq!(
        http_excluded[0]["ticket_id"].as_str(),
        Some(fx.alpha_id.as_str()),
        "HTTP excluded_by_board must match shared helper"
    );
    assert!(
        http_warning_strings
            .iter()
            .any(|warning| warning.contains("WIP limit reached")),
        "HTTP warnings must include WIP limit warning; got {http_warning_strings:?}"
    );

    // ── MCP ───────────────────────────────────────────────────────────────
    let server = fx.mcp_server();
    let mcp_result = server
        .next_tickets(Parameters(NextTicketsInput {
            workspace: mcp_ws(),
            filter: None,
            root: None,
            limit: None,
        }))
        .await
        .expect("MCP next_tickets");
    let mcp_text = extract_text(&mcp_result);
    let mcp: Value =
        serde_json::from_str(&mcp_text).expect("valid JSON from MCP");

    let mcp_items = mcp["items"].as_array().expect("items");
    let mcp_ids: Vec<String> = mcp_items
        .iter()
        .map(|item| item["id"].as_str().unwrap_or("").to_owned())
        .collect();
    let excluded = mcp["excluded_by_board"]
        .as_array()
        .expect("excluded_by_board");
    let mcp_warning_strings: Vec<String> = mcp["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .filter_map(|warning| warning.as_str().map(ToOwned::to_owned))
        .collect();
    assert_eq!(
        mcp_ids, api_ids,
        "MCP visible items must match shared helper"
    );
    assert_eq!(
        excluded[0]["ticket_id"].as_str(),
        Some(fx.alpha_id.as_str()),
        "MCP excluded_by_board must match shared helper"
    );
    assert!(
        mcp_warning_strings
            .iter()
            .any(|warning| warning.contains("WIP limit reached")),
        "MCP warnings must include WIP limit warning; got {mcp_warning_strings:?}"
    );

    assert_eq!(http_ids, mcp_ids, "HTTP and MCP visible items must match");
    assert_eq!(
        http_excluded[0]["ticket_id"], excluded[0]["ticket_id"],
        "HTTP and MCP excluded_by_board must match"
    );
}

// ── scope metadata parity ──────────────────────────────────────────────────────

/// HTTP and MCP must both emit `scope.active_index_root` pointing to the
/// same store root path.
#[tokio::test]
async fn scope_active_index_root_parity_http_and_mcp() {
    let fx = ParityFixture::build();

    // HTTP scope
    let app = fx.http_router();
    let ws = &fx.workspace;
    let http =
        http_get_json(app, format!("/api/workflow/next?workspace={ws}")).await;
    let http_index_root = http["scope"]["active_index_root"]
        .as_str()
        .expect("HTTP scope.active_index_root must be a string")
        .to_owned();
    assert!(
        !http_index_root.is_empty(),
        "HTTP scope.active_index_root must not be empty"
    );

    // MCP scope (embedded in next_tickets response)
    let server = fx.mcp_server();
    let mcp_result = server
        .next_tickets(Parameters(NextTicketsInput {
            workspace: mcp_ws(),
            filter: None,
            root: None,
            limit: None,
        }))
        .await
        .expect("MCP next_tickets");
    let mcp_text = extract_text(&mcp_result);
    let mcp: Value =
        serde_json::from_str(&mcp_text).expect("valid JSON from MCP");
    let mcp_index_root = mcp["scope"]["active_index_root"]
        .as_str()
        .expect("MCP scope.active_index_root must be a string")
        .to_owned();
    assert!(
        !mcp_index_root.is_empty(),
        "MCP scope.active_index_root must not be empty"
    );

    // Both must point to the same store (path may differ by separator style,
    // so normalise to forward slashes before comparing).
    let normalise = |p: &str| p.replace('\\', "/");
    assert_eq!(
        normalise(&http_index_root),
        normalise(&mcp_index_root),
        "HTTP and MCP scope.active_index_root must point to the same store root"
    );
}

fn extract_text(result: &rmcp::model::CallToolResult) -> String {
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
        .unwrap_or_default()
}

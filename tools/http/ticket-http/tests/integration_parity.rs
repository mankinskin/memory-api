//! Cross-interface parity tests for workflow and health surfaces.
//!
//! These tests verify that the HTTP, MCP, and ticket-api (the common data
//! layer) surfaces produce equivalent results from the same shared fixture
//! store. Assertions ignore documented transport-envelope differences:
//!
//! ## Documented transport-envelope differences (not tested for parity)
//!
//! | Feature                  | ticket-api | HTTP | MCP  |
//! |--------------------------|------------|------|------|
//! | `scope` metadata field   | n/a        | ✓    | ✓    |
//!
//! Transport-local envelopes remain different, but board-aware workflow-next
//! semantics are shared across the ticket-api helper, HTTP, and MCP.
//!
//! ## Parity contract (what IS guaranteed equivalent)
//!
//! - Given the same fixture store, the set of actionable candidate IDs and
//!   their sort order are equal across ticket-api, HTTP, and MCP (when no
//!   board exclusion applies).
//! - Health finding `check` keys, `severity` values, and `ticket_id` targets
//!   are identical across ticket-api, HTTP, and MCP.
//! - `scope.active_index_root` is present and non-empty for HTTP and MCP.

use std::{
    collections::BTreeMap,
    sync::Arc,
};

use axum::{
    body::{
        Body,
        to_bytes,
    },
    http::{
        Request,
        StatusCode,
    },
};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{
    Value,
    json,
};
use ticket_api::{
    BoardConfig,
    health::collect_findings,
    model::edge::EdgeRecord,
    model::filesystem::ScanRoot,
    storage::store::TicketStore,
    workflow::{
        WorkflowModel,
        apply_board_filter,
    },
};
use ticket_mcp::server::{
    NextTicketsInput,
    TicketServer,
};
use tower::ServiceExt;

use ticket_http::serve::{
    AppState,
    StreamBroker,
    WorkspaceRegistry,
    routes::build_router,
};

// ── Shared fixture ─────────────────────────────────────────────────────────────

/// The parity fixture populates a shared store used by all three surfaces.
///
/// Fixture topology
/// ─────────────────
///  alpha  (ready, high, NO description)
///  beta   (ready, high, HAS description ≥ 50 chars)
///  gamma  (new, depends_on alpha AND beta) → blocked; not actionable
///
/// With this topology:
/// - alpha and beta are the only actionable candidates.
/// - alpha has a `missing_description` health finding (severity = "warning").
/// - beta does not have a `missing_description` finding.
/// - When alpha is board-checked-in, only beta appears in MCP/CLI next items.
struct ParityFixture {
    /// Keeps the temp dir alive for the duration of the test.
    _dir: tempfile::TempDir,
    pub store: Arc<TicketStore>,
    pub alpha_id: String,
    pub beta_id: String,
    pub _gamma_id: String,
    /// Workspace name as resolved by the HTTP registry.
    pub workspace: String,
}

impl ParityFixture {
    fn build() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(TicketStore::init(dir.path()).expect("init store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.path().join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");

        let high_fields =
            BTreeMap::from([(String::from("priority"), json!("high"))]);

        // alpha: ready, high priority, no description → triggers missing_description
        let alpha = store
            .create(
                None,
                "tracker-improvement",
                Some("[parity] Alpha — no description"),
                Some("ready"),
                high_fields.clone(),
                None,
                None, // no description.md
            )
            .expect("create alpha");

        // beta: ready, high priority, good description → no health findings
        let beta = store
            .create(
                None,
                "tracker-improvement",
                Some("[parity] Beta — with description"),
                Some("ready"),
                high_fields,
                None,
                Some("This parity-fixture description is definitely long enough to satisfy the fifty-character health check threshold."),
            )
            .expect("create beta");

        // gamma: new, depends on alpha AND beta → not actionable
        let gamma = store
            .create(
                None,
                "tracker-improvement",
                Some("[parity] Gamma — blocked"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create gamma");
        for dep in [alpha, beta] {
            store
                .add_edge(EdgeRecord {
                    from: gamma,
                    to: dep,
                    kind: String::from("depends_on"),
                    created_at: chrono::Utc::now(),
                })
                .expect("add depends_on");
        }

        let workspace = workspace_name_for(dir.path());

        Self {
            _dir: dir,
            store,
            alpha_id: alpha.to_string(),
            beta_id: beta.to_string(),
            _gamma_id: gamma.to_string(),
            workspace,
        }
    }

    /// Build an HTTP router backed by this fixture's store.
    fn http_router(&self) -> axum::Router {
        let registry =
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&self.store)));
        let state = AppState::new(registry, Arc::new(StreamBroker::new()));
        build_router(state)
    }

    /// Build an MCP TicketServer backed by this fixture's store root.
    fn mcp_server(&self) -> TicketServer {
        TicketServer::new(self.store.index_root.clone())
    }

    /// Collect workflow/next candidates directly via ticket-api (the canonical
    /// data layer shared by all adapters).
    fn api_next_candidates(&self) -> Vec<String> {
        let tickets = self.store.list(None, None, None).expect("list");
        let edges = self.store.list_all_edges().expect("edges");
        let model = WorkflowModel::build(&self.store, tickets, edges)
            .expect("build model");
        let mut candidates = model.actionable_candidate_ids(None);
        model.sort_candidate_ids(&mut candidates);
        candidates
            .into_iter()
            .map(|id| id.to_string())
            .collect()
    }

    /// Collect health findings directly via ticket-api.
    fn api_health_findings(&self) -> Vec<(String, String, String)> {
        let tickets = self.store.list(None, None, None).expect("list");
        let edges = self.store.list_all_edges().expect("edges");
        let workflow = WorkflowModel::build(
            &self.store,
            tickets.clone(),
            edges.clone(),
        )
        .expect("build model");
        let report = collect_findings(&self.store, &tickets, &edges, &workflow);
        report
            .findings
            .into_iter()
            .map(|f| (f.ticket_id.to_string(), f.check, f.severity))
            .collect()
    }

    fn api_board_filtered_candidates(&self) -> (Vec<String>, Vec<String>, Vec<String>) {
        let tickets = self.store.list(None, None, None).expect("list");
        let edges = self.store.list_all_edges().expect("edges");
        let model = WorkflowModel::build(&self.store, tickets, edges)
            .expect("build model");
        let mut candidates = model.actionable_candidate_ids(None);
        model.sort_candidate_ids(&mut candidates);
        let board_snap = self.store.board_show(None).ok();
        let filtered = apply_board_filter(candidates, board_snap.as_ref(), false);

        (
            filtered
                .candidates
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
            filtered
                .excluded_by_board
                .into_iter()
                .map(|entry| entry.ticket_id.to_string())
                .collect(),
            filtered.warnings,
        )
    }
}

fn workspace_name_for(dir: &std::path::Path) -> String {
    // WorkspaceRegistry::single_opened computes the name from the index_root
    // path and exposes it via `primary_workspace_name()`.
    let registry =
        WorkspaceRegistry::single_opened(Arc::new(TicketStore::init(dir).expect("open")));
    registry.primary_workspace_name().to_owned()
}

async fn http_get_json(app: axum::Router, uri: String) -> Value {
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "HTTP GET {}", "request failed");
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn mcp_ws() -> String {
    "default".to_string()
}

// ── workflow/next parity ──────────────────────────────────────────────────────

/// All three surfaces must agree on which tickets are actionable and in what
/// order, given a fixture store with no board entries.
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
    let http = http_get_json(
        app,
        format!("/api/workflow/next?workspace={ws}"),
    )
    .await;

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
    let http_alpha_pos = http_ids
        .iter()
        .position(|id| id == &fx.alpha_id)
        .unwrap();
    let http_beta_pos = http_ids
        .iter()
        .position(|id| id == &fx.beta_id)
        .unwrap();
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
    let mcp: Value = serde_json::from_str(&mcp_text).expect("valid JSON from MCP");

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
    let mcp_alpha_pos = mcp_ids
        .iter()
        .position(|id| id == &fx.alpha_id)
        .unwrap();
    let mcp_beta_pos = mcp_ids
        .iter()
        .position(|id| id == &fx.beta_id)
        .unwrap();
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
            None,  // root
            true,  // all
            &[],   // ids
            None,  // depth
            None,  // direction
        )
        .await
        .expect("MCP run_health_checks");
    let mcp_text = extract_text(&mcp_result);
    let mcp: Value = serde_json::from_str(&mcp_text).expect("valid JSON from MCP");

    let mcp_findings = mcp["findings"].as_array().expect("findings array in MCP");
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
        )
        .expect("board check-in");

    let (api_ids, api_excluded, api_warnings) = fx.api_board_filtered_candidates();
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
    let http = http_get_json(
        app,
        format!("/api/workflow/next?workspace={ws}"),
    )
    .await;
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
    assert_eq!(http_ids, api_ids, "HTTP visible items must match shared helper");
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
    let mcp: Value = serde_json::from_str(&mcp_text).expect("valid JSON from MCP");

    let mcp_items = mcp["items"].as_array().expect("items");
    let mcp_ids: Vec<String> = mcp_items
        .iter()
        .map(|item| item["id"].as_str().unwrap_or("").to_owned())
        .collect();
    let excluded = mcp["excluded_by_board"].as_array().expect("excluded_by_board");
    let mcp_warning_strings: Vec<String> = mcp["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .filter_map(|warning| warning.as_str().map(ToOwned::to_owned))
        .collect();
    assert_eq!(mcp_ids, api_ids, "MCP visible items must match shared helper");
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
        http_excluded[0]["ticket_id"],
        excluded[0]["ticket_id"],
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
    let http = http_get_json(
        app,
        format!("/api/workflow/next?workspace={ws}"),
    )
    .await;
    let http_index_root = http["scope"]["active_index_root"]
        .as_str()
        .expect("HTTP scope.active_index_root must be a string")
        .to_owned();
    assert!(!http_index_root.is_empty(), "HTTP scope.active_index_root must not be empty");

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
    let mcp: Value = serde_json::from_str(&mcp_text).expect("valid JSON from MCP");
    let mcp_index_root = mcp["scope"]["active_index_root"]
        .as_str()
        .expect("MCP scope.active_index_root must be a string")
        .to_owned();
    assert!(!mcp_index_root.is_empty(), "MCP scope.active_index_root must not be empty");

    // Both must point to the same store (path may differ by separator style,
    // so normalise to forward slashes before comparing).
    let normalise = |p: &str| p.replace('\\', "/");
    assert_eq!(
        normalise(&http_index_root),
        normalise(&mcp_index_root),
        "HTTP and MCP scope.active_index_root must point to the same store root"
    );
}

// ── helpers ────────────────────────────────────────────────────────────────────

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

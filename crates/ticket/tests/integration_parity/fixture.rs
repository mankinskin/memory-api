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
use serde_json::{
    Value,
    json,
};
use ticket_api::{
    health::collect_findings,
    model::{
        edge::EdgeRecord,
        filesystem::ScanRoot,
    },
    storage::store::TicketStore,
    workflow::{
        WorkflowModel,
        apply_board_filter,
    },
};
use ticket::server::TicketServer;
use tower::ServiceExt;

use ticket::serve::{
    AppState,
    StreamBroker,
    WorkspaceRegistry,
    routes::build_router,
};

pub(super) struct ParityFixture {
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
    pub(super) fn build() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let store =
            Arc::new(TicketStore::init(dir.path()).expect("init store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.path().join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");

        let high_fields =
            BTreeMap::from([(String::from("priority"), json!("high"))]);

        // alpha: ready, high priority, no description -> triggers missing_description
        let alpha = store
            .create(
                None,
                "tracker-improvement",
                Some("[parity] Alpha - no description"),
                Some("planned"),
                high_fields.clone(),
                None,
                None, // no description.md
            )
            .expect("create alpha");

        // beta: ready, high priority, good description -> no health findings
        let beta = store
            .create(
                None,
                "tracker-improvement",
                Some("[parity] Beta - with description"),
                Some("planned"),
                high_fields,
                None,
                Some("This parity-fixture description is definitely long enough to satisfy the fifty-character health check threshold."),
            )
            .expect("create beta");

        // gamma: new, depends on alpha AND beta -> not actionable
        let gamma = store
            .create(
                None,
                "tracker-improvement",
                Some("[parity] Gamma - blocked"),
                Some("open"),
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
    pub(super) fn http_router(&self) -> axum::Router {
        let registry =
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&self.store)));
        let state = AppState::new(registry, Arc::new(StreamBroker::new()));
        build_router(state)
    }

    /// Build an MCP TicketServer backed by this fixture's store root.
    pub(super) fn mcp_server(&self) -> TicketServer {
        TicketServer::new(self.store.index_root.clone())
    }

    /// Collect workflow/next candidates directly via ticket-api (the canonical
    /// data layer shared by all adapters).
    pub(super) fn api_next_candidates(&self) -> Vec<String> {
        let tickets = self.store.list(None, None, None).expect("list");
        let edges = self.store.list_all_edges().expect("edges");
        let model = WorkflowModel::build(&self.store, tickets, edges)
            .expect("build model");
        let mut candidates = model.actionable_candidate_ids(None);
        model.sort_candidate_ids(&mut candidates);
        candidates.into_iter().map(|id| id.to_string()).collect()
    }

    /// Collect health findings directly via ticket-api.
    pub(super) fn api_health_findings(&self) -> Vec<(String, String, String)> {
        let tickets = self.store.list(None, None, None).expect("list");
        let edges = self.store.list_all_edges().expect("edges");
        let workflow =
            WorkflowModel::build(&self.store, tickets.clone(), edges.clone())
                .expect("build model");
        let report = collect_findings(&self.store, &tickets, &edges, &workflow);
        report
            .findings
            .into_iter()
            .map(|f| (f.ticket_id.to_string(), f.check, f.severity))
            .collect()
    }

    pub(super) fn api_board_filtered_candidates(
        &self
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let tickets = self.store.list(None, None, None).expect("list");
        let edges = self.store.list_all_edges().expect("edges");
        let model = WorkflowModel::build(&self.store, tickets, edges)
            .expect("build model");
        let mut candidates = model.actionable_candidate_ids(None);
        model.sort_candidate_ids(&mut candidates);
        let board_snap = self.store.board_show(None).ok();
        let filtered =
            apply_board_filter(candidates, board_snap.as_ref(), false);

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

pub(super) fn workspace_name_for(dir: &std::path::Path) -> String {
    // WorkspaceRegistry::single_opened computes the name from the index_root
    // path and exposes it via `primary_workspace_name()`.
    let registry = WorkspaceRegistry::single_opened(Arc::new(
        TicketStore::init(dir).expect("open"),
    ));
    registry.primary_workspace_name().to_owned()
}

pub(super) async fn http_get_json(
    app: axum::Router,
    uri: String,
) -> Value {
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "HTTP GET {}",
        "request failed"
    );
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

pub(super) fn mcp_ws() -> String {
    "default".to_string()
}

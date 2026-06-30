use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use axum::{
    body::{
        Body,
        to_bytes,
    },
    http::{
        Method,
        Request,
        StatusCode,
    },
};
use chrono::Utc;

use memory_fixtures::{
    FixtureError,
    LoadedFixture,
    materialize_fixture,
};
use test_api::{
    TestStoreConfig,
    ValidationExecution,
    ValidationOutcome,
    ValidationProvenance,
    ValidationSpec,
};
use ticket_api::{
    model::filesystem::ScanRoot,
    storage::store::TicketStore,
};
use ticket_http::{
    AppState,
    WorkspaceRegistry,
    build_router,
    serve::StreamBroker,
};
use tower::ServiceExt;

use crate::domains::{
    AuditDomain,
    DocDomain,
    LogDomain,
    RuleDomain,
    SessionDomain,
    SpecDomain,
    TestDomain,
    TicketDomain,
};

/// The ticket this matrix provides evidence for.
pub(crate) const MATRIX_TICKET_ID: &str = "751f0e71";

/// Operation columns exercised for every domain row.
pub const OPERATIONS: &[&str] = &[
    "get", "search", "create", "update", "delete", "move", "scan",
];

/// Transport axis exercised by the matrix.
pub const TRANSPORTS: &[&str] = &["in-process", "cli", "mcp", "http"];

/// Outcome of a single matrix cell that ran without an internal error.
pub enum Cell {
    /// The operation ran and its correctness assertions held.
    Passed,
    /// The operation could not be exercised; carries a concrete reason.
    Blocked(String),
}

/// Result of a cell run. `Err` maps to a `Failed` execution.
pub type CellResult = Result<Cell, String>;

pub(crate) fn pass() -> CellResult {
    Ok(Cell::Passed)
}

pub(crate) fn blocked(reason: impl Into<String>) -> CellResult {
    Ok(Cell::Blocked(reason.into()))
}

pub(crate) fn unsupported(
    operation: &str,
    domain: &str,
) -> String {
    format!("{domain}-api storage surface exposes no `{operation}` operation")
}

/// Shared context handed to every cell: the materialized workspace root.
pub struct MatrixCtx {
    pub workspace_root: PathBuf,
}

impl MatrixCtx {
    /// Build a context rooted at a materialized fixture workspace.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    /// Resolve a hidden store directory under the materialized workspace.
    pub(crate) fn store_root(
        &self,
        dir: &str,
    ) -> PathBuf {
        self.workspace_root.join(dir)
    }
}

/// One domain row of the matrix.
///
/// Every operation defaults to `Blocked` with an "unsupported" reason; a domain
/// overrides only the operations its storage API genuinely supports.
pub(crate) trait DomainOps {
    fn domain(&self) -> &'static str;

    fn get(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("get", self.domain()))
    }
    fn search(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("search", self.domain()))
    }
    fn create(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("create", self.domain()))
    }
    fn update(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("update", self.domain()))
    }
    fn delete(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("delete", self.domain()))
    }
    fn move_op(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(format!(
            "{} move surface is not adapter-backed in memory-matrix yet",
            self.domain()
        ))
    }
    fn scan(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("scan", self.domain()))
    }
}

fn dispatch(
    ops: &dyn DomainOps,
    transport: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    if transport == "cli" {
        return dispatch_cli(ops.domain(), operation, ctx);
    }

    if transport == "http" {
        return dispatch_http(ops.domain(), operation, ctx);
    }

    if transport != "in-process" {
        return blocked(format!(
            "transport `{transport}` for domain `{}` operation `{operation}` is not wired in the matrix harness yet; \
             recorded as blocked-with-reason per real-transport rollout",
            ops.domain()
        ));
    }

    match operation {
        "get" => ops.get(ctx),
        "search" => ops.search(ctx),
        "create" => ops.create(ctx),
        "update" => ops.update(ctx),
        "delete" => ops.delete(ctx),
        "move" => ops.move_op(ctx),
        "scan" => ops.scan(ctx),
        other => Err(format!("unknown operation `{other}`")),
    }
}

fn dispatch_http(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match domain {
        "ticket" => dispatch_ticket_http(operation, ctx),
        _ => blocked(format!(
            "http transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn dispatch_ticket_http(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match operation {
        "get" => run_ticket_http_get(ctx),
        "search" => run_ticket_http_search(ctx),
        _ => blocked(format!(
            "http transport for domain `ticket` operation `{operation}` is not wired yet; \
             currently only `ticket.get@http` and `ticket.search@http` are exercised through the ticket-http router surface"
        )),
    }
}

fn run_ticket_http_get(ctx: &MatrixCtx) -> CellResult {
    let (id, workspace, app) = build_ticket_http_fixture(ctx)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("build tokio runtime for http matrix cell: {err}"))?;

    runtime
        .block_on(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri(format!("/api/tickets/{id}?workspace={workspace}"))
                .body(Body::empty())
                .map_err(|err| format!("build ticket get request: {err}"))?;

            let response = app
                .oneshot(request)
                .await
                .map_err(|err| format!("dispatch ticket get request: {err}"))?;

            if response.status() != StatusCode::OK {
                return Err(format!(
                    "ticket-http get returned unexpected status {}",
                    response.status()
                ));
            }

            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .map_err(|err| format!("read ticket get response body: {err}"))?;
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|err| format!("parse ticket get response body: {err}"))?;

            let returned_id = payload["ticket"]["ticket_ref"]["id"]
                .as_str()
                .ok_or_else(|| {
                    "ticket get payload missing ticket.ticket_ref.id".to_string()
                })?;
            if returned_id != id.to_string() {
                return Err(format!(
                    "ticket-http get returned mismatched id: expected {id}, got {returned_id}"
                ));
            }

            Ok(Cell::Passed)
        })
}

fn run_ticket_http_search(ctx: &MatrixCtx) -> CellResult {
    let (id, workspace, app) = build_ticket_http_fixture(ctx)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("build tokio runtime for http matrix cell: {err}"))?;

    runtime
        .block_on(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/tickets?workspace={workspace}&query=matrix-http-ticket"
                ))
                .body(Body::empty())
                .map_err(|err| format!("build ticket search request: {err}"))?;

            let response = app
                .oneshot(request)
                .await
                .map_err(|err| format!("dispatch ticket search request: {err}"))?;

            if response.status() != StatusCode::OK {
                return Err(format!(
                    "ticket-http search returned unexpected status {}",
                    response.status()
                ));
            }

            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .map_err(|err| format!("read ticket search response body: {err}"))?;
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|err| format!("parse ticket search response body: {err}"))?;

            let items = payload["items"]
                .as_array()
                .ok_or_else(|| "ticket search payload missing items array".to_string())?;
            let expected_id = id.to_string();
            let found = items.iter().any(|item| {
                item["ticket_ref"]["id"]
                    .as_str()
                    .map(|candidate| candidate == expected_id)
                    .unwrap_or(false)
            });
            if !found {
                return Err(format!(
                    "ticket-http search did not return seeded ticket id {expected_id}"
                ));
            }

            Ok(Cell::Passed)
        })
}

fn build_ticket_http_fixture(
    ctx: &MatrixCtx,
) -> Result<(uuid::Uuid, String, axum::Router), String> {
    let ticket_store_root = ctx.store_root(".ticket");
    let tickets_scan_root = ctx.workspace_root.join("tickets");

    std::fs::create_dir_all(&tickets_scan_root).map_err(|err| {
        format!(
            "failed to create ticket scan root `{}`: {err}",
            tickets_scan_root.display()
        )
    })?;

    let store = Arc::new(
        TicketStore::open_or_init(&ticket_store_root)
            .map_err(|err| format!("open ticket store: {err}"))?,
    );

    let has_scan_root = store
        .list_scan_roots()
        .map_err(|err| format!("list scan roots: {err}"))?
        .into_iter()
        .any(|root| root.path == tickets_scan_root);
    if !has_scan_root {
        store
            .add_scan_root(ScanRoot {
                path: tickets_scan_root,
                label: "default".into(),
            })
            .map_err(|err| format!("add ticket scan root: {err}"))?;
    }

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("matrix-http-ticket-get"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .map_err(|err| format!("seed ticket for http get: {err}"))?;

    let state = AppState::new(
        Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
        Arc::new(StreamBroker::new()),
    );
    let workspace = state.registry.primary_workspace_name().to_string();
    let app = build_router(state);

    Ok((id, workspace, app))
}

fn dispatch_cli(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    if operation == "move" {
        return blocked(format!(
            "cli transport for domain `{domain}` operation `move` is not wired in memory-matrix yet; in-process move cells exercise the adapter-backed move kernel"
        ));
    }

    match domain {
        "ticket" => dispatch_ticket_cli(operation, ctx),
        "spec" => dispatch_spec_cli(operation, ctx),
        "rule" => dispatch_rule_cli(operation, ctx),
        _ => blocked(format!(
            "cli transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn run_ticket_cli(args: Vec<String>) -> Result<(), String> {
    let cli = ticket_cli::cli::parse_cli_from(args)
        .map_err(|err| err.to_string())?;
    ticket_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_ticket_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let token = format!("matrix-cli-ticket-{}", uuid::Uuid::new_v4().simple());

    match operation {
        "create" => run_ticket_cli(vec![
            "ticket".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--id".into(),
            id,
            "--type".into(),
            "tracker-improvement".into(),
            "--title".into(),
            token,
            "--state".into(),
            "new".into(),
        ]),
        "get" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                id,
            ])
        },
        "search" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id,
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token.clone(),
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                id,
                "--to-state".into(),
                "ready".into(),
            ])
        },
        "delete" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                id,
            ])
        },
        "scan" => run_ticket_cli(vec![
            "ticket".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

fn run_spec_cli(args: Vec<String>) -> Result<(), String> {
    let cli = spec_cli::cli::parse_cli_from(args).map_err(|err| err.to_string())?;
    spec_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_spec_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/cli/{suffix}");
    let token = format!("Matrix CLI Spec {suffix}");

    match operation {
        "create" => run_spec_cli(vec![
            "spec".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--title".into(),
            token,
            "--slug".into(),
            slug,
            "--component".into(),
            "matrix".into(),
        ]),
        "get" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                slug,
            ])
        },
        "search" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token.clone(),
                "--slug".into(),
                slug,
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                slug,
                "--field".into(),
                "scope=internal".into(),
            ])
        },
        "delete" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                slug,
            ])
        },
        "scan" => run_spec_cli(vec![
            "spec".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

fn run_rule_cli(args: Vec<String>) -> Result<(), String> {
    let cli = rule_cli::cli::parse_cli_from(args).map_err(|err| err.to_string())?;
    rule_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_rule_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/cli/{suffix}");
    let token = format!("Matrix CLI Rule {suffix}");

    match operation {
        "create" => run_rule_cli(vec![
            "rule".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--title".into(),
            token,
            "--slug".into(),
            slug,
            "--file-kind".into(),
            "markdown".into(),
            "--section".into(),
            "matrix".into(),
            "--body".into(),
            "matrix body".into(),
        ]),
        "get" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                slug,
            ])
        },
        "search" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token.clone(),
                "--slug".into(),
                slug,
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                slug,
                "--body".into(),
                "updated body".into(),
            ])
        },
        "delete" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                slug,
            ])
        },
        "scan" => run_rule_cli(vec![
            "rule".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

/// All `(domain, operation)` cells of the matrix, in registration order.
pub fn cells() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for domain in domains() {
        for &operation in OPERATIONS {
            out.push((domain.domain(), operation));
        }
    }
    out
}

/// Stable, path-safe Criterion benchmark id for a `domain x operation` cell.
///
/// Both the bench harness and the ingest runner derive the Criterion output
/// directory from this id, so they must agree on its form.
pub fn bench_id(
    domain: &str,
    operation: &str,
) -> String {
    format!("{domain}__{operation}")
}

/// Run a single matrix cell, selected by domain + operation name, against
/// `ctx`. This is the per-cell entry point reused by the benchmark harness.
pub fn run_one(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    for candidate in domains() {
        if candidate.domain() == domain {
            return dispatch(&*candidate, "in-process", operation, ctx);
        }
    }
    Err(format!("unknown domain `{domain}`"))
}

/// The registered domain rows of the matrix.
fn domains() -> Vec<Box<dyn DomainOps>> {
    vec![
        Box::new(TicketDomain),
        Box::new(SpecDomain),
        Box::new(RuleDomain),
        Box::new(AuditDomain),
        Box::new(SessionDomain),
        Box::new(TestDomain),
        Box::new(DocDomain),
        Box::new(LogDomain),
    ]
}

/// Recorded result for one matrix cell.
#[derive(Debug, Clone)]
pub struct CellRecord {
    pub domain: String,
    pub transport: String,
    pub operation: String,
    pub spec_id: String,
    pub execution_id: String,
    pub outcome: ValidationOutcome,
    pub duration_ms: u64,
    pub detail: String,
}

/// Full result of a matrix run. Holds the fixture so its `.test` store stays
/// readable until the caller drops the run.
pub struct MatrixRun {
    pub records: Vec<CellRecord>,
    pub test_store_root: PathBuf,
    _fixture: LoadedFixture,
}

impl MatrixRun {
    /// Open the isolated test store the matrix recorded executions into.
    pub fn test_store(&self) -> TestStoreConfig {
        TestStoreConfig::new(self.test_store_root.clone(), "default")
    }
}

/// Materialize the fixture and run the full domain x operation matrix,
/// recording a [`ValidationExecution`] (with duration) for every cell.
pub fn run_matrix() -> Result<MatrixRun, FixtureError> {
    let fixture = materialize_fixture()?;
    let workspace_root = fixture.workspace_root.clone();
    let ctx = MatrixCtx {
        workspace_root: workspace_root.clone(),
    };

    let test_store_root = workspace_root.join(".test");
    let test_store = TestStoreConfig::new(test_store_root.clone(), "default");
    let run_id = format!("matrix-{}", Utc::now().format("%Y%m%dT%H%M%SZ"));

    // Keep run_one strict: only create paths initialize missing roots.
    bootstrap_core_store_roots(&ctx);

    let mut records = Vec::new();

    for domain in domains() {
        for &transport in TRANSPORTS {
            for &operation in OPERATIONS {
                let record = run_cell(
                    &test_store,
                    &*domain,
                    transport,
                    operation,
                    &ctx,
                    &run_id,
                );
                records.push(record);
            }
        }
    }

    Ok(MatrixRun {
        records,
        test_store_root,
        _fixture: fixture,
    })
}

fn bootstrap_core_store_roots(ctx: &MatrixCtx) {
    let _ = ticket_api::storage::TicketStore::open_or_init(
        &ctx.store_root(".ticket"),
    );
    let _ = spec_api::SpecStore::open_or_init(&ctx.store_root(".spec"));
    let _ = rule_api::RuleStore::open_or_init(&ctx.store_root(".rule"));
}

/// Record the per-cell validation spec, run the cell, time it, and record the
/// execution. This is the fixed harness machinery - it never changes when a
/// domain or operation is added.
fn run_cell(
    test_store: &TestStoreConfig,
    domain: &dyn DomainOps,
    transport: &str,
    operation: &str,
    ctx: &MatrixCtx,
    run_id: &str,
) -> CellRecord {
    let spec_id = format!(
        "vt-matrix-{}-{}-{}",
        domain.domain(),
        transport,
        operation
    );
    let execution_id = format!("exec-{run_id}-{spec_id}");

    let mut spec = ValidationSpec::new(
        spec_id.clone(),
        format!("matrix: {} {} {}", domain.domain(), transport, operation),
    );
    spec.detail = Some(format!(
        "Cross-domain operation matrix cell `{}.{}@{}`",
        domain.domain(),
        operation,
        transport
    ));
    spec.links.ticket_ids = vec![MATRIX_TICKET_ID.to_string()];
    spec.provenance = ValidationProvenance {
        source_path: Some(file!().to_string()),
        test_id: Some(format!("{}.{}@{}", domain.domain(), operation, transport)),
        domain: Some(domain.domain().to_string()),
        operation: Some(operation.to_string()),
        transport: Some(transport.to_string()),
        run_id: Some(run_id.to_string()),
    };
    // Best-effort: spec recording failure should not abort the whole matrix.
    let _ = test_store.record_spec(&spec);

    let started = Instant::now();
    let result = dispatch(domain, transport, operation, ctx);
    let duration_ms = started.elapsed().as_millis() as u64;

    let (outcome, detail) = match result {
        Ok(Cell::Passed) => (
            ValidationOutcome::Passed,
            format!("{}.{} passed", domain.domain(), operation),
        ),
        Ok(Cell::Blocked(reason)) => (ValidationOutcome::Blocked, reason),
        Err(reason) => (ValidationOutcome::Failed, reason),
    };

    let mut execution = ValidationExecution::new(
        execution_id.clone(),
        spec_id.clone(),
        outcome.clone(),
        Utc::now(),
    );
    execution.duration_ms = Some(duration_ms);
    execution.detail = Some(detail.clone());
    execution.links.spec_ids = vec![spec_id.clone()];
    execution.links.ticket_ids = vec![MATRIX_TICKET_ID.to_string()];
    execution.provenance = ValidationProvenance {
        source_path: Some(file!().to_string()),
        test_id: Some(format!("{}.{}@{}", domain.domain(), operation, transport)),
        domain: Some(domain.domain().to_string()),
        operation: Some(operation.to_string()),
        transport: Some(transport.to_string()),
        run_id: Some(run_id.to_string()),
    };
    let _ = test_store.record_execution(&execution);

    CellRecord {
        domain: domain.domain().to_string(),
        transport: transport.to_string(),
        operation: operation.to_string(),
        spec_id,
        execution_id,
        outcome,
        duration_ms,
        detail,
    }
}

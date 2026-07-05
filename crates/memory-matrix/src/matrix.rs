use std::{
    collections::BTreeMap,
    io::{
        Read,
        Write,
    },
    path::PathBuf,
    process::{
        Command,
        Stdio,
    },
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
use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
};
use rule_mcp::server::{
    CreateRuleInput,
    RuleRefInput,
    RuleServer,
    ScanInput as RuleScanInput,
    SearchRulesInput,
    UpdateRuleInput,
};
use spec_mcp::server::{
    CreateSpecInput,
    GetSpecInput,
    ScanInput as SpecScanInput,
    SearchSpecsInput,
    SpecRefInput,
    SpecServer,
    UpdateSpecInput,
};
use ticket_mcp::server::{
    CreateTicketInput,
    DeleteTicketInput,
    ListTicketsInput,
    TicketRefInput,
    TicketServer,
    UpdateTicketInput,
};

use memory_fixtures::{
    FixtureError,
    LoadedFixture,
    materialize_fixture,
};

#[path = "cli.rs"]
mod cli;
#[path = "http.rs"]
mod http;
#[path = "mcp.rs"]
mod mcp;
#[path = "support.rs"]
mod support;

#[path = "runner.rs"]
mod runner;

pub use runner::{
    CellRecord,
    MatrixRun,
    run_matrix,
    run_ticket_get_mcp_subprocess_failure_probe,
    run_ticket_spawn_fail_mcp_subprocess_failure_probe,
};
use support::{
    domain_names,
    expected_blocked_reason,
    is_supported,
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

/// Fixture profile name emitted for every matrix cell execution.
pub const FIXTURE_PROFILE_DEFAULT: &str = "memory-fixtures/default";

/// Expected status declared by the matrix registry for a given cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedOutcome {
    Passed,
    Blocked,
}

/// Canonical transport-matrix registry entry.
#[derive(Debug, Clone)]
pub struct CellSpec {
    pub cell_id: String,
    pub domain: String,
    pub operation: String,
    pub transport: String,
    pub fixture_profile: String,
    pub expected_outcome: ExpectedOutcome,
    pub blocked_reason: Option<String>,
}

/// Outcome of a single matrix cell that ran without an internal error.
pub enum Cell {
    /// The operation ran and its correctness assertions held.
    Passed,
    /// The operation could not be exercised; carries a concrete reason.
    Blocked(String),
}

/// Result of a cell run. `Err` maps to a `Failed` execution.
pub type CellResult = Result<Cell, String>;

#[derive(Debug, Clone)]
pub(crate) struct DispatchMetadata {
    pub run_id: String,
    pub cell_id: String,
    pub transport: String,
    pub operation: String,
    pub execution_id: String,
    pub log_session_ids: Vec<String>,
}

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
    metadata: Option<&DispatchMetadata>,
) -> CellResult {
    if transport == "cli" {
        return cli::dispatch_cli(ops.domain(), operation, ctx);
    }

    if transport == "http" {
        return http::dispatch_http(ops.domain(), operation, ctx);
    }

    if transport == "mcp" {
        return mcp::dispatch_mcp(ops.domain(), operation, ctx, metadata);
    }

    if transport == "mcp-subprocess-fail" {
        return mcp::dispatch_mcp_subprocess_failure_probe(
            ops.domain(),
            operation,
            ctx,
            metadata,
        );
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

fn expected_outcome_for_cell(
    domain: &str,
    transport: &str,
    operation: &str,
) -> (ExpectedOutcome, Option<String>) {
    if is_supported(domain, transport, operation) {
        (ExpectedOutcome::Passed, None)
    } else {
        (
            ExpectedOutcome::Blocked,
            Some(expected_blocked_reason(domain, transport, operation)),
        )
    }
}

/// Canonical transport-cell registry for `domain x operation x transport`.
pub fn transport_cells() -> Vec<CellSpec> {
    let mut out = Vec::new();
    for domain in domain_names() {
        for &operation in OPERATIONS {
            for &transport in TRANSPORTS {
                let (expected_outcome, blocked_reason) =
                    expected_outcome_for_cell(domain, transport, operation);
                out.push(CellSpec {
                    cell_id: format!("{domain}.{operation}.{transport}"),
                    domain: domain.to_string(),
                    operation: operation.to_string(),
                    transport: transport.to_string(),
                    fixture_profile: FIXTURE_PROFILE_DEFAULT.to_string(),
                    expected_outcome,
                    blocked_reason,
                });
            }
        }
    }
    out
}

/// All `(domain, operation)` cells of the matrix, in registration order.
pub fn cells() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for domain in domain_names() {
        for &operation in OPERATIONS {
            out.push((domain, operation));
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
            return dispatch(&*candidate, "in-process", operation, ctx, None);
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

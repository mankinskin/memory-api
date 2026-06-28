use std::path::PathBuf;
use std::time::Instant;

use chrono::Utc;

use memory_fixtures::{materialize_fixture, FixtureError, LoadedFixture};
use test_api::{
    TestStoreConfig, ValidationExecution, ValidationOutcome, ValidationSpec,
};

use crate::domains::{
    AuditDomain, DocDomain, LogDomain, RuleDomain, SessionDomain, SpecDomain,
    TestDomain, TicketDomain,
};

/// The ticket this matrix provides evidence for.
pub(crate) const MATRIX_TICKET_ID: &str = "751f0e71";

/// Operation columns exercised for every domain row.
pub const OPERATIONS: &[&str] =
    &["get", "search", "create", "update", "delete", "move", "scan"];

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

pub(crate) fn unsupported(operation: &str, domain: &str) -> String {
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
    pub(crate) fn store_root(&self, dir: &str) -> PathBuf {
        self.workspace_root.join(dir)
    }
}

/// One domain row of the matrix.
///
/// Every operation defaults to `Blocked` with an "unsupported" reason; a domain
/// overrides only the operations its storage API genuinely supports.
pub(crate) trait DomainOps {
    fn domain(&self) -> &'static str;

    fn get(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(unsupported("get", self.domain()))
    }
    fn search(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(unsupported("search", self.domain()))
    }
    fn create(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(unsupported("create", self.domain()))
    }
    fn update(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(unsupported("update", self.domain()))
    }
    fn delete(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(unsupported("delete", self.domain()))
    }
    fn move_op(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(
            "generic move kernel (ticket 0a510279) not yet landed; \
             cross-worktree move is blocked-with-reason until it lands",
        )
    }
    fn scan(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(unsupported("scan", self.domain()))
    }
}

fn dispatch(ops: &dyn DomainOps, operation: &str, ctx: &MatrixCtx) -> CellResult {
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
pub fn bench_id(domain: &str, operation: &str) -> String {
    format!("{domain}__{operation}")
}

/// Run a single matrix cell, selected by domain + operation name, against
/// `ctx`. This is the per-cell entry point reused by the benchmark harness.
pub fn run_one(domain: &str, operation: &str, ctx: &MatrixCtx) -> CellResult {
    for candidate in domains() {
        if candidate.domain() == domain {
            return dispatch(&*candidate, operation, ctx);
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

    let mut records = Vec::new();

    for domain in domains() {
        for &operation in OPERATIONS {
            let record = run_cell(&test_store, &*domain, operation, &ctx);
            records.push(record);
        }
    }

    Ok(MatrixRun {
        records,
        test_store_root,
        _fixture: fixture,
    })
}

/// Record the per-cell validation spec, run the cell, time it, and record the
/// execution. This is the fixed harness machinery - it never changes when a
/// domain or operation is added.
fn run_cell(
    test_store: &TestStoreConfig,
    domain: &dyn DomainOps,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellRecord {
    let spec_id = format!("vt-matrix-{}-{}", domain.domain(), operation);
    let execution_id = format!("exec-{spec_id}");

    let mut spec = ValidationSpec::new(
        spec_id.clone(),
        format!("matrix: {} {}", domain.domain(), operation),
    );
    spec.detail = Some(format!(
        "Cross-domain operation matrix cell `{}.{}`",
        domain.domain(),
        operation
    ));
    spec.links.ticket_ids = vec![MATRIX_TICKET_ID.to_string()];
    // Best-effort: spec recording failure should not abort the whole matrix.
    let _ = test_store.record_spec(&spec);

    let started = Instant::now();
    let result = dispatch(domain, operation, ctx);
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
    let _ = test_store.record_execution(&execution);

    CellRecord {
        domain: domain.domain().to_string(),
        operation: operation.to_string(),
        spec_id,
        execution_id,
        outcome,
        duration_ms,
        detail,
    }
}

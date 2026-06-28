//! Data-driven cross-domain operation test matrix.
//!
//! Exercises the basic operations of every memory domain against a freshly
//! materialized [`memory_fixtures`] workspace and records each cell as a
//! `test-api` [`ValidationExecution`] with a wall-clock duration.
//!
//! ## Matrix shape
//!
//! Rows are domains (`ticket`, `spec`, `rule`, `audit`, `session`, `test`,
//! `doc`, `log`); columns are operations
//! ([`OPERATIONS`]: `get`, `search`, `create`, `update`, `delete`, `move`,
//! `scan`).
//!
//! Each cell either:
//! - runs its operation against the fixture, asserts correctness, and is
//!   recorded `Passed`, or
//! - is recorded `Blocked` with a concrete reason (e.g. an unsupported `move`
//!   pending the generic move kernel, or a domain whose storage API genuinely
//!   exposes no such operation). **Cells are never silently skipped.**
//!
//! Adding a domain is a new [`DomainOps`] implementation registered in
//! [`domains`]; adding an operation is a new column in [`OPERATIONS`] plus a
//! trait method and dispatch arm. The harness loop, timing, and recording
//! machinery never change.
//!
//! ## Running
//!
//! ```text
//! cargo test -p memory-matrix
//! ```
//!
//! The suite materializes the fixture into a tempdir and writes every
//! execution into that workspace's isolated `.test` store (see
//! [`MatrixRun::test_store`]).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use chrono::Utc;
use serde_json::{json, Value};

use memory_fixtures::{materialize_fixture, FixtureError, LoadedFixture};
use test_api::{
    TestStoreConfig, ValidationExecution, ValidationOutcome, ValidationSpec,
};

/// The ticket this matrix provides evidence for.
const MATRIX_TICKET_ID: &str = "751f0e71";

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

fn pass() -> CellResult {
    Ok(Cell::Passed)
}

fn blocked(reason: impl Into<String>) -> CellResult {
    Ok(Cell::Blocked(reason.into()))
}

fn unsupported(operation: &str, domain: &str) -> String {
    format!("{domain}-api storage surface exposes no `{operation}` operation")
}

/// Shared context handed to every cell: the materialized workspace root.
pub struct MatrixCtx {
    pub workspace_root: PathBuf,
}

impl MatrixCtx {
    /// Resolve a hidden store directory under the materialized workspace.
    fn store_root(&self, dir: &str) -> PathBuf {
        self.workspace_root.join(dir)
    }
}

/// One domain row of the matrix.
///
/// Every operation defaults to `Blocked` with an "unsupported" reason; a domain
/// overrides only the operations its storage API genuinely supports.
trait DomainOps {
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
/// execution. This is the fixed harness machinery — it never changes when a
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

// ── ticket domain ─────────────────────────────────────────────────────────────

struct TicketDomain;

impl TicketDomain {
    fn open(ctx: &MatrixCtx) -> Result<ticket_api::storage::TicketStore, String> {
        ticket_api::storage::TicketStore::open_or_init(&ctx.store_root(".ticket"))
            .map_err(|err| err.to_string())
    }
}

impl DomainOps for TicketDomain {
    fn domain(&self) -> &'static str {
        "ticket"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("matrix create"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        store.get(&id).map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        let seeded =
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let manifest = store.get(&seeded).map_err(|err| err.to_string())?;
        match manifest.extra.get("title").and_then(Value::as_str) {
            Some("Root fixture ticket") => pass(),
            other => Err(format!("unexpected seeded ticket title: {other:?}")),
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        store
            .create(
                None,
                "tracker-improvement",
                Some("matrixsearchtoken ticket"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let results = store
            .search_tickets("matrixsearchtoken", 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            return Err("search returned no hit for indexed token".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("matrix update"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        let manifest = store
            .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
            .map_err(|err| err.to_string())?;
        match manifest.extra.get("state").and_then(Value::as_str) {
            Some("ready") => pass(),
            other => Err(format!("update did not transition state: {other:?}")),
        }
    }

    fn delete(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("matrix delete"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .map_err(|err| err.to_string())?;
        store.delete(&id).map_err(|err| err.to_string())?;
        if store.get(&id).is_ok() {
            return Err("ticket still readable after delete".to_string());
        }
        pass()
    }

    fn scan(&self, ctx: &MatrixCtx) -> CellResult {
        let store = Self::open(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        pass()
    }
}

// ── spec domain ───────────────────────────────────────────────────────────────

struct SpecDomain;

impl SpecDomain {
    fn open(ctx: &MatrixCtx) -> Result<spec_api::SpecStore, String> {
        spec_api::SpecStore::open_or_init(&ctx.store_root(".spec"))
            .map_err(|err| err.to_string())
    }

    fn new_manifest(slug: &str, title: &str) -> spec_api::SpecManifest {
        spec_api::SpecManifest::new(slug, title, "matrix")
    }
}

impl DomainOps for SpecDomain {
    fn domain(&self) -> &'static str {
        "spec"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/create", "Matrix Create");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store
            .get(&manifest.id().to_string())
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/get", "Matrix Get");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let fetched = store.get("matrix/get").map_err(|err| err.to_string())?;
        match fetched.title() {
            Some("Matrix Get") => pass(),
            other => Err(format!("unexpected spec title: {other:?}")),
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest =
            Self::new_manifest("matrix/search", "Matrixspectoken Spec");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let results = store
            .entity_store()
            .search("Matrixspectoken", 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            return Err("spec search returned no hit for indexed token".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/update", "Matrix Update");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        let mut patch = BTreeMap::new();
        patch.insert("scope".to_string(), json!("internal"));
        let updated = store
            .update("matrix/update", patch, None)
            .map_err(|err| err.to_string())?;
        match updated.scope() {
            Some("internal") => pass(),
            other => Err(format!("spec update did not apply patch: {other:?}")),
        }
    }

    fn delete(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/delete", "Matrix Delete");
        store
            .create(&manifest, "matrix body", None)
            .map_err(|err| err.to_string())?;
        store
            .delete("matrix/delete")
            .map_err(|err| err.to_string())?;
        if store.get("matrix/delete").is_ok() {
            return Err("spec still readable after delete".to_string());
        }
        pass()
    }

    fn scan(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        pass()
    }
}

// ── rule domain ───────────────────────────────────────────────────────────────

struct RuleDomain;

impl RuleDomain {
    fn open(ctx: &MatrixCtx) -> Result<rule_api::RuleStore, String> {
        rule_api::RuleStore::open_or_init(&ctx.store_root(".rule"))
            .map_err(|err| err.to_string())
    }

    fn new_manifest(slug: &str, title: &str) -> rule_api::RuleManifest {
        rule_api::RuleManifest::new(slug, title, "markdown", "matrix", "body")
    }
}

impl DomainOps for RuleDomain {
    fn domain(&self) -> &'static str {
        "rule"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/create", "Matrix Create");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store
            .get(&manifest.id.to_string())
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/get", "Matrix Get");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let fetched = store.get("matrix/get").map_err(|err| err.to_string())?;
        match fetched.title() {
            Some("Matrix Get") => pass(),
            other => Err(format!("unexpected rule title: {other:?}")),
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest =
            Self::new_manifest("matrix/search", "Matrixruletoken Rule");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store.scan(true).map_err(|err| err.to_string())?;
        let results = store
            .search("Matrixruletoken", &rule_api::RuleFilter::default(), 10)
            .map_err(|err| err.to_string())?;
        if results.is_empty() {
            return Err("rule search returned no hit for indexed token".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/update", "Matrix Update");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store
            .update_body("matrix/update", "updated body")
            .map_err(|err| err.to_string())?;
        let fetched = store.get("matrix/update").map_err(|err| err.to_string())?;
        match fetched.body() {
            Some("updated body") => pass(),
            other => Err(format!("rule update_body did not apply: {other:?}")),
        }
    }

    fn delete(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        let manifest = Self::new_manifest("matrix/delete", "Matrix Delete");
        store
            .create(&manifest, None)
            .map_err(|err| err.to_string())?;
        store
            .delete("matrix/delete")
            .map_err(|err| err.to_string())?;
        if store.get("matrix/delete").is_ok() {
            return Err("rule still readable after delete".to_string());
        }
        pass()
    }

    fn scan(&self, ctx: &MatrixCtx) -> CellResult {
        let mut store = Self::open(ctx)?;
        store.scan(true).map_err(|err| err.to_string())?;
        pass()
    }
}

// ── audit domain ──────────────────────────────────────────────────────────────

struct AuditDomain;

impl AuditDomain {
    fn open(ctx: &MatrixCtx) -> Result<audit_api::index::RepositoryIndex, String> {
        audit_api::index::RepositoryIndex::open(&ctx.workspace_root)
            .map_err(|err| err.to_string())
    }
}

impl DomainOps for AuditDomain {
    fn domain(&self) -> &'static str {
        "audit"
    }

    fn scan(&self, ctx: &MatrixCtx) -> CellResult {
        let index = Self::open(ctx)?;
        index
            .sync_source_files(&[])
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let index = Self::open(ctx)?;
        index.sync_source_files(&[]).map_err(|err| err.to_string())?;
        index.indexed_files().map_err(|err| err.to_string())?;
        pass()
    }

    fn create(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(
            "audit-api `record_audit_run` requires a fully populated \
             AuditMetrics snapshot produced by a complete `audit()` run; \
             not exercisable as a unit create in the matrix",
        )
    }
}

// ── session domain ────────────────────────────────────────────────────────────

struct SessionDomain;

impl SessionDomain {
    fn config(ctx: &MatrixCtx) -> session_api::SessionStoreConfig {
        session_api::SessionStoreConfig::new(ctx.store_root(".session"), "default")
    }

    fn payload(session_id: &str, content: &str) -> session_api::CopilotHookPayload {
        Self::payload_multi(session_id, &[content])
    }

    fn payload_multi(
        session_id: &str,
        contents: &[&str],
    ) -> session_api::CopilotHookPayload {
        session_api::CopilotHookPayload {
            session_id: session_id.to_string(),
            workspace_slug: "default".to_string(),
            captured_at: Utc::now(),
            conversation_id: None,
            agent_id: Some("matrix".to_string()),
            model: None,
            trigger: Some("matrix".to_string()),
            messages: contents
                .iter()
                .map(|content| session_api::CopilotHookMessage {
                    role: session_api::SessionRole::User,
                    content: (*content).to_string(),
                    tool_name: None,
                    captured_at: None,
                })
                .collect(),
        }
    }
}

impl DomainOps for SessionDomain {
    fn domain(&self) -> &'static str {
        "session"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .capture_copilot_hook(Self::payload("matrix-create", "hello"))
            .map_err(|err| err.to_string())?;
        config
            .read_session("matrix-create")
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .capture_copilot_hook(Self::payload("matrix-get", "hello"))
            .map_err(|err| err.to_string())?;
        let record = config
            .read_session("matrix-get")
            .map_err(|err| err.to_string())?;
        if record.session_id == "matrix-get" {
            pass()
        } else {
            Err(format!("unexpected session id: {}", record.session_id))
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .capture_copilot_hook(Self::payload("matrix-search", "hello"))
            .map_err(|err| err.to_string())?;
        let records = config
            .query_sessions(&session_api::SessionQuery::default())
            .map_err(|err| err.to_string())?;
        if records.is_empty() {
            return Err("session query returned no records".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .capture_copilot_hook(Self::payload("matrix-update", "first"))
            .map_err(|err| err.to_string())?;
        let before = config
            .read_session("matrix-update")
            .map_err(|err| err.to_string())?;
        config
            .capture_copilot_hook(Self::payload_multi(
                "matrix-update",
                &["first", "second"],
            ))
            .map_err(|err| err.to_string())?;
        let after = config
            .read_session("matrix-update")
            .map_err(|err| err.to_string())?;
        if after.turns.len() > before.turns.len() {
            pass()
        } else {
            Err("append capture did not grow the session transcript".to_string())
        }
    }
}

// ── test domain ───────────────────────────────────────────────────────────────

struct TestDomain;

impl TestDomain {
    fn config(ctx: &MatrixCtx) -> TestStoreConfig {
        // Isolated from the matrix's own evidence store (`.test`).
        TestStoreConfig::new(ctx.store_root(".test-domain"), "default")
    }

    fn execution(id: &str, outcome: ValidationOutcome) -> ValidationExecution {
        let mut execution =
            ValidationExecution::new(id, "vt-test-domain", outcome, Utc::now());
        execution.duration_ms = Some(1);
        execution
    }
}

impl DomainOps for TestDomain {
    fn domain(&self) -> &'static str {
        "test"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_execution(&Self::execution(
                "matrix-create",
                ValidationOutcome::Passed,
            ))
            .map_err(|err| err.to_string())?;
        config
            .get_execution("matrix-create")
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_execution(&Self::execution(
                "matrix-get",
                ValidationOutcome::Passed,
            ))
            .map_err(|err| err.to_string())?;
        let fetched = config
            .get_execution("matrix-get")
            .map_err(|err| err.to_string())?;
        if fetched.id == "matrix-get" {
            pass()
        } else {
            Err(format!("unexpected execution id: {}", fetched.id))
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_execution(&Self::execution(
                "matrix-search",
                ValidationOutcome::Passed,
            ))
            .map_err(|err| err.to_string())?;
        let executions = config
            .list_executions(&test_api::ExecutionQuery::default())
            .map_err(|err| err.to_string())?;
        if executions.is_empty() {
            return Err("execution query returned no records".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_execution(&Self::execution(
                "matrix-update",
                ValidationOutcome::Passed,
            ))
            .map_err(|err| err.to_string())?;
        config
            .record_execution(&Self::execution(
                "matrix-update",
                ValidationOutcome::Failed,
            ))
            .map_err(|err| err.to_string())?;
        let fetched = config
            .get_execution("matrix-update")
            .map_err(|err| err.to_string())?;
        if fetched.outcome == ValidationOutcome::Failed {
            pass()
        } else {
            Err("re-record did not overwrite execution outcome".to_string())
        }
    }

    fn delete(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked("test-api exposes no delete operation for executions")
    }

    fn scan(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(
            "test-api has no scan/index reconcile; the store index is generated \
             (ticket 90de77b1), not scanned from disk",
        )
    }
}

// ── doc domain ────────────────────────────────────────────────────────────────

struct DocDomain;

impl DomainOps for DocDomain {
    fn domain(&self) -> &'static str {
        "doc"
    }
    // doc-api is a read-only cargo-doc analysis surface with no entity store, so
    // every operation falls through to the default blocked-with-reason cell.
}

// ── log domain ────────────────────────────────────────────────────────────────

struct LogDomain;

impl LogDomain {
    fn config(ctx: &MatrixCtx) -> log_api::LogStoreConfig {
        log_api::LogStoreConfig::new(ctx.store_root(".log"), "default")
    }

    fn capture(id: &str, detail: &str) -> log_api::ValidationLogCapture {
        log_api::ValidationLogCapture {
            id: id.to_string(),
            validation_execution_id: "vt-log-domain".to_string(),
            kind: log_api::ValidationLogKind::CombinedOutput,
            captured_at: Utc::now(),
            media_type: "text/plain".to_string(),
            locator: "memory://matrix".to_string(),
            detail: Some(detail.to_string()),
            links: log_api::ValidationLogLinks::default(),
        }
    }
}

impl DomainOps for LogDomain {
    fn domain(&self) -> &'static str {
        "log"
    }

    fn create(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-create", "first"))
            .map_err(|err| err.to_string())?;
        config
            .get_capture("matrix-create")
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-get", "first"))
            .map_err(|err| err.to_string())?;
        let fetched = config
            .get_capture("matrix-get")
            .map_err(|err| err.to_string())?;
        if fetched.id == "matrix-get" {
            pass()
        } else {
            Err(format!("unexpected capture id: {}", fetched.id))
        }
    }

    fn search(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-search", "first"))
            .map_err(|err| err.to_string())?;
        let captures = config
            .list_captures(&log_api::LogCaptureQuery::default())
            .map_err(|err| err.to_string())?;
        if captures.is_empty() {
            return Err("capture query returned no records".to_string());
        }
        pass()
    }

    fn update(&self, ctx: &MatrixCtx) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_capture(&Self::capture("matrix-update", "first"))
            .map_err(|err| err.to_string())?;
        config
            .record_capture(&Self::capture("matrix-update", "second"))
            .map_err(|err| err.to_string())?;
        let fetched = config
            .get_capture("matrix-update")
            .map_err(|err| err.to_string())?;
        if fetched.detail.as_deref() == Some("second") {
            pass()
        } else {
            Err("re-record did not overwrite capture detail".to_string())
        }
    }

    fn delete(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked("log-api exposes no delete operation for captures")
    }

    fn scan(&self, _ctx: &MatrixCtx) -> CellResult {
        blocked(
            "log-api has no scan/index reconcile; captures are listed directly \
             from disk",
        )
    }
}

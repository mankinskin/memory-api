use std::path::PathBuf;

use clap::{
    Args,
    Parser,
    Subcommand,
    ValueEnum,
};
use log_api::ValidationLogKind;
use test_api::{
    ExecutionSort,
    ValidationOutcome,
};

/// Directory name for the test-result store (sibling of `.ticket` / `.spec`).
pub(crate) const TEST_STORE_DIR: &str = ".test";
/// Directory name for the validation-log store (sibling of `.test`).
pub(crate) const LOG_STORE_DIR: &str = ".log";

#[derive(Debug, Parser)]
#[command(
    name = "test",
    about = "Test system CLI: record and query validation evidence (specs + executions)",
    version,
    arg_required_else_help = true
)]
pub struct TestCli {
    /// Return machine-readable JSON output.
    #[arg(long, global = true, conflicts_with = "toon")]
    pub json: bool,

    /// Return machine-readable TOON output.
    #[arg(long, global = true, conflicts_with = "json")]
    pub toon: bool,

    /// Explicit test store root (the `.test` directory).
    #[arg(long, global = true)]
    pub store_root: Option<PathBuf>,

    /// Workspace/repo root to normalize to the canonical `.test` store.
    #[arg(long = "workspace", alias = "workspace-root", global = true)]
    pub workspace_root: Option<PathBuf>,

    /// Workspace slug that scopes test storage.
    #[arg(long, global = true, default_value = "default")]
    pub workspace_slug: String,

    #[command(subcommand)]
    pub command: TestCommand,
}

#[derive(Debug, Subcommand)]
pub enum TestCommand {
    /// Record (create or overwrite) a validation spec.
    RecordSpec(RecordSpecArgs),
    /// Record (create or overwrite) a validation execution.
    Record(RecordArgs),
    /// Read a validation spec by id.
    GetSpec(GetArgs),
    /// Read a validation execution by id.
    Get(GetArgs),
    /// List validation specs.
    ListSpecs,
    /// List validation executions with optional filters.
    List(ListArgs),
    /// List benchmark executions with domain/operation/over-budget filters.
    Benchmarks(BenchmarkListArgs),
    /// Generate and write the deterministic test-store index (index.toon + README.md).
    StoreIndex,
    /// Render the store-index summary (markdown + digest) without writing files.
    Summary,
    /// Surface failed, over-budget, and slow runs ordered by severity.
    Audit,
    /// Record a validation log capture for an execution.
    LogRecord(LogRecordArgs),
    /// List validation log captures, optionally filtered by execution id.
    Logs(LogsArgs),
    /// Run a test/bench command, capturing timing + output into the test and log stores.
    Run(RunArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutcomeArg {
    Passed,
    Failed,
    Blocked,
}

impl From<OutcomeArg> for ValidationOutcome {
    fn from(value: OutcomeArg) -> Self {
        match value {
            OutcomeArg::Passed => ValidationOutcome::Passed,
            OutcomeArg::Failed => ValidationOutcome::Failed,
            OutcomeArg::Blocked => ValidationOutcome::Blocked,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SortArg {
    NewestFirst,
    SlowestFirst,
}

impl From<SortArg> for ExecutionSort {
    fn from(value: SortArg) -> Self {
        match value {
            SortArg::NewestFirst => ExecutionSort::NewestFirst,
            SortArg::SlowestFirst => ExecutionSort::SlowestFirst,
        }
    }
}

#[derive(Debug, Args)]
pub struct RecordSpecArgs {
    /// Stable spec id (path-safe).
    #[arg(long)]
    pub id: String,
    /// Human-readable title.
    #[arg(long)]
    pub title: String,
    /// Command this validation runs.
    #[arg(long)]
    pub command: Option<String>,
    /// Free-text detail.
    #[arg(long)]
    pub detail: Option<String>,
    /// Slow-run budget threshold in milliseconds for this validation spec.
    #[arg(long)]
    pub slow_threshold_ms: Option<u64>,
    /// Linked ticket ids (repeatable).
    #[arg(long = "ticket")]
    pub ticket_ids: Vec<String>,
    /// Linked spec ids (repeatable).
    #[arg(long = "spec")]
    pub spec_ids: Vec<String>,
    /// Linked acceptance-criterion ids (repeatable).
    #[arg(long = "criterion")]
    pub criterion_ids: Vec<String>,
}

#[derive(Debug, Args)]
pub struct RecordArgs {
    /// Stable execution id (path-safe).
    #[arg(long)]
    pub id: String,
    /// The validation spec id this execution belongs to.
    #[arg(long)]
    pub spec_id: String,
    /// Outcome of the execution.
    #[arg(long, value_enum)]
    pub outcome: OutcomeArg,
    /// Free-text detail (command output summary, blocker reason, etc.).
    #[arg(long)]
    pub detail: Option<String>,
    /// RFC3339 execution timestamp. Defaults to now (UTC).
    #[arg(long)]
    pub executed_at: Option<String>,
    /// Wall time in milliseconds for the validated operation.
    #[arg(long)]
    pub duration_ms: Option<u64>,
    /// Optional throughput metric (ops/sec or items/sec).
    #[arg(long)]
    pub throughput: Option<f64>,
    /// Linked ticket ids (repeatable).
    #[arg(long = "ticket")]
    pub ticket_ids: Vec<String>,
    /// Linked spec ids (repeatable).
    #[arg(long = "spec")]
    pub spec_ids: Vec<String>,
    /// Linked log ids (repeatable).
    #[arg(long = "log")]
    pub log_ids: Vec<String>,
    /// Source file path that produced the execution.
    #[arg(long)]
    pub source_path: Option<String>,
    /// Stable test/cell identifier inside source_path.
    #[arg(long)]
    pub test_id: Option<String>,
    /// Domain under test (e.g. `ticket`).
    #[arg(long)]
    pub domain: Option<String>,
    /// Operation under test (e.g. `get`).
    #[arg(long)]
    pub operation: Option<String>,
    /// Transport used to execute the check (e.g. `cli`, `mcp`, `http`, `in-process`).
    #[arg(long)]
    pub transport: Option<String>,
    /// Run id grouping executions from one harness invocation.
    #[arg(long)]
    pub run_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct GetArgs {
    /// Identifier to read.
    #[arg(long)]
    pub id: String,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Only executions linked to this ticket id.
    #[arg(long)]
    pub ticket: Option<String>,
    /// Only executions for this validation spec id.
    #[arg(long)]
    pub spec_id: Option<String>,
    /// Only executions with this outcome.
    #[arg(long, value_enum)]
    pub outcome: Option<OutcomeArg>,
    /// Only executions with duration >= this threshold.
    #[arg(long)]
    pub min_duration_ms: Option<u64>,
    /// Only executions with duration <= this threshold.
    #[arg(long)]
    pub max_duration_ms: Option<u64>,
    /// Only executions in this provenance domain.
    #[arg(long)]
    pub domain: Option<String>,
    /// Only executions for this provenance operation.
    #[arg(long)]
    pub operation: Option<String>,
    /// Only executions recorded for this transport.
    #[arg(long)]
    pub transport: Option<String>,
    /// Only executions with this run id.
    #[arg(long)]
    pub run_id: Option<String>,
    /// Sort executions by newest-first or slowest-first.
    #[arg(long, value_enum)]
    pub sort: Option<SortArg>,
    /// Maximum number of executions to return.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Args)]
pub struct BenchmarkListArgs {
    /// Only benchmarks in this domain (e.g. `ticket`).
    #[arg(long)]
    pub domain: Option<String>,
    /// Only benchmarks for this operation (e.g. `get`).
    #[arg(long)]
    pub operation: Option<String>,
    /// Only benchmarks that exceeded their latency budget.
    #[arg(long)]
    pub over_budget: bool,
    /// Maximum number of benchmarks to return.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogKindArg {
    Stdout,
    Stderr,
    CombinedOutput,
    StructuredSummary,
}

impl From<LogKindArg> for ValidationLogKind {
    fn from(value: LogKindArg) -> Self {
        match value {
            LogKindArg::Stdout => ValidationLogKind::Stdout,
            LogKindArg::Stderr => ValidationLogKind::Stderr,
            LogKindArg::CombinedOutput => ValidationLogKind::CombinedOutput,
            LogKindArg::StructuredSummary => ValidationLogKind::StructuredSummary,
        }
    }
}

#[derive(Debug, Args)]
pub struct LogRecordArgs {
    /// Stable capture id (path-safe).
    #[arg(long)]
    pub id: String,
    /// The validation execution id this capture belongs to.
    #[arg(long = "execution")]
    pub execution_id: String,
    /// Kind of captured output.
    #[arg(long, value_enum, default_value = "combined-output")]
    pub kind: LogKindArg,
    /// Media type of the captured artifact.
    #[arg(long, default_value = "text/plain")]
    pub media_type: String,
    /// Locator (path/URL) of the captured artifact.
    #[arg(long)]
    pub locator: String,
    /// Free-text detail.
    #[arg(long)]
    pub detail: Option<String>,
    /// RFC3339 capture timestamp. Defaults to now (UTC).
    #[arg(long)]
    pub captured_at: Option<String>,
    /// Linked ticket ids (repeatable).
    #[arg(long = "ticket")]
    pub ticket_ids: Vec<String>,
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    /// Only captures linked to this validation execution id.
    #[arg(long = "execution")]
    pub execution_id: Option<String>,
    /// Maximum number of captures to return.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Shell command to execute (the test/bench suite to run).
    #[arg(long)]
    pub command: String,
    /// The validation spec id this run belongs to.
    #[arg(long)]
    pub spec_id: String,
    /// Explicit execution id. Defaults to `<run-id>-<spec-id>` when a run id is
    /// supplied, otherwise `<spec-id>`.
    #[arg(long)]
    pub id: Option<String>,
    /// Run id grouping all executions from one harness invocation.
    #[arg(long)]
    pub run_id: Option<String>,
    /// Slow-run budget in milliseconds. Overrides the recorded spec threshold.
    #[arg(long)]
    pub slow_threshold_ms: Option<u64>,
    /// Optional throughput metric (ops/sec or items/sec).
    #[arg(long)]
    pub throughput: Option<f64>,
    /// Directory for captured combined stdout/stderr logs.
    #[arg(long, default_value = "target/test-logs")]
    pub log_dir: PathBuf,
    /// Linked ticket ids (repeatable).
    #[arg(long = "ticket")]
    pub ticket_ids: Vec<String>,
    /// Source file path that produced this run.
    #[arg(long)]
    pub source_path: Option<String>,
    /// Stable test/case id for this run.
    #[arg(long)]
    pub test_id: Option<String>,
    /// Domain under test (e.g. `ticket`).
    #[arg(long)]
    pub domain: Option<String>,
    /// Operation under test (e.g. `get`).
    #[arg(long)]
    pub operation: Option<String>,
    /// Transport used by this run (e.g. `cli`, `mcp`, `http`, `in-process`).
    #[arg(long)]
    pub transport: Option<String>,
}

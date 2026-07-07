use std::path::PathBuf;

use chrono::{
    DateTime,
    Utc,
};
use clap::{
    Args,
    Parser,
    Subcommand,
    ValueEnum,
};
use serde_json::{
    json,
    Value,
};

use memory_api::workspace;
use log_api::{
    LogCaptureQuery,
    LogError,
    LogStoreConfig,
    ValidationLogCapture,
    ValidationLogKind,
    ValidationLogLinks,
};
use test_api::{
    ExecutionSort,
    ExecutionQuery,
    BenchmarkQuery,
    TestError,
    TestStoreConfig,
    ValidationExecution,
    ValidationLinks,
    ValidationOutcome,
    ValidationProvenance,
    ValidationSpec,
};

/// Directory name for the test-result store (sibling of `.ticket` / `.spec`).
const TEST_STORE_DIR: &str = ".test";
/// Directory name for the validation-log store (sibling of `.test`).
const LOG_STORE_DIR: &str = ".log";

// ── CLI root ────────────────────────────────────────────────────────────────

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

// ── output helpers ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineOutputFormat {
    Json,
    Toon,
}

pub enum CliOutput {
    Machine(Value, MachineOutputFormat),
    Text(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("test error: {0}")]
    Test(#[from] TestError),
    #[error("log error: {0}")]
    Log(#[from] LogError),
    #[error("invalid timestamp '{0}': {1}")]
    Timestamp(String, String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("io error at {0}: {1}")]
    Io(String, String),
    #[error("failed to launch command '{0}': {1}")]
    Spawn(String, String),
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run(cli: TestCli) -> Result<CliOutput, CliRunError> {
    if matches!(
        cli.command,
        TestCommand::RecordSpec(_)
            | TestCommand::Record(_)
            | TestCommand::LogRecord(_)
            | TestCommand::Run(_)
    ) && cli.store_root.is_none()
        && cli.workspace_root.is_none()
    {
        return Err(CliRunError::BadRequest(
            "entity creation requires explicit --workspace <path> or --store-root <path>".to_string(),
        ));
    }

    let store_root = workspace::resolve_requested_store_root(
        cli.store_root.as_deref(),
        cli.workspace_root.as_deref(),
        None,
        TEST_STORE_DIR,
    );
    let config = TestStoreConfig::new(store_root.clone(), cli.workspace_slug.clone());

    // The validation-log store is the `.log` sibling of the `.test` store.
    let log_root = match store_root.parent() {
        Some(parent) => parent.join(LOG_STORE_DIR),
        None => workspace::resolve_requested_store_root(
            None,
            cli.workspace_root.as_deref(),
            None,
            LOG_STORE_DIR,
        ),
    };
    let log_config = LogStoreConfig::new(log_root, cli.workspace_slug.clone());

    let payload = dispatch(&config, &log_config, cli.command)?;

    match machine_output_format(cli.json, cli.toon) {
        Some(format) => Ok(CliOutput::Machine(payload, format)),
        None => Ok(CliOutput::Text(render_human(&payload))),
    }
}

fn dispatch(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    command: TestCommand,
) -> Result<Value, CliRunError> {
    match command {
        TestCommand::RecordSpec(_)
        | TestCommand::Record(_)
        | TestCommand::LogRecord(_)
        | TestCommand::Run(_) => dispatch_recording(config, log_config, command),
        TestCommand::GetSpec(_)
        | TestCommand::Get(_)
        | TestCommand::ListSpecs
        | TestCommand::List(_)
        | TestCommand::Logs(_) => dispatch_read_queries(config, log_config, command),
        TestCommand::StoreIndex
        | TestCommand::Benchmarks(_)
        | TestCommand::Summary
        | TestCommand::Audit => dispatch_reporting(config, command),
    }
}

fn dispatch_recording(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    command: TestCommand,
) -> Result<Value, CliRunError> {
    match command {
        TestCommand::RecordSpec(args) => {
            let mut spec = ValidationSpec::new(args.id, args.title);
            spec.command = args.command;
            spec.detail = args.detail;
            spec.slow_threshold_ms = args.slow_threshold_ms;
            spec.links = ValidationLinks {
                spec_ids: args.spec_ids,
                acceptance_criterion_ids: args.criterion_ids,
                ticket_ids: args.ticket_ids,
                ..Default::default()
            };
            let path = config.record_spec(&spec)?;
            to_value(&json!({
                "status": "recorded",
                "kind": "validation-spec",
                "id": spec.id,
                "path": path,
            }))
        }
        TestCommand::Record(args) => {
            let executed_at = parse_timestamp(args.executed_at.as_deref())?;
            let mut execution = ValidationExecution::new(
                args.id,
                args.spec_id,
                args.outcome.into(),
                executed_at,
            );
            execution.duration_ms = args.duration_ms;
            execution.throughput = args.throughput;
            execution.detail = args.detail;
            execution.links = ValidationLinks {
                spec_ids: args.spec_ids,
                ticket_ids: args.ticket_ids,
                log_ids: args.log_ids,
                ..Default::default()
            };
            execution.provenance = ValidationProvenance {
                source_path: args.source_path,
                test_id: args.test_id,
                domain: args.domain,
                operation: args.operation,
                transport: args.transport,
                run_id: args.run_id,
            };
            let path = config.record_execution(&execution)?;
            to_value(&json!({
                "status": "recorded",
                "kind": "validation-execution",
                "id": execution.id,
                "outcome": execution.outcome,
                "path": path,
            }))
        }
        TestCommand::LogRecord(args) => {
            let captured_at = parse_timestamp(args.captured_at.as_deref())?;
            let capture = ValidationLogCapture {
                id: args.id,
                validation_execution_id: args.execution_id.clone(),
                kind: args.kind.into(),
                captured_at,
                media_type: args.media_type,
                locator: args.locator,
                detail: args.detail,
                links: ValidationLogLinks {
                    ticket_ids: args.ticket_ids,
                    validation_execution_ids: vec![args.execution_id],
                    ..Default::default()
                },
            };
            let path = log_config.record_capture(&capture)?;
            to_value(&json!({
                "status": "recorded",
                "kind": "validation-log-capture",
                "id": capture.id,
                "path": path,
            }))
        }
        TestCommand::Run(args) => run_harness(config, log_config, args),
        _ => unreachable!("handled in recording dispatch"),
    }
}

fn dispatch_read_queries(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    command: TestCommand,
) -> Result<Value, CliRunError> {
    match command {
        TestCommand::GetSpec(args) => {
            let spec = config.get_spec(&args.id)?;
            to_value(&spec)
        }
        TestCommand::Get(args) => {
            let execution = config.get_execution(&args.id)?;
            to_value(&execution)
        }
        TestCommand::ListSpecs => {
            let specs = config.list_specs()?;
            to_value(&json!({
                "count": specs.len(),
                "specs": specs,
            }))
        }
        TestCommand::List(args) => {
            let query = ExecutionQuery {
                ticket_id: args.ticket,
                validation_spec_id: args.spec_id,
                outcome: args.outcome.map(Into::into),
                min_duration_ms: args.min_duration_ms,
                max_duration_ms: args.max_duration_ms,
                domain: args.domain,
                operation: args.operation,
                transport: args.transport,
                run_id: args.run_id,
                sort: args.sort.map(Into::into).unwrap_or_default(),
                limit: args.limit,
            };
            let executions = config.list_executions(&query)?;
            to_value(&json!({
                "count": executions.len(),
                "executions": executions,
            }))
        }
        TestCommand::Logs(args) => {
            let query = LogCaptureQuery {
                execution_id: args.execution_id,
                limit: args.limit,
            };
            let captures = log_config.list_captures(&query)?;
            to_value(&json!({
                "count": captures.len(),
                "captures": captures,
            }))
        }
        _ => unreachable!("handled in read/query dispatch"),
    }
}

fn dispatch_reporting(
    config: &TestStoreConfig,
    command: TestCommand,
) -> Result<Value, CliRunError> {
    match command {
        TestCommand::StoreIndex => {
            let (digest, toon_path, readme_path) = config.regenerate_store_index()?;
            to_value(&json!({
                "status": "generated",
                "kind": "test-store-index",
                "digest": digest,
                "toon_path": toon_path,
                "readme_path": readme_path,
            }))
        }
        TestCommand::Benchmarks(args) => {
            let query = BenchmarkQuery {
                domain: args.domain,
                operation: args.operation,
                over_budget: if args.over_budget { Some(true) } else { None },
                limit: args.limit,
            };
            let benchmarks = config.list_benchmarks(&query)?;
            to_value(&json!({
                "count": benchmarks.len(),
                "benchmarks": benchmarks,
            }))
        }
        TestCommand::Summary => {
            let artifacts = config.generate_store_index()?;
            to_value(&json!({
                "kind": "test-store-summary",
                "digest": artifacts.digest,
                "summary": artifacts.summary,
                "markdown": artifacts.markdown,
            }))
        }
        TestCommand::Audit => {
            let artifacts = config.generate_store_index()?;
            let summary = &artifacts.summary;

            let failed: Vec<&_> = summary
                .issues
                .iter()
                .filter(|i| i.kind == "execution")
                .collect();
            let over_budget: Vec<&_> = summary
                .issues
                .iter()
                .filter(|i| i.kind == "benchmark")
                .collect();

            to_value(&json!({
                "kind": "test-audit",
                "digest": artifacts.digest,
                "failed_count": failed.len(),
                "over_budget_count": over_budget.len(),
                "slow_count": summary.slow.len(),
                "failed": failed,
                "over_budget": over_budget,
                "slow": summary.slow,
            }))
        }
        _ => unreachable!("handled in reporting dispatch"),
    }
}

/// Execute a test/bench command, capturing its combined stdout/stderr to a log
/// file, measuring wall time, mapping the exit status to a [`ValidationOutcome`],
/// and recording both a [`ValidationExecution`] and a [`ValidationLogCapture`].
fn run_harness(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    args: RunArgs,
) -> Result<Value, CliRunError> {
    use std::{
        fs,
        process::Command,
        time::Instant,
    };

    // Derive a stable execution id grouped by the optional run id.
    let execution_id = match (args.id, args.run_id.as_deref()) {
        (Some(id), _) => id,
        (None, Some(run_id)) => format!("{run_id}-{}", args.spec_id),
        (None, None) => args.spec_id.clone(),
    };
    let capture_id = format!("{execution_id}-log");

    // Resolve the slow threshold: explicit override, else the recorded spec.
    let slow_threshold_ms = match args.slow_threshold_ms {
        Some(threshold) => Some(threshold),
        None => config
            .get_spec(&args.spec_id)
            .ok()
            .and_then(|spec| spec.slow_threshold_ms),
    };

    // Run the command through the platform shell so callers can pass a full
    // command line (e.g. `cargo test -p test-api`).
    let (shell, shell_flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let started = Instant::now();
    let executed_at = Utc::now();
    let output = Command::new(shell)
        .arg(shell_flag)
        .arg(&args.command)
        .output()
        .map_err(|err| CliRunError::Spawn(args.command.clone(), err.to_string()))?;
    let duration_ms = started.elapsed().as_millis() as u64;

    let outcome = if output.status.success() {
        ValidationOutcome::Passed
    } else {
        ValidationOutcome::Failed
    };

    // Persist combined stdout/stderr under the log directory.
    fs::create_dir_all(&args.log_dir)
        .map_err(|err| CliRunError::Io(args.log_dir.display().to_string(), err.to_string()))?;
    let log_path = args.log_dir.join(format!("{execution_id}.log"));
    let mut combined = output.stdout.clone();
    if !output.stderr.is_empty() {
        if !combined.is_empty() {
            combined.push(b'\n');
        }
        combined.extend_from_slice(&output.stderr);
    }
    fs::write(&log_path, &combined)
        .map_err(|err| CliRunError::Io(log_path.display().to_string(), err.to_string()))?;
    let locator = log_path.to_string_lossy().replace('\\', "/");

    let exit_code = output.status.code();
    let over_budget = match (slow_threshold_ms, duration_ms) {
        (Some(threshold), duration) => duration > threshold,
        _ => false,
    };
    let detail = format!(
        "command `{}` exited with {} in {duration_ms}ms",
        args.command,
        exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string()),
    );

    // Record the execution.
    let mut execution = ValidationExecution::new(
        execution_id.clone(),
        args.spec_id.clone(),
        outcome.clone(),
        executed_at,
    );
    execution.duration_ms = Some(duration_ms);
    execution.throughput = args.throughput;
    execution.detail = Some(detail.clone());
    execution.links = ValidationLinks {
        spec_ids: vec![args.spec_id.clone()],
        ticket_ids: args.ticket_ids.clone(),
        log_ids: vec![capture_id.clone()],
        ..Default::default()
    };
    execution.provenance = ValidationProvenance {
        source_path: args.source_path.clone(),
        test_id: args.test_id.clone(),
        domain: args.domain.clone(),
        operation: args.operation.clone(),
        transport: args.transport.clone(),
        run_id: args.run_id.clone(),
    };
    let execution_path = config.record_execution(&execution)?;

    // Record the linked log capture.
    let capture = ValidationLogCapture {
        id: capture_id.clone(),
        validation_execution_id: execution_id.clone(),
        kind: ValidationLogKind::CombinedOutput,
        captured_at: executed_at,
        media_type: "text/plain".to_string(),
        locator: locator.clone(),
        detail: Some(detail),
        links: ValidationLogLinks {
            ticket_ids: args.ticket_ids,
            validation_execution_ids: vec![execution_id.clone()],
            ..Default::default()
        },
    };
    let capture_path = log_config.record_capture(&capture)?;

    to_value(&json!({
        "status": "ran",
        "kind": "validation-run",
        "execution_id": execution_id,
        "run_id": args.run_id,
        "spec_id": args.spec_id,
        "source_path": args.source_path,
        "test_id": args.test_id,
        "domain": args.domain,
        "operation": args.operation,
        "transport": args.transport,
        "outcome": outcome,
        "exit_code": exit_code,
        "duration_ms": duration_ms,
        "over_budget": over_budget,
        "slow_threshold_ms": slow_threshold_ms,
        "log_capture_id": capture_id,
        "log_locator": locator,
        "execution_path": execution_path,
        "capture_path": capture_path,
    }))
}

fn parse_timestamp(raw: Option<&str>) -> Result<DateTime<Utc>, CliRunError> {
    match raw {
        None => Ok(Utc::now()),
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|err| CliRunError::Timestamp(value.to_string(), err.to_string())),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, CliRunError> {
    serde_json::to_value(value).map_err(|err| CliRunError::Serialization(err.to_string()))
}

fn render_human(payload: &Value) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| format!("{payload:?}"))
}

pub fn error_output(
    message: &str,
    format: Option<MachineOutputFormat>,
) -> String {
    let payload = json!({"status": "error", "message": message});
    match format {
        Some(MachineOutputFormat::Json) => payload.to_string(),
        Some(MachineOutputFormat::Toon) => toon_format::encode_default(&payload)
            .unwrap_or_else(|_| format!("status: error\nmessage: {message}")),
        None => message.to_string(),
    }
}

pub fn render_machine_output(
    payload: &Value,
    format: MachineOutputFormat,
) -> Result<String, String> {
    match format {
        MachineOutputFormat::Json => {
            serde_json::to_string_pretty(payload).map_err(|err| err.to_string())
        },
        MachineOutputFormat::Toon => {
            toon_format::encode_default(payload).map_err(|err| err.to_string())
        },
    }
}

pub fn machine_output_format(
    as_json: bool,
    as_toon: bool,
) -> Option<MachineOutputFormat> {
    if as_json {
        Some(MachineOutputFormat::Json)
    } else if as_toon {
        Some(MachineOutputFormat::Toon)
    } else {
        None
    }
}

pub fn requested_machine_output_format_from_args() -> Option<MachineOutputFormat> {
    machine_output_format(
        std::env::args().any(|arg| arg == "--json"),
        std::env::args().any(|arg| arg == "--toon"),
    )
}

pub fn parse_cli_from<I, T>(args: I) -> Result<TestCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    TestCli::try_parse_from(args)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn store_args(dir: &TempDir) -> Vec<String> {
        vec![
            "test".to_string(),
            "--store-root".to_string(),
            dir.path().join(".test").to_string_lossy().to_string(),
        ]
    }

    #[test]
    fn parses_record_command() {
        let cli = parse_cli_from([
            "test", "record", "--id", "exec-1", "--spec-id", "vt-a", "--outcome", "passed",
            "--ticket", "ticket-1",
        ])
        .expect("parse record");
        assert_eq!(cli.workspace_slug, "default");
        match cli.command {
            TestCommand::Record(args) => {
                assert_eq!(args.id, "exec-1");
                assert_eq!(args.spec_id, "vt-a");
                assert_eq!(args.ticket_ids, vec!["ticket-1".to_string()]);
                assert!(matches!(args.outcome, OutcomeArg::Passed));
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn json_and_toon_conflict() {
        let result = parse_cli_from([
            "test", "--json", "--toon", "get", "--id", "exec-1",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn record_then_list_round_trips_through_store() {
        let dir = TempDir::new().unwrap();

        let mut spec_args = store_args(&dir);
        spec_args.extend(
            [
                "record-spec", "--id", "vt-core", "--title", "Core tests", "--command",
                "cargo test -p ticket-vscode-core", "--ticket", "ticket-parity",
            ]
            .map(String::from),
        );
        run(parse_cli_from(spec_args).unwrap()).expect("record spec");

        let mut exec_args = store_args(&dir);
        exec_args.extend(
            [
                "record", "--id", "exec-core", "--spec-id", "vt-core", "--outcome", "passed",
                "--detail", "16 passed", "--executed-at", "2026-06-15T12:00:00Z", "--ticket",
                "ticket-parity",
            ]
            .map(String::from),
        );
        run(parse_cli_from(exec_args).unwrap()).expect("record execution");

        let mut list_args = store_args(&dir);
        list_args.extend(["--json", "list", "--ticket", "ticket-parity"].map(String::from));
        let output = run(parse_cli_from(list_args).unwrap()).expect("list");

        match output {
            CliOutput::Machine(value, MachineOutputFormat::Json) => {
                assert_eq!(value["count"], 1);
                assert_eq!(value["executions"][0]["id"], "exec-core");
                assert_eq!(value["executions"][0]["outcome"], "passed");
            },
            other => panic!("unexpected output variant: {}", matches!(other, CliOutput::Text(_))),
        }
    }

    #[test]
    fn log_record_then_logs_round_trips_through_store() {
        let dir = TempDir::new().unwrap();

        let mut record_args = store_args(&dir);
        record_args.extend(
            [
                "--json", "log-record", "--id", "cap-1", "--execution", "exec-1",
                "--kind", "stderr", "--locator", "target/test-logs/x.log", "--ticket",
                "ticket-1",
            ]
            .map(String::from),
        );
        run(parse_cli_from(record_args).unwrap()).expect("record log capture");

        let mut logs_args = store_args(&dir);
        logs_args.extend(["--json", "logs", "--execution", "exec-1"].map(String::from));
        let output = run(parse_cli_from(logs_args).unwrap()).expect("list logs");

        match output {
            CliOutput::Machine(value, MachineOutputFormat::Json) => {
                assert_eq!(value["count"], 1);
                assert_eq!(value["captures"][0]["id"], "cap-1");
                assert_eq!(value["captures"][0]["kind"], "stderr");
            },
            other => panic!("unexpected output variant: {}", matches!(other, CliOutput::Text(_))),
        }
    }

    #[test]
    fn audit_reports_failed_and_slow_counts() {
        let dir = TempDir::new().unwrap();

        // Spec with a slow threshold so a slow execution is surfaced.
        let mut spec_args = store_args(&dir);
        spec_args.extend(
            [
                "record-spec", "--id", "vt-a", "--title", "A", "--slow-threshold-ms", "10",
            ]
            .map(String::from),
        );
        run(parse_cli_from(spec_args).unwrap()).expect("record spec");

        let mut fail_args = store_args(&dir);
        fail_args.extend(
            [
                "record", "--id", "exec-fail", "--spec-id", "vt-a", "--outcome", "failed",
                "--duration-ms", "50", "--executed-at", "2026-06-15T12:00:00Z",
            ]
            .map(String::from),
        );
        run(parse_cli_from(fail_args).unwrap()).expect("record failed execution");

        let mut audit_args = store_args(&dir);
        audit_args.extend(["--json", "audit"].map(String::from));
        let output = run(parse_cli_from(audit_args).unwrap()).expect("audit");

        match output {
            CliOutput::Machine(value, MachineOutputFormat::Json) => {
                assert_eq!(value["failed_count"], 1);
                assert_eq!(value["slow_count"], 1);
                assert_eq!(value["failed"][0]["id"], "exec-fail");
            },
            other => panic!("unexpected output variant: {}", matches!(other, CliOutput::Text(_))),
        }
    }
}

#[cfg(test)]
mod harness_tests {
    use tempfile::TempDir;

    use super::*;

    fn store_args(dir: &TempDir) -> Vec<String> {
        vec![
            "test".to_string(),
            "--store-root".to_string(),
            dir.path().join(".test").to_string_lossy().to_string(),
        ]
    }

    fn run_value(args: Vec<String>) -> Value {
        match run(parse_cli_from(args).unwrap()).expect("run command") {
            CliOutput::Machine(value, MachineOutputFormat::Json) => value,
            _ => panic!("expected json output"),
        }
    }

    #[test]
    fn run_passes_records_execution_and_capture() {
        let dir = TempDir::new().unwrap();
        let log_dir = dir.path().join("logs");

        let mut args = store_args(&dir);
        args.extend(
            [
                "--json",
                "run",
                "--command",
                "echo harness-ok",
                "--spec-id",
                "vt-a",
                "--run-id",
                "run-1",
                "--log-dir",
                log_dir.to_string_lossy().as_ref(),
                "--ticket",
                "ticket-1",
            ]
            .map(String::from),
        );
        let value = run_value(args);

        assert_eq!(value["status"], "ran");
        assert_eq!(value["outcome"], "passed");
        assert_eq!(value["exit_code"], 0);
        assert_eq!(value["execution_id"], "run-1-vt-a");
        assert_eq!(value["log_capture_id"], "run-1-vt-a-log");

        // The log file exists and contains the captured output.
        let locator = value["log_locator"].as_str().unwrap();
        let contents = std::fs::read_to_string(locator).expect("read log");
        assert!(contents.contains("harness-ok"));

        // The execution is queryable by ticket, with duration recorded.
        let mut list_args = store_args(&dir);
        list_args.extend(["--json", "list", "--ticket", "ticket-1"].map(String::from));
        let listed = run_value(list_args);
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["executions"][0]["id"], "run-1-vt-a");
        assert!(listed["executions"][0]["duration_ms"].is_number());

        // The capture is queryable by execution id.
        let mut logs_args = store_args(&dir);
        logs_args.extend(["--json", "logs", "--execution", "run-1-vt-a"].map(String::from));
        let logs = run_value(logs_args);
        assert_eq!(logs["count"], 1);
        assert_eq!(logs["captures"][0]["id"], "run-1-vt-a-log");
    }

    #[test]
    fn run_failure_maps_to_failed_outcome() {
        let dir = TempDir::new().unwrap();
        let log_dir = dir.path().join("logs");

        let mut args = store_args(&dir);
        args.extend(
            [
                "--json",
                "run",
                "--command",
                "exit 3",
                "--spec-id",
                "vt-fail",
                "--log-dir",
                log_dir.to_string_lossy().as_ref(),
            ]
            .map(String::from),
        );
        let value = run_value(args);

        assert_eq!(value["outcome"], "failed");
        assert_eq!(value["exit_code"], 3);
        // No explicit execution id or run id → defaults to the spec id.
        assert_eq!(value["execution_id"], "vt-fail");
    }

    #[test]
    fn run_flags_over_budget_against_spec_threshold() {
        let dir = TempDir::new().unwrap();
        let log_dir = dir.path().join("logs");

        // Record a spec with a 0ms slow threshold so any real run is over budget.
        let mut spec_args = store_args(&dir);
        spec_args.extend(
            [
                "record-spec",
                "--id",
                "vt-slow",
                "--title",
                "Slow",
                "--slow-threshold-ms",
                "0",
            ]
            .map(String::from),
        );
        run(parse_cli_from(spec_args).unwrap()).expect("record spec");

        let mut args = store_args(&dir);
        args.extend(
            [
                "--json",
                "run",
                "--command",
                "echo slow",
                "--spec-id",
                "vt-slow",
                "--log-dir",
                log_dir.to_string_lossy().as_ref(),
            ]
            .map(String::from),
        );
        let value = run_value(args);

        assert_eq!(value["over_budget"], true);
        assert_eq!(value["slow_threshold_ms"], 0);
    }
}

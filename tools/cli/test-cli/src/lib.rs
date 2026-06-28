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
use test_api::{
    ExecutionSort,
    ExecutionQuery,
    TestError,
    TestStoreConfig,
    ValidationExecution,
    ValidationLinks,
    ValidationOutcome,
    ValidationSpec,
};

/// Directory name for the test-result store (sibling of `.ticket` / `.spec`).
const TEST_STORE_DIR: &str = ".test";

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
    #[arg(long, global = true)]
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
    /// Sort executions by newest-first or slowest-first.
    #[arg(long, value_enum)]
    pub sort: Option<SortArg>,
    /// Maximum number of executions to return.
    #[arg(long)]
    pub limit: Option<usize>,
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
    #[error("invalid timestamp '{0}': {1}")]
    Timestamp(String, String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run(cli: TestCli) -> Result<CliOutput, CliRunError> {
    let store_root = workspace::resolve_requested_store_root(
        cli.store_root.as_deref(),
        cli.workspace_root.as_deref(),
        None,
        TEST_STORE_DIR,
    );
    let config = TestStoreConfig::new(store_root, cli.workspace_slug.clone());

    let payload = dispatch(&config, cli.command)?;

    match machine_output_format(cli.json, cli.toon) {
        Some(format) => Ok(CliOutput::Machine(payload, format)),
        None => Ok(CliOutput::Text(render_human(&payload))),
    }
}

fn dispatch(
    config: &TestStoreConfig,
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
        },
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
            let path = config.record_execution(&execution)?;
            to_value(&json!({
                "status": "recorded",
                "kind": "validation-execution",
                "id": execution.id,
                "outcome": execution.outcome,
                "path": path,
            }))
        },
        TestCommand::GetSpec(args) => {
            let spec = config.get_spec(&args.id)?;
            to_value(&spec)
        },
        TestCommand::Get(args) => {
            let execution = config.get_execution(&args.id)?;
            to_value(&execution)
        },
        TestCommand::ListSpecs => {
            let specs = config.list_specs()?;
            to_value(&json!({
                "count": specs.len(),
                "specs": specs,
            }))
        },
        TestCommand::List(args) => {
            let query = ExecutionQuery {
                ticket_id: args.ticket,
                validation_spec_id: args.spec_id,
                outcome: args.outcome.map(Into::into),
                min_duration_ms: args.min_duration_ms,
                max_duration_ms: args.max_duration_ms,
                sort: args.sort.map(Into::into).unwrap_or_default(),
                limit: args.limit,
            };
            let executions = config.list_executions(&query)?;
            to_value(&json!({
                "count": executions.len(),
                "executions": executions,
            }))
        },
    }
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
}

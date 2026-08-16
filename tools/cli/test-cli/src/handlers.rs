use chrono::{
    DateTime,
    Utc,
};
use std::path::Path;

use serde_json::{
    Value,
    json,
};

use log_api::{
    LogCaptureQuery,
    LogStoreConfig,
    ValidationLogCapture,
    ValidationLogKind,
    ValidationLogLinks,
};
use spec_api::{
    SpecStore,
    SpecVerificationOutcome,
    recompute_spec_verified_state,
};
use test_api::{
    BenchmarkQuery,
    ExecutionQuery,
    TestStoreConfig,
    ValidationExecution,
    ValidationLinks,
    ValidationOutcome,
    ValidationProvenance,
    ValidationSpec,
};

use crate::{
    CliRunError,
    RunArgs,
    TestCommand,
};

pub(crate) fn dispatch_recording(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    spec_root: &Path,
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
            execution.provenance = ValidationProvenance {
                source_path: args.source_path,
                test_id: args.test_id,
                domain: args.domain,
                operation: args.operation,
                transport: args.transport,
                run_id: args.run_id,
            };
            let path = config.record_execution(&execution)?;
            let (verified_spec_ids, spec_verification) = recompute_linked_specs(
                spec_root,
                config,
                &execution.links.spec_ids,
            );
            to_value(&json!({
                "status": "recorded",
                "kind": "validation-execution",
                "id": execution.id,
                "outcome": execution.outcome,
                "path": path,
                "verified_spec_ids": verified_spec_ids,
                "spec_verification": spec_verification,
            }))
        },
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
        },
        TestCommand::Run(args) =>
            run_harness(config, log_config, spec_root, args),
        _ => unreachable!("handled in recording dispatch"),
    }
}

pub(crate) fn dispatch_read_queries(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    command: TestCommand,
) -> Result<Value, CliRunError> {
    match command {
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
        },
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
        },
        _ => unreachable!("handled in read/query dispatch"),
    }
}

pub(crate) fn dispatch_reporting(
    config: &TestStoreConfig,
    command: TestCommand,
) -> Result<Value, CliRunError> {
    match command {
        TestCommand::StoreIndex => {
            let (digest, toon_path, readme_path) =
                config.regenerate_store_index()?;
            to_value(&json!({
                "status": "generated",
                "kind": "test-store-index",
                "digest": digest,
                "toon_path": toon_path,
                "readme_path": readme_path,
            }))
        },
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
        },
        TestCommand::Summary => {
            let artifacts = config.generate_store_index()?;
            to_value(&json!({
                "kind": "test-store-summary",
                "digest": artifacts.digest,
                "summary": artifacts.summary,
                "markdown": artifacts.markdown,
            }))
        },
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
        },
        _ => unreachable!("handled in reporting dispatch"),
    }
}

/// Execute a test/bench command, capture output to the log store, and record
/// both execution and capture metadata in the test stores.
pub(crate) fn run_harness(
    config: &TestStoreConfig,
    log_config: &LogStoreConfig,
    spec_root: &Path,
    args: RunArgs,
) -> Result<Value, CliRunError> {
    use std::{
        fs,
        process::Command,
        time::Instant,
    };

    let execution_id = match (args.id, args.run_id.as_deref()) {
        (Some(id), _) => id,
        (None, Some(run_id)) => format!("{run_id}-{}", args.spec_id),
        (None, None) => args.spec_id.clone(),
    };
    let capture_id = format!("{execution_id}-log");

    let slow_threshold_ms = match args.slow_threshold_ms {
        Some(threshold) => Some(threshold),
        None => config
            .get_spec(&args.spec_id)
            .ok()
            .and_then(|spec| spec.slow_threshold_ms),
    };

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
        .map_err(|err| {
            CliRunError::Spawn(args.command.clone(), err.to_string())
        })?;
    let duration_ms = started.elapsed().as_millis() as u64;

    let outcome = if output.status.success() {
        ValidationOutcome::Passed
    } else {
        ValidationOutcome::Failed
    };

    fs::create_dir_all(&args.log_dir).map_err(|err| {
        CliRunError::Io(args.log_dir.display().to_string(), err.to_string())
    })?;
    let log_path = args.log_dir.join(format!("{execution_id}.log"));
    let mut combined = output.stdout.clone();
    if !output.stderr.is_empty() {
        if !combined.is_empty() {
            combined.push(b'\n');
        }
        combined.extend_from_slice(&output.stderr);
    }
    fs::write(&log_path, &combined).map_err(|err| {
        CliRunError::Io(log_path.display().to_string(), err.to_string())
    })?;
    let locator = log_path.to_string_lossy().replace('\\', "/");

    let exit_code = output.status.code();
    let over_budget = matches!(
        (slow_threshold_ms, duration_ms),
        (Some(threshold), duration) if duration > threshold
    );
    let detail = format!(
        "command `{}` exited with {} in {duration_ms}ms",
        args.command,
        exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string()),
    );

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
    let (verified_spec_ids, spec_verification) =
        recompute_linked_specs(spec_root, config, &execution.links.spec_ids);

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
        "verified_spec_ids": verified_spec_ids,
        "spec_verification": spec_verification,
    }))
}

fn recompute_linked_specs(
    spec_root: &Path,
    test_store: &TestStoreConfig,
    spec_ids: &[String],
) -> (Vec<String>, Vec<Value>) {
    if spec_ids.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut spec_store = match SpecStore::open_or_init(spec_root) {
        Ok(store) => store,
        Err(error) => {
            let reports = spec_ids
                .iter()
                .map(|spec_id| {
                    json!({
                        "spec_id": spec_id,
                        "status": "error",
                        "error": format!(
                            "failed to open spec store at {}: {}",
                            spec_root.display(),
                            error
                        ),
                    })
                })
                .collect();
            return (Vec::new(), reports);
        },
    };

    let mut verified_spec_ids = Vec::new();
    let mut reports = Vec::new();

    for spec_id in spec_ids {
        let report = match recompute_spec_verified_state(
            &mut spec_store,
            test_store,
            None,
            spec_id,
        ) {
            Ok(outcome) => {
                if outcome.is_verified() {
                    verified_spec_ids.push(spec_id.clone());
                }
                spec_outcome_json(spec_id, &outcome)
            },
            Err(error) => json!({
                "spec_id": spec_id,
                "status": "error",
                "error": error,
            }),
        };
        reports.push(report);
    }

    (verified_spec_ids, reports)
}

/// Render a single spec verification outcome as an actionable JSON report.
///
/// The `status` label distinguishes `no-guards`, `pending`, `failed`,
/// `verified`, and `error`, and pending/failed reports carry the specific
/// guard ids responsible so the result is directly actionable.
fn spec_outcome_json(
    spec_id: &str,
    outcome: &SpecVerificationOutcome,
) -> Value {
    let mut report = json!({
        "spec_id": spec_id,
        "status": outcome.label(),
    });
    match outcome {
        SpecVerificationOutcome::Pending { missing_guards } => {
            report["missing_guards"] = json!(missing_guards);
        },
        SpecVerificationOutcome::Failed { failed_guards } => {
            report["failed_guards"] = json!(failed_guards);
        },
        SpecVerificationOutcome::NoGuards
        | SpecVerificationOutcome::Verified => {},
    }
    report
}

pub(crate) fn parse_timestamp(
    raw: Option<&str>
) -> Result<DateTime<Utc>, CliRunError> {
    match raw {
        None => Ok(Utc::now()),
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|err| {
                CliRunError::Timestamp(value.to_string(), err.to_string())
            }),
    }
}

pub(crate) fn to_value<T: serde::Serialize>(
    value: &T
) -> Result<Value, CliRunError> {
    serde_json::to_value(value)
        .map_err(|err| CliRunError::Serialization(err.to_string()))
}

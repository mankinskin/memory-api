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

    let locator = value["log_locator"].as_str().unwrap();
    let contents = std::fs::read_to_string(locator).expect("read log");
    assert!(contents.contains("harness-ok"));

    let mut list_args = store_args(&dir);
    list_args.extend(["--json", "list", "--ticket", "ticket-1"].map(String::from));
    let listed = run_value(list_args);
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["executions"][0]["id"], "run-1-vt-a");
    assert!(listed["executions"][0]["duration_ms"].is_number());

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
    assert_eq!(value["execution_id"], "vt-fail");
}

#[test]
fn run_flags_over_budget_against_spec_threshold() {
    let dir = TempDir::new().unwrap();
    let log_dir = dir.path().join("logs");

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

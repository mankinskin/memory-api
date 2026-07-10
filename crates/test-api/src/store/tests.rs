use chrono::TimeZone;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use super::*;
use crate::{
    ValidationLinks,
    ValidationProvenance,
};

fn at(secs: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .with_ymd_and_hms(2026, 6, 15, 12, 0, secs)
        .single()
        .unwrap()
}

fn config(dir: &TempDir) -> TestStoreConfig {
    TestStoreConfig::new(dir.path().join(".test"), "default")
}

#[test]
fn records_and_reads_spec() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    let mut spec = ValidationSpec::new("vt-core-tests", "Core unit tests");
    spec.command = Some("cargo test -p ticket-vscode-core".to_string());

    let path = cfg.record_spec(&spec).unwrap();
    assert!(path.exists());
    assert_eq!(cfg.get_spec("vt-core-tests").unwrap(), spec);
}

#[test]
fn records_and_reads_execution() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    let mut exec =
        ValidationExecution::passed("exec-1", "vt-core-tests", at(0));
    exec.links = ValidationLinks {
        ticket_ids: vec!["ticket-parity".to_string()],
        ..Default::default()
    };
    exec.provenance = ValidationProvenance {
        domain: Some("ticket".to_string()),
        operation: Some("get".to_string()),
        run_id: Some("run-1".to_string()),
        ..Default::default()
    };

    cfg.record_execution(&exec).unwrap();
    assert_eq!(cfg.get_execution("exec-1").unwrap(), exec);
}

#[test]
fn missing_entries_report_not_found() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    assert!(matches!(
        cfg.get_spec("nope"),
        Err(TestError::SpecNotFound(_))
    ));
    assert!(matches!(
        cfg.get_execution("nope"),
        Err(TestError::ExecutionNotFound(_))
    ));
}

#[test]
fn lists_executions_filtered_by_ticket_and_outcome() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);

    let mut passed = ValidationExecution::passed("exec-pass", "vt-a", at(1));
    passed.duration_ms = Some(40);
    passed.links = ValidationLinks {
        ticket_ids: vec!["ticket-x".to_string()],
        ..Default::default()
    };
    passed.provenance = ValidationProvenance {
        domain: Some("ticket".to_string()),
        operation: Some("get".to_string()),
        transport: Some("cli".to_string()),
        run_id: Some("run-2".to_string()),
        ..Default::default()
    };
    let mut blocked =
        ValidationExecution::blocked("exec-blocked", "vt-b", at(2));
    blocked.duration_ms = Some(80);
    blocked.links = ValidationLinks {
        ticket_ids: vec!["ticket-x".to_string()],
        ..Default::default()
    };
    blocked.provenance = ValidationProvenance {
        domain: Some("spec".to_string()),
        operation: Some("search".to_string()),
        transport: Some("mcp".to_string()),
        run_id: Some("run-2".to_string()),
        ..Default::default()
    };
    let mut other = ValidationExecution::passed("exec-other", "vt-a", at(3));
    other.duration_ms = Some(15);
    other.links = ValidationLinks {
        ticket_ids: vec!["ticket-y".to_string()],
        ..Default::default()
    };
    other.provenance = ValidationProvenance {
        domain: Some("ticket".to_string()),
        operation: Some("search".to_string()),
        transport: Some("http".to_string()),
        run_id: Some("run-3".to_string()),
        ..Default::default()
    };

    cfg.record_execution(&passed).unwrap();
    cfg.record_execution(&blocked).unwrap();
    cfg.record_execution(&other).unwrap();

    let by_ticket = cfg
        .list_executions(&ExecutionQuery {
            ticket_id: Some("ticket-x".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_ticket.len(), 2);
    assert_eq!(by_ticket[0].id, "exec-blocked");

    let only_passed = cfg
        .list_executions(&ExecutionQuery {
            outcome: Some(ValidationOutcome::Passed),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_passed.len(), 2);

    let by_spec = cfg
        .list_executions(&ExecutionQuery {
            validation_spec_id: Some("vt-b".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_spec.len(), 1);
    assert_eq!(by_spec[0].id, "exec-blocked");

    let by_duration = cfg
        .list_executions(&ExecutionQuery {
            min_duration_ms: Some(20),
            max_duration_ms: Some(60),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_duration.len(), 1);
    assert_eq!(by_duration[0].id, "exec-pass");

    let by_provenance = cfg
        .list_executions(&ExecutionQuery {
            domain: Some("ticket".to_string()),
            operation: Some("get".to_string()),
            transport: Some("cli".to_string()),
            run_id: Some("run-2".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_provenance.len(), 1);
    assert_eq!(by_provenance[0].id, "exec-pass");

    let slowest = cfg
        .list_executions(&ExecutionQuery {
            sort: ExecutionSort::SlowestFirst,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(slowest[0].id, "exec-blocked");
    assert_eq!(slowest[1].id, "exec-pass");
    assert_eq!(slowest[2].id, "exec-other");
}

#[test]
fn record_execution_keeps_only_newest_two_runs() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);

    let mut run1 = ValidationExecution::passed("exec-run1", "vt-a", at(1));
    run1.links.ticket_ids = vec!["ticket-run".to_string()];
    run1.provenance.domain = Some("test".to_string());
    run1.provenance.operation = Some("run".to_string());
    run1.provenance.run_id = Some("run-1".to_string());
    cfg.record_execution(&run1).unwrap();

    let mut run2 = ValidationExecution::passed("exec-run2", "vt-a", at(2));
    run2.links.ticket_ids = vec!["ticket-run".to_string()];
    run2.provenance.domain = Some("test".to_string());
    run2.provenance.operation = Some("run".to_string());
    run2.provenance.run_id = Some("run-2".to_string());
    cfg.record_execution(&run2).unwrap();

    let mut run3 = ValidationExecution::passed("exec-run3", "vt-a", at(3));
    run3.links.ticket_ids = vec!["ticket-run".to_string()];
    run3.provenance.domain = Some("test".to_string());
    run3.provenance.operation = Some("run".to_string());
    run3.provenance.run_id = Some("run-3".to_string());
    cfg.record_execution(&run3).unwrap();

    assert!(matches!(
        cfg.get_execution("exec-run1"),
        Err(TestError::ExecutionNotFound(_))
    ));
    assert_eq!(cfg.get_execution("exec-run2").unwrap().id, "exec-run2");
    assert_eq!(cfg.get_execution("exec-run3").unwrap().id, "exec-run3");
}

#[test]
fn rejects_path_traversal_ids() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    let spec = ValidationSpec::new("../escape", "bad");
    assert!(matches!(
        cfg.record_spec(&spec),
        Err(TestError::InvalidId(_))
    ));
}

#[test]
fn list_specs_sorted_and_empty_when_absent() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    assert!(cfg.list_specs().unwrap().is_empty());

    cfg.record_spec(&ValidationSpec::new("vt-b", "B")).unwrap();
    cfg.record_spec(&ValidationSpec::new("vt-a", "A")).unwrap();
    let specs = cfg.list_specs().unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].id, "vt-a");
    assert_eq!(specs[1].id, "vt-b");
}

#[test]
fn records_and_queries_benchmarks_by_domain_and_over_budget() {
    use crate::benchmark::{
        BenchmarkExecution,
        BenchmarkQuery,
    };

    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);

    let mut get = BenchmarkExecution::new(
        "bench-get",
        "get_by_id",
        "get",
        "ticket",
        at(1),
    );
    get.run_id = Some("bench-run-1".to_string());
    get.links.ticket_ids = vec!["ticket-bench".to_string()];
    get.mean_ns = 75_000_000;
    get.apply_budget(Some(50_000_000));

    let mut scan = BenchmarkExecution::new(
        "bench-scan",
        "scan_root",
        "scan",
        "ticket",
        at(2),
    );
    scan.run_id = Some("bench-run-1".to_string());
    scan.links.ticket_ids = vec!["ticket-bench".to_string()];
    scan.mean_ns = 400_000_000;
    scan.apply_budget(Some(1_000_000_000));

    let mut spec_search = BenchmarkExecution::new(
        "bench-search",
        "search_q",
        "search",
        "spec",
        at(3),
    );
    spec_search.run_id = Some("bench-run-2".to_string());
    spec_search.links.ticket_ids = vec!["ticket-bench".to_string()];

    cfg.record_benchmark(&get).unwrap();
    cfg.record_benchmark(&scan).unwrap();
    cfg.record_benchmark(&spec_search).unwrap();

    assert_eq!(cfg.get_benchmark("bench-get").unwrap(), get);

    let ticket_benches = cfg
        .list_benchmarks(&BenchmarkQuery {
            domain: Some("ticket".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ticket_benches.len(), 2);
    assert_eq!(ticket_benches[0].id, "bench-scan");

    let over_budget = cfg
        .list_benchmarks(&BenchmarkQuery {
            over_budget: Some(true),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(over_budget.len(), 1);
    assert_eq!(over_budget[0].id, "bench-get");

    let by_op = cfg
        .list_benchmarks(&BenchmarkQuery {
            operation: Some("search".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(by_op.len(), 1);
    assert_eq!(by_op[0].domain, "spec");
}

#[test]
fn missing_benchmark_reports_not_found() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);
    assert!(matches!(
        cfg.get_benchmark("nope"),
        Err(TestError::BenchmarkNotFound(_))
    ));
}

#[test]
fn record_benchmark_rejects_missing_interoperability_contract_fields() {
    use crate::benchmark::BenchmarkExecution;

    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);

    let bench = BenchmarkExecution::new(
        "bench-missing",
        "scan_root",
        "scan",
        "ticket",
        at(1),
    );

    assert!(matches!(
        cfg.record_benchmark(&bench),
        Err(TestError::InteroperabilityContract { .. })
    ));
}

#[test]
fn record_execution_rejects_missing_interoperability_contract_fields() {
    let dir = TempDir::new().unwrap();
    let cfg = config(&dir);

    let exec =
        ValidationExecution::passed("exec-missing", "vt-core-tests", at(1));

    assert!(matches!(
        cfg.record_execution(&exec),
        Err(TestError::InteroperabilityContract { .. })
    ));
}

use chrono::TimeZone;
use pretty_assertions::assert_eq;

use super::{
    ExecutionSort,
    ValidationExecution,
    ValidationLinks,
    ValidationOutcome,
    ValidationProvenance,
    ValidationSpec,
};

fn sample_time() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .with_ymd_and_hms(2026, 6, 2, 12, 0, 0)
        .single()
        .unwrap()
}

#[test]
fn validation_entities_round_trip_through_serde() {
    let spec = ValidationSpec {
        id: "validation-spec-1".to_string(),
        title: "Spec health check".to_string(),
        command: Some("cargo test -p spec-api contract -- --nocapture".to_string()),
        detail: Some("Covers expectation-oriented contract health".to_string()),
        slow_threshold_ms: Some(500),
        links: ValidationLinks {
            spec_ids: vec!["spec-api/contract".to_string()],
            acceptance_criterion_ids: vec!["criterion-contract-health".to_string()],
            ticket_ids: vec!["ticket-contract-health".to_string()],
            doc_evidence_ids: vec!["doc-evidence-1".to_string()],
            log_ids: vec!["log-1".to_string()],
        },
        provenance: ValidationProvenance {
            source_path: Some("crates/test-api/tests/contracts.rs".to_string()),
            test_id: Some("contract_health".to_string()),
            domain: Some("spec".to_string()),
            operation: Some("health".to_string()),
            transport: Some("in-process".to_string()),
            run_id: Some("run-20260628".to_string()),
        },
    };
    let execution = ValidationExecution {
        id: "validation-exec-1".to_string(),
        validation_spec_id: spec.id.clone(),
        outcome: ValidationOutcome::Passed,
        executed_at: sample_time(),
        duration_ms: Some(420),
        throughput: Some(2.5),
        detail: Some("Contract tests passed against structured fields".to_string()),
        links: spec.links.clone(),
        provenance: spec.provenance.clone(),
    };

    let json = serde_json::to_string_pretty(&(spec.clone(), execution.clone())).unwrap();
    let reparsed: (ValidationSpec, ValidationExecution) =
        serde_json::from_str(&json).unwrap();

    assert_eq!(reparsed.0, spec);
    assert_eq!(reparsed.1, execution);
    assert!(json.contains("passed"));
}

#[test]
fn execution_helpers_cover_passed_failed_and_blocked_outcomes() {
    let passed = ValidationExecution::passed("exec-pass", "spec-a", sample_time());
    let failed = ValidationExecution::failed("exec-fail", "spec-a", sample_time());
    let blocked =
        ValidationExecution::blocked("exec-blocked", "spec-a", sample_time());

    assert!(passed.outcome.is_passed());
    assert!(failed.outcome.is_failed());
    assert!(blocked.outcome.is_blocked());
}

#[test]
fn links_connect_specs_tickets_doc_evidence_and_future_logs() {
    let mut spec = ValidationSpec::new("validation-spec-1", "Guidance validation");
    spec.links = ValidationLinks {
        spec_ids: vec!["spec-guidance".to_string()],
        acceptance_criterion_ids: vec!["criterion-guidance".to_string()],
        ticket_ids: vec!["ticket-guidance".to_string()],
        doc_evidence_ids: vec!["doc-guidance".to_string()],
        log_ids: vec!["log-guidance".to_string()],
    };

    let execution = ValidationExecution {
        id: "exec-guidance".to_string(),
        validation_spec_id: spec.id.clone(),
        outcome: ValidationOutcome::Blocked,
        executed_at: sample_time(),
        duration_ms: Some(750),
        throughput: None,
        detail: Some("Blocked by missing generated guidance output".to_string()),
        links: spec.links.clone(),
        provenance: ValidationProvenance::default(),
    };

    assert!(spec.targets_acceptance("criterion-guidance"));
    assert!(spec.links.links_to_spec("spec-guidance"));
    assert!(spec.links.links_to_ticket("ticket-guidance"));
    assert!(execution.references_doc_evidence("doc-guidance"));
    assert!(execution.references_log("log-guidance"));
    assert!(!execution.references_doc_evidence("doc-other"));
}

#[test]
fn over_budget_helper_uses_duration_against_threshold() {
    let mut spec = ValidationSpec::new("validation-spec-1", "Budgeted validation");
    spec.slow_threshold_ms = Some(100);

    let mut execution =
        ValidationExecution::passed("exec-1", "validation-spec-1", sample_time());
    execution.duration_ms = Some(150);

    assert!(spec.is_over_budget(&execution));

    execution.duration_ms = Some(100);
    assert!(!spec.is_over_budget(&execution));

    execution.duration_ms = None;
    assert!(!spec.is_over_budget(&execution));
}

#[test]
fn deserializes_execution_without_timing_fields() {
    let legacy = serde_json::json!({
        "id": "exec-legacy",
        "validation_spec_id": "validation-spec-1",
        "outcome": "passed",
        "executed_at": "2026-06-02T12:00:00Z",
        "links": {
            "ticket_ids": ["ticket-1"]
        }
    });

    let execution: ValidationExecution = serde_json::from_value(legacy).unwrap();
    assert_eq!(execution.duration_ms, None);
    assert_eq!(execution.throughput, None);
    assert!(execution.provenance.is_empty());
}

#[test]
fn execution_sort_defaults_to_newest_first() {
    let parsed: ExecutionSort = serde_json::from_str("\"newest-first\"").unwrap();
    assert_eq!(parsed, ExecutionSort::NewestFirst);
    assert_eq!(ExecutionSort::default(), ExecutionSort::NewestFirst);
}

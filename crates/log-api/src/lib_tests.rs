use chrono::TimeZone;
use pretty_assertions::assert_eq;
use test_api::{
    ValidationExecution,
    ValidationLinks,
};

use super::{
    LogError,
    RuntimeLogFormat,
    RuntimeLogLinks,
    RuntimeLogSession,
    RuntimeLogStatus,
    RuntimeLogTransport,
    ValidationLogCapture,
    ValidationLogKind,
    ValidationLogLinks,
    ValidationLogRetrieval,
};

fn sample_time() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .with_ymd_and_hms(2026, 6, 2, 12, 30, 0)
        .single()
        .unwrap()
}

fn sample_execution() -> ValidationExecution {
    let mut execution = ValidationExecution::passed(
        "exec-1",
        "validation-spec-1",
        sample_time(),
    );
    execution.links = ValidationLinks {
        spec_ids: vec!["spec-1".to_string()],
        acceptance_criterion_ids: vec!["criterion-1".to_string()],
        ticket_ids: vec!["ticket-1".to_string()],
        doc_evidence_ids: vec!["doc-1".to_string()],
        log_ids: vec!["future-log".to_string()],
    };
    execution
}

#[test]
fn captures_and_retrievals_round_trip_through_serde() {
    let execution = sample_execution();
    let capture = ValidationLogCapture::from_execution(
        "capture-1",
        &execution,
        ValidationLogKind::CombinedOutput,
        sample_time(),
        "text/plain",
        "target/test-logs/spec.log",
    );
    let retrieval = ValidationLogRetrieval::new(
        "retrieval-1",
        capture.id.clone(),
        sample_time(),
        capture.locator.clone(),
        capture.media_type.clone(),
        capture.links.clone(),
    );

    let json =
        serde_json::to_string_pretty(&(capture.clone(), retrieval.clone()))
            .unwrap();
    let reparsed: (ValidationLogCapture, ValidationLogRetrieval) =
        serde_json::from_str(&json).unwrap();

    assert_eq!(reparsed.0, capture);
    assert_eq!(reparsed.1, retrieval);
    assert!(json.contains("combined-output"));
}

#[test]
fn captures_inherit_execution_links_and_identity() {
    let execution = sample_execution();
    let capture = ValidationLogCapture::from_execution(
        "capture-1",
        &execution,
        ValidationLogKind::Stdout,
        sample_time(),
        "text/plain",
        "target/test-logs/spec.stdout",
    );

    assert_eq!(capture.validation_execution_id, execution.id);
    assert!(capture.links.links_to_execution("exec-1"));
    assert!(capture.links.links_to_spec("spec-1"));
    assert!(capture.links.links_to_ticket("ticket-1"));
    assert!(capture.links.links_to_doc_evidence("doc-1"));
}

#[test]
fn capture_interoperability_contract_requires_execution_back_link() {
    let execution = sample_execution();
    let mut capture = ValidationLogCapture::from_execution(
        "capture-interop",
        &execution,
        ValidationLogKind::Stdout,
        sample_time(),
        "text/plain",
        "target/test-logs/spec.stdout",
    );
    capture.links.validation_execution_ids.clear();

    let gaps = capture.interoperability_gaps();
    assert!(gaps.contains(&"missing execution link"));
    assert!(matches!(
        capture.validate_interoperability_contract(),
        Err(crate::LogError::InteroperabilityContract { .. })
    ));
}

#[test]
fn retrievals_preserve_locator_and_link_metadata() {
    let links = ValidationLogLinks {
        spec_ids: vec!["spec-1".to_string()],
        acceptance_criterion_ids: vec!["criterion-1".to_string()],
        ticket_ids: vec!["ticket-1".to_string()],
        doc_evidence_ids: vec!["doc-1".to_string()],
        validation_execution_ids: vec!["exec-1".to_string()],
    };

    let retrieval = ValidationLogRetrieval::new(
        "retrieval-1",
        "capture-1",
        sample_time(),
        "target/test-logs/spec.stderr",
        "text/plain",
        links,
    );

    assert_eq!(retrieval.locator, "target/test-logs/spec.stderr");
    assert!(retrieval.links.links_to_spec("spec-1"));
    assert!(retrieval.links.links_to_ticket("ticket-1"));
    assert!(retrieval.links.links_to_doc_evidence("doc-1"));
    assert!(retrieval.links.links_to_execution("exec-1"));
}

#[test]
fn runtime_sessions_round_trip_through_serde() {
    let mut session = RuntimeLogSession::new(
        "runtime-1",
        sample_time(),
        RuntimeLogStatus::Active,
        "ticket-api",
        RuntimeLogTransport::Mcp,
        "target/test-logs/runtime.jsonl",
        "application/json",
        RuntimeLogFormat::JsonLines,
    );
    session.operation = Some("scan".to_string());
    session.tool = Some("ticket.next".to_string());
    session.route = Some("/api/log/sessions".to_string());
    session.run_id = Some("run-1".to_string());
    session.process_id = Some(4242);
    session.workspace_root = Some("/repo/context-engine".to_string());
    session.store_root = Some("/repo/context-engine/.ticket".to_string());
    session.rotation_policy = Some("size:10MB,keep:5".to_string());
    session.active_filters =
        vec!["info".to_string(), "ticket_api=debug".to_string()];
    session.byte_offset_checkpoint = Some(2048);
    session.links = RuntimeLogLinks {
        spec_ids: vec!["spec-1".to_string()],
        ticket_ids: vec!["ticket-1".to_string()],
        doc_evidence_ids: vec!["doc-1".to_string()],
        validation_execution_ids: vec!["exec-1".to_string()],
        benchmark_ids: vec!["bench-1".to_string()],
        agent_session_ids: vec!["agent-1".to_string()],
        journal_ids: vec!["journal-1".to_string()],
        graph_operation_ids: vec!["graph-op-1".to_string()],
    };

    let json = serde_json::to_string_pretty(&session).unwrap();
    let reparsed: RuntimeLogSession = serde_json::from_str(&json).unwrap();

    assert_eq!(reparsed, session);
    assert!(reparsed.links.links_to_ticket("ticket-1"));
    assert!(reparsed.links.links_to_execution("exec-1"));
    assert!(reparsed.links.links_to_benchmark("bench-1"));
    assert!(reparsed.links.links_to_agent_session("agent-1"));
    assert!(reparsed.links.links_to_journal("journal-1"));
    assert!(reparsed.links.links_to_graph_operation("graph-op-1"));
    assert!(reparsed.interoperability_gaps().is_empty());
    assert!(reparsed.validate_interoperability_contract().is_ok());
}

#[test]
fn runtime_session_interoperability_contract_requires_correlation_links() {
    let session = RuntimeLogSession::new(
        "runtime-2",
        sample_time(),
        RuntimeLogStatus::Active,
        "ticket-api",
        RuntimeLogTransport::Mcp,
        "target/test-logs/runtime-2.jsonl",
        "application/json",
        RuntimeLogFormat::JsonLines,
    );

    let gaps = session.interoperability_gaps();
    assert!(gaps.contains(&"missing operation"));
    assert!(gaps.contains(&"missing run_id"));
    assert!(gaps.contains(&"missing execution, benchmark, journal, agent-session, or graph-operation links"));

    assert!(matches!(
        session.validate_interoperability_contract(),
        Err(LogError::InteroperabilityContract { .. })
    ));
}

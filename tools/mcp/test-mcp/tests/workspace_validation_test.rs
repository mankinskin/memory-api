//! Workspace validation tests for test-mcp.
//!
//! Verifies that invalid workspace selectors produce the canonical error shape
//! for write operations (record_spec, record_execution).

use rmcp::handler::server::wrapper::Parameters;
use test_mcp::server::{
    RecordExecutionInput,
    RecordSpecInput,
    TestServer,
};

fn make_test_server() -> TestServer {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store_root = tmp.path().join(".test");
    std::fs::create_dir_all(&store_root).expect("create .test");
    
    // Keep temp dir alive by leaking it (test cleanup handles this)
    let tmp_path = tmp.keep();
    TestServer::new(tmp_path.join(".test"), "test-workspace".to_string())
}

#[tokio::test]
async fn test_record_spec_workspace_validation() {
    let server = make_test_server();

    // Test 'default' rejection
    let result = server
        .test_record_spec(Parameters(RecordSpecInput {
            workspace: "default".to_string(),
            id: "test-spec".to_string(),
            title: "Test Spec".to_string(),
            command: None,
            detail: None,
            spec_ids: vec![],
            ticket_ids: vec![],
            acceptance_criterion_ids: vec![],
            slow_threshold_ms: None,
        }))
        .await;

    let err = result.expect_err("should fail with 'default'");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("invalid workspace selector"),
        "error should mention 'invalid workspace selector': {err_msg}"
    );
    assert!(
        err_msg.contains("entity creation requires an explicit workspace path"),
        "error should state the requirement: {err_msg}"
    );
    assert!(
        err_msg.contains("'default'"),
        "error should list 'default' as rejected: {err_msg}"
    );

    // Test empty string rejection
    let result = server
        .test_record_spec(Parameters(RecordSpecInput {
            workspace: "".to_string(),
            id: "test-spec".to_string(),
            title: "Test Spec".to_string(),
            command: None,
            detail: None,
            spec_ids: vec![],
            ticket_ids: vec![],
            acceptance_criterion_ids: vec![],
            slow_threshold_ms: None,
        }))
        .await;

    let err = result.expect_err("should fail with empty string");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("invalid workspace selector"),
        "error should mention 'invalid workspace selector': {err_msg}"
    );
}

#[tokio::test]
async fn test_record_execution_workspace_validation() {
    let server = make_test_server();

    // Test 'default' rejection
    let result = server
        .test_record_execution(Parameters(RecordExecutionInput {
            workspace: "default".to_string(),
            id: "test-exec".to_string(),
            validation_spec_id: "test-spec".to_string(),
            outcome: "passed".to_string(),
            executed_at: None,
            duration_ms: None,
            throughput: None,
            detail: None,
            run_id: None,
            test_id: None,
            source_path: None,
            transport: None,
            domain: None,
            operation: None,
            ticket_ids: vec![],
            spec_ids: vec![],
            acceptance_criterion_ids: vec![],
            doc_evidence_ids: vec![],
            log_ids: vec![],
        }))
        .await;

    let err = result.expect_err("should fail with 'default'");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("invalid workspace selector"),
        "error should mention 'invalid workspace selector': {err_msg}"
    );
    assert!(
        err_msg.contains("entity creation requires an explicit workspace path"),
        "error should state the requirement: {err_msg}"
    );

    // Test '..' rejection
    let result = server
        .test_record_execution(Parameters(RecordExecutionInput {
            workspace: "..".to_string(),
            id: "test-exec".to_string(),
            validation_spec_id: "test-spec".to_string(),
            outcome: "passed".to_string(),
            executed_at: None,
            duration_ms: None,
            throughput: None,
            detail: None,
            run_id: None,
            test_id: None,
            source_path: None,
            transport: None,
            domain: None,
            operation: None,
            ticket_ids: vec![],
            spec_ids: vec![],
            acceptance_criterion_ids: vec![],
            doc_evidence_ids: vec![],
            log_ids: vec![],
        }))
        .await;

    let err = result.expect_err("should fail with '..'");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("invalid workspace selector"),
        "error should mention 'invalid workspace selector': {err_msg}"
    );
    assert!(
        err_msg.contains("'..'"),
        "error should list '..' as rejected: {err_msg}"
    );
}

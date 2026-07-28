//! Tests for handoff record round-trip persistence.
//!
//! Validates that all fields accepted by `session_handoff` are persisted
//! and returned unchanged, with no silent drops.

use session_api::{
    SessionHandoffPackage, SessionRuntimeInitRequest, SessionStoreConfig,
    SessionValidationGate,
};
use std::path::PathBuf;

fn setup_test_store() -> (SessionStoreConfig, PathBuf) {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let store_root = temp_dir.path().to_path_buf();
    let config = SessionStoreConfig::new(&store_root, "test-workspace");
    (config, store_root)
}

fn init_test_session(config: &SessionStoreConfig, workspace_session_id: &str) {
    config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(workspace_session_id.to_string()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .expect("init runtime context");
}

#[test]
fn open_escalations_field_persists_and_round_trips() {
    let (config, _temp_dir) = setup_test_store();
    let workspace_session_id = "test-session-123";

    init_test_session(&config, workspace_session_id);

    // Create a handoff package with non-empty open_escalations
    let package = SessionHandoffPackage {
        objective: "Fix the bug".to_string(),
        target_tickets: vec!["ticket-123".to_string()],
        target_files: vec!["memory-api/crates/session-api/src/lib.rs".to_string()],
        decisions: vec!["Use async/await".to_string()],
        non_goals: vec!["No refactoring".to_string()],
        context_anchors: vec!["Related PR #456".to_string()],
        open_escalations: vec![
            "Need clarification on API design".to_string(),
            "Blocked on upstream merge".to_string(),
        ],
        risk_notes: Some("Database migration required".to_string()),
        predecessor_handoff: None,
    };

    let validation = vec![];

    // Create handoff record
    let record = config
        .create_handoff_record(workspace_session_id, Some(package.clone()), validation, None)
        .expect("create handoff record");

    // ASSERT: open_escalations should persist unchanged
    assert_eq!(
        record.open_escalations, package.open_escalations,
        "open_escalations should round-trip unchanged; got {:?} but expected {:?}",
        record.open_escalations, package.open_escalations
    );
    assert_eq!(record.open_escalations.len(), 2);
    assert!(record.open_escalations.contains(&"Need clarification on API design".to_string()));
    assert!(record.open_escalations.contains(&"Blocked on upstream merge".to_string()));
}

#[test]
fn empty_open_escalations_is_persisted_as_empty_list() {
    let (config, _temp_dir) = setup_test_store();
    let workspace_session_id = "test-session-456";

    init_test_session(&config, workspace_session_id);

    let package = SessionHandoffPackage {
        objective: "Implement feature".to_string(),
        target_tickets: vec!["ticket-789".to_string()],
        target_files: vec!["memory-api/crates/session-api/src/error.rs".to_string()],
        decisions: vec!["Use trait bounds".to_string()],
        non_goals: vec!["No optimization yet".to_string()],
        context_anchors: vec!["Spec doc#12".to_string()],
        open_escalations: vec![], // Explicitly empty
        risk_notes: None,
        predecessor_handoff: None,
    };

    let record = config
        .create_handoff_record(workspace_session_id, Some(package.clone()), vec![], None)
        .expect("create handoff record");

    // ASSERT: empty open_escalations should persist as empty list (not absent/null)
    assert_eq!(record.open_escalations, Vec::<String>::new());
    assert!(record.open_escalations.is_empty());
}

#[test]
fn validation_gate_command_field_persists_and_round_trips() {
    let (config, _temp_dir) = setup_test_store();
    let workspace_session_id = "test-session-789";

    init_test_session(&config, workspace_session_id);

    let package = SessionHandoffPackage {
        objective: "Run tests".to_string(),
        target_tickets: vec!["ticket-101".to_string()],
        target_files: vec!["memory-api/crates/session-api/src/store.rs".to_string()],
        decisions: vec!["Use Criterion benchmarks".to_string()],
        non_goals: vec!["No UI tests".to_string()],
        context_anchors: vec!["Test plan doc".to_string()],
        open_escalations: vec![],
        risk_notes: None,
        predecessor_handoff: None,
    };

    let validation = vec![SessionValidationGate {
        validation_spec_id: "val-test-suite".to_string(),
        required: true,
        outcome: None,
        command: Some("cargo test -p session-api".to_string()),
    }];

    let record = config
        .create_handoff_record(workspace_session_id, Some(package), validation.clone(), None)
        .expect("create handoff record");

    // ASSERT: command field should persist unchanged
    assert_eq!(record.validation.len(), 1);
    let gate = &record.validation[0];
    assert_eq!(gate.validation_spec_id, "val-test-suite");
    assert_eq!(gate.required, true);
    assert_eq!(
        gate.command,
        Some("cargo test -p session-api".to_string()),
        "command field should round-trip unchanged"
    );
}

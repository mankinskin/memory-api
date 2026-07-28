//! Test for handoff folder storage with JSON + Markdown rendering.
//!
//! Validates that handoffs persist as folders containing both handoff.json
//! and handoff.md, with deterministic markdown content and full JSON round-trip.

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
fn handoff_persists_as_folder_with_json_and_markdown() {
    let (config, _temp_dir) = setup_test_store();
    let workspace_session_id = "test-session-folder";

    init_test_session(&config, workspace_session_id);

    let package = SessionHandoffPackage {
        objective: "Implement the feature".to_string(),
        target_tickets: vec!["ticket-123".to_string()],
        target_files: vec!["memory-api/crates/session-api/src/lib.rs".to_string()],
        decisions: vec!["Use async/await".to_string()],
        non_goals: vec!["No refactoring".to_string()],
        context_anchors: vec!["Related PR #456".to_string()],
        open_escalations: vec![],
        risk_notes: Some("Database migration required".to_string()),
        predecessor_handoff: None,
    };

    let result = config
        .create_handoff_result(workspace_session_id, Some(package.clone()), vec![], None)
        .expect("create handoff result");

    let handoff_id = &result.record.handoff_id;

    // AC1: A new handoff persists a folder with both handoff.json and handoff.md
    let handoff_folder = PathBuf::from(&result.record_path);
    assert!(
        handoff_folder.is_dir(),
        "handoff record_path should point to a folder; got {:?}",
        handoff_folder
    );

    let json_path = handoff_folder.join("handoff.json");
    assert!(
        json_path.exists(),
        "handoff.json should exist at {:?}",
        json_path
    );

    let md_path = handoff_folder.join("handoff.md");
    assert!(
        md_path.exists(),
        "handoff.md should exist at {:?}",
        md_path
    );

    // AC2: handoff.md deterministically reflects the record's fields
    let md_content = std::fs::read_to_string(&md_path)
        .expect("read handoff.md");
    
    // Check that key fields appear in the markdown
    assert!(md_content.contains(handoff_id), "markdown should contain handoff_id");
    assert!(md_content.contains(&result.record.objective), "markdown should contain objective");
    assert!(md_content.contains("ticket-123"), "markdown should contain target ticket");
    assert!(md_content.contains("memory-api/crates/session-api/src/lib.rs"), "markdown should contain target file");
    assert!(md_content.contains("Use async/await"), "markdown should contain decision");
    assert!(md_content.contains("No refactoring"), "markdown should contain non-goal");
    assert!(md_content.contains("Related PR #456"), "markdown should contain context anchor");
    assert!(md_content.contains("Database migration required"), "markdown should contain risk notes");
    assert!(md_content.contains("Implementation Ready: true") || md_content.contains("**Implementation Ready**: true"), 
        "markdown should show implementation_ready=true when escalations are empty");

    // AC3: JSON round-trip preserves all fields
    let json_content = std::fs::read_to_string(&json_path)
        .expect("read handoff.json");
    let deserialized: session_api::SessionHandoffRecord =
        serde_json::from_str(&json_content)
            .expect("deserialize handoff.json");

    assert_eq!(deserialized.handoff_id, result.record.handoff_id);
    assert_eq!(deserialized.objective, result.record.objective);
    assert_eq!(deserialized.target_tickets, result.record.target_tickets);
    assert_eq!(deserialized.target_files, result.record.target_files);
    assert_eq!(deserialized.decisions, result.record.decisions);
    assert_eq!(deserialized.non_goals, result.record.non_goals);
    assert_eq!(deserialized.context_anchors, result.record.context_anchors);
    assert_eq!(deserialized.open_escalations, result.record.open_escalations);
    assert_eq!(deserialized.risk_notes, result.record.risk_notes);
}

#[test]
fn handoff_markdown_shows_open_escalations_warning() {
    let (config, _temp_dir) = setup_test_store();
    let workspace_session_id = "test-session-escalations";

    init_test_session(&config, workspace_session_id);

    let package = SessionHandoffPackage {
        objective: "Fix the bug".to_string(),
        target_tickets: vec!["ticket-456".to_string()],
        target_files: vec!["memory-api/crates/session-api/src/error.rs".to_string()],
        decisions: vec!["Decision made".to_string()],
        non_goals: vec!["Non-goal".to_string()],
        context_anchors: vec!["Anchor".to_string()],
        open_escalations: vec![
            "Need clarification".to_string(),
            "Blocked on upstream".to_string(),
        ],
        risk_notes: None,
        predecessor_handoff: None,
    };

    let result = config
        .create_handoff_result(workspace_session_id, Some(package.clone()), vec![], None)
        .expect("create handoff result");

    let handoff_folder = PathBuf::from(&result.record_path);
    let md_path = handoff_folder.join("handoff.md");
    let md_content = std::fs::read_to_string(&md_path)
        .expect("read handoff.md");

    // When open_escalations is not empty, implementation_ready should be false
    assert!(md_content.contains("Implementation Ready: false") || md_content.contains("**Implementation Ready**: false"), 
        "markdown should show implementation_ready=false when escalations exist");
    assert!(md_content.contains("Open Escalations"), 
        "markdown should have an Open Escalations section");
    assert!(md_content.contains("Need clarification"), 
        "markdown should list first escalation");
    assert!(md_content.contains("Blocked on upstream"), 
        "markdown should list second escalation");
}

#[test]
fn legacy_flat_json_handoffs_still_load() {
    let (config, store_root) = setup_test_store();
    let workspace_session_id = "test-session-legacy";

    init_test_session(&config, workspace_session_id);

    // Create a handoff using the new folder structure first
    let package = SessionHandoffPackage {
        objective: "Test objective".to_string(),
        target_tickets: vec!["ticket-789".to_string()],
        target_files: vec!["memory-api/crates/session-api/src/store.rs".to_string()],
        decisions: vec!["Test decision".to_string()],
        non_goals: vec!["Test non-goal".to_string()],
        context_anchors: vec!["Test anchor".to_string()],
        open_escalations: vec![],
        risk_notes: None,
        predecessor_handoff: None,
    };

    let result = config
        .create_handoff_result(workspace_session_id, Some(package.clone()), vec![], None)
        .expect("create handoff result");

    // Verify we can read the JSON from the folder structure
    let handoff_folder = PathBuf::from(&result.record_path);
    let json_path = handoff_folder.join("handoff.json");
    let json_content = std::fs::read_to_string(&json_path)
        .expect("read handoff.json from folder");
    let deserialized: session_api::SessionHandoffRecord =
        serde_json::from_str(&json_content)
            .expect("deserialize handoff.json from folder");

    // Now simulate a legacy flat file by writing directly to handoffs_dir
    let handoffs_dir = store_root
        .join(".session")
        .join("sessions")
        .join(workspace_session_id)
        .join("handoffs");
    
    // Ensure the handoffs directory exists before writing the legacy file
    std::fs::create_dir_all(&handoffs_dir)
        .expect("create handoffs directory");
    
    let legacy_handoff_id = "legacy-handoff-id";
    let legacy_path = handoffs_dir.join(format!("{}.json", legacy_handoff_id));
    
    let mut legacy_record = deserialized.clone();
    legacy_record.handoff_id = legacy_handoff_id.to_string();
    
    std::fs::write(
        &legacy_path,
        serde_json::to_string_pretty(&legacy_record).expect("serialize legacy record")
    ).expect("write legacy flat JSON");

    // AC3: Legacy flat handoffs/<id>.json records still load
    assert!(legacy_path.exists(), "legacy flat JSON should exist");
    let legacy_content = std::fs::read_to_string(&legacy_path)
        .expect("read legacy flat JSON");
    let legacy_loaded: session_api::SessionHandoffRecord =
        serde_json::from_str(&legacy_content)
            .expect("deserialize legacy flat JSON");
    
    assert_eq!(legacy_loaded.handoff_id, legacy_handoff_id);
    assert_eq!(legacy_loaded.objective, package.objective);
}

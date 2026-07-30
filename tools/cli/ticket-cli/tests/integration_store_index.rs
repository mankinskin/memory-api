//! Focused integration tests for `ticket store-index`.

mod common;

use std::fs;

use common::{
    TicketCommands,
    WorkspaceSandbox,
    create_ticket,
};

#[test]
fn store_index_writes_expected_artifacts_and_check_passes() {
    let s = WorkspaceSandbox::new();

    let ticket_a = create_ticket(&s, "Store index ticket A");
    let ticket_b = create_ticket(&s, "Store index ticket B");

    let _ = s.ticket_json(&[
        "update",
        &ticket_a,
        "--to-state",
        "planned",
        "--field",
        "component=ticket-api",
        "--field",
        "priority=high",
        "--description",
        "Primary summary for ticket A.",
        "--description-mode",
        "replace",
    ]);
    let _ = s.ticket_json(&[
        "update",
        &ticket_b,
        "--field",
        "component=spec-api",
        "--field",
        "priority=medium",
        "--description",
        "Primary summary for ticket B.",
        "--description-mode",
        "replace",
    ]);

    let write_payload = s.ticket_json(&["store-index"]);
    assert_eq!(write_payload["status"], "ok");
    assert_eq!(write_payload["command"], "store-index");
    assert_eq!(write_payload["check"], false);
    assert!(write_payload["tickets"].as_u64().unwrap() >= 2);

    let readme = s.workspace_root().join(".ticket").join("README.md");
    let sidecar = s.workspace_root().join(".ticket").join("index.toon");
    let hook = s.workspace_root().join(".agents").join("ticket-catalog.md");

    assert!(readme.exists(), "README should be generated");
    assert!(sidecar.exists(), "index.toon should be generated");
    assert!(hook.exists(), "agent hook should be generated");

    let readme_text = fs::read_to_string(&readme).unwrap();
    assert!(readme_text.contains("# Ticket Catalog"));
    assert!(readme_text.contains("## State: planned"));
    assert!(readme_text.contains("## State: open"));
    assert!(readme_text.contains("### Component: ticket-api"));
    assert!(readme_text.contains("### Component: spec-api"));

    let check_payload = s.ticket_json(&["store-index", "--check"]);
    assert_eq!(check_payload["status"], "ok");
    assert_eq!(check_payload["check"], true);
    assert_eq!(check_payload["drift"], false);
}

#[test]
fn store_index_check_detects_readme_drift() {
    let s = WorkspaceSandbox::new();

    let ticket_id = create_ticket(&s, "Drift detection ticket");
    let _ = s.ticket_json(&[
        "update",
        &ticket_id,
        "--field",
        "component=ticket-api",
        "--description",
        "Summary used by store-index.",
        "--description-mode",
        "replace",
    ]);

    let _ = s.ticket_json(&["store-index"]);

    let readme = s.workspace_root().join(".ticket").join("README.md");
    let mut tampered = fs::read_to_string(&readme).unwrap();
    tampered.push_str("\n<!-- tampered -->\n");
    fs::write(&readme, tampered).unwrap();

    let (code, stderr) = s.ticket_fail(&["store-index", "--check"]);
    assert_eq!(code, 1);
    assert!(
        stderr.contains("ticket store-index is out of date"),
        "expected drift error in stderr, got: {stderr}"
    );
}

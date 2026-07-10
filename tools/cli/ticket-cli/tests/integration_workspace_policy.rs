//! Integration coverage for the `ticket workspace` policy command group:
//! show → set → ignore add → rescan, including scan-root skip enforcement.

mod common;

use std::process::Command;

use common::{
    TicketCommands,
    WorkspaceSandbox,
};

const TICKET: &str = env!("CARGO_BIN_EXE_ticket");

#[test]
fn workspace_policy_show_set_ignore_rescan_flow() {
    let sandbox = WorkspaceSandbox::new();

    // 1. show — no file yet, so the source is compatibility defaults.
    let shown = sandbox.ticket_json(&["workspace", "policy", "show"]);
    assert_eq!(shown["status"], "ok");
    assert_eq!(shown["source"], "compatibility-defaults");

    // 2. set — persist boolean fields; file is created on first set.
    let set = sandbox.ticket_json(&[
        "workspace",
        "policy",
        "set",
        "--include-descendants",
        "true",
        "--include-ancestors",
        "false",
        "--deny-external-paths",
        "true",
    ]);
    assert_eq!(set["status"], "ok");
    assert_eq!(set["policy"]["include_descendants"], true);
    assert_eq!(set["policy"]["include_ancestors"], false);

    // 3. show again — now sourced from the file, preserving the set values.
    let shown = sandbox.ticket_json(&["workspace", "policy", "show"]);
    assert_eq!(shown["source"], "file");
    assert_eq!(shown["policy"]["include_descendants"], true);
    assert_eq!(shown["policy"]["deny_external_paths"], true);

    // Create a descendant fixture store with its own ticket.
    let fixture_index =
        sandbox.workspace_root().join("fixtures").join(".ticket");
    run_ticket(&[
        "--index-root",
        fixture_index.to_str().unwrap(),
        "--json",
        "init",
    ]);
    run_ticket(&[
        "--index-root",
        fixture_index.to_str().unwrap(),
        "--json",
        "create",
        "--title",
        "Fixture ticket zzmarker",
        "--type",
        "tracker-improvement",
        "--state",
        "ready",
    ]);

    // First rescan (no ignore): fixture root is included and indexed.
    let rescan =
        sandbox.ticket_json(&["workspace", "rescan", "--apply-policy"]);
    assert_eq!(rescan["status"], "ok");
    assert_eq!(rescan["apply_policy"], true);
    let visible_before = sandbox
        .ticket_json(&["search", "zzmarker"])
        .get("results")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);
    assert!(
        visible_before >= 1,
        "fixture ticket should be visible before ignore"
    );

    // 4. ignore add — exclude the fixture workspace by relative path.
    let ignored =
        sandbox.ticket_json(&["workspace", "ignore", "add", "fixtures"]);
    assert_eq!(ignored["status"], "ok");
    assert_eq!(ignored["changed"], true);
    assert_eq!(
        ignored["policy"]["ignore_workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|value| *value == "fixtures")
            .count(),
        1
    );

    // 5. rescan again — the fixture root is now skipped by policy.
    let rescan =
        sandbox.ticket_json(&["workspace", "rescan", "--apply-policy"]);
    let skipped = rescan["skipped_roots"].as_array().unwrap();
    assert!(
        skipped.iter().any(|label| label == "fixtures"),
        "expected fixtures in skipped_roots, got {skipped:?}"
    );

    // The fixture ticket is no longer visible through search.
    let visible_after = sandbox
        .ticket_json(&["search", "zzmarker"])
        .get("results")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);
    assert_eq!(
        visible_after, 0,
        "fixture ticket must be excluded after ignore"
    );
}

#[test]
fn workspace_commands_are_forbidden_in_batch() {
    use std::{
        io::Write,
        process::Stdio,
    };

    let sandbox = WorkspaceSandbox::new();
    let mut child = Command::new(TICKET)
        .arg("--index-root")
        .arg(sandbox.index_root())
        .arg("--json")
        .arg("batch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ticket batch");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"workspace policy set --include-descendants false\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| {
            panic!(
                "batch stdout not JSON: {e}\nraw: {}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
    let payload = &envelope["payload"];
    assert_eq!(
        payload["status"], "error",
        "batch should reject workspace command"
    );
    let message = payload["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("workspace") && message.contains("batch"),
        "expected batch rejection message, got: {message}"
    );
    assert_eq!(payload["rolled_back"], true);
}

fn run_ticket(args: &[&str]) {
    let out = Command::new(TICKET)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ticket: {e}"));
    assert!(
        out.status.success(),
        "ticket {:?} failed ({})\nstdout: {}\nstderr: {}",
        args,
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

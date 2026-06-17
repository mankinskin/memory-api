//! Focused integration tests for `ticket store-index`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tempfile::TempDir;

const TICKET: &str = env!("CARGO_BIN_EXE_ticket");

struct StoreIndexSandbox {
    _dir: TempDir,
    index_root: PathBuf,
    workspace_root: PathBuf,
}

impl StoreIndexSandbox {
    fn new() -> Self {
        let dir = TempDir::new().expect("failed to create sandbox temp dir");
        let workspace_root = dir.path().to_path_buf();
        let index_root = workspace_root.join(".ticket");

        let out = Command::new(TICKET)
            .arg("--index-root")
            .arg(&index_root)
            .arg("--json")
            .arg("init")
            .output()
            .expect("failed to spawn ticket init");
        assert!(
            out.status.success(),
            "ticket init failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        Self {
            _dir: dir,
            index_root,
            workspace_root,
        }
    }

    fn ticket_json(
        &self,
        args: &[&str],
    ) -> serde_json::Value {
        let out = Command::new(TICKET)
            .arg("--index-root")
            .arg(&self.index_root)
            .arg("--json")
            .args(args)
            .output()
            .expect("failed to spawn ticket command");

        assert!(
            out.status.success(),
            "ticket {:?} failed ({})\nstdout: {}\nstderr: {}",
            args,
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );

        let envelope: serde_json::Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| {
                panic!(
                    "stdout is not valid JSON: {e}\nraw: {}",
                    String::from_utf8_lossy(&out.stdout)
                )
            });
        envelope["payload"].clone()
    }

    fn ticket_fail(
        &self,
        args: &[&str],
    ) -> (i32, String) {
        let out = Command::new(TICKET)
            .arg("--index-root")
            .arg(&self.index_root)
            .arg("--json")
            .args(args)
            .output()
            .expect("failed to spawn ticket command");

        assert!(
            !out.status.success(),
            "expected ticket {:?} to fail but it succeeded\nstdout: {}",
            args,
            String::from_utf8_lossy(&out.stdout)
        );

        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }
}

fn create_ticket(
    s: &StoreIndexSandbox,
    title: &str,
) -> String {
    let payload = s.ticket_json(&[
        "create",
        "--title",
        title,
        "--type",
        "tracker-improvement",
    ]);
    payload["id"].as_str().unwrap().to_string()
}

#[test]
fn store_index_writes_expected_artifacts_and_check_passes() {
    let s = StoreIndexSandbox::new();

    let ticket_a = create_ticket(&s, "Store index ticket A");
    let ticket_b = create_ticket(&s, "Store index ticket B");

    let _ = s.ticket_json(&[
        "update",
        &ticket_a,
        "--to-state",
        "ready",
        "--field",
        "component=ticket-api",
        "--field",
        "priority=high",
        "--description",
        "Primary summary for ticket A.",
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
    ]);

    let write_payload = s.ticket_json(&["store-index"]);
    assert_eq!(write_payload["status"], "ok");
    assert_eq!(write_payload["command"], "store-index");
    assert_eq!(write_payload["check"], false);
    assert!(write_payload["tickets"].as_u64().unwrap() >= 2);

    let readme = s.workspace_root.join(".ticket").join("README.md");
    let sidecar = s.workspace_root.join(".ticket").join("index.toon");
    let hook = s.workspace_root.join(".agents").join("ticket-catalog.md");

    assert!(readme.exists(), "README should be generated");
    assert!(sidecar.exists(), "index.toon should be generated");
    assert!(hook.exists(), "agent hook should be generated");

    let readme_text = fs::read_to_string(&readme).unwrap();
    assert!(readme_text.contains("# Ticket Catalog"));
    assert!(readme_text.contains("## State: ready"));
    assert!(readme_text.contains("## State: new"));
    assert!(readme_text.contains("### Component: ticket-api"));
    assert!(readme_text.contains("### Component: spec-api"));

    let check_payload = s.ticket_json(&["store-index", "--check"]);
    assert_eq!(check_payload["status"], "ok");
    assert_eq!(check_payload["check"], true);
    assert_eq!(check_payload["drift"], false);
}

#[test]
fn store_index_check_detects_readme_drift() {
    let s = StoreIndexSandbox::new();

    let ticket_id = create_ticket(&s, "Drift detection ticket");
    let _ = s.ticket_json(&[
        "update",
        &ticket_id,
        "--field",
        "component=ticket-api",
        "--description",
        "Summary used by store-index.",
    ]);

    let _ = s.ticket_json(&["store-index"]);

    let readme = s.workspace_root.join(".ticket").join("README.md");
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

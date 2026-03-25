//! Shared sandboxed test harness for context-tasks integration tests.
//!
//! Each test creates a `Sandbox` that owns an exclusive `TempDir` used as the
//! `--index-root` for every `ticket` invocation.  When the `Sandbox` is
//! dropped the entire directory tree — redb database, Tantivy search index,
//! and all ticket folders — is deleted automatically.
//!
//! The `ticket` binary is located via `env!("CARGO_BIN_EXE_ticket")`, which
//! Cargo resolves at compile time to the correct path in `target/`.

#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Binary path — resolved at compile time by Cargo.
// ---------------------------------------------------------------------------

const TICKET: &str = env!("CARGO_BIN_EXE_ticket");

// ---------------------------------------------------------------------------
// Sandbox — per-test isolated environment
// ---------------------------------------------------------------------------

/// An isolated sandbox: a fresh temp directory that acts as the sole
/// `--index-root` for all `ticket` invocations within a single test.
pub struct Sandbox {
    /// Keeps the temp directory alive for the duration of the test.
    pub _dir: TempDir,
    /// Path used as `--index-root` for every `ticket` call.
    pub index_root: PathBuf,
}

impl Sandbox {
    /// Create a new isolated sandbox backed by a fresh temporary directory.
    pub fn new() -> Self {
        let dir = TempDir::new().expect("failed to create sandbox temp dir");
        let index_root = dir.path().to_path_buf();
        Self { _dir: dir, index_root }
    }

    // ------------------------------------------------------------------
    // Internal helper: build a base Command with --index-root pre-set.
    // ------------------------------------------------------------------

    fn base(&self) -> Command {
        let mut cmd = Command::new(TICKET);
        cmd.arg("--index-root").arg(&self.index_root);
        cmd
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    /// Run `ticket --json <args>` and return the inner `payload` object.
    ///
    /// Panics with full diagnostic output if the command exits non-zero or the
    /// output cannot be parsed as JSON.
    pub fn ticket_json(&self, args: &[&str]) -> serde_json::Value {
        let out = self
            .base()
            .arg("--json")
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn ticket: {e}"));

        if !out.status.success() {
            panic!(
                "ticket {:?} failed ({})\nstdout: {}\nstderr: {}",
                args,
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
        }

        let envelope: serde_json::Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| {
                panic!(
                    "stdout is not valid JSON: {e}\nraw: {}",
                    String::from_utf8_lossy(&out.stdout)
                )
            });

        // With --json the binary emits { "request_id": "...", "payload": { ... } }.
        envelope["payload"].clone()
    }

    /// Run `ticket --json <args>` and **expect** it to exit with a non-zero code.
    ///
    /// Panics if the command succeeds instead.  Returns `(exit_code, stderr)`.
    pub fn ticket_fail(&self, args: &[&str]) -> (i32, String) {
        let out = self
            .base()
            .arg("--json")
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn ticket: {e}"));

        assert!(
            !out.status.success(),
            "expected ticket {:?} to fail but it succeeded\nstdout: {}",
            args,
            String::from_utf8_lossy(&out.stdout),
        );

        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    /// Run `ticket --json <args>` and feed `stdin_payload` to stdin.
    ///
    /// Panics if the command exits non-zero. Returns envelope `payload`.
    pub fn ticket_json_stdin(&self, args: &[&str], stdin_payload: &str) -> serde_json::Value {
        let mut child = self
            .base()
            .arg("--json")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn ticket with stdin");

        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin_payload.as_bytes())
            .unwrap();

        let out = child
            .wait_with_output()
            .expect("failed to wait for ticket command");

        if !out.status.success() {
            panic!(
                "ticket {:?} failed ({})\nstdin: {}\nstdout: {}\nstderr: {}",
                args,
                out.status,
                stdin_payload,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
        }

        let envelope: serde_json::Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| {
                panic!(
                    "stdout is not valid JSON: {e}\nraw: {}",
                    String::from_utf8_lossy(&out.stdout)
                )
            });

        envelope["payload"].clone()
    }
}

// ---------------------------------------------------------------------------
// Workflow helpers — reduce boilerplate inside individual test functions
// ---------------------------------------------------------------------------

/// Create a `tracker-improvement` ticket with the given title.
/// Returns the UUID string of the created ticket.
pub fn create_ticket(s: &Sandbox, title: &str) -> String {
    let r = s.ticket_json(&[
        "create",
        "--title",
        title,
        "--type",
        "tracker-improvement",
    ]);
    assert_eq!(r["status"], "ok", "create should succeed for title '{title}'");
    r["id"].as_str().expect("id must be a string").to_string()
}


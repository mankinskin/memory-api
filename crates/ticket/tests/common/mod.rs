//! Shared sandboxed test harness for ticket-cli integration tests.
//!
//! The generic sandbox infrastructure lives in `memory_kernel::testing` and is
//! shared across all domains.  This module supplies two domain-specific
//! [`memory_kernel::testing::SandboxSetup`] implementations:
//!
//! - [`FlatTicketSetup`] — `index_root` == `workspace_root` (the tempdir
//!   itself acts as the index root).  Used by every test except those that
//!   exercise the `store-index` command.
//! - [`WorkspaceTicketSetup`] — `index_root` == `workspace_root/.ticket/`.
//!   Used by `store-index` tests that need the conventional project-root /
//!   `.ticket/` layout.
//!
//! Domain-specific CLI helpers (`ticket_json`, `ticket_fail`, …) are added
//! via the [`TicketCommands`] extension trait, which is implemented for any
//! [`memory_kernel::testing::Sandbox<S>`] regardless of setup type.
//!
//! The `ticket` binary is located via `env!("CARGO_BIN_EXE_ticket")`, which
//! Cargo resolves at compile time to the correct path in `target/`.

#![allow(dead_code)]

use std::{
    io::Write,
    path::Path,
    process::{
        Command,
        Stdio,
    },
};

use memory_kernel::testing::{
    Sandbox,
    SandboxPaths,
    SandboxSetup,
};

// ---------------------------------------------------------------------------
// Binary path — resolved at compile time by Cargo.
// ---------------------------------------------------------------------------

const TICKET: &str = env!("CARGO_BIN_EXE_ticket");

// ---------------------------------------------------------------------------
// Domain-specific SandboxSetup implementations
// ---------------------------------------------------------------------------

/// Flat layout: `index_root == workspace_root`.
///
/// The tempdir itself is the ticket index root.  This is the default layout
/// for all tests that do not exercise workspace-root–relative features.
pub struct FlatTicketSetup;

impl SandboxSetup for FlatTicketSetup {
    fn setup(workspace_root: &Path) -> SandboxPaths {
        let index_root = workspace_root.to_path_buf();
        let out = Command::new(TICKET)
            .arg("--index-root")
            .arg(&index_root)
            .arg("--json")
            .arg("init")
            .output()
            .unwrap_or_else(|e| panic!("failed to run ticket init: {e}"));
        assert!(
            out.status.success(),
            "ticket init failed ({})\nstdout: {}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        SandboxPaths {
            index_root,
            workspace_root: workspace_root.to_path_buf(),
        }
    }
}

/// Workspace layout: `index_root == workspace_root/.ticket/`.
///
/// Used by `store-index` tests that verify the conventional project-root /
/// `.ticket/` directory structure.
pub struct WorkspaceTicketSetup;

impl SandboxSetup for WorkspaceTicketSetup {
    fn setup(workspace_root: &Path) -> SandboxPaths {
        let index_root = workspace_root.join(".ticket");
        let out = Command::new(TICKET)
            .arg("--index-root")
            .arg(&index_root)
            .arg("--json")
            .arg("init")
            .output()
            .unwrap_or_else(|e| panic!("failed to run ticket init: {e}"));
        assert!(
            out.status.success(),
            "ticket init failed ({})\nstdout: {}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        SandboxPaths {
            index_root,
            workspace_root: workspace_root.to_path_buf(),
        }
    }
}

// ---------------------------------------------------------------------------
// Type aliases used by tests
// ---------------------------------------------------------------------------

/// Default sandbox for ticket-cli tests (flat layout).
pub type TicketSandbox = Sandbox<FlatTicketSetup>;

/// Workspace-layout sandbox for tests that need a real project-root /
/// `.ticket/` directory structure (e.g. `store-index` tests).
pub type WorkspaceSandbox = Sandbox<WorkspaceTicketSetup>;

// ---------------------------------------------------------------------------
// Extension trait: ticket-specific CLI helpers
//
// Implemented for all `Sandbox<S>` so both `TicketSandbox` and
// `WorkspaceSandbox` share the same helper methods without duplication.
// ---------------------------------------------------------------------------

/// Ticket CLI helpers available on any `Sandbox<S>`.
pub trait TicketCommands {
    fn index_root(&self) -> &Path;

    fn base(&self) -> Command {
        let mut cmd = Command::new(TICKET);
        cmd.arg("--index-root").arg(self.index_root());
        cmd
    }

    /// Run `ticket --json <args>` and return the inner `payload` object.
    fn ticket_json(
        &self,
        args: &[&str],
    ) -> serde_json::Value {
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
        envelope["payload"].clone()
    }

    /// Run `ticket --json <args>` and expect a non-zero exit.
    ///
    /// Returns `(exit_code, stderr)`.
    fn ticket_fail(
        &self,
        args: &[&str],
    ) -> (i32, String) {
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

    /// Run `ticket --json <args>` with `stdin_payload` on stdin.
    ///
    /// Panics if the command exits non-zero. Returns envelope `payload`.
    fn ticket_json_stdin(
        &self,
        args: &[&str],
        stdin_payload: &str,
    ) -> serde_json::Value {
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

impl<S: SandboxSetup> TicketCommands for Sandbox<S> {
    fn index_root(&self) -> &Path {
        Sandbox::index_root(self)
    }
}

// ---------------------------------------------------------------------------
// Workflow helpers
// ---------------------------------------------------------------------------

/// Create a `tracker-improvement` ticket with the given title.
/// Returns the UUID string of the created ticket.
pub fn create_ticket<S: SandboxSetup>(
    s: &Sandbox<S>,
    title: &str,
) -> String
where
    Sandbox<S>: TicketCommands,
{
    let r = s.ticket_json(&[
        "create",
        "--title",
        title,
        "--type",
        "tracker-improvement",
    ]);
    assert_eq!(
        r["status"], "ok",
        "create should succeed for title '{title}'"
    );
    r["id"].as_str().expect("id must be a string").to_string()
}

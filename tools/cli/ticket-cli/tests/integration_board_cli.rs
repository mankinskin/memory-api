//! Integration tests for the `ticket board` subcommand family.
//!
//! Each test runs against a fully isolated `Sandbox` and exercises the real
//! `ticket` binary (via `CARGO_BIN_EXE_ticket`). No internal Rust APIs are
//! called directly; all assertions are made on the JSON output.

mod common;

use std::process::Command;

use chrono::{
    DateTime,
    Datelike,
    Timelike,
    Utc,
};

use common::{
    TicketCommands,
    TicketSandbox as Sandbox,
    create_ticket,
};

// ---------------------------------------------------------------------------
// Typed representation of a `board show` recommended_next entry.
//
// Asserting against this struct instead of ad-hoc JSON indexing or human
// string fragments gives compile-time field coverage: adding or removing a
// field in the JSON contract breaks the struct definition, not a runtime
// string comparison.
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, serde::Deserialize)]
struct NextTicketEntry {
    ticket_id: String,
    state: Option<String>,
    priority: String,
    effort: Option<String>,
    dependee_count: usize,
    dependency_count: usize,
}

const TICKET: &str = env!("CARGO_BIN_EXE_ticket");

fn format_expected_board_created_at(created_at: &str) -> String {
    let timestamp = DateTime::parse_from_rfc3339(created_at)
        .expect("board recommendation created_at should be RFC3339")
        .with_timezone(&Utc);
    let month = timestamp.format("%b");

    format!(
        "{month} {} {} {:02}:{:02} UTC",
        timestamp.day(),
        timestamp.year(),
        timestamp.hour(),
        timestamp.minute()
    )
}

// ---------------------------------------------------------------------------
// Full lifecycle: check-in → heartbeat → update-files → show → check-out → show
// ---------------------------------------------------------------------------


#[path = "integration_board_cli/tests_board_ops.rs"]
mod tests_board_ops;
#[path = "integration_board_cli/tests_next_display.rs"]
mod tests_next_display;
#[path = "integration_board_cli/tests_dependency_trees.rs"]
mod tests_dependency_trees;
#[path = "integration_board_cli/tests_prioritization_and_help.rs"]
mod tests_prioritization_and_help;

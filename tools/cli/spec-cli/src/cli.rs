use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use spec_api::error::SpecError;

#[path = "cli/args.rs"]
mod args;
#[path = "cli/commands/mod.rs"]
pub mod commands;
#[path = "cli/dispatch.rs"]
mod dispatch;

pub use args::*;

// ── CLI root ───────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(name = "spec", about = "Specification system CLI", version)]
pub struct SpecCli {
    /// Return machine-readable JSON output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Root directory for the redb index and Tantivy search index.
    #[arg(long, global = true)]
    pub index_root: Option<PathBuf>,

    #[command(subcommand)]
    pub command: SpecCommandCli,
}

#[derive(Debug, Subcommand)]
pub enum SpecCommandCli {
    /// Create a new spec.
    Create(CreateArgs),
    /// Get a spec by ID or slug.
    Get(GetArgs),
    /// Update a spec's fields or state.
    Update(UpdateArgs),
    /// Soft-delete a spec.
    Delete(IdArgs),
    /// List specs with optional filtering.
    List(ListArgs),
    /// Full-text search over specs.
    Search(SearchArgs),
    /// Run full scan/reindex over registered scan roots.
    Scan(ScanArgs),
    /// Register a scan root directory.
    #[command(name = "add-root")]
    AddRoot(AddRootArgs),
    /// Show hierarchy as a tree.
    Tree(TreeArgs),
    /// List or validate code references for a spec.
    Refs(RefsArgs),
    /// Manage spec sections.
    Section(SectionArgs),
    /// Run health checks on specs.
    Health(HealthArgs),
    /// Bootstrap specs from a Rust crate's public API.
    Bootstrap(BootstrapArgs),
}

// ── error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("spec error: {0}")]
    Spec(#[from] SpecError),
    #[error("storage error: {0}")]
    Storage(#[from] memory_api::error::StorageError),
    #[error("{0}")]
    BadRequest(String),
}

pub enum CliOutput {
    Json(Value),
    Text(String),
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run(cli: SpecCli) -> Result<CliOutput, CliRunError> {
    let payload = dispatch::dispatch(cli.command, cli.index_root.as_deref(), cli.json)?;
    if cli.json {
        Ok(CliOutput::Json(payload))
    } else {
        Ok(CliOutput::Text(render_human(&payload)))
    }
}

fn render_human(payload: &Value) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| format!("{:?}", payload))
}

pub fn error_output(message: &str, as_json: bool) -> String {
    if as_json {
        json!({"status": "error", "message": message}).to_string()
    } else {
        message.to_string()
    }
}

pub fn parse_cli_from<I, T>(args: I) -> Result<SpecCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    SpecCli::try_parse_from(args)
}

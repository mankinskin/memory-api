use std::path::PathBuf;

use clap::{
    Parser,
    Subcommand,
};
use serde_json::{
    Value,
    json,
};

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
#[command(
    name = "spec",
    about = "Specification system CLI",
    version,
    arg_required_else_help = true
)]
pub struct SpecCli {
    /// Return machine-readable JSON output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Root directory for the SQLite index and Tantivy search index.
    #[arg(long, global = true)]
    pub index_root: Option<PathBuf>,

    /// Workspace/repo root to normalize to the canonical `.spec` store.
    /// Useful for targeting a nested workspace from an ancestor checkout.
    #[arg(long, global = true)]
    pub workspace_root: Option<PathBuf>,

    #[command(subcommand)]
    pub command: SpecCommandCli,
}

#[derive(Debug, Subcommand)]
pub enum SpecCommandCli {
    /// Initialize a new spec workspace in the current directory (or at --index-root).
    ///
    /// Creates the `.spec/` store directory and all required index files.
    /// Idempotent: succeeds without error if the workspace already exists.
    Init,
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
    let payload =
        dispatch::dispatch(
            cli.command,
            cli.index_root.as_deref(),
            cli.workspace_root.as_deref(),
            cli.json,
        )?;
    if cli.json {
        Ok(CliOutput::Json(payload))
    } else {
        Ok(CliOutput::Text(render_human(&payload)))
    }
}

fn render_human(payload: &Value) -> String {
    serde_json::to_string_pretty(payload)
        .unwrap_or_else(|_| format!("{:?}", payload))
}

pub fn error_output(
    message: &str,
    as_json: bool,
) -> String {
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn parse_refs_validate_keeps_workspace_root_meanings_distinct() {
        let cli = parse_cli_from([
            "spec",
            "--workspace-root",
            "memory-viewers/memory-api",
            "refs",
            "0386c4d0",
            "validate",
            "--code-workspace-root",
            ".",
        ])
        .unwrap();

        assert_eq!(
            cli.workspace_root,
            Some(PathBuf::from("memory-viewers/memory-api"))
        );

        match cli.command {
            SpecCommandCli::Refs(RefsArgs {
                id,
                subcommand:
                    Some(RefsSubcommand::Validate {
                        code_workspace_root,
                    }),
            }) => {
                assert_eq!(id, "0386c4d0");
                assert_eq!(code_workspace_root, Some(PathBuf::from(".")));
            },
            other => panic!("expected refs validate command, got {other:?}"),
        }
    }

    #[test]
    fn parse_bootstrap_uses_source_workspace_root_name() {
        let cli = parse_cli_from([
            "spec",
            "bootstrap",
            "crates/spec-api",
            "--source-workspace-root",
            "memory-viewers/memory-api",
        ])
        .unwrap();

        match cli.command {
            SpecCommandCli::Bootstrap(args) => {
                assert_eq!(args.crate_path, PathBuf::from("crates/spec-api"));
                assert_eq!(
                    args.source_workspace_root,
                    Some(PathBuf::from("memory-viewers/memory-api"))
                );
            },
            other => panic!("expected bootstrap command, got {other:?}"),
        }
    }
}

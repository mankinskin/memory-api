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
    #[arg(long, global = true, conflicts_with = "toon")]
    pub json: bool,

    /// Return machine-readable TOON output.
    #[arg(long, global = true, conflicts_with = "json")]
    pub toon: bool,

    /// Root directory for the SQLite index and Tantivy search index.
    #[arg(long, global = true)]
    pub index_root: Option<PathBuf>,

    /// Workspace/repo root to normalize to the canonical `.spec` store.
    /// Useful for targeting a nested workspace from an ancestor checkout.
    #[arg(long = "workspace", alias = "workspace-root", global = true)]
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
    /// Regenerate declared spec artifacts from rule targets.
    #[command(name = "sync-generated")]
    SyncGenerated(SyncGeneratedArgs),
    /// Manage spec sections.
    Section(SectionArgs),
    /// Run health checks on specs.
    Health(HealthArgs),
    /// Generate the committed spec catalog (.spec README + index.toon + .agents hook).
    #[command(name = "store-index")]
    StoreIndex(StoreIndexArgs),
    /// Bootstrap specs from a Rust crate's public API.
    Bootstrap(BootstrapArgs),
    /// Move a spec to another workspace store (dry-run/resume/rollback).
    Move(MoveArgs),
}

// ── error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("spec error: {0}")]
    Spec(#[from] SpecError),
    #[error("storage error: {0}")]
    Storage(#[from] memory_api::error::StorageError),
    #[error("rule error: {0}")]
    Rule(#[from] rule_api::error::RuleError),
    #[error("target config error: {0}")]
    TargetConfig(#[from] rule_api::TargetConfigError),
    #[error("{0}")]
    BadRequest(String),
}

pub enum CliOutput {
    Machine(Value, MachineOutputFormat),
    Text(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineOutputFormat {
    Json,
    Toon,
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run(cli: SpecCli) -> Result<CliOutput, CliRunError> {
    let payload =
        dispatch::dispatch(
            cli.command,
            cli.index_root.as_deref(),
            cli.workspace_root.as_deref(),
            cli.json || cli.toon,
        )?;
    if let Some(format) = machine_output_format(cli.json, cli.toon) {
        Ok(CliOutput::Machine(payload, format))
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
    format: Option<MachineOutputFormat>,
) -> String {
    let payload = json!({"status": "error", "message": message});
    match format {
        Some(MachineOutputFormat::Json) => payload.to_string(),
        Some(MachineOutputFormat::Toon) =>
            toon_format::encode_default(&payload)
                .unwrap_or_else(|_| format!("status: error\nmessage: {message}")),
        None => message.to_string(),
    }
}

pub fn render_machine_output(
    payload: &Value,
    format: MachineOutputFormat,
) -> Result<String, String> {
    match format {
        MachineOutputFormat::Json =>
            serde_json::to_string_pretty(payload).map_err(|err| err.to_string()),
        MachineOutputFormat::Toon =>
            toon_format::encode_default(payload).map_err(|err| err.to_string()),
    }
}

pub fn machine_output_format(
    as_json: bool,
    as_toon: bool,
) -> Option<MachineOutputFormat> {
    if as_json {
        Some(MachineOutputFormat::Json)
    } else if as_toon {
        Some(MachineOutputFormat::Toon)
    } else {
        None
    }
}

pub fn requested_machine_output_format_from_args() -> Option<MachineOutputFormat> {
    machine_output_format(
        std::env::args().any(|arg| arg == "--json"),
        std::env::args().any(|arg| arg == "--toon"),
    )
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
    fn parse_list_accepts_toon_flag() {
        let cli = parse_cli_from([
            "spec",
            "--toon",
            "list",
        ])
        .unwrap();

        match cli.command {
            SpecCommandCli::List(ListArgs { .. }) => {},
            other => panic!("expected list command, got {other:?}"),
        }
    }

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

    #[test]
    fn parse_sync_generated_keeps_target_spec_id() {
        let cli = parse_cli_from([
            "spec",
            "sync-generated",
            "0386c4d0",
        ])
        .unwrap();

        match cli.command {
            SpecCommandCli::SyncGenerated(args) => {
                assert_eq!(args.id, "0386c4d0");
            },
            other => panic!("expected sync-generated command, got {other:?}"),
        }
    }
}

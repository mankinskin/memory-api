use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use uuid::Uuid;

use ticket_api::contracts::command_schema::{CommandEnvelope, ErrorEnvelope};
use ticket_api::error::StorageError;

#[path = "cli/args.rs"]
mod args;
#[path = "cli/commands/mod.rs"]
mod commands;
#[path = "cli/dispatch.rs"]
mod dispatch;
#[path = "cli/batch.rs"]
mod batch;
#[path = "cli/helpers.rs"]
mod helpers;
#[path = "cli/human_output.rs"]
mod human_output;
#[path = "cli/workspace_commands.rs"]
mod workspace_commands;

pub use args::*;
pub(crate) use helpers::*;

// ── CLI root ───────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(name = "ticket", about = "Task tracker CLI", version)]
pub struct TicketCli {
    /// Return machine-readable JSON envelope output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Optional request identifier propagated in JSON envelope output.
    #[arg(long, global = true)]
    pub request_id: Option<String>,

    /// Root directory for the redb index and Tantivy search index.
    /// Defaults to $TICKET_INDEX_ROOT env var, then ~/.ticket-index/.
    #[arg(long, global = true)]
    pub index_root: Option<PathBuf>,

    /// Directory containing additional ticket type schema TOML files.
    /// Each `<type-id>.toml` file overrides or supplements the built-in schemas.
    #[arg(long, global = true)]
    pub schema_dir: Option<PathBuf>,

    /// Preview mutating commands without writing to storage.
    #[arg(long, global = true, default_value_t = false)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: TicketCommandCli,
}

#[derive(Debug, Subcommand)]
pub enum TicketCommandCli {
    /// Create a new ticket.
    Create(CreateArgs),
    /// Get a ticket by UUID.
    Get(IdArgs),
    /// Get the markdown description body of a ticket.
    Describe(IdArgs),
    /// Update a ticket with field patches and optional state transition.
    Update(UpdateArgs),
    /// Record a bug reproduction event with commit and timestamp metadata.
    Repro(ReproArgs),
    /// List tickets with optional state/type filtering.
    List(ListArgs),
    /// Soft-delete a ticket.
    Delete(IdArgs),
    /// Run full scan/reindex over registered scan roots.
    Scan(ScanArgs),
    /// Claim a ticket lease for active work.
    Claim(ClaimArgs),
    /// Release an active ticket lease.
    Unclaim(UnclaimArgs),
    /// List all active leases.
    Leases,
    /// Full-text + metadata search over tickets.
    Search(TextArgs),
    /// Unified query expression (alias for search).
    Query(TextArgs),
    /// Register a scan root directory.
    #[command(name = "add-root")]
    AddRoot(AddRootArgs),
    /// History log for a ticket (Phase 2 — stub).
    History(HistoryArgs),
    /// Diff a ticket between revisions (Phase 2 — stub).
    Diff(DiffArgs),
    /// Revert a ticket to a historical revision (Phase 2 — stub).
    Revert(RevertArgs),
    /// Mark merge-boundary completion metadata (Phase 2 — stub).
    #[command(name = "finalize-merge")]
    FinalizeMerge(FinalizeMergeArgs),
    /// Execute a batch of CLI commands (one per line) from stdin or file, with transactional rollback.
    Batch(BatchArgs),
    /// Export the command namespace/schema for automation clients.
    #[command(name = "export-command-schema")]
    ExportCommandSchema,
    /// Add a directed edge (dependency/link) between two tickets.
    Link(LinkArgs),
    /// Remove a directed edge between two tickets.
    Unlink(UnlinkArgs),
    /// List edges originating from a ticket, or all edges with --all.
    Links(LinksArgs),
    /// Show the dependency subgraph rooted at a ticket.
    Subgraph(SubgraphArgs),
    /// Show all tickets that depend on a given ticket (reverse dependency tree).
    Topgraph(TopgraphArgs),
    /// Manage named workspaces (named index roots).
    Workspace(WorkspaceArgs),
    /// Watch filesystem scan roots and auto-reconcile on changes.
    Watch(WatchArgs),
    /// Dashboard: current state summary + ready tickets + parallel opportunities.
    Status(StatusArgs),
    /// Return a JSON overview of ready tickets.
    #[command(name = "ready-overview")]
    ReadyOverview(ReadyOverviewArgs),
    /// Start the HTTP server exposing the ticket API (REST + SSE).
    Serve(ServeCliArgs),
    /// Fast-forward a ticket to a target state (default: done).
    Close(CloseArgs),
    /// Cancel a ticket (shortcut for close --to-state cancelled).
    Cancel(IdArgs),
    /// Attach a file as an asset to a ticket.
    Attach(AttachArgs),
    /// List assets attached to a ticket.
    Assets(IdArgs),
    /// Run health checks on a ticket. Use --depth to walk the subgraph.
    Health(HealthArgs),
    /// Audit the ticket store: report health, counts, and orphan checks.
    Audit,
}

// ── error type ─────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("invalid field patch: {0}")]
    InvalidFieldPatch(String),
    #[error("failed to serialize command schema: {0}")]
    CommandSchema(#[from] serde_json::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("invalid exec command payload: {0}")]
    InvalidExecPayload(String),
    #[error("{0}")]
    BadRequest(String),
}

pub enum CliOutput {
    Json(Value),
    Text(String),
}

// ── entry point ────────────────────────────────────────────────────────────────

pub fn run(cli: TicketCli) -> Result<CliOutput, CliRunError> {
    let payload = dispatch::dispatch(
        cli.command,
        cli.index_root.as_deref(),
        cli.schema_dir.as_deref(),
        cli.json,
        cli.dry_run,
    )?;
    if cli.json {
        let request_id = cli.request_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let envelope = CommandEnvelope { request_id, payload };
        Ok(CliOutput::Json(json!(envelope)))
    } else {
        Ok(CliOutput::Text(render_human(payload)))
    }
}

// ── output helpers ─────────────────────────────────────────────────────────────

fn render_human(payload: Value) -> String {
    human_output::render_human_readable(&payload)
}

pub fn error_output(message: &str, as_json: bool) -> String {
    if as_json {
        serde_json::to_string_pretty(&ErrorEnvelope {
            code: "invalid_request".to_string(),
            message: message.to_string(),
        })
        .unwrap_or_else(|_| format!("{{\"code\":\"invalid_request\",\"message\":\"{}\"}}", message))
    } else {
        message.to_string()
    }
}

pub fn parse_cli_from<I, T>(args: I) -> Result<TicketCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    TicketCli::try_parse_from(args)
}

pub fn payload_as_json_object(payload: &Value) -> Option<&serde_json::Map<String, Value>> {
    payload.as_object()
}



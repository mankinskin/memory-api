use std::path::PathBuf;

use clap::{
    Parser,
    Subcommand,
};
use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use ticket_api::{
    contracts::command_schema::{
        CommandEnvelope,
        ErrorEnvelope,
    },
    error::StorageError,
    storage::board::BoardError,
};

#[path = "cli/args.rs"]
mod args;
#[path = "cli/batch.rs"]
mod batch;
#[path = "cli/commands/mod.rs"]
mod commands;
#[path = "cli/dispatch.rs"]
mod dispatch;
#[path = "cli/helpers.rs"]
mod helpers;
#[path = "cli/human_output.rs"]
mod human_output;

pub use args::*;
pub(crate) use helpers::*;

// ── CLI root ───────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "ticket",
    about = "Task tracker CLI",
    version,
    arg_required_else_help = true
)]
pub struct TicketCli {
    /// Return machine-readable JSON envelope output.
    #[arg(long, global = true, conflicts_with = "toon")]
    pub json: bool,

    /// Return machine-readable TOON envelope output.
    #[arg(long, global = true, conflicts_with = "json")]
    pub toon: bool,

    /// Optional request identifier propagated in JSON envelope output.
    #[arg(long, global = true)]
    pub request_id: Option<String>,

    /// Root directory for the SQLite index and Tantivy search index.
    /// Defaults to $TICKET_INDEX_ROOT, then the nearest local .ticket/.
    #[arg(long, global = true)]
    pub index_root: Option<PathBuf>,

    /// Workspace/repo root to normalize to the canonical `.ticket` store.
    /// Useful for targeting a nested workspace from an ancestor checkout.
    #[arg(long = "workspace", alias = "workspace-root", global = true)]
    pub workspace_root: Option<PathBuf>,

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
    /// Initialize a new ticket workspace in the current directory (or at --index-root).
    ///
    /// Creates the `.ticket/` store directory and all required index files.
    /// Idempotent: succeeds without error if the workspace already exists.
    Init,
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
    /// Permanently delete a ticket.
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
    /// List the canonical ticket/spec/rule workflows, required params, and
    /// nested-root targeting semantics (self-describing capability catalog).
    Catalog,
    /// Add a directed edge (dependency/link) between two tickets.
    Link(LinkArgs),
    /// Remove a directed edge between two tickets.
    Unlink(UnlinkArgs),
    /// List edges originating from a ticket, or all edges with --all.
    Links(LinksArgs),
    /// Remove or report dangling edges (missing target tickets) for one ticket or globally.
    #[command(name = "prune-dangling")]
    PruneDangling(PruneDanglingArgs),
    /// Show the dependency subgraph rooted at a ticket.
    Subgraph(SubgraphArgs),
    /// Show all tickets that depend on a given ticket (reverse dependency tree).
    Topgraph(TopgraphArgs),
    /// Watch filesystem scan roots and auto-reconcile on changes.
    Watch(WatchArgs),
    /// Dashboard: current state summary + ready tickets + parallel opportunities.
    Status(StatusArgs),
    /// Return a JSON overview of ready tickets.
    #[command(name = "ready-overview")]
    ReadyOverview(ReadyOverviewArgs),
    /// List unblocked, dependency-satisfied tickets ordered by workflow progress, priority, and dependee count for worker agents.
    Next(NextArgs),
    /// Show the unresolved upstream blocker tree for a ticket, emphasizing frontier leaves.
    Blockers(BlockersArgs),
    /// Show which reverse dependents a ticket would unlock immediately versus still leave blocked if treated as satisfied.
    UnblockedBy(UnblockedByArgs),
    /// Start the HTTP server exposing the ticket API (REST + SSE).
    Serve(ServeCliArgs),
    /// Fast-forward a ticket to a target state (default: done).
    Close(CloseArgs),
    /// Cancel a ticket (shortcut for close --to-state cancelled).
    Cancel(CancelArgs),
    /// Plan, execute, resume, or roll back a cross-workspace ticket move.
    Move(MoveArgs),
    /// Attach a file as an asset to a ticket.
    Attach(AttachArgs),
    /// List assets attached to a ticket.
    Assets(IdArgs),
    /// Show the legal state-transition graph for a ticket: current state,
    /// allowed next states, required intermediate/terminal states.
    Transitions(IdArgs),
    /// Run health checks on a ticket. Use --depth to walk the subgraph.
    Health(HealthArgs),
    /// Generate or check the committed ticket catalog (.ticket README + index.toon + .agents hook).
    #[command(name = "store-index")]
    StoreIndex(StoreIndexArgs),
    /// Audit the ticket store: report health, counts, and orphan checks.
    Audit,
    /// Reformat all ticket.toml files to canonical field ordering.
    ///
    /// Writes fields in the order: id, created_at, title, state,
    /// acceptance_criteria, then remaining fields alphabetically.
    /// Use --check to report without writing (CI gate).
    Fmt(FmtArgs),
    /// Manage the work-in-progress board (check-in, check-out, heartbeat, show, clean).
    Board(BoardArgs),
    /// Inspect or edit the workspace policy and rescan with policy applied.
    Workspace(WorkspaceArgs),
    /// Validate related_specs links: detect dangling spec refs, wrong-store
    /// refs, and bidirectional inconsistencies against the referenced spec
    /// store(s).
    #[command(name = "validate-links")]
    ValidateLinks,
    /// List a ticket's content parts (objective, requirements, review, ...),
    /// including frozen state and any orphaned part files.
    #[command(name = "list-parts")]
    ListParts(ListPartsArgs),
    /// Get a single ticket content part by its opaque part id.
    #[command(name = "get-part")]
    GetPart(GetPartArgs),
    /// Write a ticket content part: update an existing part via --part-id,
    /// or create a new part of --kind. Rejected if the addressed part is
    /// frozen by plan freezing.
    #[command(name = "write-part")]
    WritePart(WritePartArgs),
    /// Write an `amendment` part that supersedes a (typically frozen) part,
    /// recording a correction without unfreezing the original.
    #[command(name = "write-amendment")]
    WriteAmendment(WriteAmendmentArgs),
    /// Restore a part to the content it held immediately before its most
    /// recent write. Rejected if the part is currently frozen.
    #[command(name = "undo-part")]
    UndoPart(UndoPartArgs),
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
    #[error("board error: {0}")]
    Board(#[from] BoardError),
    #[error("invalid exec command payload: {0}")]
    InvalidExecPayload(String),
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

// ── entry point ────────────────────────────────────────────────────────────────

pub fn run(cli: TicketCli) -> Result<CliOutput, CliRunError> {
    let request_id = cli
        .request_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let _span_guard = tracing::debug_span!(
        target: "ticket_cli::transport",
        "ticket_cli_run",
        request_id = %request_id,
        command = command_name(&cli.command),
        machine_output = machine_output_format(cli.json, cli.toon).is_some(),
        dry_run = cli.dry_run,
    )
    .entered();

    let mut payload = dispatch::dispatch(
        cli.command,
        cli.index_root.as_deref(),
        cli.workspace_root.as_deref(),
        cli.schema_dir.as_deref(),
        cli.json,
        cli.dry_run,
    )?;
    ticket_api::output::strip_default_metadata(&mut payload);
    if let Some(format) = machine_output_format(cli.json, cli.toon) {
        let envelope = CommandEnvelope {
            request_id,
            payload,
        };
        Ok(CliOutput::Machine(json!(envelope), format))
    } else {
        Ok(CliOutput::Text(render_human(payload)))
    }
}

fn command_name(command: &TicketCommandCli) -> String {
    let variant_debug = format!("{command:?}");
    let variant_name = variant_debug
        .split_once('(')
        .map(|(name, _)| name)
        .unwrap_or(variant_debug.as_str());
    camel_to_kebab(variant_name)
}

fn camel_to_kebab(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 4);
    for (idx, ch) in input.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx != 0 {
                out.push('-');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

// ── output helpers ─────────────────────────────────────────────────────────────

fn render_human(payload: Value) -> String {
    human_output::render_human_readable(&payload)
}

pub fn error_output(
    message: &str,
    format: Option<MachineOutputFormat>,
) -> String {
    let error = ErrorEnvelope {
        code: "invalid_request".to_string(),
        message: message.to_string(),
    };
    match format {
        Some(MachineOutputFormat::Json) => serde_json::to_string_pretty(&error)
            .unwrap_or_else(|_| {
                format!(
                    "{{\"code\":\"invalid_request\",\"message\":\"{}\"}}",
                    message
                )
            }),
        Some(MachineOutputFormat::Toon) =>
            toon_format::encode_default(&json!(error)).unwrap_or_else(|_| {
                format!("code: invalid_request\nmessage: {message}")
            }),
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

pub fn requested_machine_output_format_from_args() -> Option<MachineOutputFormat>
{
    machine_output_format(
        std::env::args().any(|arg| arg == "--json"),
        std::env::args().any(|arg| arg == "--toon"),
    )
}

pub fn parse_cli_from<I, T>(args: I) -> Result<TicketCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    TicketCli::try_parse_from(args)
}

pub fn payload_as_json_object(
    payload: &Value
) -> Option<&serde_json::Map<String, Value>> {
    payload.as_object()
}

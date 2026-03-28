use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use uuid::Uuid;

// ── arg structs ────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct CreateArgs {
    #[arg(long)]
    pub id: Option<Uuid>,
    #[arg(long = "type")]
    pub ticket_type: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub state: Option<String>,
    #[arg(long = "field")]
    pub fields: Vec<String>,
    /// Copy the contents of this file into the ticket as description.md.
    #[arg(long = "body-file")]
    pub body_file: Option<PathBuf>,
    /// Place the ticket in this scan root (defaults to first registered root).
    #[arg(long = "root")]
    pub target_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct IdArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
}

#[derive(Debug, Args)]
pub struct LinksArgs {
    /// Ticket UUID or 8+ character hex prefix (omit with --all to list all edges globally).
    #[arg(required_unless_present = "all")]
    pub id: Option<String>,
    /// List all edges in the store instead of filtering by source ticket.
    #[arg(long, default_value_t = false)]
    pub all: bool,
    /// Filter edges by kind (e.g. depends_on). Omit to show all kinds.
    #[arg(long)]
    pub kind: Option<String>,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Optional prefix filter — only include tickets whose title starts with this string.
    /// E.g. "[bootstrap]" to scope the view to the bootstrap track.
    #[arg(long)]
    pub filter: Option<String>,
    /// Include blocked tickets in the output (default: omitted for brevity).
    #[arg(long, default_value_t = false)]
    pub show_blocked: bool,
}

#[derive(Debug, Args)]
pub struct ReadyOverviewArgs {
    /// Optional prefix filter — only include tickets whose title starts with this string.
    #[arg(long)]
    pub filter: Option<String>,
    /// Optional scope label included in the JSON response.
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(Debug, Args)]
pub struct NextArgs {
    /// Maximum number of tickets to return.
    #[arg(long, default_value = "20")]
    pub limit: usize,
    /// Optional prefix filter — only include tickets whose title starts with this string.
    #[arg(long)]
    pub filter: Option<String>,
}

#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Debounce time in milliseconds before triggering reconcile after an event.
    #[arg(long, default_value = "200")]
    pub debounce_ms: u64,
}

#[derive(Debug, Args)]
pub struct ServeCliArgs {
    /// TCP port to bind to.
    #[arg(long, default_value = "8080")]
    pub port: u16,
    /// Host address to bind to.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    /// Serve a specific named workspace only (default: all registered).
    #[arg(long)]
    pub workspace: Option<String>,
}

#[derive(Debug, Args)]
pub struct CloseArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Target state to fast-forward to (default: done).
    #[arg(long = "to-state", default_value = "done")]
    pub to_state: String,
}

#[derive(Debug, Args)]
pub struct AttachArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Path to the file to attach.
    pub path: PathBuf,
    /// Optional name for the asset (defaults to source filename).
    #[arg(long = "as")]
    pub asset_name: Option<String>,
}

#[derive(Debug, Args)]
pub struct LinkArgs {
    /// UUID or 8+ character hex prefix of the source ticket.
    #[arg(long)]
    pub from: String,
    /// UUID or 8+ character hex prefix of the target ticket.
    #[arg(long)]
    pub to: String,
    /// Edge kind (e.g. depends_on, linked).
    #[arg(long)]
    pub kind: String,
    /// Human-readable reason for this edge (optional, stored in response only).
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct UnlinkArgs {
    /// UUID or 8+ character hex prefix of the source ticket.
    #[arg(long)]
    pub from: String,
    /// UUID or 8+ character hex prefix of the target ticket.
    #[arg(long)]
    pub to: String,
    /// Edge kind (e.g. depends_on, linked).
    #[arg(long)]
    pub kind: String,
    /// Human-readable reason for this removal (optional, stored in response only).
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct SubgraphArgs {
    /// Root ticket UUID or 8+ character hex prefix.
    pub root: String,
    /// Maximum traversal depth (default: 4, max: 8).
    #[arg(long, default_value = "4")]
    pub depth: usize,
    /// Edge direction to follow: out, in, or both.
    #[arg(long, default_value = "out")]
    pub direction: String,
    /// Filter edges by kind (default: all).
    #[arg(long = "edge-kind", default_value = "all")]
    pub edge_kind: String,
}

#[derive(Debug, Args)]
pub struct TopgraphArgs {
    /// Root ticket UUID or 8+ character hex prefix.
    pub root: String,
    /// Maximum traversal depth (default: 4, max: 8).
    #[arg(long, default_value = "4")]
    pub depth: usize,
    /// Edge direction to follow: out, in, or both.
    #[arg(long, default_value = "in")]
    pub direction: String,
    /// Filter edges by kind (default: all).
    #[arg(long = "edge-kind", default_value = "all")]
    pub edge_kind: String,
}

#[derive(Debug, Args)]
pub struct HealthArgs {
    /// Root ticket UUID or 8+ character hex prefix. Checks the subgraph rooted here.
    #[arg(required_unless_present_any = ["all", "stdin"])]
    pub root: Option<String>,
    /// Check all tickets instead of a subgraph.
    #[arg(long, default_value_t = false)]
    pub all: bool,
    /// Read newline-delimited ticket UUIDs from stdin instead of traversing a subgraph.
    #[arg(long, default_value_t = false)]
    pub stdin: bool,
    /// Maximum traversal depth when walking the subgraph (default: 0 = single ticket; max: 8).
    #[arg(long, default_value = "0")]
    pub depth: usize,
    /// Edge direction to follow for subgraph: out, in, or both.
    #[arg(long, default_value = "out")]
    pub direction: String,
    /// Filter by field values (key=value). Can be repeated.
    #[arg(long = "where")]
    pub where_clauses: Vec<String>,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long = "from-state")]
    pub from_state: Option<String>,
    #[arg(long = "to-state")]
    pub to_state: Option<String>,
    #[arg(long = "field")]
    pub fields: Vec<String>,
    /// Revert to the previous history revision (undo the last change).
    #[arg(long)]
    pub undo: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReproOutcome {
    Reproduced,
    NotReproduced,
    Intermittent,
    Fixed,
}

impl ReproOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reproduced => "reproduced",
            Self::NotReproduced => "not_reproduced",
            Self::Intermittent => "intermittent",
            Self::Fixed => "fixed",
        }
    }
}

#[derive(Debug, Args)]
pub struct ReproArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Reproduction outcome.
    #[arg(long, value_enum, default_value_t = ReproOutcome::Reproduced)]
    pub outcome: ReproOutcome,
    /// Commit SHA where reproduction was attempted (defaults to git HEAD if available).
    #[arg(long)]
    pub commit: Option<String>,
    /// Optional reproduction command used.
    #[arg(long)]
    pub command: Option<String>,
    /// Optional short note.
    #[arg(long)]
    pub note: Option<String>,
    /// Optional RFC3339 timestamp (defaults to now/UTC).
    #[arg(long)]
    pub timestamp: Option<String>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long)]
    pub state: Option<String>,
    #[arg(long = "type")]
    pub ticket_type: Option<String>,
    #[arg(long)]
    pub limit: Option<usize>,
    /// Include latest reproduction metadata in each list item.
    #[arg(long, default_value_t = false)]
    pub with_repro: bool,
    /// Include soft-deleted tickets in the listing.
    #[arg(long, default_value_t = false)]
    pub include_deleted: bool,
    /// Filter by field values (key=value). Can be repeated.
    #[arg(long = "where")]
    pub where_clauses: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ScanArgs {
    #[arg(long = "reindex")]
    pub reindex: bool,
}

#[derive(Debug, Args)]
pub struct ClaimArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long = "agent")]
    pub agent_id: String,
    #[arg(long = "ttl-secs", default_value_t = 300)]
    pub ttl_secs: u64,
    #[arg(long = "intent")]
    pub work_intent: Option<String>,
}

#[derive(Debug, Args)]
pub struct UnclaimArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct TextArgs {
    pub expression: String,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct AddRootArgs {
    pub path: PathBuf,
    #[arg(long, default_value = "default")]
    pub label: String,
}

#[derive(Debug, Args)]
pub struct HistoryArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
}

#[derive(Debug, Args)]
pub struct RevertArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long = "to")]
    pub to_sha: String,
}

#[derive(Debug, Args)]
pub struct FinalizeMergeArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long = "merge-commit")]
    pub merge_commit: String,
}

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceSubCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceSubCommand {
    /// List all registered workspaces.
    List,
    /// Register a new named workspace.
    New(WorkspaceNewArgs),
    /// Set the active workspace by name.
    Use(WorkspaceUseArgs),
    /// Show the currently active workspace and how it was resolved.
    Current,
    /// Unregister a workspace (data on disk is not removed).
    Remove(WorkspaceRemoveArgs),
}

#[derive(Debug, Args)]
pub struct WorkspaceNewArgs {
    /// Name for the new workspace.
    pub name: String,
    /// Index root path (defaults to ~/.ticket-<name>/).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct WorkspaceUseArgs {
    /// Name of the workspace to activate.
    pub name: String,
    /// Write a .ticket-workspace file in the current directory instead of
    /// updating the global active pointer.
    #[arg(long)]
    pub local: bool,
}

#[derive(Debug, Args)]
pub struct WorkspaceRemoveArgs {
    /// Name of the workspace to unregister.
    pub name: String,
}

#[derive(Debug, Args)]
pub struct BatchArgs {
    /// File containing CLI commands, one per line. If omitted, read from stdin.
    /// Blank lines and lines starting with '#' are ignored.
    /// Example line: create --title "Fix bug" --type tracker-improvement
    #[arg(long)]
    pub file: Option<PathBuf>,
}

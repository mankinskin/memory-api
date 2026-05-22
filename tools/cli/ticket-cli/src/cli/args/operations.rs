use std::path::PathBuf;

use clap::{
    Args,
    ValueEnum,
};

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
    /// Optional ticket UUID or 8+ character hex prefix.
    /// When set, scope results to actionable remaining blockers for reachable reverse dependents.
    pub root: Option<String>,
    /// Maximum number of tickets to return.
    #[arg(long, default_value = "20")]
    pub limit: usize,
    /// Optional prefix filter — only include tickets whose title starts with this string.
    #[arg(long)]
    pub filter: Option<String>,
    /// Skip board-awareness: include tickets already tracked on the board in results.
    #[arg(long, default_value_t = false)]
    pub no_board: bool,
}

#[derive(Debug, Args)]
pub struct UnblockedByArgs {
    /// Ticket UUID or 8+ character hex prefix to treat as satisfied.
    pub id: String,
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
    /// Author/user identity to record in history revisions (overrides TICKET_AUTHOR env var).
    #[arg(long)]
    pub author: Option<String>,
}

#[derive(Debug, Args)]
pub struct CancelArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Author/user identity to record in history revisions (overrides TICKET_AUTHOR env var).
    #[arg(long)]
    pub author: Option<String>,
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
pub struct FmtArgs {
    /// Report files needing reordering without writing any changes.
    ///
    /// When set, the command exits with `status = "needs_formatting"` and a
    /// positive `reformatted` count if any ticket.toml is out of canonical
    /// field order. Useful for CI gating.
    #[arg(long, default_value_t = false)]
    pub check: bool,
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
    /// Markdown description to write/overwrite as description.md.
    #[arg(long)]
    pub description: Option<String>,
    /// Author/user identity to record in the history revision (overrides TICKET_AUTHOR env var).
    #[arg(long)]
    pub author: Option<String>,
    /// After a successful update, also check the agent in to the board.
    #[arg(long, default_value_t = false)]
    pub board_check_in: bool,
    /// Agent identity to use for --board-check-in.
    #[arg(long)]
    pub board_agent: Option<String>,
    /// Work intent description to use for --board-check-in.
    #[arg(long)]
    pub board_intent: Option<String>,
    /// Files to claim ownership of during --board-check-in.
    #[arg(long = "board-file")]
    pub board_files: Vec<String>,
    /// Heartbeat TTL in seconds for --board-check-in (default: 3600).
    #[arg(long)]
    pub board_ttl_secs: Option<u64>,
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

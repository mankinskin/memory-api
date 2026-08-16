use std::path::PathBuf;

use clap::{
    Args,
    ValueEnum,
};
use uuid::Uuid;

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
    /// When set, scope results to actionable leaf blockers beneath this ticket.
    pub root: Option<String>,
    /// Maximum number of tickets to return.
    #[arg(long)]
    pub limit: Option<usize>,
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
pub struct BlockersArgs {
    /// Ticket UUID or 8+ character hex prefix to inspect for unresolved prerequisites.
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
pub struct MoveArgs {
    /// Ticket UUID or 8+ character hex prefix (required for plan/execute mode).
    pub id: Option<String>,
    /// Destination workspace root (normalized to its canonical `.ticket` store).
    #[arg(long = "to-workspace-root")]
    pub to_workspace_root: Option<PathBuf>,
    /// Resume an interrupted move by journal UUID.
    #[arg(long)]
    pub resume: Option<String>,
    /// Roll back a move by journal UUID.
    #[arg(long)]
    pub rollback: Option<String>,
    /// Preview planning output without mutating storage.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
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
    #[arg(required_unless_present_any = ["all", "stdin", "ids"])]
    pub root: Option<String>,
    /// Check all tickets instead of a subgraph.
    #[arg(long, default_value_t = false)]
    pub all: bool,
    /// Read newline-delimited ticket UUIDs from stdin instead of traversing a subgraph.
    #[arg(long, default_value_t = false)]
    pub stdin: bool,
    /// Explicit ticket IDs (UUID or 8+ prefix). Can be repeated.
    #[arg(long = "id")]
    pub ids: Vec<String>,
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
pub struct StoreIndexArgs {
    /// Check-only mode: render the ticket catalog and exit non-zero on drift.
    #[arg(long, default_value_t = false)]
    pub check: bool,
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
pub struct UpdateArgsCli {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    #[arg(long = "transition-state")]
    pub transition_states: Vec<String>,
    #[arg(long = "to-state")]
    pub to_state: Option<String>,
    /// Opt out of auto-walking multi-hop transitions. When set, a `--to-state`
    /// that would skip a required waypoint is rejected with recovery guidance
    /// instead of traversing the intermediate states.
    #[arg(
        long = "single-hop",
        visible_alias = "strict",
        default_value_t = false
    )]
    pub single_hop: bool,
    #[arg(long = "field")]
    pub fields: Vec<String>,
    /// Revert to the previous history revision (undo the last change).
    #[arg(long)]
    pub undo: bool,
    /// Markdown description to write/overwrite as description.md.
    #[arg(long)]
    pub description: Option<String>,
    /// How to apply `--description`: `replace` (overwrites) or `append`
    /// (preserves existing content, concatenating onto it). Required when
    /// setting a description; there is no default.
    #[arg(long = "description-mode", value_enum)]
    pub description_mode: Option<DescriptionMode>,
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

/// The real `update` command input, converted from [`UpdateArgsCli`] once at
/// the CLI boundary. There is no separate `description_mode` field here: a
/// `description` supplied without a mode cannot be represented past
/// [`UpdateArgsCli::try_into`] (AC5 of ticket 3d952036) — the raw two-flag
/// clap struct is a boundary decoder only, never used past parsing.
#[derive(Debug)]
pub struct UpdateArgs {
    pub id: String,
    pub transition_states: Vec<String>,
    pub to_state: Option<String>,
    pub single_hop: bool,
    pub fields: Vec<String>,
    pub undo: bool,
    pub description_update: ticket_api::storage::DescriptionUpdate,
    pub author: Option<String>,
    pub board_check_in: bool,
    pub board_agent: Option<String>,
    pub board_intent: Option<String>,
    pub board_files: Vec<String>,
    pub board_ttl_secs: Option<u64>,
}

impl TryFrom<UpdateArgsCli> for UpdateArgs {
    type Error = String;

    fn try_from(cli: UpdateArgsCli) -> Result<Self, Self::Error> {
        let description_mode_str =
            cli.description_mode.map(|mode| match mode {
                DescriptionMode::Replace => "replace",
                DescriptionMode::Append => "append",
            });
        let description_update =
            ticket_api::storage::DescriptionUpdate::decode(
                cli.description,
                description_mode_str,
            )?;
        Ok(UpdateArgs {
            id: cli.id,
            transition_states: cli.transition_states,
            to_state: cli.to_state,
            single_hop: cli.single_hop,
            fields: cli.fields,
            undo: cli.undo,
            description_update,
            author: cli.author,
            board_check_in: cli.board_check_in,
            board_agent: cli.board_agent,
            board_intent: cli.board_intent,
            board_files: cli.board_files,
            board_ttl_secs: cli.board_ttl_secs,
        })
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReproOutcome {
    Reproduced,
    NotReproduced,
    Intermittent,
    Fixed,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DanglingStrategy {
    /// Remove each dangling edge directly.
    Unlink,
    /// Reconcile only: report candidates without mutation.
    ReconcileOnly,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DescriptionMode {
    Replace,
    Append,
}

impl DanglingStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unlink => "unlink",
            Self::ReconcileOnly => "reconcile_only",
        }
    }

    pub fn mutates(self) -> bool {
        matches!(self, Self::Unlink)
    }
}

#[derive(Debug, Args)]
pub struct PruneDanglingArgs {
    /// Source ticket UUID or 8+ character hex prefix.
    /// Omit with --all to inspect all source tickets.
    #[arg(required_unless_present = "all")]
    pub root: Option<String>,
    /// Check all source tickets instead of a single root ticket.
    #[arg(long, default_value_t = false)]
    pub all: bool,
    /// Edge kind to inspect (default: depends_on).
    #[arg(long, default_value = "depends_on")]
    pub kind: String,
    /// Cleanup strategy.
    #[arg(long, value_enum, default_value_t = DanglingStrategy::Unlink)]
    pub strategy: DanglingStrategy,
    /// Optional reason recorded in response output.
    #[arg(long)]
    pub reason: Option<String>,
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
pub struct ListPartsArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Include each part's full markdown content in the output.
    #[arg(long, default_value_t = false)]
    pub with_content: bool,
}

#[derive(Debug, Args)]
pub struct GetPartArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Opaque part id (UUID) to fetch.
    #[arg(long = "part-id")]
    pub part_id: Uuid,
}

#[derive(Debug, Args)]
pub struct WritePartArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Opaque part id (UUID) to update. Omit to create a new part.
    #[arg(long = "part-id")]
    pub part_id: Option<Uuid>,
    /// Part kind (e.g. objective, requirements, review, or any free-form
    /// attachment kind). Used when creating a new part; ignored when
    /// updating an existing part (kind is assigned once at creation).
    #[arg(long)]
    pub kind: String,
    /// New markdown content for the part.
    #[arg(long, conflicts_with = "content_file")]
    pub content: Option<String>,
    /// Read new markdown content for the part from this file.
    #[arg(long = "content-file", conflicts_with = "content")]
    pub content_file: Option<PathBuf>,
    /// Author/user identity to record in the history revision (overrides TICKET_AUTHOR env var).
    #[arg(long)]
    pub author: Option<String>,
}

#[derive(Debug, Args)]
pub struct WriteAmendmentArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Opaque part id (UUID) of the frozen (or any) part this amendment corrects.
    #[arg(long)]
    pub supersedes: Uuid,
    /// Opaque part id (UUID) for the new amendment part. Omit to generate one.
    #[arg(long = "part-id")]
    pub part_id: Option<Uuid>,
    /// Markdown content of the amendment.
    #[arg(long, conflicts_with = "content_file")]
    pub content: Option<String>,
    /// Read markdown content of the amendment from this file.
    #[arg(long = "content-file", conflicts_with = "content")]
    pub content_file: Option<PathBuf>,
    /// Author/user identity to record in the history revision (overrides TICKET_AUTHOR env var).
    #[arg(long)]
    pub author: Option<String>,
}

#[derive(Debug, Args)]
pub struct UndoPartArgs {
    /// Ticket UUID or 8+ character hex prefix.
    pub id: String,
    /// Opaque part id (UUID) to restore to its content prior to its most recent write.
    #[arg(long = "part-id")]
    pub part_id: Uuid,
    /// Author/user identity to record in the history revision (overrides TICKET_AUTHOR env var).
    #[arg(long)]
    pub author: Option<String>,
}

use clap::{
    Args,
    Subcommand,
};

#[derive(Debug, Args)]
pub struct BoardArgs {
    #[command(subcommand)]
    pub command: BoardCommand,
}

#[derive(Debug, Subcommand)]
pub enum BoardCommand {
    /// Show the current board snapshot (all agents, or filtered to one agent).
    Show {
        /// Filter entries to a specific agent; also refreshes that agent's heartbeats.
        #[arg(long, alias = "agent-id")]
        agent: Option<String>,
    },
    /// Show recently completed board history separately from active board work.
    History {
        /// Filter historical entries to a specific agent.
        #[arg(long, alias = "agent-id")]
        agent: Option<String>,
    },
    /// List active worktrees and the board entries associated with each one.
    Worktrees,
    /// Check an agent in to the board as actively working a ticket.
    #[command(name = "check-in")]
    CheckIn {
        /// Ticket UUID or 8+ character hex prefix.
        id: String,
        /// Agent identity string.
        #[arg(long, alias = "agent-id")]
        agent: String,
        /// Short description of what the agent intends to do.
        #[arg(long)]
        intent: Option<String>,
        /// Files this agent claims ownership of.
        #[arg(long = "file", alias = "files")]
        files: Vec<String>,
        /// Heartbeat TTL in seconds (default: 3600).
        #[arg(long, alias = "ttl")]
        ttl_secs: Option<u64>,
        /// Session identity that owns this board entry.
        #[arg(long)]
        session_id: Option<String>,
        /// Git worktree path associated with this board entry.
        #[arg(long)]
        worktree_path: Option<String>,
        /// Git branch associated with this board entry.
        #[arg(long)]
        branch: Option<String>,
    },
    /// Check an agent out of the board (mark entry completed).
    #[command(name = "check-out")]
    CheckOut {
        /// Ticket UUID or 8+ character hex prefix.
        id: String,
        /// Agent identity (defaults to any active agent on this ticket).
        #[arg(long, alias = "agent-id")]
        agent: Option<String>,
        /// Optional handoff reason.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Refresh the heartbeat for a board entry so it doesn't go stale.
    Heartbeat {
        /// The board entry UUID (from the entry_id field of a board show response).
        entry_id: String,
    },
    /// Read or update board configuration (max_wip, stale_after_secs, etc.).
    Configure {
        /// Maximum number of concurrent active WIP entries.
        #[arg(long)]
        max_wip: Option<u32>,
        /// Seconds after which an entry without a heartbeat is considered stale.
        #[arg(long)]
        stale_after_secs: Option<u64>,
        /// How long completed entries remain visible in board history (seconds; 0 means all history).
        #[arg(long)]
        completed_audit_window_secs: Option<u64>,
    },
    /// Board cleanup commands.
    Clean(BoardCleanArgs),
    /// Add or remove files from an active board entry's owned_files list.
    #[command(name = "update-files")]
    UpdateFiles {
        /// Ticket UUID or 8+ character hex prefix.
        id: String,
        /// Agent identity string.
        #[arg(long, alias = "agent-id")]
        agent: String,
        /// Files to add to owned_files.
        #[arg(long)]
        add: Vec<String>,
        /// Files to remove from owned_files.
        #[arg(long)]
        remove: Vec<String>,
    },
    /// Atomically rename a file path in an active board entry's owned_files.
    #[command(name = "rename-file")]
    RenameFile {
        /// Ticket UUID or 8+ character hex prefix.
        id: String,
        /// Agent identity string.
        #[arg(long, alias = "agent-id")]
        agent: String,
        /// Current file path.
        #[arg(long, alias = "old-path")]
        from: String,
        /// New file path.
        #[arg(long, alias = "new-path")]
        to: String,
    },
}

#[derive(Debug, Args)]
pub struct BoardCleanArgs {
    #[command(subcommand)]
    pub command: BoardCleanCommand,
}

#[derive(Debug, Subcommand)]
pub enum BoardCleanCommand {
    /// Preview entries eligible for removal (returns a token for apply).
    Preview {
        /// Also include stale (heartbeat-expired) entries in the candidate set.
        #[arg(long, default_value_t = false)]
        include_stale: bool,
    },
    /// Apply a previously obtained clean preview token to remove entries.
    Apply {
        /// The token returned by `board clean preview`.
        token: String,
        /// Also include stale entries (must match the flag used during preview).
        #[arg(long, default_value_t = false)]
        include_stale: bool,
    },
}

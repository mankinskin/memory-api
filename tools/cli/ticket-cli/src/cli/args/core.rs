use std::path::PathBuf;

use clap::Args;
use uuid::Uuid;

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
    /// Force-reconcile: re-read every ticket.toml from disk, update all indexes,
    /// and report which tickets changed.
    #[arg(long)]
    pub force: bool,
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
pub struct BatchArgs {
    /// File containing CLI commands, one per line. If omitted, read from stdin.
    /// Blank lines and lines starting with '#' are ignored.
    /// Example line: create --title "Fix bug" --type tracker-improvement
    #[arg(long)]
    pub file: Option<PathBuf>,
}

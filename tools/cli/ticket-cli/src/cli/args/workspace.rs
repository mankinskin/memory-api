use std::path::PathBuf;

use clap::{
    Args,
    Subcommand,
};

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceSubCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceSubCommand {
    /// Initialize the local ticket workspace root.
    Init(WorkspaceInitArgs),
    /// Show the current local workspace root and how it was resolved.
    Current,
}

#[derive(Debug, Args)]
pub struct WorkspaceInitArgs {
    /// Index root path (defaults to local .ticket/).
    #[arg(long)]
    pub path: Option<PathBuf>,
}

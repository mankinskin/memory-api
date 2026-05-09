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

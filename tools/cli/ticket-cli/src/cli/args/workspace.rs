use clap::{
    Args,
    Subcommand,
};

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// Inspect or edit the workspace policy (`.ticket/workspace-policy.toml`).
    Policy(WorkspacePolicyArgs),
    /// Manage `ignore_workspaces` glob/path patterns.
    Ignore(WorkspacePatternArgs),
    /// Manage `include_overrides` glob/path patterns (override ignores).
    Include(WorkspacePatternArgs),
    /// Re-run discovery + scan, re-registering scan roots with fresh policy metadata.
    Rescan {
        /// Apply the resolved workspace policy when re-registering scan roots.
        #[arg(long)]
        apply_policy: bool,
    },
}

#[derive(Debug, Args)]
pub struct WorkspacePolicyArgs {
    #[command(subcommand)]
    pub command: WorkspacePolicyCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspacePolicyCommand {
    /// Print the resolved policy and its source (file vs compatibility defaults).
    Show,
    /// Set boolean policy fields, preserving unspecified fields.
    Set {
        /// Include descendant stores discovered beneath the workspace root.
        #[arg(long)]
        include_descendants: Option<bool>,
        /// Include ancestor stores discovered above the workspace root.
        #[arg(long)]
        include_ancestors: Option<bool>,
        /// Refuse to index roots outside the workspace subtree when enabled.
        #[arg(long)]
        deny_external_paths: Option<bool>,
    },
}

#[derive(Debug, Args)]
pub struct WorkspacePatternArgs {
    #[command(subcommand)]
    pub command: WorkspacePatternCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspacePatternCommand {
    /// Add a path or glob pattern to the list.
    Add {
        /// Path or glob pattern (relative to the workspace root).
        pattern: String,
    },
    /// Remove a path or glob pattern from the list.
    Remove {
        /// Path or glob pattern to remove.
        pattern: String,
    },
}

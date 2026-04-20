use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Spec title (required).
    #[arg(long)]
    pub title: String,
    /// Hierarchical slug (e.g. "ticket-api/storage/store").
    #[arg(long)]
    pub slug: String,
    /// Component this spec belongs to.
    #[arg(long)]
    pub component: String,
    /// Parent spec ID or slug for hierarchy.
    #[arg(long)]
    pub parent: Option<String>,
    /// Scope (e.g. "public", "internal").
    #[arg(long)]
    pub scope: Option<String>,
    /// Read spec body from this file.
    #[arg(long = "body-file")]
    pub body_file: Option<PathBuf>,
    /// Place the spec in this scan root.
    #[arg(long = "root")]
    pub target_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct GetArgs {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Include body and sections in output.
    #[arg(long, default_value_t = false)]
    pub full: bool,
}

#[derive(Debug, Args)]
pub struct IdArgs {
    /// Spec UUID, prefix, or slug.
    pub id: String,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Field patches as key=value pairs.
    #[arg(long = "field")]
    pub fields: Vec<String>,
    /// Transition to this state.
    #[arg(long = "state")]
    pub to_state: Option<String>,
    /// Update body from file.
    #[arg(long = "body-file")]
    pub body_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Filter by field=value predicates.
    #[arg(long = "where")]
    pub where_clauses: Vec<String>,
    /// Maximum results.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Search query.
    pub query: String,
    /// Maximum results.
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct ScanArgs {
    /// Force full reindex (rebuilds search index).
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct AddRootArgs {
    /// Directory path to register as a scan root.
    pub path: PathBuf,
    /// Optional label for this root.
    #[arg(long)]
    pub label: Option<String>,
}

#[derive(Debug, Args)]
pub struct TreeArgs {
    /// Root spec ID or slug to start from (shows full tree if omitted).
    pub id: Option<String>,
}

#[derive(Debug, Args)]
pub struct RefsArgs {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    #[command(subcommand)]
    pub subcommand: Option<RefsSubcommand>,
}

#[derive(Debug, Subcommand)]
pub enum RefsSubcommand {
    /// Validate code references (check file existence, line ranges).
    Validate {
        /// Workspace root for resolving file paths.
        #[arg(long, default_value = ".")]
        workspace_root: PathBuf,
    },
}

#[derive(Debug, Args)]
pub struct SectionArgs {
    #[command(subcommand)]
    pub command: SectionCommand,
}

#[derive(Debug, Subcommand)]
pub enum SectionCommand {
    /// Add a section to a spec.
    Add {
        /// Spec UUID, prefix, or slug.
        id: String,
        /// Section name (will be used as filename, .md appended if missing).
        #[arg(long)]
        name: String,
        /// Read section content from this file.
        #[arg(long)]
        file: PathBuf,
    },
    /// List sections of a spec.
    List {
        /// Spec UUID, prefix, or slug.
        id: String,
    },
    /// Get section content.
    Get {
        /// Spec UUID, prefix, or slug.
        id: String,
        /// Section name.
        name: String,
    },
    /// Delete a section.
    Delete {
        /// Spec UUID, prefix, or slug.
        id: String,
        /// Section name.
        name: String,
    },
}

#[derive(Debug, Args)]
pub struct BootstrapArgs {
    /// Path to the crate root (must contain Cargo.toml and src/).
    pub crate_path: std::path::PathBuf,
    /// Override the component name (defaults to crate name).
    #[arg(long)]
    pub component: Option<String>,
    /// Print what would be created without writing to the store.
    #[arg(long)]
    pub dry_run: bool,
    /// Workspace root used for computing relative file paths.
    #[arg(long)]
    pub workspace_root: Option<std::path::PathBuf>,
    /// Place the created specs in this scan root.
    #[arg(long = "root")]
    pub target_root: Option<std::path::PathBuf>,
}

#[derive(Debug, Args)]
pub struct HealthArgs {
    /// Spec UUID, prefix, or slug (omit with --all for all specs).
    pub id: Option<String>,
    /// Check all specs.
    #[arg(long, default_value_t = false)]
    pub all: bool,
}

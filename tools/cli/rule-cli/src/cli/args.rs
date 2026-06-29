use std::path::PathBuf;

use clap::{
    Args,
    Parser,
    Subcommand,
};

#[derive(Debug, Parser)]
#[command(
    name = "rule",
    about = "Rule system CLI",
    version,
    arg_required_else_help = true
)]
pub struct RuleCli {
    #[arg(long, global = true, conflicts_with = "toon")]
    pub json: bool,

    #[arg(long, global = true, conflicts_with = "json")]
    pub toon: bool,

    #[arg(long, global = true)]
    pub index_root: Option<PathBuf>,

    #[arg(long, global = true)]
    pub workspace_root: Option<PathBuf>,

    #[command(subcommand)]
    pub command: RuleCommandCli,
}

#[derive(Debug, Subcommand)]
pub enum RuleCommandCli {
    /// Initialize a new rule workspace in the current directory (or at --index-root).
    ///
    /// Creates the `.rule/` store directory and all required index files.
    /// Idempotent: succeeds without error if the workspace already exists.
    Init,
    Create(CreateArgs),
    Get(IdArgs),
    Delete(IdArgs),
    #[command(name = "import-file")]
    ImportFile(ImportFileArgs),
    Update(UpdateArgs),
    Feedback(FeedbackArgs),
    #[command(name = "generate-file")]
    GenerateFile(GenerateFileArgs),
    #[command(name = "generate-target")]
    GenerateTarget(GenerateTargetArgs),
    #[command(name = "explain-target")]
    ExplainTarget(ExplainTargetArgs),
    #[command(name = "sync-targets")]
    SyncTargets(SyncTargetsArgs),
    List(ListArgs),
    Search(SearchArgs),
    Scan(ScanArgs),
    #[command(name = "store-index")]
    StoreIndex(StoreIndexArgs),
    #[command(name = "add-root")]
    AddRoot(AddRootArgs),
    /// Move a rule to another workspace store (dry-run/resume/rollback).
    Move(MoveArgs),
}

/// Move a rule to another workspace store, reusing the safe move kernel.
#[derive(Debug, Args)]
pub struct MoveArgs {
    /// Rule UUID, prefix, or slug to move (required unless --resume/--rollback).
    pub id: Option<String>,
    /// Destination workspace root (required in plan/execute mode).
    #[arg(long = "to-workspace-root")]
    pub to_workspace_root: Option<PathBuf>,
    /// Plan only; do not execute the move.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Resume an interrupted move from a journal UUID.
    #[arg(long)]
    pub resume: Option<String>,
    /// Roll back a move from a journal UUID.
    #[arg(long)]
    pub rollback: Option<String>,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub slug: String,
    #[arg(long = "file-kind")]
    pub file_kind: String,
    #[arg(long)]
    pub section: String,
    #[arg(long)]
    pub body: Option<String>,
    #[arg(long = "body-file")]
    pub body_file: Option<PathBuf>,
    #[arg(long = "repo")]
    pub repo_scope: Vec<String>,
    #[arg(long = "path-scope")]
    pub path_scope: Vec<String>,
    #[arg(long = "order-key")]
    pub order_key: Option<i64>,
    #[arg(long = "source-repo")]
    pub source_repo: Option<String>,
    #[arg(long = "source-path")]
    pub source_path: Option<String>,
    #[arg(long = "source-start-line")]
    pub source_start_line: Option<i64>,
    #[arg(long = "source-end-line")]
    pub source_end_line: Option<i64>,
}

#[derive(Debug, Args)]
pub struct IdArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct ImportFileArgs {
    pub path: PathBuf,
    #[arg(long = "file-kind")]
    pub file_kind: String,
    #[arg(long = "repo")]
    pub repo_scope: Vec<String>,
    #[arg(long = "slug-prefix")]
    pub slug_prefix: String,
    #[arg(long = "default-section")]
    pub default_section: Option<String>,
    #[arg(long = "path-scope")]
    pub path_scope: Vec<String>,
    #[arg(long = "source-repo")]
    pub source_repo: Option<String>,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    pub id: String,
    #[arg(long = "field")]
    pub fields: Vec<String>,
    #[arg(long = "state")]
    pub to_state: Option<String>,
    #[arg(long)]
    pub body: Option<String>,
    #[arg(long = "body-file")]
    pub body_file: Option<PathBuf>,
    /// Replace path_scopes entirely with the given values.
    #[arg(long = "path-scope", conflicts_with = "add_path_scope")]
    pub path_scope: Vec<String>,
    /// Append one or more values to the existing path_scopes (deduplicates).
    #[arg(long = "add-path-scope", conflicts_with = "path_scope")]
    pub add_path_scope: Vec<String>,
}

#[derive(Debug, Args)]
pub struct FeedbackArgs {
    pub id: String,
    #[arg(long, value_parser = ["helpful", "mixed", "not-helpful"])]
    pub rating: String,
    #[arg(long)]
    pub note: Option<String>,
    #[arg(long = "note-kind", value_parser = ["note", "suggestion"])]
    pub note_kind: Option<String>,
    #[arg(long = "session-id")]
    pub session_id: Option<String>,
    #[arg(long = "agent-or-user-id")]
    pub agent_or_user_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct GenerateFileArgs {
    #[arg(long = "file-kind")]
    pub file_kind: String,
    #[arg(long = "repo")]
    pub repo_scope: String,
    #[arg(long = "path-scope")]
    pub path_scope: Option<String>,
    #[arg(long)]
    pub section: Option<String>,
    #[arg(long)]
    pub state: Option<String>,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub check: bool,
}

#[derive(Debug, Args)]
pub struct GenerateTargetArgs {
    #[arg(long)]
    pub config: PathBuf,
    #[arg(long)]
    pub target: String,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub check: bool,
}

#[derive(Debug, Args)]
pub struct ExplainTargetArgs {
    #[arg(long)]
    pub config: PathBuf,
    #[arg(long)]
    pub target: String,
}

#[derive(Debug, Args)]
pub struct SyncTargetsArgs {
    #[arg(long)]
    pub config: PathBuf,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = false)]
    pub check: bool,
}

#[derive(Debug, Args)]
pub struct FilterArgs {
    #[arg(long)]
    pub state: Option<String>,
    #[arg(long = "file-kind")]
    pub file_kind: Option<String>,
    #[arg(long)]
    pub section: Option<String>,
    #[arg(long = "repo")]
    pub repo_scope: Option<String>,
    #[arg(long = "path-scope")]
    pub path_scope: Option<String>,
    #[arg(long)]
    pub slug: Option<String>,
    #[arg(long = "low-rated-only", default_value_t = false)]
    pub low_rated_only: bool,
    #[arg(long = "unresolved-only", default_value_t = false)]
    pub unresolved_only: bool,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[command(flatten)]
    pub filter: FilterArgs,
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    pub query: String,
    #[command(flatten)]
    pub filter: FilterArgs,
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct ScanArgs {
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct AddRootArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub label: Option<String>,
}

/// Generate (or check) the committed rule catalog artifacts:
/// `.rule/README.md`, `.rule/index.toon`, and `.agents/rules-catalog.md`.
#[derive(Debug, Args)]
pub struct StoreIndexArgs {
    /// Check-only mode: render the catalog and exit non-zero on drift without
    /// writing. Mirrors `sync-targets --check`; the hook uses this variant.
    #[arg(long, default_value_t = false)]
    pub check: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn parse_feedback_command() {
        let cli = RuleCli::parse_from([
            "rule",
            "feedback",
            "shared/agents/feedback",
            "--rating",
            "not-helpful",
            "--note",
            "Needs a concrete example.",
            "--note-kind",
            "suggestion",
            "--session-id",
            "session-42",
            "--agent-or-user-id",
            "copilot-gpt-5.4",
        ]);

        match cli.command {
            RuleCommandCli::Feedback(args) => {
                assert_eq!(args.id, "shared/agents/feedback");
                assert_eq!(args.rating, "not-helpful");
                assert_eq!(
                    args.note.as_deref(),
                    Some("Needs a concrete example.")
                );
                assert_eq!(args.note_kind.as_deref(), Some("suggestion"));
                assert_eq!(args.session_id.as_deref(), Some("session-42"));
                assert_eq!(
                    args.agent_or_user_id.as_deref(),
                    Some("copilot-gpt-5.4")
                );
            },
            _ => panic!("expected feedback command"),
        }
    }

    #[test]
    fn parse_delete_command() {
        let cli =
            RuleCli::parse_from(["rule", "delete", "shared/agents/delete-me"]);

        match cli.command {
            RuleCommandCli::Delete(args) => {
                assert_eq!(args.id, "shared/agents/delete-me");
            },
            _ => panic!("expected delete command"),
        }
    }
}

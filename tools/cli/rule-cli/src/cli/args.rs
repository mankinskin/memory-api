use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "rule",
    about = "Rule system CLI",
    version,
    arg_required_else_help = true
)]
pub struct RuleCli {
    #[arg(long, global = true)]
    pub json: bool,

    #[arg(long, global = true)]
    pub index_root: Option<PathBuf>,

    #[command(subcommand)]
    pub command: RuleCommandCli,
}

#[derive(Debug, Subcommand)]
pub enum RuleCommandCli {
    Create(CreateArgs),
    Get(IdArgs),
    #[command(name = "import-file")]
    ImportFile(ImportFileArgs),
    Update(UpdateArgs),
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
    #[command(name = "add-root")]
    AddRoot(AddRootArgs),
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
    #[arg(long = "root")]
    pub target_root: Option<PathBuf>,
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
    #[arg(long = "root")]
    pub target_root: Option<PathBuf>,
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
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::BTreeMap;

use clap::{Args, Parser, Subcommand};
use memory_api::model::filesystem::ScanRoot;
use rule_api::{RuleFilter, RuleManifest, RuleStore};
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(name = "rule", about = "Rule system CLI", version)]
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
    Update(UpdateArgs),
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
pub struct FilterArgs {
    #[arg(long)]
    pub state: Option<String>,
    #[arg(long = "file-kind")]
    pub file_kind: Option<String>,
    #[arg(long)]
    pub section: Option<String>,
    #[arg(long = "repo")]
    pub repo_scope: Option<String>,
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

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("rule error: {0}")]
    Rule(#[from] rule_api::error::RuleError),
    #[error("storage error: {0}")]
    Storage(#[from] memory_api::error::StorageError),
    #[error("{0}")]
    BadRequest(String),
}

pub enum CliOutput {
    Json(Value),
    Text(String),
}

pub fn run(cli: RuleCli) -> Result<CliOutput, CliRunError> {
    let index_root = resolve_index_root(cli.index_root.as_deref());
    let payload = dispatch(cli.command, &index_root)?;
    if cli.json {
        Ok(CliOutput::Json(payload))
    } else {
        Ok(CliOutput::Text(
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| format!("{payload:?}")),
        ))
    }
}

pub fn error_output(message: &str, as_json: bool) -> String {
    if as_json {
        json!({"status": "error", "message": message}).to_string()
    } else {
        message.to_string()
    }
}

pub fn parse_cli_from<I, T>(args: I) -> Result<RuleCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    RuleCli::try_parse_from(args)
}

fn dispatch(command: RuleCommandCli, index_root: &Path) -> Result<Value, CliRunError> {
    let mut store = RuleStore::open(index_root)?;

    match command {
        RuleCommandCli::Create(args) => {
            let body = read_body(args.body, args.body_file.as_deref())?;
            let mut manifest = RuleManifest::new(
                &args.slug,
                &args.title,
                &args.file_kind,
                &args.section,
                &body,
            );
            if let Some(order_key) = args.order_key {
                manifest.set_order_key(order_key);
            }
            if !args.repo_scope.is_empty() {
                manifest.set_repo_scopes(args.repo_scope);
            }
            if !args.path_scope.is_empty() {
                manifest.set_path_scopes(args.path_scope);
            }
            apply_source_location(
                &mut manifest,
                args.source_repo.as_deref(),
                args.source_path.as_deref(),
                args.source_start_line,
                args.source_end_line,
            )?;

            let id = store.create(&manifest, args.target_root.as_deref())?;
            Ok(json!({
                "status": "ok",
                "id": id,
                "slug": manifest.slug(),
                "title": manifest.title(),
                "file_kind": manifest.file_kind(),
                "section": manifest.section(),
            }))
        }
        RuleCommandCli::Get(args) => {
            let rule = store.get(&args.id)?;
            Ok(json!({
                "status": "ok",
                "rule": rule_json(&rule),
            }))
        }
        RuleCommandCli::Update(args) => {
            let patch = parse_fields(&args.fields)?;
            if let Some(body) = read_optional_body(args.body, args.body_file.as_deref())? {
                store.update_body(&args.id, &body)?;
            }
            let rule = store.update(&args.id, patch, args.to_state.as_deref())?;
            Ok(json!({
                "status": "ok",
                "rule": rule_json(&rule),
            }))
        }
        RuleCommandCli::List(args) => {
            let filter = list_filter(&args.filter);
            let rules = store.list(&filter, args.limit)?;
            Ok(json!({
                "status": "ok",
                "count": rules.len(),
                "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
            }))
        }
        RuleCommandCli::Search(args) => {
            let filter = list_filter(&args.filter);
            let rules = store.search(&args.query, &filter, args.limit)?;
            Ok(json!({
                "status": "ok",
                "query": args.query,
                "count": rules.len(),
                "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
            }))
        }
        RuleCommandCli::Scan(args) => {
            let report = store.scan(args.force)?;
            Ok(json!({
                "status": "ok",
                "force": args.force,
                "integrated": report.integrated,
                "pruned": report.pruned,
                "diagnostics_count": report.diagnostics.len(),
            }))
        }
        RuleCommandCli::AddRoot(args) => {
            let label = args.label.unwrap_or_else(|| {
                args.path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("rules")
                    .to_string()
            });
            store.entity_store().add_scan_root(ScanRoot {
                path: args.path.clone(),
                label: label.clone(),
            })?;
            Ok(json!({
                "status": "ok",
                "path": args.path,
                "label": label,
            }))
        }
    }
}

fn resolve_index_root(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Ok(path) = std::env::var("RULE_INDEX_ROOT") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("TICKET_INDEX_ROOT") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::current_dir().ok().map(|dir| dir.join(".rule")) {
        if path.exists() {
            return path;
        }
    }
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        return PathBuf::from(home).join(".rule-index");
    }
    PathBuf::from(".rule")
}

fn read_body(inline: Option<String>, body_file: Option<&Path>) -> Result<String, CliRunError> {
    match (inline, body_file) {
        (_, Some(path)) => fs::read_to_string(path)
            .map_err(|err| CliRunError::BadRequest(format!("read body file {}: {err}", path.display()))),
        (Some(body), None) => Ok(body),
        (None, None) => Ok(String::new()),
    }
}

fn read_optional_body(
    inline: Option<String>,
    body_file: Option<&Path>,
) -> Result<Option<String>, CliRunError> {
    match (inline, body_file) {
        (Some(body), None) => Ok(Some(body)),
        (None, Some(path)) => fs::read_to_string(path)
            .map(Some)
            .map_err(|err| CliRunError::BadRequest(format!("read body file {}: {err}", path.display()))),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(CliRunError::BadRequest(
            "choose either --body or --body-file".to_string(),
        )),
    }
}

fn apply_source_location(
    manifest: &mut RuleManifest,
    source_repo: Option<&str>,
    source_path: Option<&str>,
    source_start_line: Option<i64>,
    source_end_line: Option<i64>,
) -> Result<(), CliRunError> {
    match (source_repo, source_path, source_start_line, source_end_line) {
        (Some(repo), Some(path), Some(start), Some(end)) => {
            manifest.set_source_location(repo, path, start, end);
            Ok(())
        }
        (None, None, None, None) => Ok(()),
        _ => Err(CliRunError::BadRequest(
            "source location requires --source-repo, --source-path, --source-start-line, and --source-end-line together".to_string(),
        )),
    }
}

fn list_filter(args: &FilterArgs) -> RuleFilter {
    RuleFilter {
        state: args.state.clone(),
        file_kind: args.file_kind.clone(),
        section: args.section.clone(),
        repo_scope: args.repo_scope.clone(),
        slug: args.slug.clone(),
        has_unresolved_feedback: args.unresolved_only.then_some(true),
    }
}

fn parse_fields(fields: &[String]) -> Result<BTreeMap<String, Value>, CliRunError> {
    let mut patch = BTreeMap::new();
    for field in fields {
        let (key, value) = field.split_once('=').ok_or_else(|| {
            CliRunError::BadRequest(format!("invalid field format '{field}', expected key=value"))
        })?;
        patch.insert(key.trim().to_string(), Value::String(value.trim().to_string()));
    }
    Ok(patch)
}

fn rule_json(rule: &RuleManifest) -> Value {
    json!({
        "id": rule.id,
        "created_at": rule.created_at,
        "fields": &rule.extra,
    })
}

fn rule_summary_json(rule: &RuleManifest) -> Value {
    json!({
        "id": rule.id,
        "slug": rule.slug(),
        "title": rule.title(),
        "state": rule.state(),
        "file_kind": rule.file_kind(),
        "section": rule.section(),
        "repo_scopes": rule.repo_scopes(),
        "path_scopes": rule.path_scopes(),
        "order_key": rule.order_key(),
        "feedback_unresolved_count": rule.feedback_unresolved_count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_search_command_with_filter_flags() {
        let cli = parse_cli_from([
            "rule",
            "search",
            "discovery",
            "--repo",
            "context-engine",
            "--limit",
            "5",
        ])
        .unwrap();

        match cli.command {
            RuleCommandCli::Search(args) => {
                assert_eq!(args.query, "discovery");
                assert_eq!(args.filter.repo_scope.as_deref(), Some("context-engine"));
                assert_eq!(args.limit, 5);
            }
            _ => panic!("expected search command"),
        }
    }
}
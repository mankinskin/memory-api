use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::BTreeMap;

use clap::{Args, Parser, Subcommand};
use memory_api::model::filesystem::ScanRoot;
use rule_api::{
    ImportedRuleBlock, MarkdownImportOptions, RuleFilter, RuleManifest, RuleStore,
    RenderTarget, import_markdown_blocks, load_render_target_config,
    render_markdown_file, render_target_by_name, resolve_render_target_output,
};
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
    #[command(name = "import-file")]
    ImportFile(ImportFileArgs),
    Update(UpdateArgs),
    #[command(name = "generate-file")]
    GenerateFile(GenerateFileArgs),
    #[command(name = "generate-target")]
    GenerateTarget(GenerateTargetArgs),
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
    pub repo_scope: String,
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

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("rule error: {0}")]
    Rule(#[from] rule_api::error::RuleError),
    #[error("target config error: {0}")]
    TargetConfig(#[from] rule_api::TargetConfigError),
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
        RuleCommandCli::ImportFile(args) => {
            let items = import_file(&mut store, &args)?;
            Ok(json!({
                "status": "ok",
                "count": items.len(),
                "dry_run": args.dry_run,
                "items": items,
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
        RuleCommandCli::GenerateFile(args) => {
            validate_generate_args(&args)?;
            let filter = RuleFilter {
                state: args.state.clone(),
                file_kind: Some(args.file_kind.clone()),
                section: args.section.clone(),
                repo_scope: Some(args.repo_scope.clone()),
                path_scope: args.path_scope.clone(),
                slug: None,
                has_unresolved_feedback: None,
            };
            let rules = store.list(&filter, None)?;
            let rendered = render_markdown_file(&rules);

            if args.check {
                let output = args.output.as_deref().expect("validated output path");
                ensure_generated_output_matches(output, &rendered)?;
            } else if !args.dry_run {
                let output = args.output.as_deref().expect("validated output path");
                write_generated_output(output, &rendered)?;
            }

            Ok(json!({
                "status": "ok",
                "count": rules.len(),
                "file_kind": args.file_kind,
                "repo_scope": args.repo_scope,
                "path_scope": args.path_scope,
                "section": args.section,
                "output": args.output,
                "dry_run": args.dry_run,
                "check": args.check,
                "content": args.dry_run.then_some(rendered),
            }))
        }
        RuleCommandCli::GenerateTarget(args) => {
            validate_generate_target_args(&args)?;
            let config = load_render_target_config(&args.config)?;
            let target = render_target_by_name(&config, &args.target)?;
            let output = resolve_render_target_output(&args.config, target);
            let payload = generate_target_payload(&store, target, args.dry_run, args.check, &output)?;

            Ok(json!({
                "status": "ok",
                "target": args.target,
                "output": output,
                "count": payload.count,
                "file_kind": target.file_kind,
                "repo_scope": target.repo_scope,
                "path_scope": target.path_scope,
                "section": target.section,
                "dry_run": args.dry_run,
                "check": args.check,
                "content": payload.content,
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

fn import_file(
    store: &mut RuleStore,
    args: &ImportFileArgs,
) -> Result<Vec<Value>, CliRunError> {
    let content = fs::read_to_string(&args.path)
        .map_err(|err| CliRunError::BadRequest(format!("read {}: {err}", args.path.display())))?;
    let default_section = args
        .default_section
        .clone()
        .unwrap_or_else(|| default_section_from_path(&args.path));
    let imported_blocks = import_markdown_blocks(
        &content,
        &MarkdownImportOptions {
            slug_prefix: args.slug_prefix.clone(),
            default_section,
        },
    );
    let source_repo = args.source_repo.as_deref().unwrap_or(&args.repo_scope);
    let source_path = args.path.to_string_lossy().replace('\\', "/");

    let mut items = Vec::new();
    for imported in imported_blocks {
        let mut manifest = RuleManifest::new(
            &imported.slug,
            &imported.title,
            &args.file_kind,
            &imported.section,
            &imported.body,
        );
        manifest.set_order_key(imported.order_key);
        manifest.set_repo_scopes([args.repo_scope.as_str()]);
        if !args.path_scope.is_empty() {
            manifest.set_path_scopes(args.path_scope.iter().map(String::as_str));
        }
        manifest.set_source_location(
            source_repo,
            &source_path,
            imported.source_start_line,
            imported.source_end_line,
        );

        let action = if args.dry_run {
            "preview"
        } else if store.get(&imported.slug).is_ok() {
            let patch = import_patch(&manifest);
            store.update_body(&imported.slug, &imported.body)?;
            let _ = store.update(&imported.slug, patch, None)?;
            "updated"
        } else {
            let _ = store.create(&manifest, args.target_root.as_deref())?;
            "created"
        };

        items.push(imported_rule_json(&imported, action));
    }

    Ok(items)
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

fn default_section_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported")
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
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

fn validate_generate_args(args: &GenerateFileArgs) -> Result<(), CliRunError> {
    if args.check && args.dry_run {
        return Err(CliRunError::BadRequest(
            "choose either --check or --dry-run".to_string(),
        ));
    }

    if (args.check || !args.dry_run) && args.output.is_none() {
        return Err(CliRunError::BadRequest(
            "--output is required unless --dry-run is used".to_string(),
        ));
    }

    Ok(())
}

fn validate_generate_target_args(args: &GenerateTargetArgs) -> Result<(), CliRunError> {
    if args.check && args.dry_run {
        return Err(CliRunError::BadRequest(
            "choose either --check or --dry-run".to_string(),
        ));
    }

    Ok(())
}

fn ensure_generated_output_matches(output: &Path, rendered: &str) -> Result<(), CliRunError> {
    let existing = fs::read_to_string(output).map_err(|err| {
        CliRunError::BadRequest(format!("read generated file {}: {err}", output.display()))
    })?;

    if existing == rendered {
        Ok(())
    } else {
        Err(CliRunError::BadRequest(format!(
            "generated output differs from {}",
            output.display()
        )))
    }
}

fn write_generated_output(output: &Path, rendered: &str) -> Result<(), CliRunError> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            CliRunError::BadRequest(format!("create {}: {err}", parent.display()))
        })?;
    }

    fs::write(output, rendered).map_err(|err| {
        CliRunError::BadRequest(format!("write generated file {}: {err}", output.display()))
    })
}

struct GenerateTargetPayload {
    count: usize,
    content: Option<String>,
}

fn generate_target_payload(
    store: &RuleStore,
    target: &RenderTarget,
    dry_run: bool,
    check: bool,
    output: &Path,
) -> Result<GenerateTargetPayload, CliRunError> {
    let filter = RuleFilter {
        state: target.state.clone(),
        file_kind: Some(target.file_kind.clone()),
        section: target.section.clone(),
        repo_scope: Some(target.repo_scope.clone()),
        path_scope: target.path_scope.clone(),
        slug: None,
        has_unresolved_feedback: None,
    };
    let rules = store.list(&filter, None)?;
    let rendered = render_markdown_file(&rules);

    if check {
        ensure_generated_output_matches(output, &rendered)?;
    } else if !dry_run {
        write_generated_output(output, &rendered)?;
    }

    Ok(GenerateTargetPayload {
        count: rules.len(),
        content: dry_run.then_some(rendered),
    })
}

fn list_filter(args: &FilterArgs) -> RuleFilter {
    RuleFilter {
        state: args.state.clone(),
        file_kind: args.file_kind.clone(),
        section: args.section.clone(),
        repo_scope: args.repo_scope.clone(),
        path_scope: args.path_scope.clone(),
        slug: args.slug.clone(),
        has_unresolved_feedback: args.unresolved_only.then_some(true),
    }
}

fn import_patch(manifest: &RuleManifest) -> BTreeMap<String, Value> {
    let mut patch = BTreeMap::new();
    for key in [
        "slug",
        "title",
        "file_kind",
        "section",
        "body",
        "order_key",
        "repo_scopes",
        "path_scopes",
        "source_repo",
        "source_path",
        "source_start_line",
        "source_end_line",
    ] {
        if let Some(value) = manifest.extra.get(key) {
            patch.insert(key.to_string(), value.clone());
        }
    }
    patch
}

fn imported_rule_json(imported: &ImportedRuleBlock, action: &str) -> Value {
    json!({
        "action": action,
        "slug": imported.slug,
        "title": imported.title,
        "section": imported.section,
        "order_key": imported.order_key,
        "source_start_line": imported.source_start_line,
        "source_end_line": imported.source_end_line,
    })
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
    use tempfile::tempdir;

    fn sample_rule(
        slug: &str,
        title: &str,
        section: &str,
        body: &str,
        order_key: i64,
    ) -> RuleManifest {
        let mut manifest = RuleManifest::new(slug, title, "AGENTS", section, body);
        manifest.set_repo_scopes(["context-engine"]);
        manifest.set_order_key(order_key);
        manifest
    }

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

    #[test]
    fn generate_file_writes_deterministic_markdown_with_provenance() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();
        let first = sample_rule(
            "shared/agents/validation",
            "Validation",
            "validation",
            "Run the focused check next.",
            20,
        );
        let second = sample_rule(
            "shared/agents/opening",
            "Opening",
            "opening",
            "Start with the concrete anchor.",
            10,
        );
        store.create(&first, None).unwrap();
        store.create(&second, None).unwrap();

        let output = dir.path().join("generated").join("AGENTS.md");
        dispatch(
            RuleCommandCli::GenerateFile(GenerateFileArgs {
                file_kind: "AGENTS".to_string(),
                repo_scope: "context-engine".to_string(),
                path_scope: None,
                section: None,
                state: None,
                output: Some(output.clone()),
                dry_run: false,
                check: false,
            }),
            dir.path(),
        )
        .unwrap();

        let rendered = fs::read_to_string(&output).unwrap();

        assert!(rendered.starts_with("<!-- rule-api:file generated=true -->\n\n"));
        let opening_idx = rendered.find("slug=shared/agents/opening").unwrap();
        let validation_idx = rendered.find("slug=shared/agents/validation").unwrap();
        assert!(opening_idx < validation_idx);
    }

    #[test]
    fn import_file_creates_rules_from_markdown_blocks() {
        let dir = tempdir().unwrap();
        let markdown = dir.path().join("AGENTS.md");
        fs::write(
            &markdown,
            "# Opening\n\nStart with the concrete anchor.\n\n## Validation\n\nRun the focused check next.",
        )
        .unwrap();

        let mut store = RuleStore::open(dir.path()).unwrap();
        let items = import_file(
            &mut store,
            &ImportFileArgs {
                path: markdown,
                file_kind: "AGENTS".to_string(),
                repo_scope: "context-engine".to_string(),
                slug_prefix: "shared/agents".to_string(),
                default_section: None,
                path_scope: vec!["AGENTS.md".to_string()],
                source_repo: Some("context-engine".to_string()),
                target_root: None,
                dry_run: false,
            },
        )
        .unwrap();

        assert_eq!(items.len(), 2);
        let imported = store
            .list(
                &RuleFilter {
                    repo_scope: Some("context-engine".to_string()),
                    ..RuleFilter::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].slug(), Some("shared/agents/opening/l1"));
        assert_eq!(imported[1].slug(), Some("shared/agents/opening/validation/l5"));
    }

    #[test]
    fn generate_target_uses_config_output_path() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();
        let mut first = sample_rule(
            "shared/agents/opening",
            "Opening",
            "opening",
            "Start with the concrete anchor.",
            10,
        );
        first.set_path_scopes(["AGENTS.md"]);
        let mut second = sample_rule(
            "shared/agents/other",
            "Other",
            "other",
            "Different file target.",
            20,
        );
        second.set_path_scopes([".github/copilot-instructions.md"]);
        store.create(&first, None).unwrap();
        store.create(&second, None).unwrap();

        let config_path = dir.path().join("rule-targets.toml");
        fs::write(
            &config_path,
            r#"
                [[targets]]
                name = "context-engine-agents"
                repo_scope = "context-engine"
                file_kind = "AGENTS"
                path_scope = "AGENTS.md"
                output_path = "generated/AGENTS.md"
            "#,
        )
        .unwrap();

        dispatch(
            RuleCommandCli::GenerateTarget(GenerateTargetArgs {
                config: config_path.clone(),
                target: "context-engine-agents".to_string(),
                dry_run: false,
                check: false,
            }),
            dir.path(),
        )
        .unwrap();

        let rendered = fs::read_to_string(dir.path().join("generated").join("AGENTS.md")).unwrap();
        assert!(rendered.contains("slug=shared/agents/opening"));
        assert!(!rendered.contains("slug=shared/agents/other"));
        assert!(rendered.starts_with("<!-- rule-api:file generated=true -->"));
    }
}
use std::fs;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use memory_api::model::filesystem::ScanRoot;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use rule_api::{RuleFilter, RuleManifest, RuleStore, render_markdown_file};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuleRefInput {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateRuleInput {
    pub id: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default)]
    pub to_state: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateRuleInput {
    pub title: String,
    pub slug: String,
    pub file_kind: String,
    pub section: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub repo_scope: Vec<String>,
    #[serde(default)]
    pub path_scope: Vec<String>,
    #[serde(default)]
    pub order_key: Option<i64>,
    #[serde(default)]
    pub source_repo: Option<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_start_line: Option<i64>,
    #[serde(default)]
    pub source_end_line: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListRulesInput {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub file_kind: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub repo_scope: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub unresolved_only: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRulesInput {
    pub query: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub file_kind: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub repo_scope: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub unresolved_only: bool,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GenerateRuleFileInput {
    pub file_kind: String,
    pub repo_scope: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub check: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanInput {
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddRootInput {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
}

fn default_search_limit() -> usize {
    20
}

#[derive(Clone)]
pub struct RuleServer {
    index_root: PathBuf,
    tool_router: ToolRouter<Self>,
    store_lock: Arc<Mutex<()>>,
}

impl RuleServer {
    pub fn new(index_root: PathBuf) -> Self {
        Self {
            index_root,
            tool_router: Self::tool_router(),
            store_lock: Arc::new(Mutex::new(())),
        }
    }

    fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|err| McpError::internal_error(format!("serialization: {err}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn rule_err(err: rule_api::error::RuleError) -> McpError {
        match &err {
            rule_api::error::RuleError::NotFound(_)
            | rule_api::error::RuleError::DuplicateSlug(_)
            | rule_api::error::RuleError::InvalidSlug(_)
            | rule_api::error::RuleError::AmbiguousPrefix(_) => {
                McpError::invalid_params(err.to_string(), None)
            }
            _ => McpError::internal_error(format!("rule error: {err}"), None),
        }
    }

    fn storage_err(err: memory_api::error::StorageError) -> McpError {
        McpError::internal_error(format!("storage error: {err}"), None)
    }

    async fn with_store<T>(
        &self,
        f: impl FnOnce(&mut RuleStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        let _guard = self.store_lock.lock().await;
        let mut store = RuleStore::open(&self.index_root).map_err(Self::rule_err)?;
        store.scan(false).map_err(Self::rule_err)?;
        let result = f(&mut store);
        drop(store);
        result
    }
}

#[tool_router]
impl RuleServer {
    #[tool(name = "rule_create", description = "Create a new rule entry.")]
    pub async fn rule_create(
        &self,
        Parameters(input): Parameters<CreateRuleInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let mut manifest = RuleManifest::new(
                &input.slug,
                &input.title,
                &input.file_kind,
                &input.section,
                input.body.as_deref().unwrap_or(""),
            );
            if let Some(order_key) = input.order_key {
                manifest.set_order_key(order_key);
            }
            if !input.repo_scope.is_empty() {
                manifest.set_repo_scopes(&input.repo_scope);
            }
            if !input.path_scope.is_empty() {
                manifest.set_path_scopes(&input.path_scope);
            }
            apply_source_location(&mut manifest, &input)?;

            let id = store.create(&manifest, None).map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "id": id,
                "slug": manifest.slug(),
                "title": manifest.title(),
                "file_kind": manifest.file_kind(),
                "section": manifest.section(),
            }))
        })
        .await
    }

    #[tool(name = "rule_get", description = "Get a rule by UUID, prefix, or slug.")]
    pub async fn rule_get(
        &self,
        Parameters(input): Parameters<RuleRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let rule = store.get(&input.id).map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "rule": rule_json(&rule),
            }))
        })
        .await
    }

    #[tool(name = "rule_update", description = "Update a rule entry's fields, state, or body.")]
    pub async fn rule_update(
        &self,
        Parameters(input): Parameters<UpdateRuleInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            if let Some(body) = &input.body {
                store.update_body(&input.id, body).map_err(Self::rule_err)?;
            }
            let patch = parse_fields(&input.fields)?;
            let rule = store
                .update(&input.id, patch, input.to_state.as_deref())
                .map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "rule": rule_json(&rule),
            }))
        })
        .await
    }

    #[tool(name = "rule_list", description = "List rules with optional metadata filters.")]
    pub async fn rule_list(
        &self,
        Parameters(input): Parameters<ListRulesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let filter = rule_filter(
                input.state,
                input.file_kind,
                input.section,
                input.repo_scope,
                input.slug,
                input.unresolved_only,
            );
            let rules = store.list(&filter, input.limit).map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "count": rules.len(),
                "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
            }))
        })
        .await
    }

    #[tool(name = "rule_generate_file", description = "Render deterministic markdown with provenance comments from canonical rule entries.")]
    pub async fn rule_generate_file(
        &self,
        Parameters(input): Parameters<GenerateRuleFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            validate_generate_input(&input)?;
            let filter = RuleFilter {
                state: input.state.clone(),
                file_kind: Some(input.file_kind.clone()),
                section: input.section.clone(),
                repo_scope: Some(input.repo_scope.clone()),
                slug: None,
                has_unresolved_feedback: None,
            };
            let rules = store.list(&filter, None).map_err(Self::rule_err)?;
            let rendered = render_markdown_file(&rules);

            if input.check {
                let output = input.output_path.as_deref().expect("validated output path");
                ensure_generated_output_matches(output, &rendered)?;
            } else if !input.dry_run {
                let output = input.output_path.as_deref().expect("validated output path");
                write_generated_output(output, &rendered)?;
            }

            Self::json_result(&json!({
                "status": "ok",
                "count": rules.len(),
                "file_kind": input.file_kind,
                "repo_scope": input.repo_scope,
                "section": input.section,
                "output_path": input.output_path,
                "dry_run": input.dry_run,
                "check": input.check,
                "content": input.dry_run.then_some(rendered),
            }))
        })
        .await
    }

    #[tool(name = "rule_search", description = "Full-text search over rule entries.")]
    pub async fn rule_search(
        &self,
        Parameters(input): Parameters<SearchRulesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let filter = rule_filter(
                input.state,
                input.file_kind,
                input.section,
                input.repo_scope,
                input.slug,
                input.unresolved_only,
            );
            let rules = store
                .search(&input.query, &filter, input.limit)
                .map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "query": input.query,
                "count": rules.len(),
                "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
            }))
        })
        .await
    }

    #[tool(name = "rule_scan", description = "Run a scan/reindex over registered rule scan roots.")]
    pub async fn rule_scan(
        &self,
        Parameters(input): Parameters<ScanInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let report = store.scan(input.force).map_err(Self::rule_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "force": input.force,
                "integrated": report.integrated,
                "pruned": report.pruned,
                "diagnostics_count": report.diagnostics.len(),
            }))
        })
        .await
    }

    #[tool(name = "rule_add_root", description = "Register a directory as a rule scan root.")]
    pub async fn rule_add_root(
        &self,
        Parameters(input): Parameters<AddRootInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let path = PathBuf::from(&input.path);
            let label = input.label.unwrap_or_else(|| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("rules")
                    .to_string()
            });
            store
                .entity_store()
                .add_scan_root(ScanRoot {
                    path: path.clone(),
                    label: label.clone(),
                })
                .map_err(Self::storage_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "path": path,
                "label": label,
            }))
        })
        .await
    }
}

#[tool_handler]
impl ServerHandler for RuleServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "rule-mcp provides direct access to the rule store. No HTTP backend required. Use named tools for rule operations."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn run_mcp_server(
    index_root: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = RuleServer::new(index_root);

    tracing::info!("Starting rule-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

fn apply_source_location(
    manifest: &mut RuleManifest,
    input: &CreateRuleInput,
) -> Result<(), McpError> {
    match (
        input.source_repo.as_deref(),
        input.source_path.as_deref(),
        input.source_start_line,
        input.source_end_line,
    ) {
        (Some(repo), Some(path), Some(start), Some(end)) => {
            manifest.set_source_location(repo, path, start, end);
            Ok(())
        }
        (None, None, None, None) => Ok(()),
        _ => Err(McpError::invalid_params(
            "source location requires source_repo, source_path, source_start_line, and source_end_line together".to_string(),
            None,
        )),
    }
}

fn rule_filter(
    state: Option<String>,
    file_kind: Option<String>,
    section: Option<String>,
    repo_scope: Option<String>,
    slug: Option<String>,
    unresolved_only: bool,
) -> RuleFilter {
    RuleFilter {
        state,
        file_kind,
        section,
        repo_scope,
        slug,
        has_unresolved_feedback: unresolved_only.then_some(true),
    }
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

fn parse_fields(fields: &[String]) -> Result<BTreeMap<String, Value>, McpError> {
    let mut patch = BTreeMap::new();
    for field in fields {
        let (key, value) = field.split_once('=').ok_or_else(|| {
            McpError::invalid_params(
                format!("invalid field format '{field}', expected key=value"),
                None,
            )
        })?;
        patch.insert(key.trim().to_string(), Value::String(value.trim().to_string()));
    }
    Ok(patch)
}

fn validate_generate_input(input: &GenerateRuleFileInput) -> Result<(), McpError> {
    if input.check && input.dry_run {
        return Err(McpError::invalid_params(
            "choose either check or dry_run".to_string(),
            None,
        ));
    }

    if (input.check || !input.dry_run) && input.output_path.is_none() {
        return Err(McpError::invalid_params(
            "output_path is required unless dry_run is true".to_string(),
            None,
        ));
    }

    Ok(())
}

fn ensure_generated_output_matches(output: &str, rendered: &str) -> Result<(), McpError> {
    let path = PathBuf::from(output);
    let existing = fs::read_to_string(&path).map_err(|err| {
        McpError::invalid_params(
            format!("read generated file {}: {err}", path.display()),
            None,
        )
    })?;

    if existing == rendered {
        Ok(())
    } else {
        Err(McpError::invalid_params(
            format!("generated output differs from {}", path.display()),
            None,
        ))
    }
}

fn write_generated_output(output: &str, rendered: &str) -> Result<(), McpError> {
    let path = PathBuf::from(output);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            McpError::internal_error(format!("create {}: {err}", parent.display()), None)
        })?;
    }

    fs::write(&path, rendered).map_err(|err| {
        McpError::internal_error(format!("write generated file {}: {err}", path.display()), None)
    })
}
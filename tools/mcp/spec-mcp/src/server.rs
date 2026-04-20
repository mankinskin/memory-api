use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

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

use spec_api::code_ref::validate_refs;
use spec_api::error::SpecError;
use spec_api::{SpecManifest, SpecStore};
use memory_api::model::filesystem::ScanRoot;

// ── Input types ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpecRefInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateSpecInput {
    /// Spec title.
    pub title: String,
    /// Hierarchical slug (e.g. "ticket-api/storage/store").
    pub slug: String,
    /// Component this spec belongs to.
    pub component: String,
    /// Parent spec ID or slug.
    #[serde(default)]
    pub parent: Option<String>,
    /// Scope (e.g. "public", "internal").
    #[serde(default)]
    pub scope: Option<String>,
    /// Body content (markdown).
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSpecInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Include body and sections in output.
    #[serde(default)]
    pub full: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateSpecInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Field patches as key=value pairs (e.g. ["title=New Title", "state=active"]).
    #[serde(default)]
    pub fields: Vec<String>,
    /// Optional state to transition to.
    #[serde(default)]
    pub to_state: Option<String>,
    /// Optional body content to replace.
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSpecsInput {
    /// Filter by field=value predicates.
    #[serde(default)]
    pub where_clauses: Vec<String>,
    /// Maximum results.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSpecsInput {
    /// Full-text search query.
    pub query: String,
    /// Maximum results.
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TreeInput {
    /// Root spec ID or slug (omit for all roots).
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HealthInput {
    /// Spec UUID, prefix, or slug (omit with all=true for all specs).
    #[serde(default)]
    pub id: Option<String>,
    /// Check all specs.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefsValidateInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Workspace root for resolving file paths.
    #[serde(default = "default_workspace_root")]
    pub workspace_root: String,
}

fn default_workspace_root() -> String {
    ".".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SectionAddInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Section name.
    pub name: String,
    /// Section content (markdown).
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SectionRefInput {
    /// Spec UUID, prefix, or slug.
    pub id: String,
    /// Section name.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScanInput {
    /// Force full reindex.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddRootInput {
    /// Directory path to register as a scan root.
    pub path: String,
    /// Optional label for this root.
    #[serde(default)]
    pub label: Option<String>,
}

// ── Server ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SpecServer {
    index_root: PathBuf,
    tool_router: ToolRouter<Self>,
    /// Serializes all SpecStore open/drop cycles so concurrent MCP calls
    /// never race on the redb file lock, while still releasing the lock
    /// between calls so the CLI can also access the database.
    store_lock: Arc<Mutex<()>>,
}

impl SpecServer {
    pub fn new(index_root: PathBuf) -> Self {
        Self {
            index_root,
            tool_router: Self::tool_router(),
            store_lock: Arc::new(Mutex::new(())),
        }
    }

    fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| McpError::internal_error(format!("serialization: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn spec_err(e: SpecError) -> McpError {
        match &e {
            SpecError::NotFound(_) => McpError::invalid_params(e.to_string(), None),
            SpecError::InvalidSlug(_) => McpError::invalid_params(e.to_string(), None),
            SpecError::DuplicateSlug(_) => McpError::invalid_params(e.to_string(), None),
            _ => McpError::internal_error(format!("spec error: {e}"), None),
        }
    }

    fn storage_err(e: memory_api::error::StorageError) -> McpError {
        McpError::internal_error(format!("storage error: {e}"), None)
    }

    /// Open a mutable SpecStore under the serialization lock, run the closure,
    /// then drop both store and lock before returning.
    ///
    /// Uses `&mut SpecStore` since create/update/delete/scan all mutate the
    /// slug index. The auto-scan ensures slug resolution works on every call.
    async fn with_store<T>(
        &self,
        f: impl FnOnce(&mut SpecStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        let _guard = self.store_lock.lock().await;
        let mut store = SpecStore::open(&self.index_root).map_err(Self::spec_err)?;
        store.scan(false).map_err(Self::spec_err)?;
        let result = f(&mut store);
        drop(store);
        result
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl SpecServer {
    #[tool(
        name = "spec_create",
        description = "Create a new spec with title, slug, component, and optional body."
    )]
    async fn spec_create(
        &self,
        Parameters(input): Parameters<CreateSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let mut manifest =
                SpecManifest::new(&input.slug, &input.title, &input.component);
            if let Some(parent) = &input.parent {
                let parent_id = store.resolve_id(parent).map_err(Self::spec_err)?;
                manifest.set_parent(&parent_id.to_string());
            }
            if let Some(scope) = &input.scope {
                manifest.set_scope(scope);
            }
            let body = input.body.as_deref().unwrap_or("");
            let id = store.create(&manifest, body, None).map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "id": id,
                "slug": input.slug,
                "title": input.title,
                "component": input.component,
                "state": "draft",
            }))
        })
        .await
    }

    #[tool(
        name = "spec_get",
        description = "Get a spec by ID or slug, optionally with body and sections."
    )]
    async fn spec_get(
        &self,
        Parameters(input): Parameters<GetSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            if input.full {
                let (spec, body) = store.get_full(&input.id).map_err(Self::spec_err)?;
                let sections = store.list_sections(&input.id).map_err(Self::spec_err)?;
                Self::json_result(&json!({
                    "status": "ok",
                    "spec": {
                        "id": spec.id,
                        "created_at": spec.created_at,
                        "fields": spec.extra,
                        "code_refs": spec.code_refs,
                    },
                    "body": body,
                    "sections": sections,
                }))
            } else {
                let spec = store.get(&input.id).map_err(Self::spec_err)?;
                Self::json_result(&json!({
                    "status": "ok",
                    "spec": {
                        "id": spec.id,
                        "created_at": spec.created_at,
                        "fields": spec.extra,
                        "code_refs": spec.code_refs,
                    },
                }))
            }
        })
        .await
    }

    #[tool(
        name = "spec_update",
        description = "Update a spec's fields, state, or body."
    )]
    async fn spec_update(
        &self,
        Parameters(input): Parameters<UpdateSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let mut patch = BTreeMap::new();
            for raw in &input.fields {
                let (k, v) = raw.split_once('=').ok_or_else(|| {
                    McpError::invalid_params(
                        format!("invalid field format '{raw}', expected key=value"),
                        None,
                    )
                })?;
                patch.insert(k.trim().to_string(), Value::String(v.trim().to_string()));
            }

            if let Some(body) = &input.body {
                store.update_body(&input.id, body).map_err(Self::spec_err)?;
            }

            let spec = store
                .update(&input.id, patch, input.to_state.as_deref())
                .map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "id": spec.id,
                "fields": spec.extra,
            }))
        })
        .await
    }

    #[tool(name = "spec_delete", description = "Soft-delete a spec.")]
    async fn spec_delete(
        &self,
        Parameters(input): Parameters<SpecRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let id = store.resolve_id(&input.id).map_err(Self::spec_err)?;
            store.delete(&input.id).map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "id": id,
            }))
        })
        .await
    }

    #[tool(
        name = "spec_list",
        description = "List specs with optional field=value filters."
    )]
    async fn spec_list(
        &self,
        Parameters(input): Parameters<ListSpecsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let all = store
                .entity_store()
                .list_indexed(false)
                .map_err(Self::storage_err)?;
            let mut items: Vec<Value> = Vec::new();
            'outer: for indexed in &all {
                let spec = match store.get(&indexed.id.to_string()) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                for clause in &input.where_clauses {
                    if let Some((k, v)) = clause.split_once('=') {
                        let field_val = spec.extra.get(k).and_then(|fv| fv.as_str());
                        if field_val != Some(v) {
                            continue 'outer;
                        }
                    }
                }
                items.push(json!({
                    "id": indexed.id,
                    "slug": spec.slug(),
                    "title": spec.title(),
                    "state": spec.state(),
                    "component": spec.component(),
                }));
                if let Some(limit) = input.limit {
                    if items.len() >= limit {
                        break;
                    }
                }
            }
            Self::json_result(&json!({
                "status": "ok",
                "count": items.len(),
                "items": items,
            }))
        })
        .await
    }

    #[tool(name = "spec_search", description = "Full-text search across specs.")]
    async fn spec_search(
        &self,
        Parameters(input): Parameters<SearchSpecsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let results = store
                .entity_store()
                .search(&input.query, input.limit)
                .map_err(Self::storage_err)?;
            let items: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "id": r.id,
                        "title": r.title,
                        "state": r.state,
                        "type": r.ticket_type,
                        "score": r.score,
                        "snippet": r.snippet,
                    })
                })
                .collect();
            Self::json_result(&json!({
                "status": "ok",
                "query": input.query,
                "count": items.len(),
                "items": items,
            }))
        })
        .await
    }

    #[tool(
        name = "spec_tree",
        description = "Get hierarchy subtree for a spec, or list all root specs."
    )]
    async fn spec_tree(
        &self,
        Parameters(input): Parameters<TreeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            if let Some(root_id) = &input.id {
                let root = store.get(root_id).map_err(Self::spec_err)?;
                let descendants = store.subtree(root_id).map_err(Self::spec_err)?;
                Self::json_result(&json!({
                    "status": "ok",
                    "root": {
                        "id": root.id,
                        "slug": root.slug(),
                        "title": root.title(),
                        "state": root.state(),
                    },
                    "descendants": descendants.iter().map(|c| json!({
                        "id": c.id,
                        "slug": c.slug(),
                        "title": c.title(),
                        "state": c.state(),
                        "parent": c.parent(),
                    })).collect::<Vec<_>>(),
                }))
            } else {
                let all = store
                    .entity_store()
                    .list_indexed(false)
                    .map_err(Self::storage_err)?;
                let mut roots = Vec::new();
                for indexed in &all {
                    if let Ok(spec) = store.get(&indexed.id.to_string()) {
                        if spec.parent().is_none() {
                            let children = store
                                .children(&indexed.id.to_string())
                                .map_err(Self::spec_err)?;
                            roots.push(json!({
                                "id": spec.id,
                                "slug": spec.slug(),
                                "title": spec.title(),
                                "children_count": children.len(),
                            }));
                        }
                    }
                }
                Self::json_result(&json!({
                    "status": "ok",
                    "roots": roots,
                }))
            }
        })
        .await
    }

    #[tool(
        name = "spec_health",
        description = "Run health checks on specs (completeness of required fields)."
    )]
    async fn spec_health(
        &self,
        Parameters(input): Parameters<HealthInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let specs = if input.all {
                let all = store
                    .entity_store()
                    .list_indexed(false)
                    .map_err(Self::storage_err)?;
                all.iter()
                    .filter_map(|e| store.get(&e.id.to_string()).ok())
                    .collect::<Vec<_>>()
            } else if let Some(id) = &input.id {
                vec![store.get(id).map_err(Self::spec_err)?]
            } else {
                return Err(McpError::invalid_params(
                    "provide spec ID or set all=true",
                    None,
                ));
            };

            let mut issues = Vec::new();
            for spec in &specs {
                if spec.slug().is_none() {
                    issues.push(json!({"id": spec.id, "issue": "missing slug"}));
                }
                if spec.title().is_none() {
                    issues.push(json!({"id": spec.id, "issue": "missing title"}));
                }
                if spec.component().is_none() {
                    issues.push(json!({"id": spec.id, "issue": "missing component"}));
                }
            }
            Self::json_result(&json!({
                "status": "ok",
                "specs_checked": specs.len(),
                "issues_count": issues.len(),
                "issues": issues,
            }))
        })
        .await
    }

    #[tool(
        name = "spec_refs_validate",
        description = "Validate code references for a spec (check file existence and line ranges)."
    )]
    async fn spec_refs_validate(
        &self,
        Parameters(input): Parameters<RefsValidateInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let spec = store.get(&input.id).map_err(Self::spec_err)?;
            let workspace_root = PathBuf::from(&input.workspace_root);
            let results = validate_refs(&spec.code_refs, &workspace_root);
            let items: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "file": r.code_ref.file,
                        "symbol": r.code_ref.symbol,
                        "kind": format!("{:?}", r.code_ref.kind),
                        "file_exists": r.file_exists,
                        "line_range_valid": r.line_range_valid,
                        "message": r.message,
                    })
                })
                .collect();
            let all_valid = results.iter().all(|r| r.file_exists && r.line_range_valid);
            Self::json_result(&json!({
                "status": "ok",
                "id": spec.id,
                "valid": all_valid,
                "count": items.len(),
                "results": items,
            }))
        })
        .await
    }

    #[tool(name = "spec_section_add", description = "Add a section to a spec.")]
    async fn spec_section_add(
        &self,
        Parameters(input): Parameters<SectionAddInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            store
                .add_section(&input.id, &input.name, &input.content)
                .map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "section": input.name,
            }))
        })
        .await
    }

    #[tool(name = "spec_section_list", description = "List sections of a spec.")]
    async fn spec_section_list(
        &self,
        Parameters(input): Parameters<SpecRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let sections = store.list_sections(&input.id).map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "count": sections.len(),
                "sections": sections,
            }))
        })
        .await
    }

    #[tool(name = "spec_section_get", description = "Get section content.")]
    async fn spec_section_get(
        &self,
        Parameters(input): Parameters<SectionRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let uuid = store.resolve_id(&input.id).map_err(Self::spec_err)?;
            let indexed = store
                .entity_store()
                .get_indexed(&uuid)
                .map_err(Self::storage_err)?
                .ok_or_else(|| McpError::invalid_params("spec not found", None))?;
            let file_name = if input.name.ends_with(".md") {
                input.name.clone()
            } else {
                format!("{}.md", input.name)
            };
            let path = indexed.path.join("sections").join(&file_name);
            let content = std::fs::read_to_string(&path).map_err(|e| {
                McpError::invalid_params(format!("section not found: {e}"), None)
            })?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "section": input.name,
                "content": content,
            }))
        })
        .await
    }

    #[tool(name = "spec_section_delete", description = "Delete a section from a spec.")]
    async fn spec_section_delete(
        &self,
        Parameters(input): Parameters<SectionRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            store
                .delete_section(&input.id, &input.name)
                .map_err(Self::spec_err)?;
            Self::json_result(&json!({
                "status": "ok",
                "spec": input.id,
                "section": input.name,
            }))
        })
        .await
    }

    #[tool(name = "spec_scan", description = "Scan and reindex all spec roots.")]
    async fn spec_scan(
        &self,
        Parameters(input): Parameters<ScanInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let report = store.scan(input.force).map_err(Self::spec_err)?;
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

    #[tool(
        name = "spec_add_root",
        description = "Register a directory as a scan root for specs."
    )]
    async fn spec_add_root(
        &self,
        Parameters(input): Parameters<AddRootInput>,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let path = PathBuf::from(&input.path);
            let label = input.label.unwrap_or_else(|| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("specs")
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

// ── MCP handler trait ─────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for SpecServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "spec-mcp provides direct access to the spec store. No HTTP backend required. Use named tools for spec operations."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Server startup ────────────────────────────────────────────────────────────

pub async fn run_mcp_server(
    index_root: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = SpecServer::new(index_root);

    tracing::info!("Starting spec-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

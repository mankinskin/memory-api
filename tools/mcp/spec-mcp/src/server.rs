use std::{
    path::PathBuf,
    sync::Arc,
};

use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
    handler::server::{
        tool::ToolRouter,
        wrapper::Parameters,
    },
    model::*,
    tool,
    tool_handler,
    tool_router,
    transport::stdio,
};
use serde::Serialize;
use tokio::sync::Mutex;

use spec_api::{
    SpecStore,
    error::SpecError,
};

mod query;
mod sections;
mod types;

pub use self::types::*;

// ── Server ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SpecServer {
    index_root: PathBuf,
    tool_router: ToolRouter<Self>,
    /// Serializes all SpecStore open/drop cycles so concurrent MCP calls
    /// never race on the SQLite write lock, while still releasing the lock
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

    fn json_result<T: Serialize>(
        value: &T
    ) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value).map_err(|e| {
            McpError::internal_error(format!("serialization: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn spec_err(e: SpecError) -> McpError {
        match &e {
            SpecError::NotFound(_) =>
                McpError::invalid_params(e.to_string(), None),
            SpecError::InvalidSlug(_) =>
                McpError::invalid_params(e.to_string(), None),
            SpecError::DuplicateSlug(_) =>
                McpError::invalid_params(e.to_string(), None),
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
        let mut store =
            SpecStore::open(&self.index_root).map_err(Self::spec_err)?;
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
    pub async fn spec_create(
        &self,
        Parameters(input): Parameters<CreateSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_create_tool(input).await
    }

    #[tool(
        name = "spec_get",
        description = "Get a spec by ID or slug, optionally with body and sections."
    )]
    pub async fn spec_get(
        &self,
        Parameters(input): Parameters<GetSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_get_tool(input).await
    }

    #[tool(
        name = "spec_update",
        description = "Update a spec's fields, state, or body. Omit untouched keys; the response returns only changed or directly relevant fields."
    )]
    pub async fn spec_update(
        &self,
        Parameters(input): Parameters<UpdateSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_update_tool(input).await
    }

    #[tool(name = "spec_delete", description = "Soft-delete a spec.")]
    pub async fn spec_delete(
        &self,
        Parameters(input): Parameters<SpecRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_delete_tool(input).await
    }

    #[tool(
        name = "spec_list",
        description = "List specs with optional field=value filters."
    )]
    pub async fn spec_list(
        &self,
        Parameters(input): Parameters<ListSpecsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_list_tool(input).await
    }

    #[tool(
        name = "spec_search",
        description = "Full-text search across specs."
    )]
    pub async fn spec_search(
        &self,
        Parameters(input): Parameters<SearchSpecsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_search_tool(input).await
    }

    #[tool(
        name = "spec_tree",
        description = "Get hierarchy subtree for a spec, or list all root specs."
    )]
    pub async fn spec_tree(
        &self,
        Parameters(input): Parameters<TreeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_tree_tool(input).await
    }

    #[tool(
        name = "spec_health",
        description = "Run health checks on specs (completeness of required fields)."
    )]
    pub async fn spec_health(
        &self,
        Parameters(input): Parameters<HealthInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_health_tool(input).await
    }

    #[tool(
        name = "spec_refs_validate",
        description = "Validate code references for a spec (check file existence and line ranges)."
    )]
    pub async fn spec_refs_validate(
        &self,
        Parameters(input): Parameters<RefsValidateInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_refs_validate_tool(input).await
    }

    #[tool(name = "spec_section_add", description = "Add a section to a spec.")]
    pub async fn spec_section_add(
        &self,
        Parameters(input): Parameters<SectionAddInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_section_add_tool(input).await
    }

    #[tool(
        name = "spec_section_list",
        description = "List sections of a spec."
    )]
    pub async fn spec_section_list(
        &self,
        Parameters(input): Parameters<SpecRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_section_list_tool(input).await
    }

    #[tool(name = "spec_section_get", description = "Get section content.")]
    pub async fn spec_section_get(
        &self,
        Parameters(input): Parameters<SectionRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_section_get_tool(input).await
    }

    #[tool(
        name = "spec_section_delete",
        description = "Delete a section from a spec."
    )]
    pub async fn spec_section_delete(
        &self,
        Parameters(input): Parameters<SectionRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_section_delete_tool(input).await
    }

    #[tool(
        name = "spec_scan",
        description = "Scan and reindex all spec roots."
    )]
    pub async fn spec_scan(
        &self,
        Parameters(input): Parameters<ScanInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_scan_tool(input).await
    }

    #[tool(
        name = "spec_add_root",
        description = "Register a directory as a scan root for specs."
    )]
    pub async fn spec_add_root(
        &self,
        Parameters(input): Parameters<AddRootInput>,
    ) -> Result<CallToolResult, McpError> {
        self.spec_add_root_tool(input).await
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
    index_root: PathBuf
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = SpecServer::new(index_root);

    tracing::info!("Starting spec-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

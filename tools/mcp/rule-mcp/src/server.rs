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

use rule_api::RuleStore;

mod admin;
mod generate;
mod importing;
mod query;
mod types;

pub use self::types::*;

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

    fn json_result<T: Serialize>(
        value: &T
    ) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value).map_err(|err| {
            McpError::internal_error(format!("serialization: {err}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn rule_err(err: rule_api::error::RuleError) -> McpError {
        match &err {
            rule_api::error::RuleError::NotFound(_)
            | rule_api::error::RuleError::DuplicateSlug(_)
            | rule_api::error::RuleError::InvalidSlug(_)
            | rule_api::error::RuleError::AmbiguousPrefix(_) =>
                McpError::invalid_params(err.to_string(), None),
            _ => McpError::internal_error(format!("rule error: {err}"), None),
        }
    }

    fn storage_err(err: memory_api::error::StorageError) -> McpError {
        McpError::internal_error(format!("storage error: {err}"), None)
    }

    fn target_config_err(err: rule_api::TargetConfigError) -> McpError {
        McpError::invalid_params(err.to_string(), None)
    }

    async fn with_store<T>(
        &self,
        f: impl FnOnce(&mut RuleStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        let _guard = self.store_lock.lock().await;
        let mut store =
            RuleStore::open(&self.index_root).map_err(Self::rule_err)?;
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
        self.rule_create_tool(input).await
    }

    #[tool(
        name = "rule_get",
        description = "Get a rule by UUID, prefix, or slug."
    )]
    pub async fn rule_get(
        &self,
        Parameters(input): Parameters<RuleRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_get_tool(input).await
    }

    #[tool(
        name = "rule_import_file",
        description = "Import markdown blocks from an existing file into canonical rule entries."
    )]
    pub async fn rule_import_file(
        &self,
        Parameters(input): Parameters<ImportRuleFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_import_file_tool(input).await
    }

    #[tool(
        name = "rule_update",
        description = "Update a rule entry's fields, state, or body."
    )]
    pub async fn rule_update(
        &self,
        Parameters(input): Parameters<UpdateRuleInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_update_tool(input).await
    }

    #[tool(
        name = "rule_list",
        description = "List rules with optional metadata filters."
    )]
    pub async fn rule_list(
        &self,
        Parameters(input): Parameters<ListRulesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_list_tool(input).await
    }

    #[tool(
        name = "rule_generate_file",
        description = "Render deterministic markdown with provenance comments from canonical rule entries."
    )]
    pub async fn rule_generate_file(
        &self,
        Parameters(input): Parameters<GenerateRuleFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_generate_file_tool(input).await
    }

    #[tool(
        name = "rule_generate_target",
        description = "Render a named configured markdown target from canonical rule entries."
    )]
    pub async fn rule_generate_target(
        &self,
        Parameters(input): Parameters<GenerateRuleTargetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_generate_target_tool(input).await
    }

    #[tool(
        name = "rule_explain_target",
        description = "Preview a named configured markdown target as an outline with matched entries per node."
    )]
    pub async fn rule_explain_target(
        &self,
        Parameters(input): Parameters<ExplainRuleTargetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_explain_target_tool(input).await
    }

    #[tool(
        name = "rule_search",
        description = "Full-text search over rule entries."
    )]
    pub async fn rule_search(
        &self,
        Parameters(input): Parameters<SearchRulesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_search_tool(input).await
    }

    #[tool(
        name = "rule_scan",
        description = "Run a scan/reindex over registered rule scan roots."
    )]
    pub async fn rule_scan(
        &self,
        Parameters(input): Parameters<ScanInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_scan_tool(input).await
    }

    #[tool(
        name = "rule_add_root",
        description = "Register a directory as a rule scan root."
    )]
    pub async fn rule_add_root(
        &self,
        Parameters(input): Parameters<AddRootInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_add_root_tool(input).await
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
    index_root: PathBuf
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = RuleServer::new(index_root);

    tracing::info!("Starting rule-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

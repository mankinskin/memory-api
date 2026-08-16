use std::{
    path::{
        Path,
        PathBuf,
    },
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

use rule_api::{
    RuleStore,
    discover_workspace_scan_roots,
    workspace_root_for_index_root,
};

mod admin;
mod feedback;
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
        let text = serde_json::to_string(value).map_err(|err| {
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

    fn storage_err(err: memory_kernel::error::StorageError) -> McpError {
        McpError::internal_error(format!("storage error: {err}"), None)
    }

    fn target_config_err(err: rule_api::TargetConfigError) -> McpError {
        McpError::invalid_params(err.to_string(), None)
    }

    fn is_rule_store_root(path: &Path) -> bool {
        path.join("rules").is_dir()
            || path.join("entities.db").is_file()
            || path.join("search_index").is_dir()
    }

    fn resolve_workspace_root(
        &self,
        workspace: &str,
    ) -> Result<PathBuf, McpError> {
        let workspace =
            memory_kernel::workspace::validate_explicit_workspace_selector(
                Some(workspace),
            )
            .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
        let resolved = memory_kernel::workspace::resolve_store_root_from(
            Path::new(workspace),
            ".rule",
        );
        if resolved.file_name().and_then(|name| name.to_str()) == Some(".rule")
            || Self::is_rule_store_root(&resolved)
        {
            return Ok(resolved);
        }

        Err(McpError::invalid_params(
            format!(
                "invalid workspace '{workspace}': expected a repo root containing .rule, the .rule store itself, a path inside that store, or an existing rule store root"
            ),
            None,
        ))
    }

    async fn with_store<T>(
        &self,
        f: impl FnOnce(&mut RuleStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        self.with_store_at(self.index_root.clone(), f).await
    }

    async fn with_store_for_workspace<T>(
        &self,
        workspace: &str,
        f: impl FnOnce(&mut RuleStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        let index_root = self.resolve_workspace_root(workspace)?;
        self.with_store_at(index_root, f).await
    }

    async fn with_store_at<T>(
        &self,
        index_root: PathBuf,
        f: impl FnOnce(&mut RuleStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        let _guard = self.store_lock.lock().await;
        let mut store = RuleStore::open(&index_root).map_err(Self::rule_err)?;
        if let Some(workspace_root) = workspace_root_for_index_root(&index_root)
        {
            for root in discover_workspace_scan_roots(&workspace_root) {
                store
                    .entity_store()
                    .add_scan_root(root)
                    .map_err(Self::storage_err)?;
            }
        }
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
        description = "Update a rule entry's fields, state, or body. Omit untouched keys; the response returns only changed or directly relevant fields."
    )]
    pub async fn rule_update(
        &self,
        Parameters(input): Parameters<UpdateRuleInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_update_tool(input).await
    }

    #[tool(
        name = "rule_record_feedback",
        description = "Attach a rating and optional note to a rule entry, and resync its indexed feedback summary."
    )]
    pub async fn rule_record_feedback(
        &self,
        Parameters(input): Parameters<RecordFeedbackInput>,
    ) -> Result<CallToolResult, McpError> {
        self.rule_record_feedback_tool(input).await
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

    #[tool(
        name = "rule_move_preflight",
        description = "Read-only preflight plan for moving a rule to another workspace store."
    )]
    pub async fn rule_move_preflight(
        &self,
        Parameters(input): Parameters<RuleMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let to = PathBuf::from(&input.to_workspace_root);
        self.with_store(move |store| {
            let id = store.resolve_id(&input.id).map_err(Self::rule_err)?;
            let report = store.plan_move_preflight(&id, &to).map_err(Self::rule_err)?;
            Self::json_result(&serde_json::json!({"status":"ok","mode":"preflight","id":id.to_string(),"supported":report.supported(),"blockers":report.blockers}))
        })
        .await
    }

    #[tool(
        name = "rule_move_apply",
        description = "Execute a supported rule move to another workspace store."
    )]
    pub async fn rule_move_apply(
        &self,
        Parameters(input): Parameters<RuleMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let to = PathBuf::from(&input.to_workspace_root);
        self.with_store(move |store| {
            let id = store.resolve_id(&input.id).map_err(Self::rule_err)?;
            let report = store.plan_move_preflight(&id, &to).map_err(Self::rule_err)?;
            if !report.supported() {
                return Err(McpError::invalid_params("move preflight blocked; run rule_move_preflight".to_string(), None));
            }
            let outcome = store.execute_move_with_journal(&report).map_err(Self::rule_err)?;
            Self::json_result(&serde_json::json!({"status":"ok","mode":"apply","id":id.to_string(),"journal_id":outcome.journal.id,"phase":outcome.journal.phase}))
        })
        .await
    }

    #[tool(
        name = "rule_move_resume",
        description = "Resume an interrupted rule move from a journal id."
    )]
    pub async fn rule_move_resume(
        &self,
        Parameters(input): Parameters<RuleMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let journal = input.id.parse::<uuid::Uuid>().map_err(|e| {
            McpError::invalid_params(format!("invalid journal id: {e}"), None)
        })?;
        self.with_store(move |store| {
            let outcome = store.resume_move_with_journal(journal).map_err(Self::rule_err)?;
            Self::json_result(&serde_json::json!({"status":"ok","mode":"resume","journal_id":outcome.journal.id,"phase":outcome.journal.phase}))
        })
        .await
    }

    #[tool(
        name = "rule_move_rollback",
        description = "Roll back a rule move from a journal id."
    )]
    pub async fn rule_move_rollback(
        &self,
        Parameters(input): Parameters<RuleMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let journal = input.id.parse::<uuid::Uuid>().map_err(|e| {
            McpError::invalid_params(format!("invalid journal id: {e}"), None)
        })?;
        self.with_store(move |store| {
            let outcome = store.rollback_move_with_journal(journal).map_err(Self::rule_err)?;
            Self::json_result(&serde_json::json!({"status":"ok","mode":"rollback","journal_id":outcome.journal.id,"phase":outcome.journal.phase}))
        })
        .await
    }
}

#[tool_handler]
impl ServerHandler for RuleServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
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

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_validation_rejects_ambient_aliases() {
        for value in [None, Some(""), Some("default"), Some("."), Some("..")] {
            let err =
                memory_kernel::workspace::validate_explicit_workspace_selector(
                    value,
                )
                .expect_err("should reject ambient selector");
            let err_msg = err.to_string();
            assert!(
                err_msg.contains("invalid workspace selector"),
                "error should mention 'invalid workspace selector': {err_msg}"
            );
            assert!(
                err_msg.contains(
                    "entity creation requires an explicit workspace path"
                ),
                "error should state the requirement: {err_msg}"
            );
        }
    }
}

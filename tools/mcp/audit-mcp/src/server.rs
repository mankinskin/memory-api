use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content},
    schemars::{self, JsonSchema},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use audit_api::audit;
use audit_api::models::AuditConfig;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuditRepositoryInput {
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    #[serde(default)]
    pub max_file_lines: Option<usize>,
    #[serde(default)]
    pub max_cyclomatic_complexity: Option<usize>,
    #[serde(default)]
    pub coverage_warn_below: Option<f64>,
}

#[derive(Clone)]
pub struct AuditServer {
    base_dir: PathBuf,
    tool_router: ToolRouter<Self>,
    audit_lock: Arc<Mutex<()>>,
}

impl AuditServer {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            tool_router: Self::tool_router(),
            audit_lock: Arc::new(Mutex::new(())),
        }
    }

    fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|err| McpError::internal_error(format!("serialization: {err}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[tool_router]
impl AuditServer {
    #[tool(
        name = "audit",
        description = "Run a repository quality audit and return structured metrics and findings."
    )]
    async fn audit(
        &self,
        Parameters(input): Parameters<AuditRepositoryInput>,
    ) -> Result<CallToolResult, McpError> {
        let _guard = self.audit_lock.lock().await;
        let repo_root = input.repo_root.unwrap_or_else(|| self.base_dir.clone());
        let mut config = AuditConfig::default();

        if let Some(max_file_lines) = input.max_file_lines {
            config.max_file_lines = max_file_lines;
        }
        if let Some(max_cyclomatic_complexity) = input.max_cyclomatic_complexity {
            config.max_cyclomatic_complexity = max_cyclomatic_complexity;
        }
        if let Some(coverage_warn_below) = input.coverage_warn_below {
            config.coverage_warn_below = coverage_warn_below;
        }

        let report = audit::audit(&repo_root, config)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;

        Self::json_result(&report)
    }
}

#[tool_handler]
impl ServerHandler for AuditServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            instructions: Some(
                "Use audit to run one synchronized repository quality audit.".
                    to_string(),
            ),
            capabilities: rmcp::model::ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn run_mcp_server(
    base_dir: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = AuditServer::new(base_dir);

    tracing::info!("Starting audit-mcp server on stdio");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}
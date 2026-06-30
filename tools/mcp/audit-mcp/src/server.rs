use std::{
    path::PathBuf,
    sync::Arc,
};

use audit_api::{
    audit,
    index::RepositoryIndex,
    models::AuditConfig,
    summary::{
        AuditSummaryBy,
        summarize_report,
    },
};
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
    handler::server::{
        tool::ToolRouter,
        wrapper::Parameters,
    },
    model::{
        CallToolResult,
        Content,
    },
    schemars::{
        self,
        JsonSchema,
    },
    tool,
    tool_handler,
    tool_router,
    transport::stdio,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::Mutex;
use uuid::Uuid;

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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuditSummaryByInput {
    Crate,
    Package,
    Category,
    Severity,
    Metric,
    Path,
}

impl From<AuditSummaryByInput> for AuditSummaryBy {
    fn from(value: AuditSummaryByInput) -> Self {
        match value {
            AuditSummaryByInput::Crate | AuditSummaryByInput::Package =>
                AuditSummaryBy::Crate,
            AuditSummaryByInput::Category => AuditSummaryBy::Category,
            AuditSummaryByInput::Severity => AuditSummaryBy::Severity,
            AuditSummaryByInput::Metric => AuditSummaryBy::Metric,
            AuditSummaryByInput::Path => AuditSummaryBy::Path,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuditSummaryInput {
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    pub by: AuditSummaryByInput,
    #[serde(default)]
    pub max_file_lines: Option<usize>,
    #[serde(default)]
    pub max_cyclomatic_complexity: Option<usize>,
    #[serde(default)]
    pub coverage_warn_below: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuditMoveInput {
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    pub id: String,
    pub to_workspace_root: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuditMoveJournalInput {
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    pub id: String,
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

    fn repo_root(&self, repo_root: Option<PathBuf>) -> PathBuf {
        repo_root.unwrap_or_else(|| self.base_dir.clone())
    }

    fn json_result<T: Serialize>(
        value: &T
    ) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string_pretty(value).map_err(|err| {
            McpError::internal_error(format!("serialization: {err}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn build_config(
        max_file_lines: Option<usize>,
        max_cyclomatic_complexity: Option<usize>,
        coverage_warn_below: Option<f64>,
    ) -> AuditConfig {
        let mut config = AuditConfig::default();

        if let Some(max_file_lines) = max_file_lines {
            config.max_file_lines = max_file_lines;
        }
        if let Some(max_cyclomatic_complexity) = max_cyclomatic_complexity {
            config.max_cyclomatic_complexity = max_cyclomatic_complexity;
        }
        if let Some(coverage_warn_below) = coverage_warn_below {
            config.coverage_warn_below = coverage_warn_below;
        }

        config
    }

    fn move_plan_json(report: &memory_api::storage::move_kernel::MovePlan) -> serde_json::Value {
        serde_json::json!({
            "supported": report.supported(),
            "entity_id": report.entity_id,
            "source_workspace_root": path_display(&report.source_workspace_root),
            "target_workspace_root": path_display(&report.target_workspace_root),
            "source_store_root": path_display(&report.source_store_root),
            "target_store_root": path_display(&report.target_store_root),
            "source_git_worktree_root": path_display(&report.source_git_worktree_root),
            "target_git_worktree_root": path_display(&report.target_git_worktree_root),
            "git_worktree_topology": report.git_worktree_topology,
            "source_entity_path": path_display(&report.source_entity_path),
            "destination_entity_path": path_display(&report.destination_entity_path),
            "inbound_related_entity_ids": report.inbound_related_entity_ids,
            "outbound_related_entity_ids": report.outbound_related_entity_ids,
            "reference_visibility": report.reference_visibility,
            "active_board_entries": report.active_board_entries,
            "historical_board_entries": report.historical_board_entries,
            "active_leases": report.active_leases,
            "path_reference_files": report.path_reference_files,
            "blockers": report.blockers,
            "captured_at": report.captured_at,
        })
    }

    fn move_outcome_json(outcome: &memory_api::storage::move_kernel::MoveOutcome) -> serde_json::Value {
        serde_json::json!({
            "resumed": outcome.resumed,
            "rolled_back": outcome.rolled_back,
            "journal": {
                "id": outcome.journal.id,
                "entity_id": outcome.journal.entity_id,
                "source_store_root": path_display(&outcome.journal.source_store_root),
                "target_store_root": path_display(&outcome.journal.target_store_root),
                "source_entity_path": path_display(&outcome.journal.source_entity_path),
                "destination_entity_path": path_display(&outcome.journal.destination_entity_path),
                "phase": outcome.journal.phase,
                "created_at": outcome.journal.created_at,
                "updated_at": outcome.journal.updated_at,
                "steps": outcome.journal.steps,
                "rollback_steps": outcome.journal.rollback_steps,
                "lock_paths": outcome.journal.lock_paths,
                "migrated_board_entries": outcome.journal.migrated_board_entries,
                "rewritten_path_files": outcome.journal.rewritten_path_files,
                "manual_followups": outcome.journal.manual_followups,
                "failure": outcome.journal.failure,
                "next_recovery_step": outcome.journal.next_recovery_step,
            },
        })
    }
}

fn path_display(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
        let repo_root =
            input.repo_root.unwrap_or_else(|| self.base_dir.clone());
        let config = Self::build_config(
            input.max_file_lines,
            input.max_cyclomatic_complexity,
            input.coverage_warn_below,
        );

        let report = audit::audit(&repo_root, config)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;

        Self::json_result(&report)
    }

    #[tool(
        name = "audit_summary",
        description = "Run a repository quality audit and return grouped issue counts."
    )]
    async fn audit_summary(
        &self,
        Parameters(input): Parameters<AuditSummaryInput>,
    ) -> Result<CallToolResult, McpError> {
        let _guard = self.audit_lock.lock().await;
        let repo_root =
            input.repo_root.unwrap_or_else(|| self.base_dir.clone());
        let config = Self::build_config(
            input.max_file_lines,
            input.max_cyclomatic_complexity,
            input.coverage_warn_below,
        );

        let report = audit::audit(&repo_root, config)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        let summary = summarize_report(&report, input.by.into())
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;

        Self::json_result(&summary)
    }

    #[tool(
        name = "audit_move_preflight",
        description = "Read-only preflight plan for moving an audit repository root to another workspace store."
    )]
    async fn audit_move_preflight(
        &self,
        Parameters(input): Parameters<AuditMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let _guard = self.audit_lock.lock().await;
        let repo_root = self.repo_root(input.repo_root);
        let audit_id = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid audit UUID: {error}"), None)
        })?;
        let target_workspace_root = PathBuf::from(input.to_workspace_root);
        let report = RepositoryIndex::open(&repo_root)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
            .plan_move_preflight(&audit_id, &target_workspace_root)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": if report.supported() { "ok" } else { "blocked" },
            "mode": "preflight",
            "repo_root": path_display(&repo_root),
            "id": audit_id,
            "plan": Self::move_plan_json(&report),
            "recovery": {"resume": "audit move --resume <journal-uuid>", "rollback": "audit move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "audit_move_apply",
        description = "Execute a supported audit move to another workspace store."
    )]
    async fn audit_move_apply(
        &self,
        Parameters(input): Parameters<AuditMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let _guard = self.audit_lock.lock().await;
        let repo_root = self.repo_root(input.repo_root);
        let audit_id = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid audit UUID: {error}"), None)
        })?;
        let target_workspace_root = PathBuf::from(input.to_workspace_root);
        let index = RepositoryIndex::open(&repo_root)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        let report = index
            .plan_move_preflight(&audit_id, &target_workspace_root)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        if !report.supported() {
            return Err(McpError::invalid_params(
                "move preflight blocked; run audit_move_preflight for details".to_string(),
                None,
            ));
        }
        let outcome = index
            .execute_move_with_journal(&report)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "apply",
            "repo_root": path_display(&repo_root),
            "id": audit_id,
            "plan": Self::move_plan_json(&report),
            "outcome": Self::move_outcome_json(&outcome),
            "recovery": {"resume": "audit move --resume <journal-uuid>", "rollback": "audit move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "audit_move_resume",
        description = "Resume an interrupted audit move from a journal id."
    )]
    async fn audit_move_resume(
        &self,
        Parameters(input): Parameters<AuditMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let _guard = self.audit_lock.lock().await;
        let repo_root = self.repo_root(input.repo_root);
        let journal = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid journal id: {error}"), None)
        })?;
        let outcome = RepositoryIndex::open(&repo_root)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
            .resume_move_with_journal(journal)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "resume",
            "repo_root": path_display(&repo_root),
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": {"resume": "audit move --resume <journal-uuid>", "rollback": "audit move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "audit_move_rollback",
        description = "Roll back an audit move from a journal id."
    )]
    async fn audit_move_rollback(
        &self,
        Parameters(input): Parameters<AuditMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let _guard = self.audit_lock.lock().await;
        let repo_root = self.repo_root(input.repo_root);
        let journal = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid journal id: {error}"), None)
        })?;
        let outcome = RepositoryIndex::open(&repo_root)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
            .rollback_move_with_journal(journal)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "rollback",
            "repo_root": path_display(&repo_root),
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": {"resume": "audit move --resume <journal-uuid>", "rollback": "audit move --rollback <journal-uuid>"},
        }))
    }
}

#[tool_handler]
impl ServerHandler for AuditServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            instructions: Some(
                "Use audit for the full report, audit_summary for grouped issue counts, or audit_move_* for move preflight/apply/resume/rollback.".
                    to_string(),
            ),
            capabilities: rmcp::model::ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

pub async fn run_mcp_server(
    base_dir: PathBuf
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = AuditServer::new(base_dir);

    tracing::info!("Starting audit-mcp server on stdio");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;
    use tempfile::TempDir;

    use super::{
        AuditMoveInput,
        AuditServer,
    };

    fn run_git(repo_root: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed: {status}");
    }

    fn extract_json(result: rmcp::model::CallToolResult) -> Value {
        let text = result
            .content
            .iter()
            .find_map(|content| {
                if let rmcp::model::RawContent::Text(text) = &content.raw {
                    Some(text.text.clone())
                } else {
                    None
                }
            })
            .expect("text content");
        serde_json::from_str(&text).expect("parse json")
    }

    #[tokio::test]
    async fn move_preflight_is_blocked_for_repository_level_audit_storage() {
        let tmp = TempDir::new().expect("tempdir");
        let repo_root = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("repo root");
        run_git(&repo_root, &["init"]);

        let source_workspace = repo_root.join("source-workspace");
        let target_workspace = repo_root.join("target-workspace");
        std::fs::create_dir_all(source_workspace.join(".audit")).expect("source audit dir");
        std::fs::create_dir_all(target_workspace.join(".audit")).expect("target audit dir");

        let server = AuditServer::new(source_workspace.clone());
        let result = server
            .audit_move_preflight(Parameters(AuditMoveInput {
                repo_root: Some(source_workspace.clone()),
                id: "7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71".to_string(),
                to_workspace_root: target_workspace.to_string_lossy().to_string(),
            }))
            .await
            .expect("audit move preflight");
        let json = extract_json(result);

        assert_eq!(json["status"], "blocked");
        assert_eq!(json["mode"], "preflight");
        assert!(json["plan"]["blockers"].as_array().unwrap().len() > 0);
    }
}

use std::path::PathBuf;

use rmcp::{
    handler::server::{
        tool::ToolRouter,
        wrapper::Parameters,
    },
    model::*,
    schemars::{
        self,
        JsonSchema,
    },
    tool,
    tool_handler,
    tool_router,
    transport::stdio,
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
};
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use memory_api::workspace;
use session_api::{
    SessionError,
    SessionQuery,
    SessionStoreConfig,
    SessionWorktreeCheckInRequest,
    DEFAULT_SKELETON_PREVIEW_CHARS,
};

// ── Input types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckInInput {
    /// Session id to check in.
    pub session_id: String,
    /// Owner (agent) identity claiming the worktree.
    pub owner_id: String,
    /// Ticket the session is working on.
    pub ticket_id: String,
    /// Assigned worktree working directory.
    pub worktree_path: String,
    /// Branch checked out in the worktree.
    pub branch: String,
    /// Predecessor session id when rotating from a prior assignment.
    #[serde(default)]
    pub predecessor_session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LookupInput {
    /// Session id to look up.
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryInput {
    /// Filter by session id prefix.
    #[serde(default)]
    pub session_id_prefix: Option<String>,
    /// Filter by conversation id.
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// Filter by agent id.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Free-text filter across session content.
    #[serde(default)]
    pub text: Option<String>,
    /// Maximum number of sessions to return.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeekRangeInput {
    /// Session id to peek.
    pub session_id: String,
    /// Inclusive start turn index (0-based).
    #[serde(default)]
    pub start: usize,
    /// Exclusive end turn index (0-based). Defaults to the end of the transcript.
    #[serde(default)]
    pub end: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeekSkeletonInput {
    /// Session id to peek.
    pub session_id: String,
    /// Maximum preview characters retained per turn.
    #[serde(default)]
    pub preview_chars: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionMoveInput {
    /// Session UUID to move.
    pub id: String,
    /// Destination workspace root.
    pub to_workspace_root: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionMoveJournalInput {
    /// Move journal UUID.
    pub id: String,
}

// ── Server ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SessionServer {
    store_root: PathBuf,
    workspace_slug: String,
    tool_router: ToolRouter<Self>,
}

impl SessionServer {
    pub fn new(
        store_root: PathBuf,
        workspace_slug: String,
    ) -> Self {
        Self {
            store_root,
            workspace_slug,
            tool_router: Self::tool_router(),
        }
    }

    fn config(&self) -> SessionStoreConfig {
        SessionStoreConfig::new(self.store_root.clone(), self.workspace_slug.clone())
    }

    fn json_result<T: Serialize>(value: &T) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string(value).map_err(|err| {
            McpError::internal_error(format!("serialization: {err}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn session_err(err: SessionError) -> McpError {
        match &err {
            SessionError::NotFound { .. }
            | SessionError::MissingSessionId
            | SessionError::MissingOwnerId
            | SessionError::MissingTicketId
            | SessionError::EmptyWorktreePath
            | SessionError::EmptyWorktreeBranch
            | SessionError::InvalidSessionId(_)
            | SessionError::InvalidWorkspaceSlug(_)
            | SessionError::MissingWorktreeAssignment { .. }
            | SessionError::SessionOwnershipMismatch { .. }
            | SessionError::WorktreeConflict { .. }
            | SessionError::CrossSessionReuseRequiresAdopt { .. }
            | SessionError::Move(_) => {
                McpError::invalid_params(err.to_string(), None)
            },
            _ => McpError::internal_error(format!("session error: {err}"), None),
        }
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

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl SessionServer {
    #[tool(
        name = "session_check_in",
        description = "Check a session into its authoritative worktree assignment and return the resolved receipt."
    )]
    pub async fn session_check_in(
        &self,
        Parameters(input): Parameters<CheckInInput>,
    ) -> Result<CallToolResult, McpError> {
        let receipt = self
            .config()
            .check_in_worktree(SessionWorktreeCheckInRequest {
                session_id: input.session_id,
                owner_id: input.owner_id,
                ticket_id: input.ticket_id,
                worktree_path: PathBuf::from(input.worktree_path),
                branch: input.branch,
                predecessor_session_id: input.predecessor_session_id,
            })
            .map_err(Self::session_err)?;
        Self::json_result(&receipt)
    }

    #[tool(
        name = "session_lookup",
        description = "Look up the authoritative worktree assignment for a session."
    )]
    pub async fn session_lookup(
        &self,
        Parameters(input): Parameters<LookupInput>,
    ) -> Result<CallToolResult, McpError> {
        let receipt = self
            .config()
            .lookup_worktree(&input.session_id)
            .map_err(Self::session_err)?;
        Self::json_result(&receipt)
    }

    #[tool(
        name = "session_query",
        description = "Query stored sessions with optional id-prefix, conversation, agent, text, and limit filters."
    )]
    pub async fn session_query(
        &self,
        Parameters(input): Parameters<QueryInput>,
    ) -> Result<CallToolResult, McpError> {
        let query = SessionQuery {
            session_id_prefix: input.session_id_prefix,
            conversation_id: input.conversation_id,
            agent_id: input.agent_id,
            text: input.text,
            limit: input.limit,
        };
        let sessions = self
            .config()
            .query_sessions(&query)
            .map_err(Self::session_err)?;
        Self::json_result(&serde_json::json!({
            "count": sessions.len(),
            "sessions": sessions,
        }))
    }

    #[tool(
        name = "session_peek_range",
        description = "Peek a bounded window of transcript turns for a session."
    )]
    pub async fn session_peek_range(
        &self,
        Parameters(input): Parameters<PeekRangeInput>,
    ) -> Result<CallToolResult, McpError> {
        let range = self
            .config()
            .peek_range(&input.session_id, input.start, input.end)
            .map_err(Self::session_err)?;
        Self::json_result(&range)
    }

    #[tool(
        name = "session_peek_skeleton",
        description = "Peek a body-stripped skeleton overview of a session transcript."
    )]
    pub async fn session_peek_skeleton(
        &self,
        Parameters(input): Parameters<PeekSkeletonInput>,
    ) -> Result<CallToolResult, McpError> {
        let preview_chars = input.preview_chars.unwrap_or(DEFAULT_SKELETON_PREVIEW_CHARS);
        let skeleton = self
            .config()
            .peek_skeleton(&input.session_id, preview_chars)
            .map_err(Self::session_err)?;
        Self::json_result(&skeleton)
    }

    #[tool(
        name = "session_move_preflight",
        description = "Read-only preflight plan for moving a session to another workspace store."
    )]
    pub async fn session_move_preflight(
        &self,
        Parameters(input): Parameters<SessionMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let session_id = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid session UUID: {error}"), None)
        })?;
        let target_workspace_root = workspace::canonicalize_workspace_root(
            std::path::Path::new(&input.to_workspace_root),
        );
        let report = self
            .config()
            .plan_move_preflight(&session_id, &target_workspace_root)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": if report.supported() { "ok" } else { "blocked" },
            "mode": "preflight",
            "id": session_id,
            "plan": Self::move_plan_json(&report),
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "session_move_apply",
        description = "Execute a supported session move to another workspace store."
    )]
    pub async fn session_move_apply(
        &self,
        Parameters(input): Parameters<SessionMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let session_id = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid session UUID: {error}"), None)
        })?;
        let target_workspace_root = workspace::canonicalize_workspace_root(
            std::path::Path::new(&input.to_workspace_root),
        );
        let report = self
            .config()
            .plan_move_preflight(&session_id, &target_workspace_root)
            .map_err(Self::session_err)?;
        if !report.supported() {
            return Err(McpError::invalid_params(
                "move preflight blocked; run session_move_preflight for details".to_string(),
                None,
            ));
        }
        let outcome = self
            .config()
            .execute_move_with_journal(&report)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "apply",
            "id": session_id,
            "plan": Self::move_plan_json(&report),
            "outcome": Self::move_outcome_json(&outcome),
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "session_move_resume",
        description = "Resume an interrupted session move from a journal id."
    )]
    pub async fn session_move_resume(
        &self,
        Parameters(input): Parameters<SessionMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let journal = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid journal id: {error}"), None)
        })?;
        let outcome = self
            .config()
            .resume_move_with_journal(journal)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "resume",
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "session_move_rollback",
        description = "Roll back a session move from a journal id."
    )]
    pub async fn session_move_rollback(
        &self,
        Parameters(input): Parameters<SessionMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let journal = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(format!("invalid journal id: {error}"), None)
        })?;
        let outcome = self
            .config()
            .rollback_move_with_journal(journal)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "rollback",
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }
}

// ── MCP handler trait ─────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for SessionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "session-mcp provides direct access to the session store. Use named tools for session worktree check-in, lookup, query, move, and transcript peeking."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Server startup ────────────────────────────────────────────────────────────

pub async fn run_mcp_server(
    store_root: PathBuf,
    workspace_slug: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = SessionServer::new(store_root, workspace_slug);

    tracing::info!("Starting session-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use std::process::Command;
    use serde_json::Value;
    use session_api::{
        CopilotHookMessage,
        CopilotHookPayload,
        SessionError,
        SessionCaptureRequest,
        SessionRole,
        SessionStoreConfig,
    };
    use tempfile::tempdir;

    use super::*;

    fn seed(
        config: &SessionStoreConfig,
        session_id: &str,
        agent: &str,
    ) {
        let payload = CopilotHookPayload {
            session_id: session_id.to_string(),
            workspace_slug: "default".to_string(),
            captured_at: Utc::now(),
            conversation_id: None,
            agent_id: Some(agent.to_string()),
            model: None,
            trigger: None,
            messages: vec![CopilotHookMessage {
                role: SessionRole::User,
                content: "alpha body\nbeta".to_string(),
                tool_name: None,
                captured_at: None,
            }],
        };
        config
            .persist_capture(SessionCaptureRequest::copilot(payload))
            .expect("seed");
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
    async fn check_in_then_lookup() {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join(".memory-api");
        let worktree = dir.path().join("wt");
        let server = SessionServer::new(store_root, "default".to_string());

        let receipt = server
            .session_check_in(Parameters(CheckInInput {
                session_id: "s1".to_string(),
                owner_id: "agent".to_string(),
                ticket_id: "t1".to_string(),
                worktree_path: worktree.to_string_lossy().to_string(),
                branch: "feature/x".to_string(),
                predecessor_session_id: None,
            }))
            .await
            .expect("check-in");
        assert!(!receipt.is_error.unwrap_or(false));

        let lookup = server
            .session_lookup(Parameters(LookupInput {
                session_id: "s1".to_string(),
            }))
            .await
            .expect("lookup");
        assert!(!lookup.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn query_and_peek() {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join(".memory-api");
        let server = SessionServer::new(store_root.clone(), "default".to_string());
        let config = SessionStoreConfig::new(store_root, "default".to_string());
        seed(&config, "s2", "agent-2");

        let query = server
            .session_query(Parameters(QueryInput {
                session_id_prefix: None,
                conversation_id: None,
                agent_id: Some("agent-2".to_string()),
                text: None,
                limit: None,
            }))
            .await
            .expect("query");
        assert!(!query.is_error.unwrap_or(false));

        let skeleton = server
            .session_peek_skeleton(Parameters(PeekSkeletonInput {
                session_id: "s2".to_string(),
                preview_chars: None,
            }))
            .await
            .expect("skeleton");
        assert!(!skeleton.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn move_preflight_and_apply_roundtrip() {
        let temp = tempdir().unwrap();
        let repo_root = temp.path().join("repo");
        std::fs::create_dir_all(&repo_root).unwrap();
        Command::new("git")
            .current_dir(&repo_root)
            .args(["init"])
            .status()
            .expect("git init")
            .success()
            .then_some(())
            .expect("git init failed");

        let source_store_root = repo_root.join(".memory-api");
        std::fs::create_dir_all(&source_store_root).unwrap();
        let target_workspace_root = repo_root.join("target-workspace");
        std::fs::create_dir_all(target_workspace_root.join(".session")).unwrap();

        let session_id = "7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71";
        let config = SessionStoreConfig::new(source_store_root.clone(), "default".to_string());
        seed(&config, session_id, "agent-3");

        let server = SessionServer::new(source_store_root.clone(), "default".to_string());
        let preflight = server
            .session_move_preflight(Parameters(SessionMoveInput {
                id: session_id.to_string(),
                to_workspace_root: target_workspace_root.to_string_lossy().to_string(),
            }))
            .await
            .expect("move preflight");
        let preflight_json = extract_json(preflight);
        assert_eq!(preflight_json["status"], "ok");
        assert_eq!(preflight_json["mode"], "preflight");
        assert!(preflight_json["plan"]["supported"].as_bool().unwrap());

        let apply = server
            .session_move_apply(Parameters(SessionMoveInput {
                id: session_id.to_string(),
                to_workspace_root: target_workspace_root.to_string_lossy().to_string(),
            }))
            .await
            .expect("move apply");
        let apply_json = extract_json(apply);
        assert_eq!(apply_json["status"], "ok");
        assert_eq!(apply_json["mode"], "apply");
        assert!(apply_json["outcome"]["journal"]["id"].is_string());

        let target_config = SessionStoreConfig::new(
            target_workspace_root.join(".session"),
            "default".to_string(),
        );
        assert!(matches!(
            config.read_session(session_id),
            Err(SessionError::NotFound { .. })
        ));
        assert_eq!(
            target_config.read_session(session_id).unwrap().session_id,
            session_id
        );
    }
}

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
        let text = serde_json::to_string_pretty(value).map_err(|err| {
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
            | SessionError::CrossSessionReuseRequiresAdopt { .. } => {
                McpError::invalid_params(err.to_string(), None)
            },
            _ => McpError::internal_error(format!("session error: {err}"), None),
        }
    }
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
}

// ── MCP handler trait ─────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for SessionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "session-mcp provides direct access to the session store. Use named tools for session worktree check-in, lookup, query, and transcript peeking."
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
    use session_api::{
        CopilotHookMessage,
        CopilotHookPayload,
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
}

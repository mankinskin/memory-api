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
use uuid::Uuid;

use ticket_api::storage::store::TicketStore;

mod board;
mod graph;
mod health;
mod mutations;
mod next_tickets;
mod query;
mod types;
mod workflow;

pub use self::types::*;

#[derive(Clone)]
pub struct TicketServer {
    index_root: PathBuf,
    tool_router: ToolRouter<Self>,
    store_lock: Arc<Mutex<()>>,
}

impl TicketServer {
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
        let mut value = serde_json::to_value(value).map_err(|error| {
            McpError::internal_error(format!("serialization: {error}"), None)
        })?;
        ticket_api::output::strip_default_metadata(&mut value);
        let text = serde_json::to_string_pretty(&value).map_err(|error| {
            McpError::internal_error(format!("serialization: {error}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn store_err(error: ticket_api::error::StorageError) -> McpError {
        McpError::internal_error(format!("store error: {error}"), None)
    }

    fn board_err(error: ticket_api::BoardError) -> McpError {
        match error {
            ticket_api::BoardError::Storage(storage_error) =>
                Self::store_err(storage_error),
            other => McpError::invalid_params(other.to_string(), None),
        }
    }

    fn is_ticket_store_root(path: &Path) -> bool {
        path.join("tickets").is_dir()
            || path.join("tickets.db").is_file()
            || path.join("search_index").is_dir()
    }

    fn resolve_workspace_root(
        &self,
        workspace: &str,
    ) -> Result<PathBuf, McpError> {
        let workspace = workspace.trim();
        if workspace.is_empty() || workspace == "default" {
            return Ok(self.index_root.clone());
        }

        let resolved = ticket_api::workspace::resolve_store_root_from(
            Path::new(workspace),
            ticket_api::workspace::TICKET_INDEX_DIR,
        );
        if resolved.file_name().and_then(|name| name.to_str())
            == Some(ticket_api::workspace::TICKET_INDEX_DIR)
            || Self::is_ticket_store_root(&resolved)
        {
            return Ok(resolved);
        }

        Err(McpError::invalid_params(
            format!(
                "invalid workspace '{workspace}': expected 'default', a repo root containing .ticket, the .ticket store itself, a path inside that store, or an existing ticket store root"
            ),
            None,
        ))
    }

    fn resolve_uuid_with(
        store: &TicketStore,
        value: &str,
    ) -> Result<Uuid, McpError> {
        if let Ok(id) = value.parse::<Uuid>() {
            return Ok(id);
        }

        let trimmed = value.trim();
        if trimmed.len() >= 8
            && trimmed.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            let tickets =
                store.list(None, None, None).map_err(Self::store_err)?;
            let prefix_lower = trimmed.to_ascii_lowercase();
            let matches: Vec<Uuid> = tickets
                .iter()
                .filter(|ticket| {
                    ticket.id.simple().to_string().starts_with(&prefix_lower)
                })
                .map(|ticket| ticket.id)
                .collect();

            return match matches.len() {
                1 => Ok(matches[0]),
                0 => Err(McpError::invalid_params(
                    format!("no ticket found matching prefix '{trimmed}'"),
                    None,
                )),
                count => Err(McpError::invalid_params(
                    format!(
                        "ambiguous prefix '{trimmed}': matches {count} tickets"
                    ),
                    None,
                )),
            };
        }

        Err(McpError::invalid_params(
            format!(
                "invalid UUID '{value}': expected full UUID or hex prefix (>= 8 chars)"
            ),
            None,
        ))
    }

    async fn with_store<T>(
        &self,
        workspace: &str,
        f: impl FnOnce(&TicketStore) -> Result<T, ticket_api::error::StorageError>,
    ) -> Result<T, McpError> {
        let index_root = self.resolve_workspace_root(workspace)?;
        let _guard = self.store_lock.lock().await;
        let store = TicketStore::open(&index_root).map_err(Self::store_err)?;
        let result = f(&store).map_err(Self::store_err);
        drop(store);
        result
    }

    async fn with_store_ext<T>(
        &self,
        workspace: &str,
        f: impl FnOnce(&TicketStore) -> Result<T, McpError>,
    ) -> Result<T, McpError> {
        let index_root = self.resolve_workspace_root(workspace)?;
        let _guard = self.store_lock.lock().await;
        let store = TicketStore::open(&index_root).map_err(Self::store_err)?;
        let result = f(&store);
        drop(store);
        result
    }
}

#[tool_router]
impl TicketServer {
    #[tool(
        name = "health",
        description = "Check that the ticket store is accessible."
    )]
    async fn health(&self) -> Result<CallToolResult, McpError> {
        self.health_tool().await
    }

    #[tool(
        name = "list_workspaces",
        description = "List available ticket workspaces."
    )]
    async fn list_workspaces(&self) -> Result<CallToolResult, McpError> {
        self.list_workspaces_tool().await
    }

    #[tool(
        name = "list_tickets",
        description = "List tickets with optional state/query/limit filters."
    )]
    async fn list_tickets(
        &self,
        Parameters(input): Parameters<ListTicketsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.list_tickets_tool(input).await
    }

    #[tool(name = "get_ticket", description = "Get one ticket by id.")]
    async fn get_ticket(
        &self,
        Parameters(input): Parameters<TicketRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.get_ticket_tool(input).await
    }

    #[tool(
        name = "get_ticket_description",
        description = "Get ticket markdown description by id."
    )]
    async fn get_ticket_description(
        &self,
        Parameters(input): Parameters<TicketRefInput>,
    ) -> Result<CallToolResult, McpError> {
        self.get_ticket_description_tool(input).await
    }

    #[tool(
        name = "list_edges",
        description = "List ticket graph edges, optionally filtered by edge kind."
    )]
    async fn list_edges(
        &self,
        Parameters(input): Parameters<ListEdgesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.list_edges_tool(input).await
    }

    #[tool(
        name = "subgraph",
        description = "Fetch dependency subgraph for a root ticket via BFS traversal."
    )]
    async fn subgraph(
        &self,
        Parameters(input): Parameters<SubgraphInput>,
    ) -> Result<CallToolResult, McpError> {
        self.bfs_graph(
            input.workspace,
            &input.root,
            input.direction.as_deref().unwrap_or("both"),
            input.edge_kind.as_deref(),
            input.depth,
            input.limit_nodes,
            input.limit_edges,
        )
        .await
    }

    #[tool(
        name = "topgraph",
        description = "Fetch reverse dependency graph (tickets that depend on the root) via BFS traversal."
    )]
    async fn topgraph(
        &self,
        Parameters(input): Parameters<TopgraphInput>,
    ) -> Result<CallToolResult, McpError> {
        self.bfs_graph(
            input.workspace,
            &input.root,
            input.direction.as_deref().unwrap_or("in"),
            input.edge_kind.as_deref(),
            input.depth,
            input.limit_nodes,
            input.limit_edges,
        )
        .await
    }

    #[tool(
        name = "next_tickets",
        description = "List unblocked tickets in any non-terminal state whose dependencies are all satisfied, ordered by workflow convergence pressure, then ascending effort (smaller token-budget work first), then recency, priority, state progress, and dependee count. Active or stale board entries are surfaced through exclusions and warnings; use board_show for the full board snapshot."
    )]
    pub async fn next_tickets(
        &self,
        Parameters(input): Parameters<NextTicketsInput>,
    ) -> Result<CallToolResult, McpError> {
        self.next_tickets_tool(input).await
    }

    #[tool(
        name = "health_check",
        description = "Run health checks on tickets: validates descriptions, titles, dependency state consistency, and dangling edges. Scope by root (BFS subgraph), explicit IDs, or all tickets."
    )]
    async fn health_check(
        &self,
        Parameters(input): Parameters<HealthCheckInput>,
    ) -> Result<CallToolResult, McpError> {
        self.run_health_checks(
            &input.workspace,
            input.root.as_deref(),
            input.all,
            &input.ids,
            input.depth,
            input.direction.as_deref(),
        )
        .await
    }

    #[tool(
        name = "update_ticket",
        description = "Update a ticket: apply field patches and/or transition state. Set undo=true to revert to the previous history revision."
    )]
    pub async fn update_ticket(
        &self,
        Parameters(input): Parameters<UpdateTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        self.update_ticket_tool(input).await
    }

    #[tool(
        name = "close_ticket",
        description = "Fast-forward a ticket to a target state by traversing all intermediate transitions (default: done)."
    )]
    async fn close_ticket(
        &self,
        Parameters(input): Parameters<CloseTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        self.close_ticket_tool(input).await
    }

    #[tool(
        name = "cancel_ticket",
        description = "Cancel a ticket (fast-forward to 'cancelled' state)."
    )]
    async fn cancel_ticket(
        &self,
        Parameters(input): Parameters<CancelTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        self.cancel_ticket_tool(input).await
    }

    #[tool(
        name = "create_ticket",
        description = "Create a new ticket with the given type, optional title, state, fields, and description."
    )]
    async fn create_ticket(
        &self,
        Parameters(input): Parameters<CreateTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        self.create_ticket_tool(input).await
    }

    #[tool(
        name = "delete_ticket",
        description = "Delete a ticket permanently, removing its folder from disk."
    )]
    async fn delete_ticket(
        &self,
        Parameters(input): Parameters<DeleteTicketInput>,
    ) -> Result<CallToolResult, McpError> {
        self.delete_ticket_tool(input).await
    }

    #[tool(
        name = "add_edge",
        description = "Add a directed edge between two tickets (e.g. depends_on, linked)."
    )]
    async fn add_edge(
        &self,
        Parameters(input): Parameters<AddEdgeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.add_edge_tool(input).await
    }

    #[tool(
        name = "remove_edge",
        description = "Remove a directed edge between two tickets."
    )]
    async fn remove_edge(
        &self,
        Parameters(input): Parameters<RemoveEdgeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.remove_edge_tool(input).await
    }

    #[tool(
        name = "move_preflight",
        description = "Run move planning / dry-run for a cross-workspace ticket move and return structured blockers, reference visibility, and touched paths."
    )]
    pub async fn move_preflight(
        &self,
        Parameters(input): Parameters<MovePreflightInput>,
    ) -> Result<CallToolResult, McpError> {
        self.move_preflight_tool(input).await
    }

    #[tool(
        name = "move_apply",
        description = "Execute a supported cross-workspace ticket move using the shared journaled storage primitive."
    )]
    pub async fn move_apply(
        &self,
        Parameters(input): Parameters<MoveApplyInput>,
    ) -> Result<CallToolResult, McpError> {
        self.move_apply_tool(input).await
    }

    #[tool(
        name = "move_resume",
        description = "Resume an interrupted move from a move journal UUID."
    )]
    pub async fn move_resume(
        &self,
        Parameters(input): Parameters<MoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        self.move_resume_tool(input).await
    }

    #[tool(
        name = "move_rollback",
        description = "Roll back a move from a move journal UUID."
    )]
    pub async fn move_rollback(
        &self,
        Parameters(input): Parameters<MoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        self.move_rollback_tool(input).await
    }

    #[tool(
        name = "workflow",
        description = "Show ready-to-run ticket MCP call sequences for common tasks."
    )]
    async fn workflow(
        &self,
        Parameters(input): Parameters<WorkflowInput>,
    ) -> Result<CallToolResult, McpError> {
        self.workflow_tool(input).await
    }

    #[tool(
        name = "board_show",
        description = "Read the current draftboard snapshot. Completed history is excluded. When agent_id is supplied, performs a follow-up heartbeat for the caller's active entries and returns the refreshed entry alongside the snapshot."
    )]
    pub async fn board_show(
        &self,
        Parameters(input): Parameters<BoardShowInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_show_tool(input).await
    }

    #[tool(
        name = "board_history",
        description = "Read recently completed board history separately from the live draftboard. Uses the configured completed_audit_window_secs as the default history window."
    )]
    pub async fn board_history(
        &self,
        Parameters(input): Parameters<BoardHistoryInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_history_tool(input).await
    }

    #[tool(
        name = "board_check_in",
        description = "Register an agent as actively working on a ticket. Returns the new board entry. Fails with WIP limit or file conflict errors."
    )]
    pub async fn board_check_in(
        &self,
        Parameters(input): Parameters<BoardCheckInInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_check_in_tool(input).await
    }

    #[tool(
        name = "board_check_out",
        description = "Remove an agent from the draftboard for the given ticket. If agent_id is omitted, the first active entry for the ticket is used."
    )]
    pub async fn board_check_out(
        &self,
        Parameters(input): Parameters<BoardCheckOutInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_check_out_tool(input).await
    }

    #[tool(
        name = "board_heartbeat",
        description = "Refresh the TTL for a board entry to prevent it from going stale. Returns the updated entry with a refreshed last_heartbeat timestamp."
    )]
    pub async fn board_heartbeat(
        &self,
        Parameters(input): Parameters<BoardHeartbeatInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_heartbeat_tool(input).await
    }

    #[tool(
        name = "board_configure",
        description = "Read or update the board configuration. Omit all optional fields to read the current config. Provide any field to patch and persist the updated config."
    )]
    pub async fn board_configure(
        &self,
        Parameters(input): Parameters<BoardConfigureInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_configure_tool(input).await
    }

    #[tool(
        name = "board_clean_preview",
        description = "Preview which board entries would be pruned by a clean operation. Returns a list of candidates and a confirmation token to pass to board_clean_apply."
    )]
    pub async fn board_clean_preview(
        &self,
        Parameters(input): Parameters<BoardCleanPreviewInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_clean_preview_tool(input).await
    }

    #[tool(
        name = "board_clean_apply",
        description = "Execute a board cleanup using the token obtained from board_clean_preview. Rejects the token if the board has changed materially since the preview."
    )]
    pub async fn board_clean_apply(
        &self,
        Parameters(input): Parameters<BoardCleanApplyInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_clean_apply_tool(input).await
    }

    #[tool(
        name = "board_update_files",
        description = "Add or remove files from an active board entry's owned_files. Conflict detection runs on newly added files."
    )]
    pub async fn board_update_files(
        &self,
        Parameters(input): Parameters<BoardUpdateFilesInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_update_files_tool(input).await
    }

    #[tool(
        name = "board_rename_file",
        description = "Atomically rename a file in an active board entry's owned_files: releases the old path and claims the new path in one audited operation."
    )]
    pub async fn board_rename_file(
        &self,
        Parameters(input): Parameters<BoardRenameFileInput>,
    ) -> Result<CallToolResult, McpError> {
        self.board_rename_file_tool(input).await
    }

    #[tool(
        name = "help",
        description = "List ticket-mcp tools and their parameters."
    )]
    async fn help(&self) -> Result<CallToolResult, McpError> {
        self.help_tool().await
    }
}

#[tool_handler]
impl ServerHandler for TicketServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "ticket-mcp provides direct access to the ticket store. No HTTP backend required. Use named tools for ticket operations."
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
    let server = TicketServer::new(index_root);

    tracing::info!("Starting ticket-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|error| {
        eprintln!("Server error: {error:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

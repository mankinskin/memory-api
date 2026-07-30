use super::{
    types::*,
    *,
};

impl TicketServer {
    pub(crate) async fn workflow_tool(
        &self,
        input: WorkflowInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace =
            input.workspace.unwrap_or_else(|| "default".to_string());
        let id = input.id.unwrap_or_else(|| "<ticket-id>".to_string());
        let query = input.query.unwrap_or_else(|| "<query>".to_string());

        let payload = match input.name {
            WorkflowName::List => serde_json::json!({
                "available": [
                    "triage_open_tickets",
                    "fetch_ticket_context",
                    "inspect_dependencies"
                ],
                "note": "Use one of the named workflows to get an ordered sequence of tool calls."
            }),
            WorkflowName::TriageOpenTickets => serde_json::json!({
                "name": "triage_open_tickets",
                "steps": [
                    {"tool": "health", "input": {}},
                    {"tool": "list_workspaces", "input": {}},
                    {"tool": "list_tickets", "input": {"workspace": workspace, "state": "open", "limit": 50}},
                    {"tool": "list_tickets", "input": {"workspace": workspace, "state": "in-implementation", "limit": 50}}
                ]
            }),
            WorkflowName::FetchTicketContext => serde_json::json!({
                "name": "fetch_ticket_context",
                "steps": [
                    {"tool": "get_ticket", "input": {"workspace": workspace, "id": id}},
                    {"tool": "get_ticket_description", "input": {"workspace": workspace, "id": id}},
                    {"tool": "list_edges", "input": {"workspace": workspace}},
                    {"tool": "subgraph", "input": {"workspace": workspace, "root": id, "depth": 2}}
                ]
            }),
            WorkflowName::InspectDependencies => serde_json::json!({
                "name": "inspect_dependencies",
                "steps": [
                    {"tool": "list_tickets", "input": {"workspace": workspace, "query": query, "limit": 20}},
                    {"tool": "list_edges", "input": {"workspace": workspace, "kind": "depends_on"}},
                    {"tool": "subgraph", "input": {"workspace": workspace, "root": id, "direction": "both", "depth": 3}}
                ]
            }),
        };

        Self::json_result(&payload)
    }

    pub(crate) async fn help_tool(&self) -> Result<CallToolResult, McpError> {
        let payload = serde_json::json!({
            "mode": "direct (no HTTP backend required)",
            "tools": [
                "health",
                "list_workspaces",
                "list_tickets",
                "get_ticket",
                "get_ticket_description",
                "create_ticket",
                "delete_ticket",
                "list_edges",
                "add_edge",
                "remove_edge",
                "prune_dangling_edges",
                "subgraph",
                "topgraph",
                "health_check",
                "update_ticket",
                "close_ticket",
                "cancel_ticket",
                "workflow",
                "next_tickets",
                "board_show",
                "board_history",
                "board_check_in",
                "board_check_out",
                "board_release_lease",
                "board_heartbeat",
                "board_configure",
                "board_clean_preview",
                "board_clean_apply",
                "board_update_files",
                "board_rename_file",
                "list_parts",
                "get_part",
                "write_part",
                "write_amendment",
                "undo_part"
            ],
            "operations": {
                "health": {
                    "description": "Check store is accessible",
                    "required": [],
                },
                "list_workspaces": {
                    "description": "List available workspaces",
                    "required": [],
                },
                "list_tickets": {
                    "description": "List/search tickets",
                    "required": ["workspace"],
                    "optional": ["state", "type", "query", "limit"],
                },
                "get_ticket": {
                    "description": "Get full ticket manifest",
                    "required": ["workspace", "id"],
                },
                "get_ticket_description": {
                    "description": "Get ticket markdown description",
                    "required": ["workspace", "id"],
                },
                "create_ticket": {
                    "description": "Create a new ticket",
                    "required": ["workspace", "type"],
                    "optional": ["title", "state", "fields", "description"],
                },
                "delete_ticket": {
                    "description": "Permanently delete a ticket",
                    "required": ["workspace", "id"],
                },
                "list_edges": {
                    "description": "List graph edges",
                    "required": ["workspace"],
                    "optional": ["kind"],
                },
                "add_edge": {
                    "description": "Add a directed edge between tickets",
                    "required": ["workspace", "from", "to", "kind"],
                },
                "remove_edge": {
                    "description": "Remove a directed edge between tickets",
                    "required": ["workspace", "from", "to", "kind"],
                },
                "prune_dangling_edges": {
                    "description": "Remove or report dangling edges for one ticket or globally",
                    "required": ["workspace"],
                    "optional": ["root", "all", "kind", "strategy", "reason"],
                },
                "subgraph": {
                    "description": "BFS dependency subgraph",
                    "required": ["workspace", "root"],
                    "optional": ["direction", "edge_kind", "depth", "limit_nodes", "limit_edges"],
                },
                "topgraph": {
                    "description": "BFS reverse dependency graph",
                    "required": ["workspace", "root"],
                    "optional": ["direction", "edge_kind", "depth", "limit_nodes", "limit_edges"],
                },
                "health_check": {
                    "description": "Run health checks on tickets (descriptions, titles, deps, edges)",
                    "required": ["workspace"],
                    "optional": ["root", "all", "ids", "depth", "direction", "where"],
                },
                "next_tickets": {
                    "description": "List unblocked ready tickets ordered by workflow progress, priority, and dependee count for worker agents",
                    "required": ["workspace"],
                    "optional": ["limit", "filter"],
                },
                "update_ticket": {
                    "description": "Update ticket fields and/or transition state. If description is provided, description_mode must also be provided; valid values are 'replace' and 'append'. There is no default. Omitting both description and description_mode leaves the description unchanged.",
                    "required": ["workspace", "id"],
                    "optional": ["transition_states", "to_state", "fields", "undo", "description", "description_mode", "author"],
                },
                "close_ticket": {
                    "description": "Fast-forward ticket to target state",
                    "required": ["workspace", "id"],
                    "optional": ["to_state", "author"],
                },
                "cancel_ticket": {
                    "description": "Cancel a ticket",
                    "required": ["workspace", "id"],
                    "optional": ["author"],
                },
                "board_show": {
                    "description": "Read current draftboard snapshot; optionally refresh caller heartbeat",
                    "required": ["workspace"],
                    "optional": ["agent_id"],
                },
                "board_history": {
                    "description": "Read recently completed board history",
                    "required": ["workspace"],
                    "optional": ["agent_id"],
                },
                "board_check_in": {
                    "description": "Register agent as working on a ticket",
                    "required": ["workspace", "ticket_id", "agent_id"],
                    "optional": ["intent", "files", "ttl_secs"],
                },
                "board_check_out": {
                    "description": "Remove agent from the draftboard for a ticket",
                    "required": ["workspace", "ticket_id"],
                    "optional": ["agent_id", "reason"],
                },
                "board_release_lease": {
                    "description": "Release a ticket lease with owner/stale safety semantics",
                    "required": ["workspace", "ticket_id", "requester"],
                },
                "board_heartbeat": {
                    "description": "Refresh TTL for a board entry (requires full entry UUID)",
                    "required": ["workspace", "entry_id"],
                },
                "board_configure": {
                    "description": "Read or update board configuration",
                    "required": ["workspace"],
                    "optional": ["max_wip", "stale_after_secs", "completed_audit_window_secs"],
                },
                "board_clean_preview": {
                    "description": "Preview board cleanup candidates and obtain a confirmation token",
                    "required": ["workspace"],
                    "optional": ["include_stale"],
                },
                "board_clean_apply": {
                    "description": "Execute cleanup using the token from board_clean_preview",
                    "required": ["workspace", "token"],
                    "optional": ["include_stale"],
                },
                "board_update_files": {
                    "description": "Add/remove files from a board entry's owned_files",
                    "required": ["workspace", "ticket_id", "agent_id"],
                    "optional": ["add", "remove"],
                },
                "board_rename_file": {
                    "description": "Atomically rename a file in a board entry's owned_files",
                    "required": ["workspace", "ticket_id", "agent_id", "old_path", "new_path"],
                },
                "list_parts": {
                    "description": "List a ticket's content parts, including frozen state and orphaned part files",
                    "required": ["workspace", "id"],
                    "optional": ["with_content"],
                },
                "get_part": {
                    "description": "Get a single ticket content part by its opaque part id",
                    "required": ["workspace", "id", "part_id"],
                },
                "write_part": {
                    "description": "Write a ticket content part (create via kind, or update via part_id); rejected with the full frozen-part error if the part is frozen",
                    "required": ["workspace", "id", "kind", "content"],
                    "optional": ["part_id", "author"],
                },
                "write_amendment": {
                    "description": "Write an amendment part that supersedes another (typically frozen) part",
                    "required": ["workspace", "id", "supersedes", "content"],
                    "optional": ["part_id", "author"],
                },
                "undo_part": {
                    "description": "Restore a part to its content prior to its most recent write; rejected if frozen",
                    "required": ["workspace", "id", "part_id"],
                    "optional": ["author"],
                },
            },
            "notes": [
                "Direct store access — no HTTP backend required.",
                "Set TICKET_INDEX_ROOT to override workspace resolution.",
            ],
        });

        Self::json_result(&payload)
    }
}

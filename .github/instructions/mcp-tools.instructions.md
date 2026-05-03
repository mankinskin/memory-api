---
description: "Use when editing MCP tools (context-mcp, doc-viewer, log-viewer, ticket-mcp). Covers tool contracts, naming stability, and validation hooks."
applyTo: "tools/context-mcp/**,tools/doc-viewer/**,tools/log-viewer/**,tools/ticket-mcp/**"
---

# MCP Tools Guidance

## Contract Stability

- Treat tool names and schemas as compatibility boundaries.
- Do not rename or remove tools without a clear migration path.
- Keep tool descriptions aligned with current behavior.

## Tooling Workflow

Before changing MCP behavior:
1. Check existing docs for tool contracts.
2. Confirm whether behavior is already covered by tests.
3. Keep response formats stable unless explicitly requested.

After changing MCP behavior:
1. Run relevant tests.
2. Run documentation validation workflows.
3. Update related prompt/instruction text if tool behavior changed.

## ticket-mcp Tool Examples

### health_check — Run quality checks on tickets

```json
// Check all tickets in a workspace
{"workspace": "default", "all": true}

// Check a subgraph rooted at a ticket
{"workspace": "default", "root": "abcd1234", "depth": 4}

// Check specific tickets by ID
{"workspace": "default", "ids": ["<UUID1>", "<UUID2>"]}
```

Returns: `tickets_checked`, `finding_count`, `summary` (counts by check), `findings[]` (ticket_id, check, severity, message).

Checks: `missing_description`, `short_description`, `missing_title`, `unblocked_with_deps`, `dangling_edge`.

### Chaining subgraph → health_check in MCP

1. Call `subgraph` with `{"workspace": "default", "root": "<id>", "depth": 3}`
2. Extract node IDs from `response.nodes[].id`
3. Call `health_check` with `{"workspace": "default", "ids": ["<id1>", "<id2>", ...]}`

### Available ticket-mcp tools

| Tool | Required | Optional |
|------|----------|----------|
| `health` | — | — |
| `list_workspaces` | — | — |
| `list_tickets` | workspace | state, query, limit |
| `get_ticket` | workspace, id | — |
| `get_ticket_description` | workspace, id | — |
| `list_edges` | workspace | kind |
| `subgraph` | workspace, root | direction, edge_kind, depth, limit_nodes, limit_edges |
| `topgraph` | workspace, root | direction, edge_kind, depth, limit_nodes, limit_edges |
| `health_check` | workspace | root, all, ids, depth, direction |
| `update_ticket` | workspace, id | to_state, fields |
| `close_ticket` | workspace, id | to_state |
| `cancel_ticket` | workspace, id | — |
| `workflow` | — | name, workspace, id, query |

## Hooks and Validation

- Follow reminders from `.github/hooks/` after MCP-adjacent edits.
- Prefer doc-viewer validation flows over ad hoc manual checklists.

## Board Tools (ticket-mcp)

Nine MCP tools cover the full board lifecycle. All require `workspace`.

### Tool reference

| Tool | Required | Optional |
|------|----------|----------|
| `board_show` | workspace | agent_id |
| `board_check_in` | workspace, ticket_id, agent_id | intent, files, ttl_secs |
| `board_check_out` | workspace, ticket_id | agent_id, reason |
| `board_heartbeat` | workspace, entry_id | — |
| `board_configure` | workspace | max_wip, stale_after_secs, completed_audit_window_secs |
| `board_clean_preview` | workspace | include_stale |
| `board_clean_apply` | workspace, token | include_stale |
| `board_update_files` | workspace, ticket_id, agent_id | add, remove |
| `board_rename_file` | workspace, ticket_id, agent_id, old_path, new_path | — |

### JSON examples

```json
// board_show — read snapshot (no heartbeat)
{"workspace": "default"}

// board_show — read snapshot + refresh caller's heartbeat
{"workspace": "default", "agent_id": "copilot-agent-1"}

// board_check_in
{
  "workspace": "default",
  "ticket_id": "abcd1234",
  "agent_id": "copilot-agent-1",
  "intent": "implementing the storage layer",
  "files": ["crates/ticket-api/src/storage/board.rs"],
  "ttl_secs": 3600
}

// board_check_out
{"workspace": "default", "ticket_id": "abcd1234", "agent_id": "copilot-agent-1", "reason": "done"}

// board_heartbeat
{"workspace": "default", "entry_id": "<full-UUID-from-check-in>"}

// board_configure — set WIP limit and stale timeout
{"workspace": "default", "max_wip": 3, "stale_after_secs": 1800}

// board_clean_preview — include stale entries
{"workspace": "default", "include_stale": true}

// board_clean_apply — consume a preview token
{"workspace": "default", "token": "<token-from-preview>", "include_stale": true}
```

### next_tickets board fields

`next_tickets` integrates board state into its response:

- `board.active_count` / `board.stale_count` — current load
- `board.wip_limit_reached` — true when new check-in would be blocked
- `board.warnings[]` — stale-entry alert strings
- `excluded_by_board[]` — candidate tickets excluded because an active/stale board entry covers
  them; fields: `ticket_id`, `agent_id`, `status`, `intent`

When `wip_limit_reached` is true, resolve existing entries before checking in.

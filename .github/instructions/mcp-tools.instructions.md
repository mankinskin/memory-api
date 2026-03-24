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

Checks: `missing_description`, `short_description`, `missing_title`, `blocked_but_resolved`, `unblocked_with_deps`, `dangling_edge`.

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

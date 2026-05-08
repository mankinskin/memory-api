<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=f7bf44a7-4f6a-4b91-848f-a02d48821ec5 slug=shared/instructions/mcp-tools/mcp-tools-instructions/l1 -->
---
description: "Use when editing MCP tools (context-mcp, doc-viewer, log-viewer, ticket-mcp). Covers tool contracts, naming stability, and validation hooks."
applyTo: "tools/context-mcp/**,tools/doc-viewer/**,tools/log-viewer/**,tools/ticket-mcp/**"
---

<!-- rule-api:entry id=d505dd6f-b79d-401c-b246-8ec4462f0d48 slug=shared/instructions/mcp-tools/mcp-tools-guidance/contract-stability/l8 -->
## Contract Stability

- Treat tool names and schemas as compatibility boundaries.
- Do not rename or remove tools without a clear migration path.
- Keep tool descriptions aligned with current behavior.

<!-- rule-api:entry id=a088945d-6e7d-4dcc-b132-24bb635bb9f6 slug=shared/instructions/mcp-tools/mcp-tools-guidance/tooling-workflow/l14 -->
## Tooling Workflow

Before changing MCP behavior:
1. Check existing docs for tool contracts.
2. Confirm whether behavior is already covered by tests.
3. Keep response formats stable unless explicitly requested.

<!-- rule-api:entry id=ab6172da-cc16-4e2a-a897-95436381fbad slug=shared/instructions/mcp-tools/mcp-tools-guidance/tooling-workflow/l21 -->
After changing MCP behavior:
1. Run relevant tests.
2. Run documentation validation workflows.
3. Update related prompt/instruction text if tool behavior changed.

<!-- rule-api:entry id=9e583dcd-5482-4a41-9d9e-2f74dc61d8da slug=shared/instructions/mcp-tools/mcp-tools-guidance/ticket-mcp-tool-examples/health-check-run-quality-checks-on-tickets/l28 -->
### health_check — Run quality checks on tickets

```json
// Check all tickets in a workspace
{"workspace": "default", "all": true}

<!-- rule-api:entry id=5f12d68c-2b06-4ee3-8e96-d48100bd23f3 slug=shared/instructions/mcp-tools/mcp-tools-guidance/ticket-mcp-tool-examples/health-check-run-quality-checks-on-tickets/l34 -->
// Check a subgraph rooted at a ticket
{"workspace": "default", "root": "abcd1234", "depth": 4}

<!-- rule-api:entry id=f35f49b5-5dd9-459b-ad38-bf6d97fbebbd slug=shared/instructions/mcp-tools/mcp-tools-guidance/ticket-mcp-tool-examples/health-check-run-quality-checks-on-tickets/l37 -->
// Check specific tickets by ID
{"workspace": "default", "ids": ["<UUID1>", "<UUID2>"]}
```

<!-- rule-api:entry id=f543eff9-56d2-421e-82db-d96409e20d83 slug=shared/instructions/mcp-tools/mcp-tools-guidance/ticket-mcp-tool-examples/health-check-run-quality-checks-on-tickets/l41 -->
Returns: `tickets_checked`, `finding_count`, `summary` (counts by check), `findings[]` (ticket_id, check, severity, message).

<!-- rule-api:entry id=a3404543-fb19-47ad-b146-7a3f71eb6f85 slug=shared/instructions/mcp-tools/mcp-tools-guidance/ticket-mcp-tool-examples/health-check-run-quality-checks-on-tickets/l43 -->
Checks: `missing_description`, `short_description`, `missing_title`, `unblocked_with_deps`, `dangling_edge`.

<!-- rule-api:entry id=375741c5-4b3a-4295-b761-4999ec3396c8 slug=shared/instructions/mcp-tools/mcp-tools-guidance/ticket-mcp-tool-examples/chaining-subgraph-health-check-in-mcp/l45 -->
### Chaining subgraph → health_check in MCP

1. Call `subgraph` with `{"workspace": "default", "root": "<id>", "depth": 3}`
2. Extract node IDs from `response.nodes[].id`
3. Call `health_check` with `{"workspace": "default", "ids": ["<id1>", "<id2>", ...]}`

<!-- rule-api:entry id=88b6ba19-4701-4e91-a0ab-8ecdfc6bed01 slug=shared/instructions/mcp-tools/mcp-tools-guidance/ticket-mcp-tool-examples/available-ticket-mcp-tools/l51 -->
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

<!-- rule-api:entry id=a92889da-16f9-4130-ada0-91ed300de668 slug=shared/instructions/mcp-tools/mcp-tools-guidance/hooks-and-validation/l69 -->
## Hooks and Validation

- Follow reminders from `.github/hooks/` after MCP-adjacent edits.
- Prefer doc-viewer validation flows over ad hoc manual checklists.

<!-- rule-api:entry id=6984aaa8-186c-45d9-8e1d-a5865c067d54 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/l74 -->
## Board Tools (ticket-mcp)

Nine MCP tools cover the full board lifecycle. All require `workspace`.

<!-- rule-api:entry id=b5bc4eed-bdca-468a-b4b7-04c89d6975f5 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/tool-reference/l78 -->
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

<!-- rule-api:entry id=dff00e90-33cb-41da-83e6-1128365a5279 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l92 -->
### JSON examples

```json
// board_show — read snapshot (no heartbeat)
{"workspace": "default"}

<!-- rule-api:entry id=9cd77f2e-9ec6-4b10-aa86-3096c1fafd88 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l98 -->
// board_show — read snapshot + refresh caller's heartbeat
{"workspace": "default", "agent_id": "copilot-agent-1"}

<!-- rule-api:entry id=790e9e0c-211f-4288-bbe7-86311911feea slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l101 -->
// board_check_in
{
  "workspace": "default",
  "ticket_id": "abcd1234",
  "agent_id": "copilot-agent-1",
  "intent": "implementing the storage layer",
  "files": ["crates/ticket-api/src/storage/board.rs"],
  "ttl_secs": 3600
}

<!-- rule-api:entry id=657d651e-92d2-463a-9883-952b7786c2d6 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l111 -->
// board_check_out
{"workspace": "default", "ticket_id": "abcd1234", "agent_id": "copilot-agent-1", "reason": "done"}

<!-- rule-api:entry id=edc88248-f46c-41bb-aeac-c1a64736d967 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l114 -->
// board_heartbeat
{"workspace": "default", "entry_id": "<full-UUID-from-check-in>"}

<!-- rule-api:entry id=27357aa7-389e-4d57-8b8b-b6a0571cfd92 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l117 -->
// board_configure — set WIP limit and stale timeout
{"workspace": "default", "max_wip": 3, "stale_after_secs": 1800}

<!-- rule-api:entry id=85b8af4b-48da-4b0f-a3b6-0f0272e5918f slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l120 -->
// board_clean_preview — include stale entries
{"workspace": "default", "include_stale": true}

<!-- rule-api:entry id=6b15fe38-a0f8-4b11-86b4-0f08f777880e slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/json-examples/l123 -->
// board_clean_apply — consume a preview token
{"workspace": "default", "token": "<token-from-preview>", "include_stale": true}
```

<!-- rule-api:entry id=fbcfa668-b788-4666-9ad9-1c6b1ad44408 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/next-tickets-board-fields/l127 -->
### next_tickets board fields

`next_tickets` integrates board state into its response:

<!-- rule-api:entry id=3d4a5a5e-1fb2-41bc-ad8e-1da40b9ed37a slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/next-tickets-board-fields/l131 -->
- `board.active_count` / `board.stale_count` — current load
- `board.wip_limit_reached` — true when new check-in would be blocked
- `board.warnings[]` — stale-entry alert strings
- `excluded_by_board[]` — candidate tickets excluded because an active/stale board entry covers
  them; fields: `ticket_id`, `agent_id`, `status`, `intent`

<!-- rule-api:entry id=ea541206-c57d-4772-ba21-91d02daf7d63 slug=shared/instructions/mcp-tools/mcp-tools-guidance/board-tools-ticket-mcp/next-tickets-board-fields/l137 -->
When `wip_limit_reached` is true, resolve existing entries before checking in.

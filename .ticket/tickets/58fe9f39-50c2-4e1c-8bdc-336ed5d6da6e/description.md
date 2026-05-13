# Plan: MCP Write Tools

## Problem

The ticket-mcp server currently exposes only read-only tools:
`health`, `list_workspaces`, `list_tickets`, `get_ticket`, `get_ticket_description`,
`list_edges`, `subgraph`, `workflow`, `help`.

Agents using MCP (Copilot, Claude, etc.) cannot create, update, or close tickets
through MCP — they must shell out to the CLI. This forces agents to use terminal
commands for every mutation, which is slower and more error-prone.

## Proposed MCP Tools

| Tool | Purpose |
|------|---------|
| `create_ticket` | Create new ticket with title, fields, optional body |
| `update_ticket` | Update fields and/or transition state |
| `close_ticket` | Fast-forward to `done` state |
| `cancel_ticket` | Transition to `cancelled` from any state |
| `attach_file` | Attach content as asset (base64 or text) |
| `batch_update` | Update multiple tickets matching filter |
| `set_field` | Convenience: set single field on one ticket |

### Security
- Write tools should respect any auth configuration
- `batch_update` should have a max-count safety limit
- All mutations recorded in history

### Priority
Start with `update_ticket` and `close_ticket` — these cover 80% of agent write needs.

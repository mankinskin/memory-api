# ticket-mcp

MCP server with direct access to the ticket store via `ticket-api`.

No separate HTTP backend required — the server opens the ticket store directly,
the same way `ticket-viewer` does.

## Tools

- `help` - Shows available tools and their parameters.
- `health` - Checks that the ticket store is accessible.
- `list_workspaces` - Lists workspaces.
- `list_tickets` - Lists tickets in one workspace with optional filters.
- `get_ticket` - Gets a ticket by id.
- `get_ticket_description` - Gets markdown description for a ticket.
- `list_edges` - Lists edge graph rows, optionally filtered by kind.
- `subgraph` - Returns dependency subgraph rooted at a ticket.
- `workflow` - Returns ready-to-run tool call sequences for common tasks.

## Usage

```bash
# Just run it — no backend needed
cargo run -p ticket-mcp

# Custom ticket index root
TICKET_INDEX_ROOT=/path/to/index cargo run -p ticket-mcp
```

## VS Code MCP configuration

In `.vscode/mcp.json`:

```json
{
  "servers": {
    "ticket-mcp": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "mcp-runner", "--release", "--", "ticket-mcp"]
    }
  }
}
```

Then start Copilot with MCP config:

```bash
copilot --additional-mcp-config @.github/copilot-mcp-config.json
```

## Request tool input

`request` accepts:

- `operation`: one of the supported operations above
- Optional fields depending on operation: `workspace`, `id`, `state`, `query`, `limit`, `kind`, `root`, `direction`, `edge_kind`, `depth`, `limit_nodes`, `limit_edges`
- `base_url` (optional): override default API URL per call

Example:

```json
{
  "operation": "list_tickets",
  "workspace": "default",
  "state": "open",
  "limit": 50
}
```

## Recommended calling pattern

1. Call `help` or `workflow` first.
2. Use named tools (`list_tickets`, `get_ticket`, etc.) for normal usage.
3. Use `request` only when you need generic operation routing.

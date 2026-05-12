# ticket-mcp

MCP server for `ticket-api`.

## Interface

`ticket-mcp` runs on stdio and opens the ticket store directly. Use it when an agent needs ticket reads, graph traversal, workflow helpers, or board operations without an HTTP backend.

Named tool groups:

- Query and graph reads: `health`, `list_workspaces`, `list_tickets`, `get_ticket`, `get_ticket_description`, `list_edges`, `subgraph`, `topgraph`, `next_tickets`, `health_check`
- Mutation and workflow: `create_ticket`, `update_ticket`, `close_ticket`, `cancel_ticket`, `delete_ticket`, `add_edge`, `remove_edge`, `workflow`
- Board coordination: `board_show`, `board_history`, `board_check_in`, `board_check_out`, `board_heartbeat`, `board_configure`, `board_clean_preview`, `board_clean_apply`, `board_update_files`, `board_rename_file`
- Help: `help`

Store discovery:

- Set `TICKET_INDEX_ROOT` to point at a specific ticket store.
- Otherwise the server resolves the nearest `.ticket` workspace from the current checkout.

## Usage

Run the server on stdio:

```bash
cargo run -p ticket-mcp
```

Example VS Code MCP configuration:

```json
{
  "servers": {
    "ticket-mcp": {
      "type": "stdio",
      "command": "cargo",
      "args": ["run", "-p", "ticket-mcp"]
    }
  }
}
```

## Examples

- Call `next_tickets` to ask for the next unblocked work item.
- Call `subgraph` before implementation when a client needs the dependency context around one ticket.
- Call `board_show` and `board_check_in` to coordinate active work without leaving the MCP client.

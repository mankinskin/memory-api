<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=b8535c8a-4097-4042-8f2a-745123d269ee slug=memory-api/readme/tools/parent-readme/l1 -->
Back to [memory-api/README.md](../../../README.md).

<!-- rule-api:entry id=3525d8dd-0fd9-4af2-bd43-480fa58ea3cd slug=memory-api/readme/tools/mcp/ticket-mcp/l1 -->
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

## Workflow notes

`next_tickets` uses the same convergence-first ranking as `ticket next`. A prerequisite in an earlier workflow state can be promoted ahead of otherwise similar candidates when more advanced dependents are still waiting on it.

Returned `next_tickets` items include the same explainability fields used by the CLI, including `dependees`, `transitive_reverse_dependents`, `affected_reverse_dependent_reach`, `max_affected_dependent_state`, and `dependency_state_gap`.

`health_check` emits `dependency_convergence` findings with dependent and prerequisite ids, both states, and the reach or state-gap evidence needed for triage.

`update_ticket` accepts sparse request payloads. Omit untouched keys entirely; use only the fields being changed. The response returns minimal update metadata such as `id`, `path`, `changed_fields`, `state_transition`, and `description_updated` when those values are directly relevant.

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

- Call `next_tickets` to ask for the next unblocked work item and inspect why an earlier-state prerequisite was promoted.
- Call `health_check` before review or automation to detect dependency-state inversions.
- Call `subgraph` before implementation when a client needs the dependency context around one ticket.
- Call `board_show` and `board_check_in` to coordinate active work without leaving the MCP client.
- For `update_ticket`, prefer sparse payloads such as `{"workspace":"default","id":"<uuid>","to_state":"in-review"}` instead of sending unchanged placeholders.

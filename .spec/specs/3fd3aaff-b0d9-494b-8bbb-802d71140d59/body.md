# Summary

Ticket-MCP read operations resolve against the configured aggregate ticket root, including policy-discovered and indexed descendant stores. Creation and other mutations remain explicitly rooted.

## Requirements

- `get_ticket` and `get_ticket_description` accept an omitted workspace selector.
- An omitted selector resolves to the server index root.
- A supplied selector resolves that store and its indexed descendants.
- On MCP startup, the selected ticket workspace re-applies its `workspace-policy.toml` before accepting requests, so enabled descendant stores are registered and reindexed without requiring a separate manual scan.
- Every read-only ID/root selector, including `get_ticket`, `get_ticket_description`, `next_tickets.root`, `subgraph.root`, and `health_check.root`/`ids`, resolves against the selected aggregate `TicketStore`.
- A no-match diagnostic reports every resolved scan root searched; MCP failures carry the typed error contract with that diagnostic context.
- Create inputs and mutation targets retain required explicit workspace selectors and do not silently redirect writes across workspace boundaries.

## Implementation

`TicketServer::resolve_uuid_for_read` resolves prefixes through the selected aggregate `TicketStore` and appends persisted scan-root paths to no-match diagnostics. All read-only identifier entrypoints must use this helper. Ticket-MCP startup derives the owning workspace root from the selected `.ticket` store and calls `TicketStore::reapply_workspace_policy` before it constructs the server.

## Validation

- Focused regression coverage verifies policy refresh discovers a child store for `next_tickets` from an explicit parent workspace.
- Focused regression coverage verifies a missing `next_tickets.root` reports all scanned workspace roots.
- `cargo test -p ticket-mcp` passes.

## Related Implementation Ticket

- `2ffd479a-ca4b-4265-a1c5-f0081b2e531e`.
- [9faa3f5f Unify workspace parameter semantics across spec-mcp/test-mcp](.ticket/tickets/9faa3f5f-e2e1-469d-994e-1bb8b90d5ab4/ticket.toml) — lives in the ROOT ticket store, not `memory-api/.ticket/`.
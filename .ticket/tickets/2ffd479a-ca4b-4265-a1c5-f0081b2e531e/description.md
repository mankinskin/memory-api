## Problem
`ticket-mcp` accepts an explicit checkout path but opens only that local `.ticket` store. Read tools thereby lose descendant tickets even though the canonical `default` workspace aggregates descendant stores. The observed failure is `next_tickets` and graph reads cannot resolve child-owned ticket IDs when invoked with the repository root path.

## Acceptance Criteria
- Every MCP operation that accepts `workspace` resolves a concrete root path through one canonical policy.
- Explicit ancestor workspace paths retain the aggregate descendant-read behavior used by `default` for read/query, workflow, graph, health, and board operations.
- Mutation targeting remains explicitly local and does not silently write to an ancestor or descendant store.
- `next_tickets` resolves and ranks a child-owned ticket when called through the canonical parent workspace.
- Focused MCP integration tests cover `next_tickets` and at least one representative query/graph operation with a descendant ticket.
- Existing `default` and explicit child-store behavior remains compatible.

## Solution Design
Audit `TicketServer::resolve_workspace_root` and all tool entrypoints. Introduce or use the ticket-api canonical workspace resolver at the server boundary so compatible read semantics are applied consistently. Keep mutation helpers on the strict local-store path. Add regression coverage using a parent store with a registered child scan root.
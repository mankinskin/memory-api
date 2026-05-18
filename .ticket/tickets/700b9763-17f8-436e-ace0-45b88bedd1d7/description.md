# Problem

The ticket-viewer and ticket-vscode frontends still model `workspace` as one route or query string that owns every returned record. On the server side both `ticket-viewer` and `ticket-http` resolve one local store from cwd or `--index-root` and register it as `default`, which then leaks into the SPA route as `/workspace/default`.

That produces two distinct problems:

- the visible URL is misleading because `default` is not the real workspace identity; it is a registry alias for the server-selected store
- mixed child-workspace results are not reversible because ticket-like items do not carry origin workspace

The current contract is not sufficient for tickets, descriptions, history, file lists, assets, graph nodes, edge endpoints, SSE updates, or mutation follow-up flows when child workspaces are included.

# Relevant code evidence

- `memory-viewers/ticket-viewer/src/main.rs` parses `--workspace` but still resolves the local store with `ticket_api::workspace::resolve_workspace()` and registers it via `WorkspaceRegistry::single_opened(...)`.
- `memory-viewers/memory-api/tools/http/ticket-http/src/main.rs` does the same for standalone serve mode.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/routes.rs` redirects `/` to `/workspace/default`.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs` builds ticket, detail, history, files, asset, graph, and stream calls around one `workspace` query parameter.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/types.rs` and `memory-viewers/memory-api/tools/ticket-vscode/src/api.ts` keep `workspace` on the response envelope while individual ticket records remain bare ids.
- `memory-viewers/memory-api/tools/http/ticket-http/src/serve/handlers/tickets/types.rs` and `memory-viewers/memory-api/tools/http/ticket-http/src/serve/handlers/graph.rs` return ticket and graph payloads without per-item workspace provenance.

# Goal

Define the frontend/backend contract for server-selected workspace context plus per-item workspace ownership so ticket-viewer and ticket-vscode can consume child-workspace tickets without guessing.

# Contract decisions required

1. Distinguish `server-selected active workspace context` from `origin workspace of a returned ticket reference`.
2. Remove the requirement that the primary viewer route encode `/workspace/default`; define the replacement route and deep-link model for root navigation and child-ticket deep links.
3. Define a reversible ticket reference shape, for example `{ workspace, id }`, for every ticket-like payload.
4. Define request semantics for selecting scope:
   - active server workspace resolved from cwd or start parameters
   - optional child-workspace inclusion
   - optional explicit include or exclude lists
   - any reverse-direction ancestor dependency expansion needed for graph or dependency views
5. Define response changes for:
   - `/api/workspaces`
   - `/api/tickets`
   - `/api/tickets/{id}`
   - `/api/tickets/{id}/description`
   - `/api/tickets/{id}/history`
   - `/api/tickets/{id}/files`
   - `/api/tickets/{id}/asset`
   - `/api/edges`
   - `/api/schema` and `/api/schema/{type_id}`
   - `/api/graph/subgraph`
   - `/api/stream`
   - any mutation response that echoes a ticket reference after edit, undo, revert, close, or cancel
6. Define a backward-compatible migration strategy for existing callers that still operate on one local store.

# Acceptance Criteria

- The current frontend endpoint inventory explicitly covers list, detail, description, history, files, asset, graph, stream, and mutation follow-up flows and calls out the fake `/workspace/default` route as legacy behavior to remove.
- The redesign specifies separate concepts for server-selected workspace context and per-item origin workspace.
- The redesign defines a reversible ticket reference contract for aggregated child-workspace results and for follow-up asset, history, and edit requests.
- The redesign defines request semantics for child-workspace inclusion, explicit workspace filters, and any reverse-direction ancestor dependency expansion.
- The redesign defines the root-route and deep-link migration so ticket-viewer no longer needs to expose `/workspace/default`, while still supporting stable links to child-owned tickets.
- The redesign identifies migration expectations for ticket-viewer and ticket-vscode, including any compatibility window or transitional response fields.

# Notes

- `80b4b77f` is prior art for workspace picking and auth; it does not solve per-item workspace provenance.
- `cccf5d99` is an unrelated explorer filter bug and should not block this design ticket.
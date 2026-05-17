# Problem

The ticket-viewer and ticket-vscode frontends both assume that a single selected workspace owns every result returned from the frontend-facing ticket endpoints.

That assumption breaks as soon as we want to show child-workspace tickets in one UI. The current response shapes only carry `workspace` at the top level of the response, while the individual ticket summaries, descriptions, and related records are still keyed by bare ticket id or are otherwise interpreted as belonging to the currently selected workspace.

We need a frontend endpoint redesign that can aggregate child-workspace results without losing reversibility: every returned ticket-like record must be mappable back to one concrete `(workspace, ticket id)` pair.

# Current frontend endpoint surface

The current frontend code paths use these workspace-scoped endpoints:

- `GET /api/workspaces`
- `GET /api/tickets?workspace=...&state=...&query=...&limit=...`
- `GET /api/tickets/{id}?workspace=...`
- `GET /api/tickets/{id}/description?workspace=...`
- `GET /api/edges?workspace=...`
- `GET /api/schema?workspace=...`
- `GET /api/graph/subgraph?workspace=...&root=...&depth=...`
- `GET /api/stream?workspace=...`

Relevant code evidence:

- `memory-viewers/ticket-viewer/frontend/dioxus/src/types.rs` keeps `workspace` on the response envelope, not on `TicketSummary` or `TicketDetail`.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs` always builds list/get URLs around one `workspace` query parameter.
- `memory-viewers/memory-api/tools/ticket-vscode/src/api.ts` returns `TicketsResponse.workspace` with `TicketSummary` items that do not carry workspace identity.
- `memory-viewers/ticket-viewer/src/main.rs` still opens `WorkspaceRegistry::single_opened(...)`, which reinforces the single-workspace frontend contract.

# Redesign goals

Define the frontend endpoint migration for child-workspace ticket integration.

The redesign should:

- make the returned identity reversible, for example by introducing an explicit per-item ticket reference such as `{ workspace, id }`
- define which list/get endpoints gain per-item workspace identity and how supporting responses (description, graph, edges, schema, stream) should represent origin workspace
- specify how clients ask for child-workspace tickets, including default behavior and optional include/exclude workspace selectors
- stay backward-compatible long enough for ticket-viewer and ticket-vscode to migrate cleanly

# Acceptance criteria

- The current frontend endpoint inventory is documented for ticket-viewer and ticket-vscode, with the existing single-workspace assumptions called out explicitly.
- The redesign specifies a reversible per-item ticket identity for aggregated child-workspace results.
- The redesign specifies request semantics for explicit workspace scope plus include/exclude child workspaces, with backward-compatible defaults.
- The redesign identifies the migration expectations for ticket-viewer and ticket-vscode, including how routes and deep links can still resolve to `/workspace/{workspace}/ticket/{id}` or an equivalent workspace-aware destination.

# Notes

Existing completed workspace-selection work is prior art, not a replacement for this redesign. The key gap here is item-level workspace identity in frontend-facing list/get responses, not the existence of a single workspace picker.
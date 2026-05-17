# Problem

The Dioxus ticket-viewer currently threads one workspace string through the entire app and assumes every list, detail, graph, search, and stream result belongs to that workspace.

That is visible in several places:

- routes are keyed as `/workspace/:workspace` and `/workspace/:workspace/ticket/:id`
- list/search/detail requests are built around a single `workspace` query parameter
- persisted UI state is keyed as `ticket-viewer:{workspace}:ui`
- graph fetch caches are keyed as `{workspace}:{root_id}`
- SSE subscriptions connect to `GET /api/stream?workspace=...`

This is workable for one workspace at a time, but it prevents the viewer from showing child-workspace tickets in a reversible way. If a list response mixes multiple workspaces, the UI needs a concrete ticket reference that carries origin workspace per item, and the UI needs to show that origin clearly.

# Current endpoint usage to migrate

Current ticket-viewer flows call:

- `GET /api/tickets?workspace=...&state=...&query=...&limit=...` from the explorer list, quick search, and dependency picker
- `GET /api/tickets/{id}?workspace=...` for ticket detail refreshes
- `GET /api/tickets/{id}/description?workspace=...` for description content
- `GET /api/schema?workspace=...` for schema-driven actions
- `GET /api/graph/subgraph?workspace=...&root=...&depth=...` for graph panes
- `GET /api/stream?workspace=...` for live refresh

The current Rust response types keep `workspace` on the outer response envelope while `TicketSummary`, `TicketDetail`, and graph nodes do not carry per-item workspace identity.

# Goal

Migrate ticket-viewer to the workspace-aware frontend endpoint contract for child-workspace tickets.

The migration should:

- adopt the new reversible ticket reference shape from the endpoint redesign
- render workspace ownership visibly anywhere mixed-workspace results can appear
- keep deep-linking and navigation correct when the selected ticket comes from a child workspace
- add a clear UX for opening one workspace, showing child workspaces, and including/excluding specific workspaces
- preserve or evolve the existing per-workspace persistence model so saved filters and layout state remain understandable under multi-workspace scopes

# Acceptance criteria

- Explorer, quick search, detail refresh, and graph fetch flows use workspace-aware ticket references instead of assuming one route workspace owns every returned item.
- Mixed-workspace results visibly show ticket workspace in the explorer, search results, and detail context.
- The UI offers an explicit workspace-scope control that can show child workspaces and include/exclude individual workspaces without making the active scope ambiguous.
- Routing, localStorage state, graph cache keys, and SSE refresh logic continue to resolve the correct ticket after the migration.
- Browser coverage is added for at least one mixed-workspace listing flow and one mixed-workspace detail/deep-link flow.

# Prior art

`80b4b77f` already added a workspace picker for the ticket-viewer. This ticket should build on that work rather than replace it.

# External dependency note

The shared frontend endpoint redesign for workspace-aware ticket references should be treated as an upstream prerequisite for this migration.
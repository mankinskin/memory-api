# Problem

The Dioxus ticket-viewer currently treats `workspace` as one ambient string that owns everything in the app. That assumption is baked into routing, selection state, caches, SSE, batch selection, and every detail, asset, history, and edit request.

It also keeps the misleading `/workspace/default` URL alive even though the backend actually chooses its active workspace from cwd or start parameters.

That breaks child-workspace support in several concrete ways:

- list, search, and graph results cannot carry origin workspace per item
- selected ticket state is keyed only by bare ticket id
- batch actions and dependency pickers assume selected ids are local to the ambient workspace
- detail, description, history, file, asset, edit, undo, and revert flows cannot follow a child-owned ticket reference safely
- the visible route suggests `default` is meaningful when it is just a registry alias

# Relevant code evidence

- `memory-viewers/ticket-viewer/frontend/dioxus/src/routes.rs` redirects `/` to `/workspace/default` and encodes workspace in the route.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/routes/list/page.rs` stores `selected_id: Option<String>` and `selected_ids: Vec<String>`.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/store.rs` persists UI state under `ticket-viewer:{workspace}:ui`.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs` builds list, detail, history, files, asset, update, undo, and revert URLs around one workspace string.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_detail/actions.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/components/history.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/components/batch_panel/page.rs`, and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/dep_graph/picker.rs` all work with bare ids plus ambient workspace.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/sse.rs` and `memory-viewers/ticket-viewer/frontend/dioxus/src/graph_fetch.rs` key live refresh and graph fetch state by workspace string plus bare id or root id.

# Goal

Migrate ticket-viewer to the workspace-aware contract so the app uses server-selected workspace context for the main view, explicit ticket references for mixed-workspace items, and correct owning-workspace follow-up calls for detail and asset flows.

# Planned changes

1. Remove the primary `/workspace/default` route requirement; define the root route and deep-link behavior that reflects the server-selected active workspace without exposing the fake alias.
2. Replace bare ticket-id selection and batch state with workspace-aware ticket references wherever a mixed-workspace result can appear.
3. Use the owning workspace from the selected ticket reference for detail, description, history, files, asset, edit, undo, revert, close, cancel, and dependency mutation flows.
4. Update tree, search, dependency picker, graph, and detail panels to show origin workspace visibly when results are mixed.
5. Rework localStorage keys, graph cache keys, and any persisted view state so they remain understandable under multi-workspace scopes.
6. Keep a stable deep-link format for child-owned tickets, whether that remains a workspace route or moves to an equivalent workspace-aware ticket reference route.
7. Add browser coverage for:
   - root navigation without `/workspace/default`
   - mixed-workspace list or search rendering
   - mixed-workspace detail or deep-link resolution
   - follow-up asset, history, or edit behavior for a child-owned ticket

# Acceptance Criteria

- Ticket-viewer no longer needs to expose `/workspace/default` for the main route.
- Explorer, quick search, dependency picker, detail refresh, and graph fetch flows use workspace-aware ticket references instead of assuming one route workspace owns every item.
- Mixed-workspace results visibly show ticket workspace in the explorer, search results, graph or detail context, and any batch or dependency picker flow where ambiguity matters.
- Routing, localStorage state, graph cache keys, selected ticket state, batch selection state, and SSE refresh logic continue to resolve the correct ticket after the migration.
- Detail, description, history, files, asset, update, undo, and revert flows resolve against the ticket's owning workspace.
- Browser coverage exercises at least one mixed-workspace listing flow and one mixed-workspace detail or deep-link flow, including a follow-up file, asset, or history action.

# Prior art and dependencies

- `80b4b77f-3fd6-4fab-98ab-028c6f6d6ef6` already added workspace-picker groundwork.
- `700b9763-17f8-436e-ace0-45b88bedd1d7` is the API-contract prerequisite.
- `429f6f1d-6429-4601-bfac-b572fdb4dbff` is the backend ancestor-visibility prerequisite for dependency and graph behavior.
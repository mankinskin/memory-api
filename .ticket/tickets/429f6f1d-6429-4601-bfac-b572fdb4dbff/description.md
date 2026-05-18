# Problem

Child memory workspaces such as `ticket-api` can open their own local store, but cross-workspace dependency and graph views still assume both ends of a relationship can be resolved inside the active store.

When a child-owned ticket points to or from an ancestor-owned ticket:

- graph or dependency payloads can only return bare ids or drop the opposite endpoint
- callers cannot tell which workspace owns the resolved endpoint
- follow-up detail, description, history, file, and asset calls have no reversible reference to the owning workspace

The result is incomplete dependency graphs and misleading ticket context.

# Relevant code evidence

- `memory-viewers/memory-api/tools/http/ticket-http/src/serve/handlers/graph.rs` returns `NodeItem` and `EdgeItem` with ids only and no per-node workspace ownership.
- `memory-viewers/memory-api/tools/http/ticket-http/src/serve/handlers/tickets/assets.rs` resolves file lists and assets from the workspace chosen by the request, so follow-up file and asset calls need a correct owning workspace.
- `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_detail/actions.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/page.rs`, and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_tree/rows.rs` all issue follow-up requests using one ambient workspace string.
- Spec `0b1888f2-7e59-45fb-95d8-1bf14ff7747f` already documents ancestor endpoint visibility as the required behavior.

# Goal

Implement the backend and storage-side behavior needed for a child workspace to resolve ancestor-owned dependency endpoints and return explicit workspace-aware references that downstream callers can use for details, history, files, and assets.

# Planned changes

1. Introduce or standardize a workspace-aware ticket reference model in ticket-api or ticket-http traversal outputs for any cross-workspace dependency or graph surface.
2. Extend dependency and graph resolution so a child workspace can materialize ancestor-owned endpoints when they are the direct source or target of a relationship involving a child-owned ticket.
3. Preserve workspace ownership explicitly; do not copy or re-home ancestor tickets into the child store.
4. Ensure the returned reference is sufficient for follow-up detail, description, history, files, and asset requests against the correct workspace.
5. Keep existing single-workspace outputs behaviorally unchanged when no ancestor-child relationship is involved.
6. Document backward-compatible defaults and any opt-in or opt-out scope behavior required by the HTTP layer.

# Acceptance Criteria

- A child workspace can resolve dependency endpoints that belong to an ancestor workspace without dropping the relationship.
- Dependency and graph responses preserve explicit workspace ownership for both local and ancestor-owned endpoints.
- Returned references are sufficient for downstream detail, history, files, and asset resolution without guessing from the active route or request workspace.
- Parent ownership remains explicit; child workspaces do not silently claim or rewrite ancestor-owned tickets.
- The behavior is documented and aligned with spec `0b1888f2-7e59-45fb-95d8-1bf14ff7747f`.

# Spec and Adjacent Work

- Spec: `0b1888f2-7e59-45fb-95d8-1bf14ff7747f` Ancestor Workspace Ticket References for Child-Workspace Dependencies.
- Adjacent API-contract work: `700b9763-17f8-436e-ace0-45b88bedd1d7` defines the frontend-facing HTTP contract that should consume these workspace-aware references.

## Problem

`ticket_ref.workspace` in list/get HTTP responses echoes a workspace id that does not match the requested (and valid) `workspace` query parameter, and does not correspond to any workspace registered in `WorkspaceRegistry`. The frontend (memory-viewers/ticket-viewer/frontend/dioxus/src/types.rs:92-104, `TicketSummary::resolved_ticket_ref`) trusts the echoed `ticket_ref` and uses it to build the follow-up `GET .../full` request, which then 404s because the workspace id is unregistered. This is a frontend trust point, NOT the root cause — do not fix it there alone.

## Reproduction

1. Start ticket-http against the `memory-api` workspace (registered as `memory-api--a6b09287`).
2. `GET /tickets/3a1ec9f8...?workspace=memory-api--a6b09287` (a valid, registered workspace id).
3. Response's `ticket_ref.workspace` is `"memory-api--8c1a6067"` — same label prefix, different hash suffix, and not present in `GET /workspaces`.
4. The viewer's follow-up `?view=full` request built from that echoed ref workspace 404s.
5. Confirmed for most tickets in the repo (all tested except the freshly-migrated `e342cc4c`), per ticket f65f2b32 validation notes (memory-api/.ticket/tickets/f65f2b32-9297-4360-9ad7-deb75e7ea401/parts/fd9ba6b0-40a5-4caf-a468-b753d4bf2d59.md).

## Root cause area (must be pinpointed by implementer, not assumed)

- `owning_workspace_for_path` in memory-api/tools/http/ticket-http/src/serve/handlers/tickets/types.rs:377-403 computes the returned workspace label by matching `ticket.path` against `store.list_scan_roots()` entries and falling back to the raw `root.label` (a bare label like `"memory-api"`), not the canonical hashed workspace id.
- The canonical hashed workspace id format (`{label}--{short_workspace_hash}`) is computed separately in `canonical_workspace_name_for_index_root` / `short_workspace_hash` (memory-api/tools/http/ticket-http/src/serve/registry.rs:448-518), keyed off `workspace_root_for_index_root(index_root)`.
- These two computations diverge: the registry treats workspace identity as `label--hash(normalized_workspace_root)`, but `ticket_ref_from_indexed` → `owning_workspace_for_path` returns whichever label/active-workspace string it resolves via scan-root path matching, which can end up hashed with a different input path (producing a same-label, different-hash mismatch) or otherwise fail to line up with a registered `WorkspaceRegistry` key.
- Ticket must identify why the hash inputs diverge (e.g., differing path normalization, symlink resolution, or working-directory-relative vs canonical index_root) and make `ticket_ref.workspace` always equal a workspace id present in `GET /workspaces` for the ticket actually being returned.

## Impact

Clicking almost any ticket in ticket-viewer shows a `desc-error` banner instead of the parts panel; no crash, but the structured-parts UI is effectively unreachable for most tickets.

## Acceptance Criteria

1. For a ticket requested via a valid, registered `workspace` query parameter, the response's `ticket_ref.workspace` is a workspace id that also appears in `GET /workspaces` for the same server instance.
2. A regression test in memory-api/tools/http/ticket-http/src/serve/handlers/tickets/tests/ asserts `ticket_ref.workspace` round-trips: requesting the same ticket again with the echoed workspace value (e.g. as a follow-up `?view=full` call) succeeds (200), not 404.
3. Existing tests asserting `ticket_ref.workspace` equality against a plain `workspace` variable (e.g. listing_workspace.rs, lifecycle.rs, mutations.rs) still pass or are updated consistently with the corrected computation.
4. Manual/E2E check: opening a ticket in ticket-viewer no longer shows the `desc-error` banner for tickets that previously failed (verify against a sample of at least 3 tickets, including `3a1ec9f8`).
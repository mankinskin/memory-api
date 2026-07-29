## Objective

Make every ticket content write state its intent explicitly. Remove the `replace` default from `description_mode`, and expose part-addressed writes so recording a review never touches the objective.

## Requirements

- `update_ticket` with a `description` and no `description_mode` is rejected with an actionable error. No default is applied.
- The error names both modes and states which one preserves existing content.
- Part-addressed writes target a stable opaque part `id`, not kind+index; `kind` only selects a display label or validation bucket.
- Writing a `review` or `validation` part never reads or writes `objective`.
- Every part write appends a revision to `history.ndjson` carrying the prior content of the part it changed, preserving today's `undo` semantics per part.
- This ticket owns only the minimal correctness fix to the agent instruction surfaces: every documented `update_ticket` call must require `description_mode`; the broader guidance rewrite for profiles, freeze contract, and role-owned part kinds belongs to 71e13480.
- All in-repo call sites are updated: ticket CLI, ticket-mcp, ticket HTTP transport, ticket-viewer, and existing tests.

## Design

`DescriptionUpdateMode` in `memory-api/crates/ticket-api/src/storage/store.rs` is the current mode enum that maps the incoming string to `Replace` or `Append`. The update path itself already funnels through `TicketStore::update_with_options` and its helper `apply_manifest_update`; this ticket makes the `description_mode` decode mandatory at the CLI, MCP, and HTTP request boundaries and keeps the store boundary explicit instead of synthesizing a default.

Part-addressed writes build on the same `TicketStore::update_with_options` flow, but the write target changes from the legacy whole-description `description.md` helper to a part-aware helper that resolves `TicketManifest.parts[*].id` before touching disk. That keeps `review`/`validation` content isolated from `objective` and preserves per-part undo history.

The minimal instruction-side fix belongs here only: the generated and hand-authored command docs that show `update_ticket` must state that `description_mode` is required, while the fuller role guidance rewrite remains in 71e13480.

## Call Sites

The explicit-mode requirement must be reflected at these real decode and mutation surfaces: `memory-api/tools/cli/ticket-cli/src/cli/args/operations.rs`, `memory-api/tools/cli/ticket-cli/src/cli/commands/crud.rs`, `memory-api/tools/mcp/ticket-mcp/src/server/types.rs`, `memory-api/tools/mcp/ticket-mcp/src/server/mutations.rs`, `memory-api/tools/mcp/ticket-mcp/src/server/workflow.rs`, `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/types.rs`, `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/mutations.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/api/backend.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/edit.rs`, and the update-path tests in `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/tests/mutations.rs` and `memory-api/tools/mcp/ticket-mcp/tests/integration_update_mcp.rs`.

## Implementation Steps

1. Change `memory-api/tools/mcp/ticket-mcp/src/server/types.rs` and `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/types.rs` so the update request schema requires `description_mode` instead of defaulting it away.
2. Update `memory-api/tools/mcp/ticket-mcp/src/server/mutations.rs` and `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/mutations.rs` to reject missing modes before mapping the string into `ticket_api::storage::DescriptionUpdateMode`.
3. Update `memory-api/tools/cli/ticket-cli/src/cli/args/operations.rs` and `memory-api/tools/cli/ticket-cli/src/cli/commands/crud.rs` so `ticket update` requires `--description-mode` on every write path.
4. Update `memory-viewers/ticket-viewer/frontend/dioxus/src/api.rs`, `memory-viewers/ticket-viewer/frontend/dioxus/src/api/backend.rs`, and `memory-viewers/ticket-viewer/frontend/dioxus/src/components/ticket_content/edit.rs` so the viewer's save path passes an explicit mode when persisting edits.
5. Route part-addressed writes through a new part-resolving helper in `memory-api/crates/ticket-api/src/storage/store.rs` and persist the pre-change content per part in `memory-api/crates/ticket-api/src/storage/ticket_fs.rs`.
6. Add regression tests in `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/tests/mutations.rs`, `memory-api/tools/mcp/ticket-mcp/tests/integration_update_mcp.rs`, and the ticket CLI tests to prove omitted modes fail and explicit modes still work.
7. Add a compile-fail or equivalent targeted test proving `UpdateTicketInput`, `UpdateTicketBody`, and `UpdateArgs` cannot be constructed without `description_mode`, so omission is a type error rather than a runtime default.

## Examples

Rejected today-legal call:

```json
{"id": "abc123", "description": "Review passed."}
```

Error: `description_mode is required. Use "append" to add to the existing description, or "replace" to overwrite it.`

Correct part-addressed alternative:

```json
{"id": "abc123", "part": {"id": "p_01J0Y2Q9A0N2J7W5D3G8J1X7R2"}, "content": "Review passed.", "mode": "append"}
```

## Acceptance Criteria

1. `update_ticket` with `description` and no mode returns an error naming both modes; the ticket on disk is unchanged.
2. `description_mode: "replace"` and `"append"` both behave exactly as today when passed explicitly.
3. A part-addressed write to `review` leaves the `objective` part's file mtime and content unchanged.
4. `undo` after a part write restores that part only, leaving sibling parts untouched.
5. Constructing `UpdateTicketInput`, `UpdateTicketBody`, or `UpdateArgs` without `description_mode` fails with a compile-time missing-field error, and a targeted search/test shows no update path relies on an implicit default.
6. Agent instruction files documenting `update_ticket` state that the mode is required.
7. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-api/crates/ticket-api/src/storage/store.rs
- code: memory-api/crates/ticket-api/src/storage/ticket_fs.rs
- code: memory-api/tools/mcp/ticket-mcp/src/server.rs
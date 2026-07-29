## Objective

Freeze a ticket's planning parts when it enters `planned`, hard-reject writes to frozen parts, route corrections into `amendment` parts, and unfreeze only by transitioning the ticket back to a pre-`planned` state.

## Requirements

- Entering `planned` sets `frozen = true` on every planning part: `objective`, `requirements`, `acceptance_criteria`, `design`, `examples`.
- `review`, `validation`, `notes`, `amendment`, and free-form parts are never frozen and stay writable in every state.
- A write targeting a frozen part is hard-rejected. There is no `--force` and no bypass flag.
- The rejection error names the frozen part, the ticket state that froze it, and the two legal recoveries: record an `amendment`, or transition back to unfreeze and re-plan.
- An `amendment` part records which frozen part it supersedes.
- Transitioning to any pre-`planned` state clears `frozen` on all parts; re-entering `planned` re-freezes and records a new plan revision in history.
- Freeze and unfreeze events are visible in `history.ndjson`.
- There is no privileged bypass for migration: f65f2b32 first transitions a planned ticket back to a pre-`planned` state, splits the description, then re-enters `planned` so the freeze contract re-applies and a new plan revision is cut.

## Design

`TicketStore::update_with_options` and its helper `apply_manifest_update` remain the write chokepoint, and the named guard `TicketStore::enforce_part_write_gate` should run before any frozen part can reach `TicketFs::update` or `TicketFs::write_description`. The freeze metadata lives in the `frozen` field of each `[[parts]]` row introduced by 5a3d152c, so the on-disk state is inspectable without reading history.

Freeze and unfreeze ride the existing state-transition machinery in `TicketStore::resolve_update_target`, `TicketStore::resolve_transition_path`, and the `close`/`update` paths that already consult `SchemaRegistry`. That keeps the re-freeze on re-entry to `planned` aligned with the same transition bookkeeping that already produces history revisions.

`TicketFs::append_history` records the plan revision that corresponds to each freeze boundary, so planned-state transitions are visible in the same append-only audit trail as ordinary mutations.

## Implementation Steps

1. Extend the part manifest model in `memory-api/crates/memory-api/src/model/entity.rs` and `memory-api/crates/ticket-api/src/model/ticket.rs` so each part can carry `frozen` and optional `supersedes` metadata.
2. Add `TicketStore::enforce_part_write_gate` in `memory-api/crates/ticket-api/src/storage/store.rs` and call it from `update_with_options`, part-aware write helpers, and any other mutation path that can touch part content.
3. Update `memory-api/crates/ticket-api/src/storage/store/lifecycle.rs` so entering `planned` freezes the planning parts and leaving `planned` clears the frozen flags before the ticket can be edited again.
4. Append freeze/unfreeze markers in `memory-api/crates/ticket-api/src/storage/ticket_fs.rs` via `TicketFs::append_history` so the audit trail shows exactly when freeze state changed.
5. Teach amendment writes in `memory-api/crates/ticket-api/src/storage/store.rs` to persist the `supersedes` relationship alongside the amendment part and to keep the superseded part frozen.
6. Add regression tests in `memory-api/crates/ticket-api/src/storage/tests/update_regression_tests.rs` and `memory-api/crates/ticket-api/src/storage/store/lifecycle.rs` covering freeze rejection, re-freeze on re-entry, amendment supersedes, and no-false-positive writes to `review`.

## Examples

Rejected write and its error:

```
error: part `objective` is frozen (ticket entered `planned` at 2026-07-29T12:00:00Z)
  to correct it:  record an `amendment` part superseding `objective`
  to re-plan it:  transition the ticket back to `open`, edit, then return to `planned`
```

Amendment manifest entry:

```toml
[[parts]]
kind = "amendment"
path = "parts/amendment-001.md"
supersedes = "objective"
frozen = false
created_at = "2026-08-01T09:00:00Z"
```

## Acceptance Criteria

1. Transitioning a ticket to `planned` sets `frozen = true` on exactly the five planning parts and on no others.
2. A write to a frozen part is rejected; the part file is byte-identical afterwards.
3. The rejection error text names the part, the freezing state, and both recovery paths.
4. A write to `review` on a `planned` ticket succeeds.
5. Transitioning back to a pre-`planned` state clears every `frozen` flag; returning to `planned` re-sets them and appends a plan revision.
6. An `amendment` part records its `supersedes` target and is retrievable alongside the part it supersedes.
7. Every part-write path routes through `TicketStore::enforce_part_write_gate`; a regression test proves there is no alternate write entry point that can touch frozen content without the state-transition path.
8. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-api/crates/ticket-api/src/model/schema_registry.rs
- code: memory-api/crates/ticket-api/src/storage/ticket_fs.rs
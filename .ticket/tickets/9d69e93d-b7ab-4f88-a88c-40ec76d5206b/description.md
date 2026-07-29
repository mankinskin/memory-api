## Objective

Give tickets a typed `[[refs]]` manifest table for references to non-ticket entities, so a ticket reaches external context by pointer instead of inlining it, and absorb the existing untyped `related_specs` field.

## Requirements

- `ticket.toml` gains a `[[refs]]` table; each entry carries `kind`, a canonical URN, and an optional `note`.
- Supported ref kinds: `spec`, `test_execution`, `log`, `rule`, `file`, `commit`.
- An unknown ref kind is rejected with the valid vocabulary in the error.
- URNs are validated for shape against their kind; a `spec` ref resolves against the spec store, a `file` ref against the repo root.
- Existing `related_specs` values in the `extra` map migrate into `[[refs]]` as `kind = "spec"` with no loss; the old field is read for compatibility and no longer written.
- `[[refs]]` is parsed by the ticket manifest model in `memory-api/crates/ticket-api/src/model/filesystem.rs`, the same struct that owns `[[parts]]`, and it lands after 3d952036 so the write-gate change is already in place and the manifest schema does not collide with the write-default removal.
- Edges remain ticket-to-ticket only. This ticket does not change the edge model or migrate existing free-form edge kinds.
- A refs consistency check reports dangling refs without failing the read.

## Design

Refs live in the manifest rather than in the edge graph so a projection can render a ticket's external context without a graph traversal, and so a ref carries a note explaining why it is relevant. The edge graph keeps expressing dependency structure between tickets, which is a different question with different query needs.

`memory_api::model::entity::EntityManifest::related_specs()` and `set_related_specs()` in `memory-api/crates/memory-api/src/model/entity.rs` are the compatibility bridge from the old `related_specs`/`spec_ids` extra keys into structured refs. The new `[[refs]]` table stays in the ticket manifest model alongside `[[parts]]`, and `memory-api/crates/memory-api/src/model/edge.rs` remains the owner of ticket-to-ticket edge records only.

## Implementation Steps

1. Extend `memory-api/crates/ticket-api/src/model/filesystem.rs` and `memory-api/crates/ticket-api/src/storage/ticket_fs.rs` so `[[refs]]` is parsed, formatted, and round-tripped through the same `TicketManifest` model as `[[parts]]`.
2. Add a typed ref-entry structure in `memory-api/crates/memory-api/src/model/entity.rs` or the ticket manifest model that captures `kind`, `urn`, and `note` without changing the edge graph.
3. Implement `related_specs`/`spec_ids` migration in `memory-api/crates/ticket-api/src/storage/store.rs`, preserving identical spec identity while writing only `[[refs]]` on new saves.
4. Add kind-specific URN validation in `memory-api/crates/ticket-api/src/model/schema_registry.rs` and the ticket API storage path so malformed or mismatched refs fail at write time.
5. Update the refs consistency check in `memory-api/crates/ticket-api/src/storage/store.rs` to report dangling refs without blocking reads.
6. Add round-trip, migration, malformed-URN, dangling-ref, and edge-byte-identical tests under `memory-api/crates/ticket-api/src/storage/tests/` and `memory-api/crates/memory-api/src/model/entity.rs`.

## Examples

```toml
[[refs]]
kind = "spec"
urn = "ce://default/spec/24b3d22b-e235-4c4f-b53c-75fb819ea95b"
note = "contract this ticket implements"

[[refs]]
kind = "file"
urn = "memory-api/crates/ticket-api/src/storage/store.rs"
note = "replace-mode default lives here"

[[refs]]
kind = "test_execution"
urn = "ce://default/test-execution/7f2c1a04"
note = "freeze rejection evidence"
```

## Acceptance Criteria

1. All six ref kinds round-trip through write, list, and read without loss.
2. An unknown ref kind is rejected with the valid vocabulary in the error.
3. A malformed URN for a given kind is rejected at write time.
4. Every existing ticket carrying `related_specs` migrates to `[[refs]]` with identical spec identity; a diff of resolved spec ids before and after is empty.
5. Reading a ticket whose `spec` ref points at a deleted spec succeeds and reports the ref as dangling.
6. The edge store is byte-identical before and after this ticket's changes.
7. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- code: memory-api/crates/ticket-api/src/model/filesystem.rs
- code: memory-api/crates/memory-api/src/model/entity.rs
- code: memory-api/crates/memory-api/src/model/edge.rs
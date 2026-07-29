## Objective

Rename the ticket lifecycle states `new` -> `open` and `ready` -> `planned` in the memory-api ticket schema, and migrate every existing ticket and history record to the new names, so the `planned` state that plan freezing anchors on actually exists.

## Requirements

- The ticket type schema renames state `new` to `open` and state `ready` to `planned`; transition edges are preserved unchanged apart from the renamed endpoints.
- Every existing ticket manifest carrying `state = "new"` or `state = "ready"` is rewritten to the new name.
- Every `history.ndjson` record referencing the old state names is rewritten, preserving record order and all other fields byte-identically.
- Any schema-declared `required_states` list referencing the old names is updated.
- Transport surfaces that hardcode the old names (CLI defaults and help text, ticket-mcp tool schemas and descriptions, HTTP transport, ticket-viewer state labels and filters) are updated.
- Agent guidance and rule entries that name `new` or `ready` are updated.
- The migration is idempotent: a second run is a no-op producing an empty diff.

## Design

State definitions live with the ticket type schema loaded by `SchemaRegistry` in memory-api/crates/ticket-api/src/model/schema_registry.rs; the transition validation that rejected `new -> planned` is the same path. Manifest state is stored on the ticket manifest model in memory-api/crates/ticket-api/src/model/filesystem.rs and materialized by memory-api/crates/ticket-api/src/storage/ticket_fs.rs. History records are appended by memory-api/crates/ticket-api/src/storage/store.rs.

The rename is a pure relabel: no new state, no new transition, no change to the transition graph shape. Because the states are persisted as strings in both `ticket.toml` and `history.ndjson`, the migration is a two-file rewrite per ticket directory.

Run the migration as a dry-run first, reporting per-ticket which files change, before applying.

## Implementation Steps

1. Rename the state identifiers in the ticket type schema definition consumed by `SchemaRegistry` in memory-api/crates/ticket-api/src/model/schema_registry.rs, keeping the transition graph shape unchanged.
2. Update any `required_states` declarations and schema fixtures that reference `new` or `ready`.
3. Add a migration routine that walks the ticket store, rewrites `state` in each `ticket.toml`, and rewrites old state names in each `history.ndjson`, preserving record order.
4. Give the migration a dry-run mode that reports affected tickets and files without writing.
5. Update the ticket CLI: state arguments, defaults, help text, and any output formatting that names the old states.
6. Update the ticket-mcp tool schemas and tool descriptions that enumerate state names.
7. Update the ticket HTTP transport and ticket-viewer state labels, badges, and filter controls.
8. Update agent guidance and rule entries naming `new` or `ready`, then regenerate the rule-generated artifacts.
9. Run the dry-run against the real store, review the report, then apply and verify the diff touches only state strings.
10. Add tests: schema accepts `open -> planned`, rejects the removed names, and the migration is idempotent on a second run.

## Acceptance Criteria

1. The schema exposes states `open` and `planned`; a transition `open -> planned` is accepted and the old names are rejected with an error naming the valid set.
2. After migration, no `ticket.toml` in the store contains `state = "new"` or `state = "ready"`.
3. After migration, no `history.ndjson` record contains the old state names, and every record's other fields and ordering are byte-identical to the pre-migration file.
4. Re-running the migration produces an empty diff.
5. A dry-run run writes nothing and reports every ticket and file it would change.
6. `ticket --help`, the ticket-mcp tool schemas, the HTTP transport, and ticket-viewer all present `open` and `planned` with no occurrence of the old names.
7. A repository-wide search for the old state names in guidance and rule entries returns no stale occurrence.
8. A test-api validation execution is recorded for each acceptance criterion above, linked to this ticket id.

## Typed References

- spec: `ticket-api/entity/structured-ticket-entities` (24b3d22b)
- ticket: f9e70385 (plan freezing, blocked on this)
- code: memory-api/crates/ticket-api/src/model/schema_registry.rs
- code: memory-api/crates/ticket-api/src/model/filesystem.rs
- code: memory-api/crates/ticket-api/src/storage/ticket_fs.rs
- code: memory-api/crates/ticket-api/src/storage/store.rs

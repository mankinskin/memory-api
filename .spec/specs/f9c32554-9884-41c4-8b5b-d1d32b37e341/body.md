<!-- spec-api:file generated=true -->

<!-- spec-api:entry id=8ec75a50-bace-4f90-bae8-e6d16c8bc461 slug=memory-api/recurring-principles/memory-api-recurring-principles/l1 -->
# memory-api recurring principles

This spec captures the cross-cutting design principles that recur across `memory-api` specs (`rule-api`, `spec-api`, `ticket-api`, `doc-api`, `audit-api`, `mem-api`). They are the canonical authority for how store, CLI, MCP, and HTTP layers in `memory-viewers/memory-api` are expected to behave.

<!-- spec-api:entry id=8a92308a-a962-4d26-be51-f3d076865791 slug=memory-api/recurring-principles/memory-api-recurring-principles/l5 -->
Each principle is its own section so a `rule scan` materialises one canonical entry per principle and downstream agent guidance can reference them individually.

<!-- spec-api:entry id=fd8cf21d-0c9e-4036-a7d0-347c42b66642 slug=memory-api/recurring-principles/memory-api-recurring-principles/sections/l7 -->
## Sections

- `workspace-identifiers` — `--workspace-root` accepts only concrete checkout paths; `default`, `..`, and synthetic aliases are rejected.
- `typed-error-envelopes` — All CLI/MCP/HTTP errors are JSON envelopes with `code`, `message`, and `request_id`.
- `json-machine-surface` — `--json` emits a stable envelope; never dump raw `[items]` lists into stdout for machine consumers.
- `api-crate-owns-model` — Each `<x>-api` crate owns the canonical model; CLI/MCP/HTTP tools are thin adapters.
- `shared-id-prefix-resolver` — Ticket/spec/rule lookups share one id/prefix resolver instead of re-implementing per surface.
- `depends-on-only-blocking-edge` — `depends_on` is the only blocking edge; other relations are non-blocking and use the edge index rather than Tantivy.
- `append-only-history-materialized-index` — Stores write append-only history files and rebuild a materialized SQLite/Tantivy index from them.
- `nested-workspace-resolution` — The workspace resolver normalises any path to a single owning root; parents declare child stores via `imports:`.
- `required-states-one-way` — Tickets and specs use a `required_states` schema-gated, one-way state machine.

<!-- spec-api:entry id=74d940e6-88b0-4c20-be70-72cdd9db2b89 slug=memory-api/recurring-principles/memory-api-recurring-principles/related-tickets/l19 -->
## Related tickets

- [f147eb0e Migrate recurring spec principles to canonical rule entries via spec sync-generated](.ticket/tickets/f147eb0e-c758-459b-a956-a1162c3e1af6/ticket.toml)
- [a5fe4c58 Adopt rule targets for generated spec artifacts](memory-viewers/memory-api/.ticket/tickets/a5fe4c58-f59c-4d97-8ee6-3447724b5fac/ticket.toml)

<!-- spec-api:entry id=f2d4d117-cee4-4749-b542-43ea435c50f2 slug=memory-api/recurring-principles/memory-api-recurring-principles/related-specs/l24 -->
## Related specs

- `spec-api/generated-documents` (`1cf68c36-7f64-4d81-b553-1947b978fbe3` in memory-viewers/memory-api)
- `context-engine/recurring-principles` (`954d9807-f357-41e5-9fd4-b1da39e0933d` at the context-engine root)

# memory-api recurring principles

This spec captures the cross-cutting design principles that recur across `memory-api` specs (`rule-api`, `spec-api`, `ticket-api`, `doc-api`, `audit-api`, `mem-api`). They are the canonical authority for how store, CLI, MCP, and HTTP layers in `memory-viewers/memory-api` are expected to behave.

Each principle is its own section so a `rule scan` materialises one canonical entry per principle and downstream agent guidance can reference them individually.

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

## Related tickets

The canonical recurring-principles migration history is tracked by the context-engine root recurring-principles spec. Keep workspace-specific ticket links here only when `memory-api` needs additional follow-up beyond that shared owner.

## Related specs

- `spec-api/generated-documents` (`1cf68c36-7f64-4d81-b553-1947b978fbe3` in memory-viewers/memory-api)
- `context-engine/recurring-principles` (`954d9807-f357-41e5-9fd4-b1da39e0933d` at the context-engine root)

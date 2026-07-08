<!-- aligned-structure:v1 -->

# Summary

Capture the recurring design principles that define how `memory-api` store, CLI, MCP, and HTTP layers are expected to behave.

## Behavior Story

`memory-api` keeps one canonical recurring-principles spec so downstream stores and tools inherit shared constraints such as workspace resolution, error envelopes, id resolution, history/index behavior, and schema-gated state progression from a stable contract surface.

## Provided Surface Contracts

- The `memory-api` recurring-principles spec is the canonical authority for shared store and transport behavior across `memory-api` subsystems.
- Each principle is maintained as its own section so generated guidance can reference it independently.
- The current principles cover workspace identifiers, typed error envelopes, JSON machine output, API ownership, shared id resolution, blocking-edge semantics, materialized indexes, nested workspace resolution, and one-way required states.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- [f147eb0e Migrate recurring spec principles to canonical rule entries via spec sync-generated](.ticket/tickets/f147eb0e-c758-459b-a956-a1162c3e1af6/ticket.toml)
- [a5fe4c58 Adopt rule targets for generated spec artifacts](memory-api/.ticket/tickets/a5fe4c58-f59c-4d97-8ee6-3447724b5fac/ticket.toml)

## Background Knowledge References

- `spec-api/generated-documents` (`1cf68c36-7f64-4d81-b553-1947b978fbe3` in memory-viewers/memory-api)
- `context-engine/recurring-principles` (`954d9807-f357-41e5-9fd4-b1da39e0933d` at the context-engine root)

## Legacy Content (Preserved)

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

- [f147eb0e Migrate recurring spec principles to canonical rule entries via spec sync-generated](.ticket/tickets/f147eb0e-c758-459b-a956-a1162c3e1af6/ticket.toml)
- [a5fe4c58 Adopt rule targets for generated spec artifacts](memory-api/.ticket/tickets/a5fe4c58-f59c-4d97-8ee6-3447724b5fac/ticket.toml)

## Related specs

- `spec-api/generated-documents` (`1cf68c36-7f64-4d81-b553-1947b978fbe3` in memory-viewers/memory-api)
- `context-engine/recurring-principles` (`954d9807-f357-41e5-9fd4-b1da39e0933d` at the context-engine root)

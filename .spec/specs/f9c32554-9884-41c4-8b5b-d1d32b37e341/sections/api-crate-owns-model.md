# <x>-api crate owns the model

For each store domain (`ticket-api`, `spec-api`, `rule-api`, `doc-api`, `audit-api`, `mem-api`), the `<x>-api` crate owns the canonical model: entity types, store traits, validation, indexing, history, and the in-process API. CLI, MCP, and HTTP tools are thin adapters that translate transport into `<x>-api` calls.

## What lives in `<x>-api`

- Entity types, field definitions, and the `required_states` schema.
- Store traits and their default implementations (filesystem, SQLite index, Tantivy search).
- Append-only history writers and the materialised-index rebuild path.
- Validation, edge semantics (including `depends_on`), and health checks.
- The single id/prefix resolver shared across all surfaces.

## What lives in the adapter crates

- `<x>-cli`: clap argument parsing, JSON envelope rendering, exit-code mapping.
- `<x>-mcp`: tool descriptors that delegate to `<x>-api`.
- `<x>-http`: route handlers that delegate to `<x>-api` and serialise the envelope.
- `viewer-ctl`-managed viewers: read-mostly consumers that talk to `<x>-http` and never duplicate model logic.

## Non-duplication rule

Adapters must not re-implement model behaviour. If a CLI or MCP tool needs to enforce a precondition, the precondition lives in `<x>-api` and the adapter calls it. New cross-cutting features are added to `<x>-api` first, then exposed by the adapters in lockstep.

<!-- spec-api:file generated=true -->

<!-- spec-api:entry id=9f4cd7be-30c7-4771-adc4-7af1023a6752 slug=memory-api/recurring-principles/api-crate-owns-model/x-api-crate-owns-the-model/l1 -->
# <x>-api crate owns the model

For each store domain (`ticket-api`, `spec-api`, `rule-api`, `doc-api`, `audit-api`, `mem-api`), the `<x>-api` crate owns the canonical model: entity types, store traits, validation, indexing, history, and the in-process API. CLI, MCP, and HTTP tools are thin adapters that translate transport into `<x>-api` calls.

<!-- spec-api:entry id=053d49f3-f93a-428c-b1be-504e45d2e2e2 slug=memory-api/recurring-principles/api-crate-owns-model/x-api-crate-owns-the-model/what-lives-in-x-api/l5 -->
## What lives in `<x>-api`

- Entity types, field definitions, and the `required_states` schema.
- Store traits and their default implementations (filesystem, SQLite index, Tantivy search).
- Append-only history writers and the materialised-index rebuild path.
- Validation, edge semantics (including `depends_on`), and health checks.
- The single id/prefix resolver shared across all surfaces.

<!-- spec-api:entry id=4c0b0086-4846-4688-ba01-6e905b842185 slug=memory-api/recurring-principles/api-crate-owns-model/x-api-crate-owns-the-model/what-lives-in-the-adapter-crates/l13 -->
## What lives in the adapter crates

- `<x>-cli`: clap argument parsing, JSON envelope rendering, exit-code mapping.
- `<x>-mcp`: tool descriptors that delegate to `<x>-api`.
- `<x>-http`: route handlers that delegate to `<x>-api` and serialise the envelope.
- `viewer-ctl`-managed viewers: read-mostly consumers that talk to `<x>-http` and never duplicate model logic.

<!-- spec-api:entry id=091f1918-3f0b-44d0-bff0-323dd8fb03ae slug=memory-api/recurring-principles/api-crate-owns-model/x-api-crate-owns-the-model/non-duplication-rule/l20 -->
## Non-duplication rule

Adapters must not re-implement model behaviour. If a CLI or MCP tool needs to enforce a precondition, the precondition lives in `<x>-api` and the adapter calls it. New cross-cutting features are added to `<x>-api` first, then exposed by the adapters in lockstep.

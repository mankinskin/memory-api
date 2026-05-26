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
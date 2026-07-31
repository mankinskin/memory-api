# `depends_on` is the only blocking edge

In `ticket-api` and `spec-api`, a ticket or spec is considered actionable only if every `depends_on` target is in a resolved state. No other edge type blocks state transitions. Non-blocking relationships use the typed edge index, not Tantivy full-text search.

## Edge taxonomy

- `depends_on` — Blocking. A ticket cannot move to `ready` or beyond while any `depends_on` target is unresolved.
- `linked` / `related` / `references` — Non-blocking. Used for cross-store traceability, follow-ups, and reading suggestions; the health checks never gate on them.
- Domain-specific edges (`parent`, `child`, `spec_refs`, `code_refs`) — Non-blocking; the resolver and health checks treat them as informational.

## Edge index, not Tantivy

Edge lookups (forward, reverse, transitive closure) are served by the typed edge index in the materialised SQLite store. Tantivy is reserved for full-text search over body content. Reusing Tantivy for edge traversal pollutes search ranking and forces edge queries through an inverted-index query plan that is far slower than a primary-key join.

## Health behaviour

- `ticket health <id>` reports actionable-with-deps when a ticket has unresolved `depends_on` targets and is in `ready` or later.
- Replacing an unresolved `depends_on` edge with a `linked` edge clears the health finding without changing the ticket state.
- `spec refs <id> validate` cross-checks `depends_on` and `spec_refs` edges and reports missing or wrongly-typed references.

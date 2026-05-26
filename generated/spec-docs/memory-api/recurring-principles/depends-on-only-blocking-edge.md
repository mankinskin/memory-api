<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=d0c3db9e-2ec0-47f8-ad43-b618664631ee slug=memory-api/recurring-principles/depends-on-only-blocking-edge/depends-on-is-the-only-blocking-edge/l1 -->
# `depends_on` is the only blocking edge

In `ticket-api` and `spec-api`, a ticket or spec is considered actionable only if every `depends_on` target is in a resolved state. No other edge type blocks state transitions. Non-blocking relationships use the typed edge index, not Tantivy full-text search.

<!-- rule-api:entry id=331bac7c-1622-4690-a6a3-19a722070e49 slug=memory-api/recurring-principles/depends-on-only-blocking-edge/depends-on-is-the-only-blocking-edge/edge-taxonomy/l5 -->
## Edge taxonomy

- `depends_on` — Blocking. A ticket cannot move to `ready` or beyond while any `depends_on` target is unresolved.
- `linked` / `related` / `references` — Non-blocking. Used for cross-store traceability, follow-ups, and reading suggestions; the health checks never gate on them.
- Domain-specific edges (`parent`, `child`, `spec_refs`, `code_refs`) — Non-blocking; the resolver and health checks treat them as informational.

<!-- rule-api:entry id=32f4d829-eaad-48d7-9678-e71d8b81a215 slug=memory-api/recurring-principles/depends-on-only-blocking-edge/depends-on-is-the-only-blocking-edge/edge-index-not-tantivy/l11 -->
## Edge index, not Tantivy

Edge lookups (forward, reverse, transitive closure) are served by the typed edge index in the materialised SQLite store. Tantivy is reserved for full-text search over body content. Reusing Tantivy for edge traversal pollutes search ranking and forces edge queries through an inverted-index query plan that is far slower than a primary-key join.

<!-- rule-api:entry id=4b104cd8-ea68-4ef4-bc38-c2e35d3cb4f5 slug=memory-api/recurring-principles/depends-on-only-blocking-edge/depends-on-is-the-only-blocking-edge/health-behaviour/l15 -->
## Health behaviour

- `ticket health <id>` reports actionable-with-deps when a ticket has unresolved `depends_on` targets and is in `ready` or later.
- Replacing an unresolved `depends_on` edge with a `linked` edge clears the health finding without changing the ticket state.
- `spec refs <id> validate` cross-checks `depends_on` and `spec_refs` edges and reports missing or wrongly-typed references.

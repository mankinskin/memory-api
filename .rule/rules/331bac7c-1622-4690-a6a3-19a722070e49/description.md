## Edge taxonomy

- `depends_on` — Blocking. A ticket cannot move to `ready` or beyond while any `depends_on` target is unresolved.
- `linked` / `related` / `references` — Non-blocking. Used for cross-store traceability, follow-ups, and reading suggestions; the health checks never gate on them.
- Domain-specific edges (`parent`, `child`, `spec_refs`, `code_refs`) — Non-blocking; the resolver and health checks treat them as informational.
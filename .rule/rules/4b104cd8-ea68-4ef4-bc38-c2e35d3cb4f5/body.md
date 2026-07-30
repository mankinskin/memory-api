## Health behaviour

- `ticket health <id>` reports actionable-with-deps when a ticket has unresolved `depends_on` targets and is in `planned` or later.
- Replacing an unresolved `depends_on` edge with a `linked` edge clears the health finding without changing the ticket state.
- `spec refs <id> validate` cross-checks `depends_on` and `spec_refs` edges and reports missing or wrongly-typed references.
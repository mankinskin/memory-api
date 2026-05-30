## Schema declaration

- Each type's TOML schema lists the allowed states in order and, for each state, the fields/edges that must be present before the entity is allowed to enter it.
- Transitions skipping a required state are rejected at the store layer; the CLI/MCP/HTTP adapters surface the rejection as a typed error envelope (`code: precondition_failed`).
- New states require a schema change; they cannot be introduced ad-hoc by callers.
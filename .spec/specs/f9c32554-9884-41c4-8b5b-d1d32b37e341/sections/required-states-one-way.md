# Required-states one-way state machine

Ticket and spec types declare their state machine through `required_states` in the type schema. Transitions are one-way (no automatic backward moves) and are gated by the schema, not by ad-hoc CLI/MCP logic.

## Schema declaration

- Each type's TOML schema lists the allowed states in order and, for each state, the fields/edges that must be present before the entity is allowed to enter it.
- Transitions skipping a required state are rejected at the store layer; the CLI/MCP/HTTP adapters surface the rejection as a typed error envelope (`code: precondition_failed`).
- New states require a schema change; they cannot be introduced ad-hoc by callers.

## One-way semantics

- Forward transitions are linear (`new → ready → in-implementation → in-review → done`, with type-specific variations).
- Backward transitions are explicit and audited: `ticket update --undo` rewinds to the previous history record; there is no "set state back to X" operation that bypasses history.
- Entities created directly in a non-initial state cannot be `--undo`'d because there is no prior history record. Authoring tools should create entities in the initial state and then transition them forward.

## Health interaction

- Health checks read the schema and report `actionable-with-deps`, `missing-required-field`, or `invalid-transition` findings without mutating state.
- A schema change that tightens requirements re-runs health on existing entities; pre-existing entities that violate the new requirements are reported as findings and not silently demoted.

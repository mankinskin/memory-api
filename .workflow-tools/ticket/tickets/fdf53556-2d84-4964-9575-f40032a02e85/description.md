# Origin

Carved from the **original scope** of 8bb97b73. That parent session delivered and validated only the **session-mcp** enum-rejection slice; its REVIEWER NOTE flags the ticket-transition scope as NOT delivered. This ticket delivers that carved scope.

# Acceptance Criteria

- Blocked ticket transitions return current state + allowed next states + mandatory intermediate states.
- CLI, ticket-MCP, and HTTP mutation surfaces share the recovery-field shape already used by session-mcp.
- A command/view shows the legal transition graph/enum set for the current ticket.
- Docs reflect the enforced state values.

# Implementation Status — in-implementation (reopened 2026-07-25 after review)

Delivered the ticket-state-transition recovery contract:

- `SchemaValidationError::InvalidTransition` now carries `from`, `to`, `allowed_next`, `intermediate`; Display names current state, allowed next states, and mandatory intermediate waypoint(s). (crates/memory-api/src/error.rs, model/schema.rs)
- `EntityTypeSchema::allowed_next_states` + `invalid_transition_error` helpers; `ensure_transition` now emits the rich error (benefits spec-api/rule-api too).
- ticket-api `resolve_transition_path` is strict: `update --to-state` performs one legal hop and rejects skipped-waypoint transitions.
- New `ticket transitions <id>` CLI inspection command: current state, allowed next, full transition graph, required/terminal states.
- CLI, ticket-MCP, HTTP share the recovery message via `StorageError` Display.
- Docs: .agents/instructions/ticket-system.instructions.md.

Validation: vt-ticket-transition-recovery / exec-vt-ticket-transition-recovery-20260725 (passed).

# REVIEW FINDINGS — 2026-07-25 (reviewer: send back to implementation)

REJECTED default behavior change. The strict single-hop default (blocking `new -> in-implementation` with a recovery message instead of auto-walking) is NOT the desired behavior.

- **Desired:** auto-hops ARE desired. Allowed PATHS in the transition graph should be traversed automatically by default (restore the pre-existing auto-walk).
- **Required change:** strict single-hop block+explain must become an **opt-out command option**, not the default.
- The two rewritten auto-walk regression tests must be restored/adjusted to assert auto-walk-by-default + strict-on-flag.

ACCEPTED and must NOT regress: the rich `InvalidTransition` error shape + Display, `allowed_next_states`/`invalid_transition_error` helpers, the `ticket transitions` inspection command, undirected `allowed_next` (internally consistent), and the docs.

Corrective work tracked in follow-up **16d8aed9** (relates). This ticket is done only after auto-walk-by-default + opt-out strict flag land and tests/docs are updated.

# Likely Surfaces

- crates/ticket-api/, tools/ticket-cli/, tools/ticket-mcp/, .agents/instructions/ticket-system.instructions.md
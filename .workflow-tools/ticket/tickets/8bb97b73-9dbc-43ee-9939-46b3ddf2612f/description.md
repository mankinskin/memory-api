# Problem

Invalid transitions and enum-valued parameters are enforced, but they are not explained
well enough at the moment they fail. The rejection names what was wrong but not what is
allowed, forcing manual schema/help spelunking to recover.

This applies to two surfaces that share the same disease:

1. **Ticket state transitions** (original scope). `new -> in-implementation` was rejected,
   and the only practical recovery was to inspect the schema and CLI help, then manually
   infer the required progression through intermediate states.
2. **session-mcp workflow mutation enums** (added scope). In session `367ac6a3` an agent
   called `session_workflow_add_node` with an invalid `kind`, got
   `"invalid workflow node kind: {value}"` with no list of legal values, and **abandoned
   the durable-workflow feature entirely** rather than guess ("I'll anchor the interview
   record after your answers rather than fight the enum"). The session was then interrupted
   and there was no durable node to resume from.

Both are technically correct behavior but poor operator/agent UX, and both are fixed by the
same recovery contract: a rejected value must return the allowed set.

# Session Evidence

## Ticket transitions
- The session attempted to move a new ticket directly into `in-implementation` and hit the state-machine guard.
- The operator then had to route the ticket through `in-refinement` and `ready` before implementation could begin.
- The failure path did not directly surface the allowed next states or the required intermediate sequence.

## session-mcp workflow enums (session 367ac6a3)
- `session_workflow_add_node.kind` is a bare `String` in `WorkflowAddNodeInput`
  (`memory-api/tools/mcp/session-mcp/src/server.rs`), so the JSON schema advertises no enum.
- `parse_node_kind` / `parse_edge_kind` / `parse_node_status` / `parse_requirement`
  reject with `"invalid workflow <thing>: {value}"` and never list the legal values.
- Real legal values live in `SessionWorkflowNodeKind` (`ticket, action, decision, checkpoint,
  validation`), `SessionWorkflowEdgeKind` (`depends-on, order`), and
  `SessionWorkflowNodeStatus` (`pending, in-progress, blocked, done, deferred`).
- The agent could not discover them from the failure and gave up on durability.

# Scope

1. Extend transition/enum failures so they return machine-readable guidance:
   - current value (for state machines: current state)
   - allowed values (for state machines: allowed next states)
   - required intermediate states, when applicable
   - ticket type / schema name / enum name
2. Teach the CLI to print a clear recovery message with the legal values/transitions and example retry commands.
3. Expose the same guidance through MCP and any HTTP-facing mutation surfaces.
4. Add a lightweight command or sub-view to inspect legal transitions/enums for a given ticket or session-mcp mutation.
5. Apply the recovery contract to session-mcp workflow mutation parsers
   (`parse_node_kind`, `parse_edge_kind`, `parse_node_status`, `parse_requirement`) so each
   rejection enumerates the allowed values.
6. Update workflow docs so the user sees the same progression/enum set described in the tools and generated guidance.

# Regression Validation Requirements

- **Specification / docs:** define the invalid-transition/invalid-enum error contract and the required recovery fields.
- **CLI:** add integration coverage for one blocked transition like `new -> in-implementation` and assert the error lists the legal next steps.
- **MCP (ticket):** add parity coverage for the same transition error structure.
- **MCP (session):** add coverage that an invalid `session_workflow_add_node` / `add_edge` / `set_status`
  value returns an error listing the allowed values.
- **Schema-aware validation:** include at least one ticket type with a nontrivial path so guidance is not hard-coded to a single state machine.
- **Manual validation:** reproduce the exact blocked ticket transition and the session 367ac6a3 workflow-kind
  rejection; confirm the first error message is sufficient to recover without separate help spelunking.

# Acceptance Criteria

- A blocked ticket transition reports the current state and allowed next states; mandatory intermediate states are named.
- An invalid session-mcp workflow `kind`/`status`/edge-`kind` returns an error listing the allowed values.
- CLI, ticket-MCP, session-MCP, and any related mutation surfaces use the same recovery-field shape.
- One command or view can show the legal transition graph / enum set for the current ticket or session-mcp mutation.
- Workflow docs and generated guidance reflect the same state/enum values the tools enforce.

# Likely Surfaces

- `crates/ticket-api/`
- `tools/ticket-cli/`
- `tools/ticket-mcp/`
- `memory-api/tools/mcp/session-mcp/src/server.rs`
- `memory-api/crates/session-api/`
- `.agents/instructions/ticket-system.instructions.md`
- `memory-api/.spec/`

# Implementation Status — in-review (2026-07-25)

Delivered (session-mcp + session-cli scope, the added scope of this ticket): `parse_node_kind`, `parse_edge_kind`, `parse_node_status`, and `parse_requirement` in both `memory-api/tools/mcp/session-mcp/src/server.rs` and `memory-api/tools/cli/session-cli/src/lib.rs` now enumerate the allowed values on rejection (including deprecated-alias notes and snake/kebab alias notes). This satisfies the "MCP (session)" regression requirement and pairs with `7f1ed44f`'s schema-level advertisement to deliver the full enum-discoverability contract.

Validation: `vt-session-workflow-tooling-fix` / `exec-vt-session-workflow-tooling-fix-20260725` (passed). Test `invalid_workflow_values_report_allowed_set` asserts each rejection lists the allowed set.

REVIEWER NOTE — remaining original-scope (NOT delivered in this session): the ticket-state-transition recovery contract for `ticket-api`/`ticket-cli`/`ticket-mcp` (allowed-next-states, required intermediate states, HTTP parity, and the transition-inspection command) was out of scope for this session, which was chartered to extend this ticket for the session-mcp surface. Confirm the ticket-transition portion before closing beyond in-review.
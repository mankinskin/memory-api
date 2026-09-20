## Problem

When a session workflow mutation is rejected, the error lists allowed values but does not tell the agent what to do instead. Observed in session `aedf210d` (turns 60, 62): the `kind` rejection and the `only ticket workflow nodes may set ticket_urn` rejection each triggered a fresh guessing round.

## Requirement

Every session workflow rejection must embed a ready-to-use alternative pattern the agent can copy directly.

## Acceptance criteria

1. `parse_node_kind` rejection includes: "for a custom label, use kind=task with category=\"<your-label>\"". Reference: memory-api/tools/mcp/session-mcp/src/server.rs (`parse_node_kind`).
2. The `only ticket workflow nodes may set ticket_urn` / `...spec_urn` rejections (memory-api/crates/session-api/src/store/config/runtime_workflow.rs, `workflow_add_node`) name the supported anchor alternative (the new non-gating anchor field from the schema ticket, or pinning the entity).
3. `parse_requirement`, `parse_edge_kind`, `parse_node_status` rejections each include a one-line "did you mean" example using a legal value.
4. Existing `invalid_workflow_values_report_allowed_set` test is extended (or a sibling added) asserting each rejection contains an actionable alternative, not just the allowed set.

## Notes

Keep messages compact (single line each) to respect token-efficiency guidance.
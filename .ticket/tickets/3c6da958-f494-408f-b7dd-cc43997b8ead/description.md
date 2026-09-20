## Problem

The workflow node model already carries a free-text `category` field that exists "so agents never hit an expressiveness wall for labels that do not drive behavior" (memory-api/crates/session-api/src/model/workflow.rs), but it is invisible in the MCP tool schema, so agents invent illegal `kind` values instead (session `aedf210d` turn 60). Separately, `ticket_urn`/`spec_urn` are hard-gated to their own node kinds, so a validation/task node cannot record which ticket/spec it belongs to (turn 62).

## Requirement

Make the escape hatch discoverable and let any node kind carry a non-gating anchor to a ticket/spec, without weakening finish-gating semantics.

## Acceptance criteria

1. `WorkflowAddNodeInput.category` gets a schema description steering agents to it for custom labels (kind stays `task`). Reference: memory-api/tools/mcp/session-mcp/src/server.rs (`WorkflowAddNodeInput`).
2. Add a non-gating `anchor_urn` (or equivalent) field usable by ANY node kind to reference a ticket/spec for context/resumability. Finish-gating stays bound to `kind` = ticket/spec/validation + their existing required URN fields; `anchor_urn` never gates finish.
3. `workflow_add_node` validation (memory-api/crates/session-api/src/store/config/runtime_workflow.rs) accepts `anchor_urn` on any kind, validates it parses as a ticket/spec URN, and no longer forces agents to drop context when the node is not that kind.
4. Persisted model round-trips the new field; back-compat: existing persisted contexts without the field still load.
5. Tests cover: task node with `category` + `anchor_urn` to a ticket succeeds; ticket-kind finish-gating unchanged.

## Design note

Prefer adding a distinct `anchor_urn` over overloading `ticket_urn`, so the gating vs. reference distinction stays explicit and the finish/handoff logic in memory-api/crates/session-api/src/store/config/handoff_finish.rs is untouched.
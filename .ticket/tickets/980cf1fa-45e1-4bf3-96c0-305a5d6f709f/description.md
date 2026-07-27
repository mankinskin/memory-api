## Problem

A `validation` workflow node can be created with a null `validation_spec_id`. Once created, that node permanently wedges the session: `session_handoff` and `session_finish` both reject with

```
session finish is blocked: required validation node <node_id> is missing validation_spec_id
```

and **there is no repair path through the MCP surface**:

- `session_workflow_add_node` / `session_workflow_add_nodes` are documented as a no-op on a duplicate `node_id`, so re-adding the node with the field set silently does nothing.
- `session_workflow_set_status` only mutates `status`.
- There is no node delete, and no node field-patch tool.
- Passing `validation` gates to `session_handoff` does not override the per-node check.
- Starting a new run does not reset the graph — the workflow is session-scoped, not run-scoped.

The only way out is hand-editing `.session/runtime/workspaces/<id>/context.json`.

## Observed incident (2026-07-27)

Session `0101b7ef-e717-4c94-bebd-c8d55f6aaa82`. The `b0d6bb1c` lane created four required validation nodes with no `validation_spec_id`:

| node_id | title |
|---|---|
| `123213da-1465-4777-822b-e056f5f0ffb2` | AC4: test_cost_gate.py passes |
| `12975a73-4ae8-4012-bcfb-1ac4c319976d` | AC1: --check round-trips with both sources |
| `671a2d6b-f509-4882-ad95-fc1366bbe6c1` | AC3: precedence documented and deterministic |
| `851ab3fa-1275-4530-b533-0a164bba9680` | AC2: MAI-Code-1-Flash resolves to real price via --query |

Two subsequent repair attempts made it worse by adding *new* required+pending nodes instead of patching:

- `c296e3d5-8c8f-4f47-98f1-69344149af35` — "Validation for spec 123213da", whose `validation_spec_id` was set to the **node UUID** `123213da-...`, which is not a validation spec at all.
- `62894c27-de71-43b3-af60-4d3e5d17ad02` — a duplicate of the AC4 node.

Net effect: 4 blocking nodes became 6, and the session could not hand off for ~50 minutes.

**Repaired manually** by editing `context.json`: the four originals were given `vt-model-prices-cost-gate`, `vt-model-prices-check-roundtrip`, `vt-model-prices-precedence-doc`, `vt-model-prices-query-resolves`, and the two malformed nodes were deleted.

## Acceptance criteria

1. `session_workflow_add_node` and `session_workflow_add_nodes` reject `kind="validation"` with an absent or empty `validation_spec_id`, with an error naming the field and, for the batch tool, the offending `nodes[index]`.
2. The same rejection applies to `kind="ticket"` without `ticket_urn` and `kind="spec"` without `spec_urn`, so all three gating kinds fail fast and consistently at creation.
3. A repair surface exists for nodes that gate incorrectly — either a field-patch tool (e.g. `session_workflow_update_node`) or a delete tool (`session_workflow_remove_node`), or both. It must be able to fix a node that already exists in a wedged graph.
4. The finish/handoff rejection message names the concrete repair tool to call, instead of only stating the node is missing the field.
5. `validation_spec_id` is validated to resolve to an actual spec in the `.test` store, so a node UUID cannot be passed as a spec id.
6. Tests cover: creation rejection for each of the three gating kinds; repair of an already-wedged graph via the new surface; and a round-trip proving handoff succeeds after repair.
7. Duplicate-`node_id` no-op behavior is documented in the tool description itself, not only in the instruction file, so callers do not assume re-add patches.

## Non-goals

- Changing the finish-gating semantics of `validation` / `ticket` / `spec` nodes.
- Migrating existing wedged sessions automatically.

## Related

Blocks reliable operation of the closed-loop iteration workflow, which requires a persisted handoff on every run.

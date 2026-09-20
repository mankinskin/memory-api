## Problem

Every workflow node and edge is created one call at a time. Building a densely-linked graph (e.g. 4 review criteria + edges) multiplies both round-trips and the cost of any single schema mistake. In session `aedf210d` this turned one logical action into 12 calls.

## Requirement

Add batch creation so agents can build many-linked workflow graphs in a single call, and so a schema error fails the batch atomically with one actionable message.

## Acceptance criteria

1. New `session_workflow_add_nodes` MCP tool accepting an array of node drafts; validates all, then inserts atomically (all-or-nothing). Reference: memory-api/tools/mcp/session-mcp/src/server.rs, memory-api/crates/session-api/src/store/config/runtime_workflow.rs (`workflow_add_node`).
2. New `session_workflow_add_edges` MCP tool accepting an array of edges; same atomic semantics.
3. Batch validation errors identify the offending array index and the actionable fix (aligns with the self-correcting-errors ticket).
4. CLI parity in memory-api/tools/cli/session-cli/src/lib.rs (or explicit note if CLI batch is deferred).
5. Tests: batch insert of N nodes + M edges succeeds in one call; one bad element rejects the whole batch with an indexed message.

## Notes

Idempotency: existing single-node insert already no-ops on duplicate node_id; preserve that behavior per-element in the batch.
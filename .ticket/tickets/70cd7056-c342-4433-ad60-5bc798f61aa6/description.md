# Goal

Implement the durable logical session workspace and mutable workflow graph defined by spec `memory-api/session-api/durable-session-workflow` (`c677182e-90da-4ac3-8b94-9e2e97c825cf`).

## Scope

- Add a durable `workspace_session_id` distinct from per-agent capture `run_id` values.
- Persist workflow state separately from transcript capture.
- Support ticket-backed nodes containing authoritative URNs plus cached display metadata.
- Support session-only action, decision, checkpoint, and validation nodes.
- Add stable node IDs, required/optional classification, statuses, timestamps, and directed dependency/order edges.
- Support add/update/link/remove-safe operations and promotion of a temporary node to a ticket URN.
- Resolve ticket state live when reading or finishing; do not copy ticket lifecycle state into session storage.
- Flush every mutation before returning and preserve graph state across idempotent init/resume.
- Keep feedback emission optional through an injected sink.

## Acceptance Criteria

1. A workflow can be initialized, mutated, reloaded, and resumed without losing nodes, edges, pins, or run lineage.
2. A new run resumes the same `workspace_session_id` with a distinct `run_id` and optional predecessor.
3. Ticket and temporary nodes coexist and promotion preserves identity.
4. Missing ticket references return per-node diagnostics without corrupting persisted state.
5. Focused `session-api` tests cover persistence, idempotency, mutation, promotion, and live-state adapter behavior.

## Depends on

- Runtime pinned-context foundation `412964a3-e1c3-47da-94ad-268ff20441c0`.

## Spec

`c677182e-90da-4ac3-8b94-9e2e97c825cf`.
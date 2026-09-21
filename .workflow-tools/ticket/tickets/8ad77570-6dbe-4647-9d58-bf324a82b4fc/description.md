# [memory-matrix] Promote non-empty log_session_ids in subprocess failure bundles

## Goal

Close the remaining observability gap where subprocess failure bundles still emit empty `linkage.log_session_ids` when runtime log session capture is unavailable.

## Scope

- Integrate matrix subprocess failure-bundle emission with runtime log-session registration paths once they land.
- Populate `linkage.log_session_ids` deterministically in failing probe executions when sessions are available.
- Preserve empty-array fallback only for explicitly unavailable session contexts, with clear reason text.
- Add deterministic assertions covering non-empty session linkage and execution-correlation continuity.

## Dependencies and Coordination

- Depends on root observability tracker child [cc78d33d](../memory-api/.ticket/tickets/cc78d33d-1744-4945-bb77-f0fd1142568e/ticket.toml) for baseline failure-bundle shape.
- Cross-workspace coordination blockers:
  - `memory-viewers/.ticket/tickets/60a2a388-c8b6-4e25-a80a-0ba686f11bf9/ticket.toml` ([LOG-1b] init_tracing_full file-logging wiring)
  - `memory-api/.ticket/tickets/12197242-b7b4-4212-83a8-4b0b65a4bd7b/ticket.toml` ([LOG-2a] tracing field-name normalization)

## Acceptance Criteria

- At least one deterministic subprocess failure probe records non-empty `linkage.log_session_ids`.
- Persisted execution evidence keeps `test_execution_id`, `correlation.run_id`, and non-empty `log_session_ids` aligned.
- Documentation in the owning ticket references retrieval flow for correlated log sessions.

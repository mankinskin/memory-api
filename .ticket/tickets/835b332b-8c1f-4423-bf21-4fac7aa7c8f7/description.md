## Problem
The health check emits `unblocked_with_deps` (info) whenever a `ready` ticket has non-terminal dependencies. But `ready` does not mean "unblocked" — it means "groomed". A `ready → ready` dependency chain is legitimate and should NOT warn.

## Desired behavior
1. Remove the blanket `unblocked_with_deps` warning for `ready` tickets whose deps are simply not `done`.
2. Instead, only flag when a ticket's state is AHEAD of a dependency's state — already covered by the `dependency_convergence` finding (via `workflow.dependency_state_inversions`).
3. Add a **state-update guard**: reject/warn on a transition that moves a ticket further along the state schema than one of its `depends_on` targets.

## Progress (this session)
- DONE: Removed the `unblocked_with_deps` emission from all four sites (canonical + triplicated transports):
  - memory-api/crates/ticket-api/src/health.rs (append_dependency_state_findings)
  - memory-api/tools/cli/ticket-cli/src/cli/commands/ops/health/findings.rs
  - memory-api/tools/http/ticket-http/src/serve/handlers/graph/quality/findings.rs
  - memory-api/tools/mcp/ticket-mcp/src/server/health/findings.rs
- Kept the `dependency_convergence` inversion check (the correct "ahead of deps" signal). `workflow.unresolved_dependencies` retained — still used by `next` scheduling.
- Validation: `cargo build -p ticket-api -p ticket-cli -p ticket-http -p ticket-mcp` green; `cargo test -p ticket-api` = 107 passed, 1 failed (pre-existing unrelated `move_planner::preflight_reports_invisible_reference_visibility_and_path_refs`).

## Remaining
- State-update guard in memory-api/crates/ticket-api/src/storage/store.rs (`resolve_transition_path`) + schema (`validate_workflow`): prevent a ticket advancing past a `depends_on` target's state-schema index. State ordering source: memory-api/crates/ticket-api/src/model/default_schema.rs.
- Tests for: ready→ready (no finding — implicitly covered), ahead-of-dep transition rejected/flagged, behind-or-equal allowed.

## Acceptance criteria
- ready→ready no longer produces a health finding. [DONE]
- A transition advancing a ticket past an unfinished dependency is guarded at the API layer (parity across CLI/MCP/HTTP). [PENDING]
- Tests cover the state-ordering comparison. [PENDING]
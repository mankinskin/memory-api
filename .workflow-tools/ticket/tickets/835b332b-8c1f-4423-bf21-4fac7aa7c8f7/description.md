## Problem
The health check emits `unblocked_with_deps` (info) whenever a `ready` ticket has non-terminal dependencies. But `ready` does not mean "unblocked" — it means "groomed". A `ready → ready` dependency chain is legitimate and should NOT warn.

## Desired behavior
1. Remove the blanket `unblocked_with_deps` warning for `ready` tickets whose deps are simply not `done`.
2. Instead, only flag when a ticket's state is AHEAD of a dependency's state — already covered by the `dependency_convergence` finding (via `workflow.dependency_state_inversions`).
3. Add a **state-update guard**: reject a transition that moves a ticket further along the state schema than one of its `depends_on` targets.

## Implementation (COMPLETE)
- DONE: Removed the `unblocked_with_deps` emission from all four sites (canonical + triplicated transports):
  - memory-api/crates/ticket-api/src/health.rs (append_dependency_state_findings)
  - memory-api/tools/cli/ticket-cli/src/cli/commands/ops/health/findings.rs
  - memory-api/tools/http/ticket-http/src/serve/handlers/graph/quality/findings.rs
  - memory-api/tools/mcp/ticket-mcp/src/server/health/findings.rs
- DONE: Kept the `dependency_convergence` inversion check (the correct "ahead of deps" signal). `workflow.unresolved_dependencies` retained — still used by `next` scheduling.
- DONE: State-update guard `enforce_dependency_progress` landed in memory-api/crates/ticket-api/src/storage/store.rs (called from the transition path at store.rs:606; guard body at store.rs:657) with typed `StorageError::DependencyNotProgressed`.
- DONE: Test coverage in memory-api/crates/ticket-api/src/storage/tests/workflow_tests.rs (asserts `DependencyNotProgressed` on ahead-of-dependency transitions).
- Validation: `cargo build -p ticket-api -p ticket-cli -p ticket-http -p ticket-mcp` green; `cargo test -p ticket-api` = 107 passed, 1 failed (pre-existing unrelated `move_planner::preflight_reports_invisible_reference_visibility_and_path_refs`).

## Acceptance criteria
- ready→ready no longer produces a health finding. [DONE]
- A transition advancing a ticket past an unfinished dependency is guarded at the API layer via typed `DependencyNotProgressed`. [DONE — CLI/MCP/HTTP inherit the store-layer guard]
- Tests cover the state-ordering comparison. [DONE — workflow_tests.rs]

## Review note (2026-07-11)
Description reconciled during EPIC 3be95a71 review pass: the two previously-`PENDING` criteria were already implemented and committed (verified against store.rs + workflow_tests.rs). Effort set to 1200. Advanced ready→in-implementation→in-review to match committed state. Close to `done` only after a fresh `cargo test -p ticket-api` gate re-confirms green (modulo the known unrelated move_planner failure).
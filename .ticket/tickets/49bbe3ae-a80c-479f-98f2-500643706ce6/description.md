# Goal
Drive a focused performance-characterization track for `ticket move` and `ticket health` so we can reproduce slow operations from the workspace-cleanup work and measure them before optimization.

# Scope
- Own the representative fixture expansion for slow move and health operations.
- Own aggressive end-to-end timing coverage for CLI-facing move and health flows.
- Own Criterion benchmarks for move preflight/execute/rollback and health scopes.
- Own failure-path and pessimistic-path scenarios that intentionally provoke slow behavior.

# Related spec
- `af0ae2a0` `ticket-api/performance/move-health-characterization`

# Acceptance criteria
- [ ] Representative fixture variants exist for root-store, parent↔submodule, path-reference-heavy, and large health traversals.
- [ ] E2E timing coverage exists for the representative `ticket move` and `ticket health` paths.
- [ ] Benchmarks exist for representative move and health operations with timing output suitable for regression tracking.
- [ ] Slow-path and failure-path scenarios are covered and can identify where runtime is spent.
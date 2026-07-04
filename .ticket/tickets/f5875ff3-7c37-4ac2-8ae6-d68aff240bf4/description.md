# Goal
Extend `memory-fixtures` so ticket move and health performance tests can materialize representative slow scenarios without bespoke setup in each test.

# Concrete extensions
- Add benchmark-scale ticket populations to both root and nested stores.
- Add path-reference-heavy tracked files that force multiple rewrite candidates during move.
- Add large graph / ticket-state mixes that make `health --all` and subgraph health representative.
- Add fixture variants that intentionally create slow but valid cross-worktree and sequential-move scenarios.

# Acceptance criteria
- [ ] Fixture helpers expose reusable variants for move-heavy and health-heavy workloads.
- [ ] Ticket-api tests and benches can consume the same fixture materialization helpers.
- [ ] Fixture data includes both success and failure-provoking scenarios.
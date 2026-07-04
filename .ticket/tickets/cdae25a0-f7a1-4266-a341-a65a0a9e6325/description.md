# Goal
Benchmark representative `ticket move` and `ticket health` operations so slow slices are measurable with Criterion instead of anecdotal CLI timing.

# Concrete benchmarks
- Criterion benches for move preflight, move execute, and rollback on representative fixture variants.
- Criterion benches for health on single-ticket, subgraph, and `--all` scopes across realistic fixture sizes.
- Benchmark groups that vary rewrite count, ticket count, and cross-worktree topology to expose scaling costs.

# Acceptance criteria
- [ ] Benchmarks produce repeatable measurements for move and health operations.
- [ ] At least one benchmark dimension scales fixture size upward to expose nonlinear behavior.
- [ ] Benchmark names are specific enough to isolate expensive phases.
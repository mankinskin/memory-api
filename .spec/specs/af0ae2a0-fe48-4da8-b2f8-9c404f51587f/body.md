# Goal
Create representative end-to-end performance coverage for `ticket move` and `ticket health` so slow operations are measurable, reproducible, and diagnosable before optimization work.

# Problem
Current coverage emphasizes scan and graph-query paths, but not the real CLI move/health flows the workspace-cleanup work is exercising. That leaves slow file-rewrite, reference-visibility, root-store traversal, and cross-worktree move costs under-instrumented.

# Scope
- Extend canonical fixtures with representative root-store and nested-store ticket layouts, path-reference-heavy files, and large-enough ticket populations to reproduce slow move and health operations.
- Add end-to-end tests that record timings for `plan_move_preflight`, `execute_move_with_journal`, `rollback_move_with_journal`, and representative health scopes.
- Add Criterion benchmarks for the same operations and fixture variants.
- Include failure-provoking scenarios that intentionally exercise slow or pessimistic paths.

# Non-goals
- Optimizing the slow paths in this spec.
- Defining final SLA thresholds before representative baselines exist.

# Acceptance criteria
1. The fixture crate can materialize benchmark-scale layouts representative of root-store tickets, nested workspaces, path-reference-heavy rewrites, and large `health --all` traversals.
2. Ticket-api owns e2e tests that time representative `move` and `health` operations across at least root-only, parent-to-submodule, and reference-heavy scenarios.
3. Ticket-api owns Criterion benchmarks for representative move preflight, move execute/rollback, and health scopes using the shared fixture variants.
4. The aggressive test surface includes failure and slow-path cases such as missing tracked reference files, many rewritten tracked files, large catalogs, and repeated sequential moves.
5. The resulting evidence identifies which operation slices dominate runtime so later optimization tickets can target them precisely.
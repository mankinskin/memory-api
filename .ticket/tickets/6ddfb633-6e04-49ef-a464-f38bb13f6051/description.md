# Goal
Add aggressive end-to-end tests that time and stress `ticket move` and `ticket health` using representative fixtures.

# Concrete coverage
- Time `plan_move_preflight`, `execute_move_with_journal`, `resume_move_with_journal`, and `rollback_move_with_journal` across root-only and parent↔submodule layouts.
- Time `health --all`, root-subgraph health, and reverse-dependency health on representative stores.
- Capture pessimistic scenarios: many tracked path rewrites, missing tracked files, large generated catalogs, repeated sequential moves, and mixed reference visibility.
- Assert timing output is recorded and large regressions/failure modes are visible in test logs.

# Acceptance criteria
- [ ] E2E tests cover representative move and health paths with timing instrumentation.
- [ ] Tests intentionally provoke failure and slow-path cases, not just happy paths.
- [ ] The resulting tests are focused enough to use during local perf debugging.
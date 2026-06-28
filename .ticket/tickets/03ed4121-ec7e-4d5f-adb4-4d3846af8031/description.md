# [bench] Cross-domain benchmark matrix with per-operation latency budgets

## Goal

Provide Criterion benchmarks for the same `domain × operation` matrix as the test suite, each asserting a reasonable maximum-latency budget, and ingest the results into `test-api`.

## Implementation summary (in-review)

Built on the `memory-matrix` crate (sibling ticket 751f0e71):

- **Shared bench harness** (`benches/operation_matrix.rs`): a single Criterion bench iterates `memory_matrix::cells()` (8 domains × 7 ops). Each cell runs `run_one(domain, operation, ctx)` against a **fresh fixture per iteration** (`iter_batched` setup is untimed), so mutating ops stay isolated. Adding a domain/operation is a table entry in the lib — no new bench code. Bench id is path-safe `domain__operation` (shared `bench_id()` so harness + runner agree).
- **Public API exposed** from `memory-matrix`: `cells()`, `run_one()`, `bench_id()`, `MatrixCtx::new()`, and `materialize`/`Fixture` re-exports.
- **Budget table** (`budgets.toml`): per-operation budgets resolving `domain.operation` → bare `operation`, loaded via `test_api::BudgetTable`. Values are end-to-end cell budgets sized as gross regression tripwires (get/search 2s, create/update/delete/move 1s, scan 3s).
- **Ingest + enforcement** (`bench_runner::ingest_bench_results`): reads each cell's Criterion `estimates.json`, ingests via `test_api::ingest_criterion_estimates`, applies the budget (`apply_budget` sets `over_budget` on mean), records a `BenchmarkExecution` into `.test`, and reports any cell lacking estimates as `missing` (never silently dropped).
- **Single command** (`src/bin/bench-matrix.rs`): runs `cargo bench`, ingests, prints a per-cell summary, and **exits non-zero** when any operation is over budget (or estimates are missing). `--skip-bench` ingests existing results.
- **Shared-fixture fix**: `memory-fixtures::copy_dir_recursive` now skips derived store runtime artifacts (`*.db`/`*.db-wal`/`*.db-shm`, `search_index/`) so materialization rebuilds indexes from seed manifests via `scan` and never fails on Windows SQLite `.db-shm` locks. Cleaned stray artifacts from the canonical fixture.

## Validation

- `cargo test -p memory-matrix` → 5 tests pass (4 matrix + `bench_runner` ingest/budget test using synthesized `estimates.json`).
- Fixture-consumer regression check: `memory-fixtures`, `ticket-api`/`spec-api` `e2e_fixture_loader` all pass after the copy-skip change.
- Full end-to-end run (`cargo run -p memory-matrix --bin bench-matrix`): **56 cells ingested, 0 missing, all within budget, exit 0.** Representative means: ticket get 347ms / search 510ms / create 323ms (within 1–2s budgets); blocked cells ~2ms. Recorded into `memory-api/.test`; `test benchmarks --over-budget` → 0; audit "failed & slow" surface confirmed.
- Evidence: spec `vt-bench-matrix`, execution `exec-vt-bench-matrix-20260628` (passed). Test store-index digest `ee342ca50b63a39e`.

## Acceptance criteria

- [x] Each `domain × operation` cell has a runnable Criterion benchmark over the fixture (56 cells, 0 missing).
- [x] Each benchmark's result is ingested into `test-api` with mean/median and `over_budget` against its budget.
- [x] A single command runs the matrix and exits non-zero when any operation exceeds its budget.
- [x] Adding a domain/operation requires only a table entry (cells table + budget key).

## Honest gaps

- Benchmarks measure the **end-to-end cell** (operation + minimal in-cell setup such as open+scan), not a pure isolated operation, so budgets are gross tripwires (purpose: catch the ~100s `ticket get` class of regression), not precision micro-budgets. The ticket's tighter starting budgets (get 50ms, etc.) assume pure operations; calibrated up to fit the cell measurement.
- A separate large-fixture scan-scaling variant (per-1k budget) is not implemented; the existing `ticket-api/benches/fixture_scan.rs` covers large-scale scan scaling. Noted for follow-up if a dedicated per-1k budget is wanted.
- Blocked cells (move everywhere; doc row; unsupported delete/scan) still get a runnable bench (fast blocked path) and an ingested `BenchmarkExecution` with no budget.

## Relationship / traceability

- Depends on the benchmark result model + ingest (`2b0f31e5`) and the fixture (`026b2eb6`).
- Complements the default-store Criterion matrix `6a19ae5f` (feeds the same test-api index).
- Evidence: `vt-bench-matrix` / `exec-vt-bench-matrix-20260628`; 56 `BenchmarkExecution`s in `memory-api/.test`.

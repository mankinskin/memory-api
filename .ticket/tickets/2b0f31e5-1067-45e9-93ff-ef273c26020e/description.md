# [test-api] Benchmark result model + Criterion ingest + latency budgets

## Goal

Let `test-api` record benchmark results (not just pass/fail validations) and compare them against per-operation maximum-latency budgets, ingesting existing Criterion output.

## Scope

- New `BenchmarkExecution` model in `test-api`:
  - `id`, `benchmark_name`, `operation` (e.g. `ticket.get`), `domain` (e.g. `ticket`), `executed_at`,
  - `mean_ns`, `median_ns`, `std_dev_ns`, `min_ns`, `max_ns`, optional `throughput`,
  - `budget_ns: Option<u64>` and derived `over_budget: bool`,
  - `links: ValidationLinks`.
- Persist under `.test/<workspace>/benchmarks/`; add `record_benchmark` / `get_benchmark` / `list_benchmarks(query)` with filters by domain/operation/over_budget.
- A **Criterion ingest** helper that reads `target/criterion/<bench>/new/estimates.json` and maps it onto `BenchmarkExecution`.
- A **budget table** (config file, e.g. `.test/budgets.toml`) mapping `domain.operation` → max latency; ingest stamps `budget_ns` and `over_budget`.

### Initial budget defaults (to be calibrated against fixture `026b2eb6`)

| Operation | Initial max budget |
|---|---|
| get (by id) | 50 ms |
| search (query) | 200 ms |
| create | 100 ms |
| update | 100 ms |
| delete | 100 ms |
| move (plan) | 250 ms |
| scan (small fixture) | 1 s |

## Acceptance criteria

- [ ] `BenchmarkExecution` persists, round-trips, and is queryable by domain/operation/over_budget.
- [ ] Criterion ingest maps `estimates.json` to a `BenchmarkExecution` with mean/median/stddev/min/max populated.
- [ ] A budget table stamps `budget_ns`/`over_budget`; over-budget benchmarks are queryable.
- [ ] Unit tests cover ingest mapping and over-budget classification.

## Relationship / traceability

- Depends on the execution timing model.
- Consumed by the benchmark matrix and the store-index generator under tracker.

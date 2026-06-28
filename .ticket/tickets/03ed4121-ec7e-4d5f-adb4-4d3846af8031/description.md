# [bench] Cross-domain benchmark matrix with per-operation latency budgets

## Goal

Provide Criterion benchmarks for the same `domain × operation` matrix as the test suite, each asserting a reasonable maximum-latency budget, and ingest the results into `test-api`.

## Concrete matrix

Same rows/columns as the operation test matrix: domains `ticket/spec/rule/audit/session/test/doc/log` × operations `get/search/create/update/delete/move-plan/scan`.

Each benchmark:
- runs against a fixed-size slice of fixture `026b2eb6` (small for per-op latency; large variant for scan/search scaling),
- is registered as a Criterion `[[bench]]`,
- has a budget from the budget table (see benchmark-model ticket); the harness records a `BenchmarkExecution` with `over_budget` set.

### Starting budgets (calibrate against fixture)

| Operation | Budget |
|---|---|
| get | 50 ms |
| search | 200 ms |
| create / update / delete | 100 ms |
| move-plan | 250 ms |
| scan (small) | 1 s |
| scan (large, per 1k entities) | 2 s |

## Scope

- A shared bench harness parameterized by `domain × operation` so a new cell is a table entry, not a new file.
- Criterion result ingest into `test-api` `BenchmarkExecution` (reuse the ingest helper).
- A CI/local command that runs the matrix and reports over-budget operations as failures (or surfaced issues).

## Acceptance criteria

- [ ] Each `domain × operation` cell has a runnable Criterion benchmark over the fixture.
- [ ] Each benchmark's result is ingested into `test-api` with mean/median and `over_budget` against its budget.
- [ ] A single command runs the matrix and exits non-zero (or emits issues) when any operation exceeds its budget.
- [ ] Adding a domain/operation requires only a table entry.

## Relationship / traceability

- Depends on the benchmark result model + ingest, and the fixture `026b2eb6`.
- Complements the default-store Criterion matrix `6a19ae5f` (referenced textually; feeds the same test-api index).

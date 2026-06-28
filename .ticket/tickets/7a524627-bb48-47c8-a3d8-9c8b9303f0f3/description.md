# [test-api][log-api] Validation runner harness — cargo test/bench → executions + log capture

## Goal

Provide a runner that executes test/benchmark suites, captures their output and timing, and records them into `test-api` (executions/benchmarks) and `log-api` (stdout/stderr/summary) — the "execute" half of the unified surface.

## Scope

- A harness/CLI entry that wraps `cargo test` / `cargo bench` (and the matrices from the test/bench tickets) and for each case:
  - measures wall time → `ValidationExecution.duration_ms` (or `BenchmarkExecution`),
  - captures stdout/stderr → `log-api` `ValidationLogCapture` (locator into `target/test-logs/`), linked by `validation_execution_id`,
  - maps test result → `ValidationOutcome` (Passed/Failed/Blocked),
  - tags failures and over-budget/slow runs.
- Reuse existing tracing (`init_test_tracing!`, `config/tracing.toml`, `target/test-logs/`) for per-test logs; the harness records the log locator rather than re-implementing capture.
- Idempotent re-runs: a run id groups executions for one invocation.

## Acceptance criteria

- [ ] Running the harness over the matrices records one execution per case with duration and a linked log capture.
- [ ] Failed and slow/over-budget cases are tagged and discoverable via `test-api` queries.
- [ ] Log captures resolve to readable files under `target/test-logs/`.
- [ ] A single documented command runs a suite and populates `.test` + `.log` stores.

## Relationship / traceability

- Depends on the execution timing model.
- Produces the data the store-index generator and audit surface consume.

# [memory-api][test] Unified validation & benchmark surface in test-api (tracker)

## Goal

Build one extensible, executable testing surface anchored in `test-api` that, at minimum, delivers:

1. **Operation tests** for basic `get` / `search` / CRUD / `move` / `scan` across **all memory domains** (ticket, spec, rule, audit, session, test, doc, log).
2. **Benchmarks** for every one of those operations with a **reasonable expected maximum latency budget** that the suite asserts against.
3. **test-api store-index generation** that produces a committed summary of all test/benchmark runs — statuses, issues, and timings — like the other domain store-index generators.

Motivated by a confirmed severe regression: a single `ticket get` measured **96–107s** in this workspace, with no surface today that would have flagged it. The unified surface must make failed and unreasonably slow operations visible automatically.

## Scope

The children below form a DAG: a timing/benchmark data model and runner first, then the cross-domain test and benchmark matrices, then the index generator and surfaces, plus a dedicated fix for the `ticket get` latency.

## Child slices

1. `test-api`: execution timing model + slow-test query (foundation).
2. `test-api`: benchmark result model + Criterion ingest + latency budgets.
3. Cross-domain operation test matrix — get/search/CRUD/move/scan × 8 domains.
4. Cross-domain benchmark matrix with per-operation latency budgets.
5. `test-api` store-index generator — run/status/issue/timing summary.
6. Validation runner harness — cargo test/bench → test-api executions + log-api capture.
7. `test`/`log` CLI + audit "failed & slow" query surface.
8. Investigate & fix the ~100s `ticket get` latency.

## Acceptance criteria

- [ ] All 8 child slices are linked under this tracker and individually satisfy their acceptance criteria.
- [ ] The operation test matrix runs get/search/CRUD/move/scan for every domain and records each as a `ValidationExecution` with a duration.
- [ ] Every operation in the matrix has a benchmark with an asserted maximum-latency budget; budget breaches fail the suite or are surfaced as issues.
- [ ] `test-api` generates a committed store-index summarizing every run with status, issues, and timing, regenerated via a documented command/hook.
- [ ] The audit surface answers "which validations failed and which are unreasonably slow" from the test-api index.
- [ ] The `ticket get` latency is root-caused and reduced to a documented budget, measured against the representative fixture.

## Relationship / traceability

- Consumes the representative multi-store/submodule fixture `026b2eb6` ([memory-api] E2E test workspace fixture repository).
- Cross-store context (default `.ticket` store, referenced textually — edges cannot cross stores): `ef3f4a91` [profiling] Performance profiling & benchmark matrix tracker and its children (`6a19ae5f` native Criterion matrix, `c37ea985` CLI/HTTP/MCP e2e matrix, `2d59b99c` throughput/latency, `d8d18128` index doc) — those produce Criterion/e2e runs that feed this test-api index rather than duplicating it.
- The cross-worktree move work (`21e6c015`) and generic move kernel (`0a510279`) provide the `move` operation under test across domains.

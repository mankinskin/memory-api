# [test-api] Backfill existing repo test/bench suites into the store

## Goal

Ingest the repo's actual `cargo test`/`cargo bench` corpus (and, eventually, TS/browser suites) so the test-api index reflects what actually runs — not just the ~14 hand-authored `vt-*` specs.

## Resolved decisions

- **D7 — scope:** **whole workspace corpus eventually.** Native Rust `cargo test`/`bench` first; **browser/TypeScript** suites are integrated per design session `93b8a331` (this ticket depends on it for the TS/browser portion).
- **D3 — provenance:** map each discovered case → typed provenance (`source_path`, `test_id`) from `a03d8a97`.

## Scope

- A backfill pass (building on runner harness `7a524627`) that runs the workspace suites, parses results (libtest JSON / cargo output / Criterion estimates.json), and records executions/benchmarks with provenance + log locators.
- Idempotent under a run id (honoring the 2-run retention cap).
- Documented command/hook to refresh the corpus.
- Native Rust corpus is in scope now; TS/browser ingestion lands after `93b8a331`.

## Brutally-honest expectations

- Ingest **real outcomes** — failing/slow cases are recorded as Failed/over-budget, not normalized away.
- Do not fabricate executions for tests that did not run; absence is reported, not invented.

## Acceptance criteria

- [ ] Backfill records one execution per discovered native test case with duration + outcome + log locator + provenance.
- [ ] Benchmarks ingested as `BenchmarkExecution`s with timings.
- [ ] Re-runs are idempotent under a run id and honor the 2-run cap.
- [ ] The store-index/audit surface reflects the ingested corpus.
- [ ] TS/browser ingestion is deferred to `93b8a331` and explicitly noted as not-yet-covered.

## Relationship / traceability

- Depends on runner harness `7a524627`, provenance `a03d8a97`, and design session `93b8a331`.
- Feeds `90de77b1` and `26d6353a`.

# [test-api] Store-index generator — run/status/issue/timing summary

## Goal

Generate a committed `test-api` store-index that summarizes all validation and benchmark runs — statuses, issues, and timings — following the domain-owned thin-generator architecture used by the other store indexes.

## Scope

- A generator (thin, in the test domain) that reads the `.test` store (executions + benchmarks) and renders a committed summary index file (TOON sidecar + human-readable markdown), consistent with the memory-index roadmap (`fe098673`, default store).
- Summary contents:
  - per `domain.operation`: latest outcome, pass/fail/blocked counts, last duration, rolling min/median/max,
  - **issues list**: failed executions and over-budget benchmarks with their reasons/links,
  - **slow list**: executions/benchmarks exceeding their `slow_threshold_ms` / `budget_ns`,
  - run metadata: last run timestamp, total executions, total benchmarks.
- Deterministic digest so unchanged data yields an unchanged file (hook-friendly).
- A documented regeneration command and optional git-hook integration.

## Acceptance criteria

- [ ] Running the generator produces a committed index file summarizing every recorded execution and benchmark with status, issues, and timings.
- [ ] The summary explicitly surfaces failed runs and over-budget/slow runs in dedicated sections.
- [ ] Regeneration is deterministic (stable digest) and documented; re-running without data changes is a no-op diff.
- [ ] Tests cover summary aggregation (counts, min/median/max, issue/slow extraction).

## Relationship / traceability

- Depends on the execution timing model and the benchmark result model.
- Aligns with memory-index roadmap `fe098673` and its store-index generators (default store, referenced textually).
- Feeds the audit "failed & slow" surface.

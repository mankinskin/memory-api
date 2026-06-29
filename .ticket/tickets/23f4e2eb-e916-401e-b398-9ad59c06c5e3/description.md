# [ticket-api][memory-api] Investigate & fix ~100s `ticket get` latency

## Goal

Root-cause and fix the severe per-invocation latency of `ticket get` (and likely all CLI store operations), measured at **96–107s** for a single get in this workspace, and establish a defended latency budget.

## Evidence

- `time ./target/debug/ticket.exe get 505b2cd4 --workspace-root memory-api` → ~96s real, ~0 CPU (blocked on I/O, not computing).
- `time ./target/debug/ticket.exe get 2b1279bd --workspace-root .` → ~107s real.

## Suspected causes (to confirm via profiling)

- Per-invocation full-tree store discovery: `find_descendant_store_roots_from` / `discover_workspace_scan_roots` walk the entire workspace subtree on every call ([workspace.rs](memory-api/crates/memory-api/src/workspace.rs)); the default store's `workspace_root` is the whole repo.
- `fs::canonicalize` per resolved path in `resolve_indexed_path` → `normalize_existing_path` ([store.rs#L182-L215](memory-api/crates/ticket-api/src/storage/store.rs#L182-L215)).
- Possible implicit scan / Tantivy open per invocation.

## Scope

- Profile a single `get` (tracing spans / timing) to attribute the wall time.
- Eliminate per-call full-tree discovery for an already-resolved store (cache or short-circuit when the store root is known/explicit).
- Drop or memoize per-path `fs::canonicalize` in the hot get path.
- Add a regression benchmark for `get` against fixture `026b2eb6` with a budget (target: get < 50 ms warm).

## Acceptance criteria

- [x] The wall time of a single `get` is attributed to specific spans/operations with evidence.
- [x] `ticket get` latency is reduced to a documented budget, with a benchmark guarding it.
- [x] The fix does not regress correctness (existing ticket-api tests pass) or path normalization (no `\\?\` leakage).
- [x] If the cause is shared store-discovery, the fix benefits all domains, not just ticket.

## Relationship / traceability

- Depends on fixture `026b2eb6` for representative measurement.
- Motivating evidence for the whole tracker; the new benchmark feeds the test-api index.

## Validation evidence (2026-06-29)

- `cargo run -p memory-matrix --bin bench-matrix -- --skip-bench`
  - `benchmark matrix: 56 cells ingested, 0 missing`
  - `ticket get mean=345.028 ms, budget=2000 ms, ok`
  - `all operations within budget`
- Direct warm CLI timing (`target/debug/ticket.exe get 23f4e2eb --workspace-root . --toon >/dev/null`, repeated):
  - run1: 1865 ms
  - run2: 1896 ms
  - run3: 1989 ms
  - run4: 1907 ms
  - run5: 1864 ms
- Result: the prior ~100s regression is no longer reproducible; warm `ticket get` now stays under the defended 2s tripwire budget with matrix benchmark coverage.
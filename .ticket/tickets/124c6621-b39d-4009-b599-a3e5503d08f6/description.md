# [test-api] Execution timing model + slow-test query

## Goal

Make `test-api` capture how long each validation took and let callers query for slow runs, so the unified surface can flag "unreasonably slow" operations.

## Scope

- Extend `ValidationExecution` ([memory-api/crates/test-api/src/lib.rs#L127](memory-api/crates/test-api/src/lib.rs#L127)) with:
  - `duration_ms: Option<u64>` (wall time of the validated operation/command),
  - optional `throughput: Option<f64>` (ops/sec or items/sec) for batch operations.
- Extend `ExecutionQuery` ([memory-api/crates/test-api/src/store.rs#L48](memory-api/crates/test-api/src/store.rs#L48)) with `min_duration_ms` / `max_duration_ms` and a `SlowestFirst` sort option (in addition to the existing newest-first).
- Add a `slow_threshold_ms` concept (per-spec optional budget on `ValidationSpec`) so "unreasonably slow" is defined per operation, not globally.
- Keep file-based JSON storage; update serde + round-trip tests.

## Acceptance criteria

- [ ] `ValidationExecution` persists and round-trips `duration_ms` (and `throughput` when set); existing records without the field still deserialize.
- [ ] `ExecutionQuery` supports min/max duration filters and slowest-first ordering.
- [ ] A `ValidationSpec` can carry an optional `slow_threshold_ms`; a helper classifies an execution as `over_budget` when `duration_ms > slow_threshold_ms`.
- [ ] Unit tests cover duration round-trip, duration filtering, slowest-first sort, and over-budget classification.

## Relationship / traceability

- Foundation for benchmark ingest, the test/benchmark matrices, the store-index generator, and the audit surface under tracker.

# [test-api] Execution provenance + persisted per-cell evidence

## Goal

Make stored executions individually discoverable and traceable to the exact test. Today the per-cell matrix executions are written to the ephemeral fixture tempdir and discarded; the committed roll-up carries no source path, test id, domain, operation, or transport.

## Resolved decisions

- **D3 — model:** add **typed fields** to the `test-api` model (not `detail` string encoding).
- **D4 — persistence:** persist per-cell executions into the **committed** `memory-api/.test` store grouped by a **run id**, with a **retention cap of 2 runs** (older runs pruned automatically).

## Scope

- Extend `ValidationExecution` / `ValidationSpec` with typed provenance: `source_path`, `test_id` (cell/case id), `domain`, `operation`, `transport`, `run_id`.
- Implement run-id grouping + retention pruning (keep newest 2 runs) in the test store.
- Migration/back-compat for existing records (older runs without the fields remain readable).
- Update `memory-matrix` + the transport matrix (`387843e4`) to populate the fields.
- Update the store-index generator (`90de77b1`) and CLI (`26d6353a`) to display/filter by the new fields.

## Acceptance criteria

- [ ] An execution resolves to an exact `(source_path, test_id, domain, operation, transport, run_id)`.
- [ ] Per-cell runs persist into the committed store grouped by run id; only the **newest 2 runs** are retained.
- [ ] `test list` filters by domain/operation/transport/run.
- [ ] Existing records remain readable after migration.
- [ ] Retention pruning is covered by a test (including the boundary where a 3rd run evicts the 1st).

## Relationship / traceability

- Depends on the timing/benchmark models (`124c6621`, `2b0f31e5`).
- Consumed by `387843e4`, `274c5119`, `90de77b1`, `26d6353a`.

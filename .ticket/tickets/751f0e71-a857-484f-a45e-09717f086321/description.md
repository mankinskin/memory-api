# [test] Cross-domain operation test matrix — get/search/CRUD/move/scan × all domains

## Goal

Implement an end-to-end test matrix that exercises the basic operations of **every memory domain** against the representative fixture, recording each run as a `test-api` `ValidationExecution` with a duration.

## Concrete matrix

Domains (rows): `ticket`, `spec`, `rule`, `audit`, `session`, `test`, `doc`, `log`.
Operations (columns): `get`, `search`, CRUD (`create`/`update`/`delete`), `move`, `scan`.

## Implementation summary (in-review)

New crate `memory-api/crates/memory-matrix` (lib + `tests/matrix.rs`), wired into the workspace.

- **Data-driven harness**: a `DomainOps` trait (one impl per domain) × `OPERATIONS` columns. Every operation defaults to `Blocked`-with-reason; a domain overrides only the ops its storage API genuinely supports. The fixed harness loop (`run_cell`) times each cell, maps `Ok(Passed)`/`Ok(Blocked)`/`Err`→`Passed`/`Blocked`/`Failed`, records a per-cell `ValidationSpec` + `ValidationExecution` (with `duration_ms`) into the materialized fixture's `.test` store. **Adding a domain = a new trait impl in `domains()`; adding an operation = a new `OPERATIONS` column + trait method + dispatch arm. No harness-loop changes.**
- **Coverage**: 8 domains × 7 ops = 56 cells, each materialized against fixture `026b2eb6`.
  - `ticket`, `spec`, `rule`: full get/search/create/update/delete/scan **pass** with correctness assertions.
  - `test`, `log`: create/get/search/update pass; `delete`/`scan` blocked-with-reason (append-only store, no scan API).
  - `session`: create/get/search/update(append) pass; delete/scan blocked-with-reason.
  - `audit`: scan(`sync_source_files`)/search(`indexed_files`) pass; create blocked (needs full `audit()` metrics snapshot); get/update/delete blocked.
  - `doc`: all cells blocked-with-reason (doc-api is a read-only cargo-doc surface with no entity store).
  - `move`: blocked-with-reason across all domains until the generic move kernel (`0a510279`) lands.
- **No silent skips**: every unsupported cell records a `Blocked` execution carrying a concrete reason.

## Validation

- `cargo test -p memory-matrix` → 4 integration tests pass (~8.5s): every cell records an execution with a duration; no cell `Failed`; core CRUD domains pass get/search/CRUD/scan; move cells blocked-with-reason; executions persisted/queryable from the workspace `.test` store.
- Evidence recorded via the `test run` harness into `memory-api/.test` / `.log`: spec `vt-cross-domain-matrix`, execution `exec-vt-cross-domain-matrix-20260628` (passed, 8483ms), log capture `exec-vt-cross-domain-matrix-20260628-log`. Test store-index digest refreshed to `cb6efab1cdf5d469`.

## Acceptance criteria

- [x] Every `domain × operation` cell runs and produces a recorded `ValidationExecution` with a duration (or a `blocked` execution with a reason).
- [x] Correctness assertions pass for get/search/CRUD/scan on the full-store domains (ticket/spec/rule) and for every supported op on the remaining domains; genuinely unsupported ops (doc entirely; delete/scan on append-only stores; audit create) are recorded `blocked`-with-reason rather than skipped.
- [x] Adding a new domain or operation requires only a new matrix row (trait impl) / column (trait method) + per-cell spec, no new harness-loop code.
- [x] The suite runs via a single documented command (`cargo test -p memory-matrix`) and writes executions into the workspace `.test` store.

## Honest gaps

- doc-api has no entity store, so its row is entirely `blocked`-with-reason; full CRUD/scan parity for doc would require new doc-api storage (out of scope here).
- `move` cells depend on `0a510279` / `21e6c015`; recorded `blocked` until those land.

## Relationship / traceability

- Depends on the execution timing model (`124c6621`) and the fixture (`026b2eb6`).
- Move cells depend on `0a510279` / `21e6c015`.
- Evidence: test-api spec `vt-cross-domain-matrix` + execution `exec-vt-cross-domain-matrix-20260628`.

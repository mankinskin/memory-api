# Goal

Extend `log-api` beyond validation-only captures so it can represent runtime log sessions for tools, servers, tests, benchmarks, graph operations, journals, and agent sessions.

## Scope

- Add runtime log session identity and metadata types.
- Track component, transport, operation/tool/route, workspace/store roots, process/run ids, file locator, format, rotation policy, active filters, start/end/status, and byte-offset checkpoints.
- Add links to tickets, specs, docs, validation executions, benchmark ids, agent/session ids, journal ids, and graph operation ids.
- Preserve compatibility for existing validation log capture types.

## Implementation evidence

Implemented in `memory-api/crates/log-api`:

- `src/lib.rs`
  - Added `RuntimeLogLinks` with link helpers for spec/ticket/validation/benchmark/agent-session/journal/graph-operation ids.
  - Added runtime session enums: `RuntimeLogTransport`, `RuntimeLogStatus`, `RuntimeLogFormat`.
  - Added `RuntimeLogSession` model with lifecycle + metadata fields (active/completed status, start/end, run/process ids, locator/format/media, rotation policy, filters, offset checkpoint, and links).
  - Added serde round-trip tests for runtime sessions.

- `src/store.rs`
  - Added `RuntimeLogSessionQuery` filters.
  - Added runtime session APIs: `record_runtime_session`, `get_runtime_session`, `list_runtime_sessions`.
  - Added runtime session path/store layout under `.log/<workspace>/sessions/<id>.json`.
  - Added filtering by status, transport, component, run id, and cross-link ids.
  - Added unit tests for record/get/not-found/list-filter behavior.

- `src/error.rs`
  - Added `RuntimeSessionNotFound` error variant.

## Validation

Command run:

- `cargo test --manifest-path memory-api/crates/log-api/Cargo.toml`

Result:

- Passed (`11 passed`).

## Acceptance criteria status

- `log-api` can register, retrieve, list, and filter runtime log sessions independently of validation captures: complete.
- Existing validation-log tests continue to pass: complete.
- Metadata can represent active and completed logs: complete (`RuntimeLogStatus` + `started_at`/`ended_at`).

## Traceability

Dependency path for `ff6637f5` profiling evidence tracker is now unblocked for runtime log-session modeling and cross-store metadata linkage.
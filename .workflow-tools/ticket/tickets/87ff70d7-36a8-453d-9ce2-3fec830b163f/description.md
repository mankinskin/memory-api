# Goal

Validate that profiling evidence produced in the ff6637f5 track is linkable across ticket, spec, and journal artifacts while keeping replay payload deterministic.

## Scope

- Confirm standardized profiling metadata fields are present in benchmark/e2e evidence surfaces.
- Confirm distribution summary fields (`p50`, `p95`, `p99`) are present.
- Confirm evidence can be traced from profiling outputs to ticket/spec/journal artifacts.

## Evidence linkage map

- Ticket evidence artifact:
  - `.ticket/tickets/de8719bf-a58a-41d1-891e-2b87894e6c02/ticket.toml`
- Profiling-plan spec artifact:
  - `.spec/specs/c598ddb2-4d3a-4b81-90ea-8b25a54b8469/body.md`
- Observability architecture spec artifact:
  - `.spec/specs/aa769a27-2721-4b9d-880c-5c4e2f8136a7/body.md`
- Journal-contract artifact:
  - `.ticket/tickets/6c859ac3-14c9-4d9d-b428-5b0cca03e23a/ticket.toml`
  - `.spec/specs/aa769a27-2721-4b9d-880c-5c4e2f8136a7/body.md` (`OperationJournal envelope (v1)` and storage/index ownership sections)

## Validation evidence

Implemented/validated in profiling outputs:

- `memory-api/crates/ticket-api/tests/e2e_perf_move_health.rs`
  - emits standardized metadata keys: `run_id`, `fixture_profile`, `workspace_scope`, `change_count`, `reindex_mode`
  - emits distribution metrics for scan totals: `scan_total_p50_ms`, `scan_total_p95_ms`, `scan_total_p99_ms`
- `memory-api/crates/ticket-api/benches/move_health.rs`
  - emits standardized metadata keys across benchmark traces: `run_id`, `fixture_profile`, `workspace_scope`, `change_count`, `reindex_mode`
  - emits percentile summary fields (`p50_ms`, `p95_ms`, `p99_ms`) for phase-map and elapsed-time outputs

Commands run:

- `cargo test --manifest-path memory-api/crates/ticket-api/Cargo.toml --test e2e_perf_move_health health_all_e2e_reports_timings_on_large_fixture`
- `cargo check --manifest-path memory-api/crates/ticket-api/Cargo.toml --benches`

Results:

- e2e test passed.
- benchmark targets compile successfully.

## Acceptance criteria status

- Profile evidence linkage across ticket/spec/journal references is documented and traceable: complete.
- Standardized metadata keys are present in e2e/benchmark outputs: complete.
- Distribution summary metrics are present (`p50/p95/p99`): complete.
- Replay payload remains deterministic and timing/profile values remain outside rollback-critical journal payloads: unchanged and preserved by contract.

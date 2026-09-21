# Goal

Decompose ticket-api integration and workflow recompute profiling into explicit sub-phase timings aligned with observability phase taxonomy.

## Scope

- Split integration timing into: manifest parse, index upsert, edge writes, description read, search upsert.
- Split workflow recompute timing into: dependency-edge fetch, dependency-ticket fetch, unresolved-count calculation, workflow-facts write.
- Preserve existing aggregate timing fields while adding sub-phase detail.

## Implementation evidence

Implemented in `memory-api/crates/ticket-api`:

- `src/storage/store/scan.rs`
  - Added integration sub-phase timing accumulation keys:
    - `integration.manifest_parse_ms`
    - `integration.index_upsert_ms`
    - `integration.edge_write_ms`
    - `integration.description_read_ms`
    - `integration.search_upsert_ms`
  - Updated scan integration path to accumulate sub-phase timing totals across scanned entries.
  - Merged detailed workflow timing totals into scan report.

- `src/storage/store/workflow_facts.rs`
  - Added detailed workflow recompute timing collection keys:
    - `workflow.fetch_dependency_edges_ms`
    - `workflow.fetch_dependency_tickets_ms`
    - `workflow.compute_unresolved_ms`
    - `workflow.write_facts_ms`
    - `workflow.recompute_total_ms`
  - Preserved workflow behavior while returning timing map for scan-report merging.

- `src/storage/tests.rs`
  - Extended scan-report timing test to assert new integration/workflow sub-phase keys.

- `tests/e2e_perf_move_health.rs`
  - Extended perf e2e assertions to require the new sub-phase timing keys in scan reports.

## Validation

Commands run:

- `cargo test --manifest-path memory-api/crates/ticket-api/Cargo.toml scan_report_includes_phase_timings_and_root_counts`
- `cargo test --manifest-path memory-api/crates/ticket-api/Cargo.toml --test e2e_perf_move_health health_all_e2e_reports_timings_on_large_fixture`

Results:

- Both passed.

## Acceptance criteria status

- Integration hot-path timings decomposed into required sub-phases: complete.
- Workflow recompute timings decomposed into required sub-phases: complete.
- Existing aggregate scan timing fields remain available (`scan_total_ms`, `rebuild_workflow_facts_ms`): complete.

## Traceability

Contributes directly to `ff6637f5` requirement: bench/e2e outputs include sub-phase timings for integration and workflow recompute paths.
## Problem
`storage::move_planner::tests::preflight_reports_invisible_reference_visibility_and_path_refs` failed during setup with `StorageError::NotFound` for its target-only fixture ticket UUID. The fixture attempted to persist a cross-store edge through `source_store.add_edge`, which correctly requires both endpoints to be visible in that store.

## Resolution
The test now seeds only the intended legacy cross-store edge directly through `RedbIndexStore`, following neighboring recovery-test fixture practice. `TicketStore::add_edge` remains unchanged and retains endpoint-visibility validation.

## Acceptance evidence
- Exact regression: `ticket-api-move-planner-invisible-reference` / `exec-ticket-api-move-planner-invisible-reference-20260725` — passed (1 passed; 127 filtered).
- Crate suite: `ticket-api-crate-suite` / `exec-ticket-api-crate-suite-20260725` — passed (128 tests across 5 suites).
- Test evidence root: `memory-api/.test/default/`.

## Traceability
- Spec: `2b0ce814-0c22-42fa-8a89-f123660690e8` (`ticket-api/move-planner/invisible-reference-fixture-visibility`).
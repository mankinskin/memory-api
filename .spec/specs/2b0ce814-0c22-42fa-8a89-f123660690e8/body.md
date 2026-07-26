<!-- aligned-structure:v2 -->

## Motivation
The move-planner regression test must construct its cross-workspace reference through a fixture path compatible with the store's endpoint-visibility invariant, rather than failing before preflight behavior is exercised.

## Dependent expectation
If this spec is implemented, dependents can rely on `preflight_reports_invisible_reference_visibility_and_path_refs` to execute its move-preflight assertions while `TicketStore::add_edge` continues to reject edges whose endpoints are not visible in the receiving store.

## Guards
- `ticket-api-move-planner-invisible-reference` — passed as `exec-ticket-api-move-planner-invisible-reference-20260725`.
- `ticket-api-crate-suite` — passed as `exec-ticket-api-crate-suite-20260725`.

## Positions
- `memory-api/crates/ticket-api/src/storage/move_planner.rs`: implemented; the fixture seeds only the intentionally legacy cross-store edge directly in the index.
- `memory-api/crates/ticket-api/src/storage/store/query.rs`: implemented; `TicketStore::add_edge` validates both endpoints through `get_indexed` and was not changed.

## Governing-rule requirement
No matching PolicyRule was returned by the focused rule-store search. This work is governed by repository ticket-system and test instructions, which require a linked ticket, focused regression validation, and stored execution evidence.

## Traceability
- Ticket: [d5771b88 Repair move-planner invisible-reference fixture visibility](memory-api/.ticket/tickets/d5771b88-ca1d-41b2-8b59-0c911a34b37f)
- Test evidence store: `memory-api/.test/default/`.
- Validation specs: `ticket-api-move-planner-invisible-reference`, `ticket-api-crate-suite`.
- Passed executions: `exec-ticket-api-move-planner-invisible-reference-20260725`, `exec-ticket-api-crate-suite-20260725`.
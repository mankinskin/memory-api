## Bug: `update_ticket` state regression + `transition_states` no-op — FIXED

### Summary of Fix
Fixed the state machine regression in ticket-api `update()` function where:
1. Symptom 1 (state preservation): **VERIFIED WORKING** — State is correctly preserved when patching description/fields without `to_state`
2. Symptom 2 (transition_states): **FIXED** — `transition_states` now correctly applies multi-step transitions

### Root Cause
When `to_state` parameter was `None`, the code ignored `transition_states` entirely, setting `new_state = indexed.state.clone()` and `transition_path = Vec::new()`. This meant transition_states was silently discarded.

### Solution Implemented
Modified `store.rs` `update()` function to handle three cases:
1. If `to_state` is provided → use it as target (existing behavior)
2. If `to_state` is NOT provided but `transition_states` IS provided → use last element of `transition_states` as target (**NEW**)
3. If neither → preserve current state (no transition)

### Tests Added
Added three regression tests in `storage/tests.rs`:
1. `bug_7f4aaa05_state_preserved_on_field_patch_without_to_state` — Verifies state preservation ✅
2. `bug_7f4aaa05_description_patch_with_to_state_transition` — Verifies combined patch+transition ✅
3. `bug_7f4aaa05_transition_states_multi_step_path` — Verifies transition_states works ✅

All 67 ticket-api tests pass. Fix is in shared `ticket-api` layer, so all transports (CLI, MCP, HTTP) inherit the fix automatically.

### Acceptance Criteria Met
✅ State preservation: Fields/description patch without `to_state` preserves state
✅ transition_states honored: Multi-step transitions now work or raise validation errors
✅ Combined edit + transition: Patch + state transition in one call works
✅ Transport parity: Fix in shared API layer ensures all transports benefit
✅ required_states enforcement: Still enforced by existing schema validation

### Files Modified
- `memory-api/crates/ticket-api/src/storage/store.rs` — Fixed `update()` logic
- `memory-api/crates/ticket-api/src/storage/tests.rs` — Added 3 regression tests
- `memory-api/tools/http/ticket-http/src/serve/handlers/tickets/tests/listing.rs` — Fixed upsert call signatures

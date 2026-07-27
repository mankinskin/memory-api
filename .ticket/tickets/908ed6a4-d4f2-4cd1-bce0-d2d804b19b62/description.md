## Problem

Follow-up from the review of `8c67b96a` (closed done). Two gaps were waived into this ticket:

1. `SessionHandoffPackage.risk_notes` and `predecessor_handoff` are not explicitly round-trip-asserted. [snapshot_and_handoff.rs](memory-api/crates/session-api/src/store_tests/workflow/snapshot_and_handoff.rs) `handoff_package_round_trip_persists_schema_fields` asserts objective / target_tickets / target_files / decisions / non_goals / context_anchors / open_escalations — but not those two.
2. `8c67b96a` AC4 (`forward_handoff_package` inversion resolved/documented) and AC5 (old inline packages still readable) were never independently verified.

Risk is low — the struct is a generic serde copy with no field-specific logic — but the assertion set claims per-parameter coverage it does not have.

## Acceptance criteria

1. `risk_notes` and `predecessor_handoff` are asserted with non-default values in a handoff round-trip test.
2. Every field accepted by `session_handoff` has a round-trip assertion; add a comment or test-name convention making that contract explicit.
3. The `forward_handoff_package` inversion is either resolved in code or documented with rationale, and the outcome is recorded on this ticket.
4. A test reads an old inline handoff package and confirms it still deserializes (back-compat).

## Context

Note for whoever picks this up: the original "`open_escalations` is being dropped" premise was a **serde artifact**, not a bug. `#[serde(default, skip_serializing_if = "Vec::is_empty")]` at [handoff.rs:41](memory-api/crates/session-api/src/model/handoff.rs#L41) omits the key from JSON output only when the vector is empty. Verified: `open_escalations_field_persists_and_round_trips` (2 non-empty entries) passes, and `empty_open_escalations_is_persisted_as_empty_list` passes. Do not re-open that thread.

## Non-goals

- Changing the handoff schema.
- Changing serde attributes.

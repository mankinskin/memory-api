# Session Objective
Split remaining **core crate** file_length offenders with behavior-preserving module extractions and focused validation.

# Scope
- memory-api/crates/memory-api/src/storage/move_kernel.rs (884)
- memory-api/crates/ticket-api/src/workflow/mod.rs (842)
- memory-api/crates/spec-api/src/manifest.rs (821)
- memory-api/crates/spec-api/src/store.rs (786)
- memory-api/crates/session-api/src/hook.rs (792)
- memory-api/crates/test-api/src/store.rs (830)
- memory-api/crates/audit-api/src/trials/ticket_graph.rs (818)

# Acceptance Criteria
- Largest-first splits within this scope only.
- Every extraction preserves public API/visibility contracts.
- Focused crate tests/checks pass after each split.
- Parent ticket c991d769 progress is updated after this group lands.

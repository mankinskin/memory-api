## Problem

The store-wide description→parts migration (ticket f65f2b32, memory-api/crates/ticket-api/src/storage/store/migration.rs) deliberately skips tickets currently in state `planned` (`MigrationApplyReport::skipped_planned`, migration.rs:189-195, 286-293), because:

1. Planning parts on a `planned` ticket are frozen by `enforce_part_write_gate` (memory-api/crates/ticket-api/src/storage/store/parts.rs:37), so an in-place part write during migration would be rejected.
2. Stepping a ticket out of `planned` and back in to bypass the freeze was attempted and hit an unrelated dependency-state gate, leaving real ticket `e342cc4c` temporarily stuck outside `planned` with no parts (manually recovered during epic bbb4bce9 validation).

As a result, any ticket left in `planned` state at the time the migration ran still has its plan content only in the legacy `description` field, not in typed parts — the migration is incomplete for that subset, and there is no safe, auditable path implemented to finish it.

Additionally, `TicketStore::migration_undo` and `TicketFs::remove_parts` (migration.rs:353, :382) were implemented as part of f65f2b32 but have never been exercised against a real ticket — only unit-tested in isolation.

## Acceptance Criteria

1. A migration path exists that converts a `planned` ticket's legacy `description` into typed parts without unfreezing/bypassing `enforce_part_write_gate` and without round-tripping the ticket's state out of and back into `planned` (e.g. an explicit frozen-aware apply path, or a documented manual/audited exception in the store layer).
2. Applying this path to a `planned` ticket in a test fixture results in the ticket having both: (a) parts populated matching the prior description content losslessly, and (b) the ticket remaining in `planned` state throughout, with plan parts frozen exactly as they would be for any other `planned` ticket.
3. A test exercises `migration_undo` end-to-end against a ticket that was migrated by this path (or the standard apply path), asserting `TicketFs::remove_parts` actually removes the created part files and the ticket is restored to its pre-migration description state.
4. Running the migration against the current `memory-api` ticket store reports zero tickets in `skipped_planned` (or documents remaining exceptions with justification).
# Goal

Dependency edges created through `ticket link` and removed through `ticket unlink` must survive a rebuild from the tracked ticket store, not only the ignored SQLite index.

# Problem

The current implementation stores edge mutations in the SQLite `edges` table but does not persist them into git-tracked ticket files. A fresh `ticket init` plus `ticket scan --force` can therefore rebuild ticket manifests and search state while reconstructing zero edges.

# Requirements

- Adding or removing a dependency edge updates a tracked file-backed representation inside the ticket store.
- The file-backed representation is deterministic and scanable from the existing ticket folder layout.
- `ticket scan --force` rebuilds the edge table from tracked files when the SQLite index has been deleted.
- Duplicate edge writes remain idempotent.
- Edge deletion removes the file-backed representation so rebuilds do not resurrect deleted edges.

# Validation

- Regression tests create file-backed edges, delete the index, run a forced scan, and assert that the rebuilt store contains the expected edges.
- Regression tests cover edge deletion so a rebuild does not restore removed edges.
- The storage documentation for ticket-api reflects the implemented source-of-truth model.

# Traceability

- Ticket: `.ticket/tickets/deeeb26d-cb73-46c5-bf2a-1778caa7f82a`
- Implementation files:
	- `crates/ticket-api/src/storage/store/query.rs`
	- `crates/ticket-api/src/storage/store/scan.rs`
	- `crates/ticket-api/src/storage/ticket_fs.rs`
	- `crates/ticket-api/src/storage/store.rs`
	- `crates/ticket-api/src/storage/tests.rs`
	- `crates/memory-api/src/storage/index.rs`
- Updated documentation:
	- `crates/ticket-api/src/storage/store.rs`

# Validation Status

- `cargo test -p ticket-api scan_force_ -- --nocapture` — passed
- `cargo run -p ticket-cli -- --index-root memory-api/.ticket scan --force --json` — passed
- The forced-scan regression coverage now includes legacy DB-only edge backfill and stale-edge pruning for missing ticket folders in `crates/ticket-api/src/storage/tests.rs`.
- The current `memory-api/.ticket` store had no non-diagnostic live edges to materialize, so the real CLI validation confirmed the workflow without producing checked-in manifest edge diffs.

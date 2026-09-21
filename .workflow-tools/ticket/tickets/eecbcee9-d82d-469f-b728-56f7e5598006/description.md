# Scan-root persistence metadata + scan-time enforcement

## Goal

Extend scan-root persistence with auditability metadata and enforce policy at scan time so roots not allowed by policy are skipped.

## Current code

- `ScanRoot { path, label }` — [filesystem.rs](memory-api/crates/memory-api/src/model/filesystem.rs#L19).
- `scan_roots` table stores only `(path, label)` — table created in [index.rs](memory-api/crates/memory-api/src/storage/index.rs#L561), read/written in [auxiliary.rs](memory-api/crates/memory-api/src/storage/index/auxiliary.rs#L48).
- `TicketStore::scan` iterates `list_scan_roots()` — [scan.rs](memory-api/crates/ticket-api/src/storage/store/scan.rs#L48).

## Changes

1. Add metadata columns to the `scan_roots` table:
   - `source` = `discovered | manual | policy`
   - `policy_decision` = `included | ignored`
   - `workspace_root`
2. Extend `ScanRoot` (or a persisted wrapper) with these fields; keep serialization backward compatible via defaults and an additive migration (`ALTER TABLE ... ADD COLUMN` guarded by existence check, or bump the schema init to include the columns with safe defaults for existing rows).
3. At scan time, skip roots whose `policy_decision = ignored`, and record skipped roots in the `ScanReport` (extend `root_entry_counts` / add a `skipped_roots` field on [ScanReport](memory-api/crates/ticket-api/src/storage/store/scan.rs)).
4. Update all `add_scan_root` call sites to populate the new metadata (default `source = discovered`, `policy_decision = included`) so existing behavior is preserved.

## Non-goals

- Query-time guard (ticket 4/6).
- CLI (ticket 5/6).

## Acceptance criteria

- [ ] `scan_roots` table gains `source`, `policy_decision`, `workspace_root` with a non-destructive migration for existing indexes.
- [ ] `ScanRoot` round-trips the new fields; existing rows read with safe defaults.
- [ ] Scan skips `ignored` roots and reports them in `ScanReport`.
- [ ] `cargo test -p ticket-api` and `cargo test -p memory-api` pass.

## Files

- [memory-api/crates/memory-api/src/model/filesystem.rs](memory-api/crates/memory-api/src/model/filesystem.rs#L19)
- [memory-api/crates/memory-api/src/storage/index.rs](memory-api/crates/memory-api/src/storage/index.rs#L561)
- [memory-api/crates/memory-api/src/storage/index/auxiliary.rs](memory-api/crates/memory-api/src/storage/index/auxiliary.rs#L48)
- [memory-api/crates/ticket-api/src/storage/store/scan.rs](memory-api/crates/ticket-api/src/storage/store/scan.rs#L48)
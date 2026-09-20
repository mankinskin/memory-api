Make `EntityStore::scan` skip re-integration for unchanged entities so sync-targets (and other store consumers) do not re-read+re-index every rule every run.

Root cause: memory-api/crates/memory-api/src/storage/entity_store.rs `scan_once` walks every entity, `entity_fs::scan_root` reads+parses every manifest, and `integrate_entry` inserts every entity into the metadata index unconditionally. No mtime/content-hash guard.

Plan (design first — this is the highest-risk change):
- Track a per-entity fingerprint (manifest mtime and/or content hash) in the index sidecar / metadata index.
- During non-reindex scan, skip `integrate_entry` (and the manifest read where possible) when the fingerprint is unchanged.
- Preserve correctness of forced reindex (`reindex=true`) and pruning of deleted entities.
- Keep search-index invariants intact (search_needs_rebuild path unaffected).

Notes:
- This crate is shared across rule/ticket/spec/log/audit stores, so scope the change carefully and add a regression test that counts index writes across two consecutive no-change scans.

Acceptance (spec 9c7c0655 AC5):
- Re-running sync with no changed rule files performs no per-entity metadata re-integration for unchanged entities.

Validation: cargo test -p memory-api; cargo test -p rule-api; cargo test -p rule-cli.
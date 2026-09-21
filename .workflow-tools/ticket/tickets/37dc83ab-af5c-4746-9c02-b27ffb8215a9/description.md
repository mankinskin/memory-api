## Problem

Full-text search across the spec store (and any store backed by the same Tantivy index path) is non-functional. `spec scan --force` panics inside Tantivy and incremental scans silently fail to populate a searchable index.

### Reproduction

```bash
./target/debug/spec.exe scan --index-root "$(pwd)/.spec" --force
```

Panics with:

```
thread '...' panicked at tantivy-0.22.1/src/fastfield/writer.rs:137:
index out of bounds: the len is 5 but the index is 5
```

An incremental (non-force) scan succeeds and reports `integrated: 436, pruned: 0`, but search still returns zero results.

## Root Cause (confirmed)

Not a Tantivy 0.22.1 defect — a stale **on-disk schema** mismatch. The current `build_schema()` in `crates/memory-api/src/storage/search.rs` declares 7 fields (`id, title, body, state, ticket_type, created_at, effort`); the last two fast fields were added after the root `.spec/search_index` was first created with only 5 fields.

`open_or_create_index` reuses the existing on-disk schema when the directory is non-empty, so the Tantivy fast-field writer's `fast_field_names` vector is sized to 5. Writing a document that references field id 5/6 indexes past the end → `index out of bounds: the len is 5 but the index is 5` panic on the background indexing thread (`fastfield/writer.rs:137`).

Confirmed on disk: root `.spec/search_index/meta.json` had 5 field entries; `memory-api/.spec` had 7.

A reactive self-heal already existed in `EntityStore::scan` (catch rebuild-worthy error → `reset_dir` → retry), and it did rebuild the index — but only **after** the panic message was already emitted on the detached Tantivy worker thread, violating the "completes without panicking" criterion.

## Fix

Added a **proactive** schema check that rebuilds the index dir *before* any write when the on-disk layout no longer matches the current schema.

- `crates/memory-api/src/storage/search.rs`
  - `TantivySearchIndex::ensure_schema_current()` — resets the dir when the on-disk schema differs from `build_schema()`.
  - `on_disk_schema_is_current()` — opens the index and structurally compares schemas; returns `true` (no reset) when the dir is empty or the index can't be opened, so corrupt indexes still flow through the reactive repair path.
  - `schemas_match()` — compares serialized schemas (field names, types, and `FAST`/`STORED`/`INDEXED` options).
- `crates/memory-api/src/storage/entity_store.rs`
  - `EntityStore::scan(reindex=true)` now calls `self.search.ensure_schema_current()?` before scanning; the reactive catch is retained as a fallback.

## Validation

- `cargo test -p memory-api --lib` → 93 passed (incl. 2 new search tests + existing `scan_reindex_self_heals_stale_search_index_schema` integration test that builds a real on-disk 5-field index and runs `store.scan(true)` with no panic).
- `cargo test -p ticket-api --lib storage` → 31 passed (reactive corrupt-meta self-heal `search_and_delete_self_heal_after_search_index_corruption` still green — proactive change does not regress it).
- Real store end-to-end:
  - `spec scan --index-root .spec --force` → exit 0, **no panic message**, `integrated: 448`.
  - `.spec/search_index/meta.json` → 7 field entries.
  - `spec search` counts: `memory`=20, `rendering`=20, `generator`=8, `thin generator`=6.
- MCP `spec_search` uses the same `EntityStore::search` path, which now returns results.

## Acceptance Criteria

- [x] `spec scan --force` completes without panicking.
- [x] `spec search "<term>"` returns non-zero results for terms known to exist in the store.
- [x] MCP `spec_search` returns matching specs (same code path as CLI search).
- [x] Regression coverage confirms full-text search stays functional after a force reindex (`ensure_schema_current_rebuilds_stale_fast_field_schema`, `ensure_schema_current_keeps_current_schema_intact`, `scan_reindex_self_heals_stale_search_index_schema`).

## Non-goals

- Does not change the spec/ticket data model or digest contract.
- Does not alter the store-index generation track (memory-index).
- Does not upgrade Tantivy (root cause was a stale on-disk schema, not a Tantivy bug).
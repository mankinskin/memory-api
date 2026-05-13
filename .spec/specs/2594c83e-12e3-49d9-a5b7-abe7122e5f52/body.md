# entity_store

Source: `crates/memory-api/src/storage/entity_store.rs`

## Public API

### `ScanReport` (Struct)

Result of a full scan across all registered roots.

### `EntityStore` (Struct)

Convenience facade composing all three storage layers:
[`RedbIndexStore`] (metadata index), [`EntityFs`] (filesystem),
and [`TantivySearchIndex`] (full-text search).

Downstream crates can use this as a single entry point instead
of managing the three stores individually.

### `EntityStore` (Impl)


# index

Source: `crates/memory-api/src/storage/index.rs`

## Public API

### `RedbIndexStore` (Struct)

Redb-backed metadata index.

Opens the [`Database`] file only for the duration of each individual
operation and releases the exclusive file lock immediately after.

A per-store [`Mutex`] serialises concurrent open attempts within the
same process (required on Windows where `LockFileEx` is per-handle, not
per-process like Unix `flock`).

### `RedbIndexStore` (Impl)


<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/storage/index.rs`

## Behavior Story

Source: `crates/memory-api/src/storage/index.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

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

<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/storage/indexed.rs`

## Behavior Story

Source: `crates/memory-api/src/storage/indexed.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# indexed

Source: `crates/memory-api/src/storage/indexed.rs`

## Public API

### `IndexedEntity` (Struct)

Metadata stored per-entity in the SQLite index.
Does not hold full content — that lives in the manifest file on disk.

### `LeaseInfo` (Struct)

Lease record stored in the LEASES SQLite table.

### `LeaseInfo` (Impl)

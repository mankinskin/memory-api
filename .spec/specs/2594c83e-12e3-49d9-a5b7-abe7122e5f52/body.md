<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/storage/entity_store.rs`

## Behavior Story

Source: `crates/memory-api/src/storage/entity_store.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

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

<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/storage/board.rs`

## Behavior Story

Source: `crates/memory-api/src/storage/board.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# board

Source: `crates/memory-api/src/storage/board.rs`

## Public API

### `BoardEntry` (Struct)

### `BoardEntry` (Impl)

### `BoardEntryStatus` (Enum)

### `BoardConfig` (Struct)

### `BoardConfig::Default` (Impl)

### `BoardSnapshot` (Struct)

### `BoardCleanPreview` (Struct)

Preview of entries that are eligible for removal by `board_clean_apply`.

### `BoardCleanResult` (Struct)

Outcome of a successful `board_clean_apply` call.

### `ReconcileAction` (Enum)

Action taken by `board_reconcile` for a given ticket.

### `BoardReconcileResult` (Struct)

Result returned by the internal `board_reconcile` helper.

### `BoardError` (Enum)

### `RedbIndexStore` (Impl)

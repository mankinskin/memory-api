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


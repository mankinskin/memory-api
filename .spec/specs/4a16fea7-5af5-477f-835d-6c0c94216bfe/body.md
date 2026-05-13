# error

Source: `crates/memory-api/src/error.rs`

## Public API

### `SchemaValidationError` (Enum)

### `QueryParseError` (Enum)

### `StorageSchemaError` (Enum)

### `StorageError` (Enum)

Runtime storage errors covering SQLite, filesystem, and search index operations.

### `ProtocolError` (Enum)

Structured errors for the canonical `TaskCommand` agent protocol.

Error codes map directly to the `code` field in the structured error envelope,
e.g. `validate.invalid_state`, `release.validation_not_passed`.

### `ProtocolError` (Impl)

Machine-readable error code extracted from a `ProtocolError`.

### `StorageError::From` (Impl)

### `StorageError::From` (Impl)

### `StorageError::From` (Impl)

### `StorageError::From` (Impl)

### `StorageError::From` (Impl)


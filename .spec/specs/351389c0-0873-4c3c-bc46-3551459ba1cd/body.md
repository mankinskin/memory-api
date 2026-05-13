# store

Source: `crates/spec-api/src/store.rs`

## Public API

### `SpecStore` (Struct)

The central spec store: wraps `EntityStore` with spec-specific features.

Adds slug uniqueness enforcement, `body.md` management, `sections/` CRUD,
and parent-child hierarchy traversal on top of the generic entity store.

### `SpecStore` (Impl)


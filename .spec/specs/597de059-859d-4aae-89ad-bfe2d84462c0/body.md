# default_schema

Source: `crates/spec-api/src/default_schema.rs`

## Public API

### `specification_schema` (Function)

Parse and return the built-in `specification` entity type schema.

Panics if the embedded TOML is malformed — this is a compile-time invariant
verified by the schema parse test in this crate.

### `spec_schema_registry` (Function)

Create a [`SchemaRegistry`] pre-loaded with the built-in `specification` schema.


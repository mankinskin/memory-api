# schema_registry

Source: `crates/memory-api/src/model/schema_registry.rs`

## Public API

### `SchemaRegistry` (Struct)

Registry of entity type schemas.

Populated from built-in defaults and/or TOML schema files loaded from a
directory. A file whose `type_id` matches a built-in replaces the built-in,
allowing full workflow customisation per test environment or project.

### `SchemaRegistry` (Impl)


<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/model/schema_registry.rs`

## Behavior Story

Source: `crates/memory-api/src/model/schema_registry.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# schema_registry

Source: `crates/memory-api/src/model/schema_registry.rs`

## Public API

### `SchemaRegistry` (Struct)

Registry of entity type schemas.

Populated from built-in defaults and/or TOML schema files loaded from a
directory. A file whose `type_id` matches a built-in replaces the built-in,
allowing full workflow customisation per test environment or project.

### `SchemaRegistry` (Impl)

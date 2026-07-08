<!-- aligned-structure:v1 -->

# Summary

Source: `crates/spec-api/src/default_schema.rs`

## Behavior Story

Source: `crates/spec-api/src/default_schema.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# default_schema

Source: `crates/spec-api/src/default_schema.rs`

## Public API

### `specification_schema` (Function)

Parse and return the built-in `specification` entity type schema.

Panics if the embedded TOML is malformed — this is a compile-time invariant
verified by the schema parse test in this crate.

### `spec_schema_registry` (Function)

Create a [`SchemaRegistry`] pre-loaded with the built-in `specification` schema.

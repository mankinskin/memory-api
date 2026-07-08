<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/model/query.rs`

## Behavior Story

Source: `crates/memory-api/src/model/query.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# query

Source: `crates/memory-api/src/model/query.rs`

## Public API

### `ValueExpr` (Enum)

### `Expr` (Enum)

### `parse_query` (Function)

### `parse_query_strict` (Function)

Strict parsing mode used by contract validation.

Rules:
- keys in `known_fields` are always valid
- dynamic keys must follow `x_<type>_<field>`
- unknown keys fail with deterministic hint text

### `is_valid_dynamic_field_key` (Function)

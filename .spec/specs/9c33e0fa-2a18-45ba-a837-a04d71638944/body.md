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


<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/model/manifest_format.rs`

## Behavior Story

Source: `crates/memory-api/src/model/manifest_format.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# manifest_format

Source: `crates/memory-api/src/model/manifest_format.rs`

## Public API

### `format_manifest_toml` (Function)

Serialize an `EntityManifest` to a canonical TOML string.

Fields are written in the order defined by [`CANONICAL_FIELD_ORDER`].
Fields not in that list follow in alphabetical order (the natural iteration
order of the `BTreeMap`-backed `extra` store).

### `is_canonically_ordered` (Function)

Returns `true` when every field in `toml_text` is already in canonical order.

### `canonical_order_for_keys` (Function)

Given the set of keys present in a manifest, compute the ordering that
[`format_manifest_toml`] would produce.

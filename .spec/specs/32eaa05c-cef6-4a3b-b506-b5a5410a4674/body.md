<!-- aligned-structure:v1 -->

# Summary

Source: `crates/spec-api/src/code_ref.rs`

## Behavior Story

Source: `crates/spec-api/src/code_ref.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# code_ref

Source: `crates/spec-api/src/code_ref.rs`

## Public API

### `SymbolKind` (Enum)

The kind of symbol a code reference points to.

### `CodeRef` (Struct)

A reference from a spec to a specific symbol in implementation code.

### `RefValidation` (Struct)

Validation result for a single code ref.

### `validate_refs` (Function)

Validate code refs against a workspace root.

### `find_refs_for_file` (Function)

Reverse lookup: find which code refs reference a given file path.

<!-- aligned-structure:v1 -->

# Summary

Source: `tools/http/spec-http/src/error.rs`

## Behavior Story

Source: `tools/http/spec-http/src/error.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# error

Source: `tools/http/spec-http/src/error.rs`

## Public API

### `spec_err` (Function)

Map a `SpecError` to an Axum `Response` with appropriate HTTP status.

### `storage_err` (Function)

Map a `StorageError` to an Axum `Response`.

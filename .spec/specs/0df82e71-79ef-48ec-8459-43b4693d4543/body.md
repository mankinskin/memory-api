<!-- aligned-structure:v1 -->

# Summary

Source: `tools/http/spec-http/src/handlers/tree.rs`

## Behavior Story

Source: `tools/http/spec-http/src/handlers/tree.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# tree

Source: `tools/http/spec-http/src/handlers/tree.rs`

## Public API

### `get_tree` (Function)

GET /api/specs/:id/tree — hierarchy subtree.

### `get_refs` (Function)

GET /api/specs/:id/refs — list code references.

### `ValidateRefsRequest` (Struct)

### `validate_refs` (Function)

POST /api/specs/:id/refs/validate — validate code references.

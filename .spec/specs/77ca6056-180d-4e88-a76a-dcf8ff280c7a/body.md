<!-- aligned-structure:v1 -->

# Summary

Source: `tools/http/spec-http/src/handlers/specs.rs`

## Behavior Story

Source: `tools/http/spec-http/src/handlers/specs.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# specs

Source: `tools/http/spec-http/src/handlers/specs.rs`

## Public API

### `ListParams` (Struct)

### `SearchParams` (Struct)

### `SpecSummary` (Struct)

### `SpecListResponse` (Struct)

### `SpecDetailResponse` (Struct)

### `SpecDetail` (Struct)

### `SpecFullResponse` (Struct)

### `CreateSpecRequest` (Struct)

### `CreateSpecResponse` (Struct)

### `UpdateSpecRequest` (Struct)

### `list_specs` (Function)

### `search_specs` (Function)

### `get_spec` (Function)

GET /api/specs/:id — accepts UUID, UUID prefix, or slug.

### `get_spec_full` (Function)

GET /api/specs/:id/full — includes body and sections list.

### `create_spec` (Function)

POST /api/specs — create a new spec.

### `update_spec` (Function)

PATCH /api/specs/:id — update fields, state, and/or body.

### `delete_spec` (Function)

DELETE /api/specs/:id — soft-delete.

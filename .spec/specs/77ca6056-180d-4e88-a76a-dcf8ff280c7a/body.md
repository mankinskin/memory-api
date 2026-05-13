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


<!-- aligned-structure:v1 -->

# Summary

Source: `tools/http/spec-http/src/handlers/sections.rs`

## Behavior Story

Source: `tools/http/spec-http/src/handlers/sections.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# sections

Source: `tools/http/spec-http/src/handlers/sections.rs`

## Public API

### `AddSectionRequest` (Struct)

### `SectionsResponse` (Struct)

### `list_sections` (Function)

GET /api/specs/:id/sections

### `get_section` (Function)

GET /api/specs/:id/sections/:name

### `add_section` (Function)

POST /api/specs/:id/sections

### `delete_section` (Function)

DELETE /api/specs/:id/sections/:name

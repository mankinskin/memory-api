<!-- aligned-structure:v1 -->

# Summary

Source: `crates/memory-api/src/model/filesystem.rs`

## Behavior Story

Source: `crates/memory-api/src/model/filesystem.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# filesystem

Source: `crates/memory-api/src/model/filesystem.rs`

## Public API

### `ScanRoot` (Struct)

### `ParseDiagnostic` (Struct)

### `EntityFolderConfig` (Struct)

Per-domain folder layout configuration.

Parameterizes the filenames used inside each entity folder so that
`ticket-api` (with `ticket.toml` / `.ticket-lock`) and `spec-api`
(with `spec.toml` / `.spec-lock`) can share the same generic
[`EntityFs`](super::super::storage::entity_fs::EntityFs) implementation.

### `EntityFolderConfig` (Impl)

### `parse_entity_manifest_toml` (Function)

### `has_minimum_entity_contract` (Function)

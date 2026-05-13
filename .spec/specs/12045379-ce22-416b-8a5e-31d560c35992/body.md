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


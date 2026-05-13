# Extract memory-api Crate

## Objective

Extract ~75% of ticket-api into a generic `memory-api` crate that provides filesystem-backed entity storage with schema validation, indexing, search, and graph edges. Both ticket-api and the new spec-api will depend on this crate.

## Extraction Inventory

### Strongly Generic (extract directly)

| Module | Current | Target | Notes |
|--------|---------|--------|-------|
| EntityManifest | `model/ticket.rs` | `memory_api::model::entity` | `{id: Uuid, created_at, extra: BTreeMap}` |
| TypeSchema + SchemaRegistry | `model/schema.rs`, `model/schema_registry.rs` | `memory_api::model::schema` | State machine, field validation, BFS transitions |
| EdgeRecord + EdgeKindRule | `model/edge.rs` | `memory_api::model::edge` | Directed/undirected, acyclicity enforcement |
| Query language (Expr) | `model/query.rs` | `memory_api::model::query` | `Field:value` + FTS parser |
| Filesystem constants | `model/filesystem.rs` | `memory_api::model::filesystem` | Parameterize manifest filename |
| Manifest formatter | `model/manifest_format.rs` | `memory_api::model::manifest_format` | Canonical TOML ordering |
| EntityFs | `storage/ticket_fs.rs` | `memory_api::storage::entity_fs` | Folder-per-entity, advisory lock, history.ndjson |
| RedbIndexStore | `storage/index.rs` | `memory_api::storage::index` | `with_db` pattern, bincode entities |
| TantivySearchIndex | `storage/search.rs` | `memory_api::storage::search` | Configurable field schema |
| Board coordination | `storage/board.rs` | `memory_api::storage::board` | Agent check-in/out, WIP limits |
| IndexedEntity + LeaseInfo | `storage/indexed.rs` | `memory_api::storage::indexed` | Redb-cached metadata |
| Schema versioning | `storage/schema.rs` | `memory_api::storage::schema` | Table names, version |
| Workspace resolution | `workspace.rs` | `memory_api::workspace` | Resolution chain |
| Filesystem watcher | `watcher/` | `memory_api::watcher` | Debounced watch loop |
| StorageError | `error.rs` | `memory_api::error` | Generic error types |

### Ticket-Specific (remain in ticket-api)

| Module | Reason |
|--------|--------|
| `model/default_schema.rs` | `tracker-improvement` state machine |
| `model/event.rs` | Git branch lifecycle |
| `contracts/command_schema.rs` | TicketCommand enum |
| `execution/` | Sandbox, Copilot provider |
| Validation/release protocol in store | Domain-specific workflows |

## Parameterization Points

The following ticket-specific constants must become configurable:

1. **Manifest filename**: `ticket.toml` → configurable per entity type (e.g. `spec.toml`)
2. **Lock filename**: `.ticket-lock` → `.entity-lock` or configurable
3. **History filename**: `history.ndjson` → keep generic
4. **Search schema fields**: Currently hardcoded `{id, title, body, state, ticket_type}` → configurable field set
5. **Workspace env var**: `TICKET_INDEX_ROOT` → consider `MEMORY_INDEX_ROOT` or keep per-domain
6. **Workspace marker file**: `.ticket-workspace` → generic or per-domain
7. **Board terminal state check**: Currently tied to schema's terminal states → callback

## Implementation Plan

1. Create `crates/memory-api/` with `Cargo.toml`
2. Move generic modules, replacing `Ticket` prefixes with `Entity`
3. Add configuration traits/structs for parameterizable aspects
4. Make TantivySearchIndex field schema configurable via builder
5. Make EntityFs manifest filename configurable
6. Export all public types from `memory_api::` root
7. Ensure all existing ticket-api tests pass via re-exports

## Acceptance Criteria

- [ ] `crates/memory-api/` exists with clean public API
- [ ] `cargo test -p memory-api` passes
- [ ] All generic storage/index/search/edge logic lives in memory-api
- [ ] Search field schema is configurable (not hardcoded to ticket fields)
- [ ] EntityFs manifest filename is configurable
- [ ] No circular dependencies between memory-api and ticket-api
- [ ] Documentation on public types
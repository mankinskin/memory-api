# SpecStore: Storage Layer

## Objective

Build `SpecStore` on top of `memory_api::EntityStore` adding spec-specific features: parent-child hierarchy, slug uniqueness, multi-file folder support, and section management.

## Architecture

```rust
pub struct SpecStore {
    inner: EntityStore,  // from memory-api
    slug_index: HashMap<String, SpecId>,  // in-memory slug→UUID cache
}
```

## Key Operations

### CRUD
- `create(manifest, body, sections?)` → create spec folder with all files
- `get(id_or_slug)` → resolve slug if needed, read manifest + body
- `get_full(id_or_slug)` → read manifest + body + all sections
- `update(id_or_slug, patch)` → update manifest fields
- `update_body(id_or_slug, content)` → update body.md
- `delete(id_or_slug)` → soft-delete

### Sections
- `add_section(spec_id, name, content)` → create sections/name.md
- `update_section(spec_id, name, content)`
- `delete_section(spec_id, name)`
- `list_sections(spec_id)` → list section filenames

### Hierarchy
- `children(spec_id)` → list specs with parent == spec_id
- `ancestors(spec_id)` → walk parent chain to root
- `subtree(spec_id)` → BFS/DFS all descendants

### Slug Resolution
- `resolve_slug(slug)` → Option<SpecId>
- Slug uniqueness enforced on create and update
- Slug index rebuilt on scan

## Acceptance Criteria

- [ ] SpecStore wraps EntityStore from memory-api
- [ ] CRUD with slug resolution
- [ ] Section CRUD
- [ ] Parent-child hierarchy traversal
- [ ] Slug uniqueness enforced
- [ ] Integration tests for full workflow
# EntityStore Convenience Facade in memory-api

## Objective

Add an `EntityStore` struct to `memory-api` that composes `RedbIndexStore`, `EntityFs`, and `TantivySearchIndex` into a single convenient type. This gives downstream crates (spec-api, ticket-api) a unified entry point rather than managing three separate stores.

## Design

```rust
pub struct EntityStore {
    pub index: RedbIndexStore,
    pub fs: EntityFs,
    pub search: TantivySearchIndex,
}

impl EntityStore {
    pub fn open(scan_root: &Path, index_root: &Path) -> Result<Self> { ... }
    pub fn scan(&mut self) -> Result<Vec<EntityManifest>> { ... }
}
```

Delegates to existing methods on the three inner stores. No new logic — purely a composition convenience.

## Acceptance Criteria

- [ ] `EntityStore` struct in memory-api with `index`, `fs`, `search` fields
- [ ] `EntityStore::open()` initializes all three stores
- [ ] `EntityStore::scan()` coordinates scan across fs + index + search
- [ ] Existing memory-api tests still pass
- [ ] ticket-api can optionally adopt EntityStore (but not required by this ticket)

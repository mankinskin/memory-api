# spec-api Crate Scaffold + SpecManifest Model

## Objective

Create the `crates/spec-api/` crate with its Cargo.toml and define the SpecManifest model using the same `extra: BTreeMap<String, Value>` pattern as EntityManifest/TicketManifest.

## Implementation

1. Create `crates/spec-api/Cargo.toml` with dependencies on `memory-api`, `serde`, `uuid`, `chrono`, `toml`
2. Create `crates/spec-api/src/lib.rs` with module structure
3. Create `crates/spec-api/src/manifest.rs` with `SpecManifest` struct
4. Add to workspace `Cargo.toml`

### SpecManifest

Uses `EntityManifest` from `memory-api` as the underlying storage model. The extra fields are:

- `slug` — hierarchical human-readable identifier
- `title` — specification title
- `type` — always "specification"
- `state` — lifecycle state (draft, reviewed, approved, etc.)
- `parent` — optional UUID of parent spec
- `component` — which crate/tool this spec covers
- `scope` — crate | module | function | trait | type

All stored in `EntityManifest.extra` BTreeMap, accessed via typed accessor methods.

## Acceptance Criteria

- [ ] `crates/spec-api/` crate compiles and is in workspace
- [ ] SpecManifest struct with typed accessors for slug, title, state, parent, component, scope
- [ ] Serde round-trip tests (TOML ↔ SpecManifest)
- [ ] Helper constructors: `SpecManifest::new(slug, title, component)`

# Code References: Symbol-Level Links

## Objective

Implement the `CodeRef` system that links spec features to exact symbols in the implementation code with file paths and line ranges.

## Data Model

```rust
pub struct CodeRef {
    pub file: String,           // workspace-relative path
    pub symbol: String,         // e.g. "TicketStore::create"
    pub kind: SymbolKind,       // struct, fn, trait, impl, enum, mod, const, type
    pub line_start: u32,
    pub line_end: u32,
    pub description: Option<String>,  // optional note about what this ref covers
}

pub enum SymbolKind {
    Struct, Function, Trait, Impl, Enum, Module, Const, Type,
}
```

## Storage

Code refs are stored inline in `spec.toml` as `[[code_refs]]` array entries.

## Validation

- `validate_refs(spec_id)` → check that referenced files exist and line ranges are plausible
- `stale_refs(spec_id)` → detect refs where the file changed since last update (via mtime or hash)
- `find_specs_for_file(path)` → reverse lookup: which specs reference this file

## Acceptance Criteria

- [ ] CodeRef struct serializes to/from TOML
- [ ] SpecStore supports adding/removing/listing code refs
- [ ] `validate_refs` checks file existence and line range validity
- [ ] `find_specs_for_file` reverse lookup works
- [ ] Unit tests for serialization and validation
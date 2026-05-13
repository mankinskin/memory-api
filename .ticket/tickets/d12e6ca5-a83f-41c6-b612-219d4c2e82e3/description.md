# Spec Creation: Bootstrap from Existing Code

## Objective

Build tooling to analyze existing Rust crate source code and generate initial spec files documenting the current implementation. This is for interfaces that are already implemented but not yet documented.

## Approach

1. **Crate-level scan**: Walk `src/` files, extract public structs/traits/functions/enums
2. **Symbol extraction**: Parse Rust source with `syn` or regex-based extraction to identify public API surface
3. **Spec scaffold**: Generate spec.toml + body.md with:
   - Title from item name
   - Slug from crate/module path
   - Code refs pointing to actual source locations
   - Placeholder body with extracted doc comments
   - Feature list with implemented items
4. **Hierarchy**: Generate parent-child structure matching module hierarchy

## CLI Interface

```
spec bootstrap <crate-path> [--dry-run] [--component <name>]
```

## Acceptance Criteria

- [ ] `spec bootstrap crates/ticket-api/` generates spec files for all public items
- [ ] Generated specs have correct code refs with line numbers
- [ ] Doc comments extracted into spec body
- [ ] Module hierarchy reflected as parent-child spec relationships
- [ ] Dry-run mode shows what would be created without writing
<!-- aligned-structure:v1 -->

# Summary

Source: `crates/spec-api/src/store.rs`

## Behavior Story

Source: `crates/spec-api/src/store.rs`

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# store

Source: `crates/spec-api/src/store.rs`

## Public API

### `SpecStore` (Struct)

The central spec store: wraps `EntityStore` with spec-specific features.

Adds slug uniqueness enforcement, `body.md` management, `sections/` CRUD,
and parent-child hierarchy traversal on top of the generic entity store.

### `SpecStore` (Impl)

## Create Root Resolution

`SpecStore::create` accepts either an explicit registered scan root or a local
workspace path that resolves to the canonical `.spec` store.

When callers pass a workspace root, the `.spec` store root, or a path inside
that store, creation must normalize the destination to `.spec/specs` before the
entity folder is created.

Paths outside the registered scan roots and outside the local `.spec` store
must be rejected with a storage error instead of writing spec folders into the
caller-provided directory.

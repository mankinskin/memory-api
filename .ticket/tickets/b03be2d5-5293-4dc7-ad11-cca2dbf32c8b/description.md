# Cross-Entity Edges

## Objective

Extend memory-api's edge system to support edges between entities of different types (spec ↔ ticket). Currently edges are within a single entity store; this enables cross-store relationships.

## Design

Since specs and tickets share the same workspace, edges can reference UUIDs from either store. The edge system needs:

1. **Entity type annotation** on edge endpoints (spec vs ticket)
2. **Cross-store resolution** — edge validation looks up the target in the correct store
3. **Edge kind rules** for cross-entity relationships

## Acceptance Criteria

- [ ] Edge endpoints can reference entities from different stores
- [ ] Validation checks target existence in correct store
- [ ] Cross-entity edges queryable from either direction
- [ ] Cycle detection works across entity types

## Coordination (added 2026-07-11)
The cross-store resolution surface here overlaps with `f3a58d3c` "[memory-api] Cross-store edge health: shared resolver + policy-aware parent-workspace warning". Both build on the same base-memory-api primitives (`Urn`/`UrnResolver` in model/urn.rs, `discover_stores` in discovery.rs, `WorkspacePolicy`). Reuse ONE shared resolver/classifier from base memory-api — do not fork a second cross-store lookup. This ticket adds cross-ENTITY-TYPE resolution (spec↔ticket within a workspace); `f3a58d3c` adds cross-WORKSPACE health classification (OK / parent-workspace warning / dangling error). Keep them aligned on the shared resolver.
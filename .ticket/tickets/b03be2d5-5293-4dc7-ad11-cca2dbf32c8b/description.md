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
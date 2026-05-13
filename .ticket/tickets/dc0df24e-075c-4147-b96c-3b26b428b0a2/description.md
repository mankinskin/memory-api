# spec-api Crate — Umbrella Ticket

## Objective

This is the parent ticket for the spec-api crate. It tracks the execution order and dependencies of all P1 sub-tickets.

## Sub-tickets (Execution Order)

### Phase 0.5 (prerequisite)
1. **d5722e8e** — EntityStore convenience facade in memory-api

### Phase 1a (foundation — no internal deps)
2. **ad531f63** — spec-api crate scaffold + SpecManifest model

### Phase 1b-1c (depend on 1a)
3. **90c88ead** — Slug system (validation, uniqueness, resolution) — depends on ad531f63
4. **614f5f2a** — Multi-file folder structure (spec.toml, body.md, sections/) — depends on ad531f63, d5722e8e
5. **4b6dc9d5** — Schema/state machine (draft→reviewed→approved→implemented→verified) — depends on ad531f63
6. **55a1b302** — Code references (CodeRef struct, validation, reverse lookup) — depends on ad531f63

### Phase 1d (integrates everything)
7. **ab47648c** — SpecStore storage layer (wraps EntityStore, slug enforcement, sections, hierarchy) — depends on 4b6dc9d5, ad531f63, 90c88ead, 614f5f2a

## Design Decisions (Resolved)

- **SpecManifest pattern**: Uses `EntityManifest` with `extra: BTreeMap` (same as tickets). Typed accessor methods for slug, title, state, parent, component, scope.
- **EntityStore**: New convenience facade in memory-api composing RedbIndexStore + EntityFs + TantivySearchIndex. SpecStore wraps EntityStore.
- **CodeRef**: Owned by ticket 55a1b302 (not this ticket).
- **FeatureStatus**: Owned by ticket c4c9e9d4 (P4).

## Outside Dependencies

- Depends on: e0b3e9a8 (memory-api extraction) ✅ DONE
- Blocked by: nothing (all prerequisites done)
- Blocks: P2 (cli, mcp, http), P3 (creation, sync), P4-P8 (all downstream)
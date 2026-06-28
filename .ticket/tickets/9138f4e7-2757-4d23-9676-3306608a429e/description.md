# [memory-api] Representative fixture population — synthesized, many entities/edges, nested workspaces

## Goal

Replace the 4-entity stub fixture with a **synthesized** representative workspace so operations run against real data instead of self-seeded throwaway stores.

## Resolved decisions

- **D5 — realism:** **synthesize** (do not snapshot a live repo) a workspace with **many entities and edges**, both **within stores and across stores**, across **nested workspaces/submodules**. Model the shape and scale after the real **`memory-api`** and **`context-engine`** workspaces (multi-store, multi-submodule, cross-store references).
- **D6 — volume vs runtime:** prioritize **completeness** of representation now; routine-vs-heavy selection is handled later by test profiles (`2dada4b7`), so the fixture may be large.

## Scope

- Deterministic, seeded generator producing all in-scope domain stores populated with internally-consistent data:
  - tickets with dependency edges, history/transitions, multiple types/states;
  - specs with sections, refs, parent/child hierarchy;
  - rules, sessions/turns, test executions/benchmarks, log captures, doc inputs;
  - realistic **cross-store** references (tickets ↔ specs, executions ↔ tickets/specs/logs) and **nested workspace/submodule** layout mirroring `memory-api`/`context-engine`.
- Update matrix cells to read/operate against seeded entities (real `get`/`search`/`scan`), not data the cell just created.
- Document generation/regeneration; keep it reproducible.

## Acceptance criteria

- [ ] Every in-scope domain store is populated with many cross-referenced entities across nested workspaces/submodules.
- [ ] `get`/`search`/`scan` cells assert against seeded data, not self-created data.
- [ ] Cross-store and nested-workspace references exist and are traversable.
- [ ] Generation is deterministic, documented, and reproducible.
- [ ] Existing fixture consumers (ticket-api/spec-api e2e loaders) still pass.

## Relationship / traceability

- Extends the E2E fixture `026b2eb6`.
- Consumed by `387843e4`, `274c5119`, `01964def`.

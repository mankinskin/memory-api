# Cleanup migration: relocate misplaced entities into their lowest-owning store

## Placement principle (product direction)
Every entity must live in the **lowest-level store that contains all of the code the entity is concerned with**:
- memory-api-concerned entities → directly in the **memory-api** store.
- viewer-api-concerned entities → directly in the **viewer-api** store.
- only **genuinely cross-workspace** concerns → tracked in a parent workspace such as **context-engine**.

We do NOT consolidate everything into one store; cross-workspace work is still supported. This ticket only relocates entities that are currently in the top-level `context-engine` store but are concerned with a single lower-level domain.

## Why
The root `context-engine/.ticket` store currently holds many tickets/specs whose domain is entirely memory-api (session-api, ticket-api, rule-api, spec-api, feedback-api, audit-api, workspace-resolution). Because the ticket-mcp server only exposes the `default` store and graph edges cannot cross stores, this misplacement blocks hard-link/cross-store work and pollutes the parent workspace.

## Depends on
- `505b2cd4` Deliver safe cross-workspace ticket move for git-backed stores (+ children) — the move tooling MUST be delivered and validated before any production move.

## Moving active work is OK (Q4)
It is acceptable to move tickets that are currently `in-implementation`. The agent owning the session-bootstrap implementation effort also owns these moves; in-flight state is preserved by the journaled move + automatic re-linking.

## Audit + move candidates

### MOVE to the memory-api store (single memory-api domain)
- session-bootstrap cluster: `effba966` (epic), `412964a3` (runtime), `6b2dc497` (cli/mcp), `b4a8dc5e` (rules), `d8f76965` (cascade), `afa00b5c` (design).
- feedback-api program (memory-api crates): `b1e9e744` (tracker), `c7542933` (core gate), `9c95c1e4`, `3a1ec9f8`, `4f86d3d2`, `b7b84c10`, `c2d6a14a`.
- workspace-resolution: `ef0ebf38` (tracker) + `07836f41`, `59d96577`.
- state-machine: `185419e0` (bidirectional transitions, in-review).
- transport parity: `39239e48`.
- audit-api tickets (audit single memory-api domain) — audit each before moving.

### STAY in context-engine (genuinely cross-workspace)
- `671d4e47` multi-store architecture tracker (spans memory-api, viewer-api, context-stack).
- `82d6ada4` URN cross-store reference model, `6bd67a7a` multi-store discovery — cross-store by definition.
- `8a90a63c` multi-store store-expansion / operational-health program.

> Audit each candidate's owning store before moving; do not move blindly. An entity that touches more than one workspace's code stays in the lowest common ancestor workspace.

## Requirements
- Use the move tooling's dry-run / preflight planner first; capture the plan.
- Automatic reference re-linking must rewrite `depends_on` edges and repo path references (move-tooling children 3a26572a / 13e9ce28).
- Board safety: no active board check-ins on tickets being moved (22cd3001).
- Preserve ticket history; the move must be journaled and reversible.
- After migration, re-run health checks in every affected store; confirm no dangling edges.

## Acceptance criteria
1. A documented preflight/dry-run plan lists every entity to move, its target store (justified by the placement principle), and the edges/paths to rewrite.
2. All audited single-domain entities are moved into their lowest-owning store with history preserved and `depends_on` edges re-linked intra-store.
3. Genuinely cross-workspace entities remain in context-engine.
4. Repo path references to moved ticket folders are rewritten (no broken links in specs/docs).
5. Health checks pass in all affected stores post-migration (no dangling edges, no orphaned references).
6. The hard-link cross-entity-edge work (b03be2d5 / f00291a3) can draw intra-store edges between the previously-split entities.

## Blocks
- Hard-link cross-store reference work: `b03be2d5` (cross-entity edges spec↔ticket) and `f00291a3` (ticket↔spec integration) depend on this cleanup so their target entities are co-located.
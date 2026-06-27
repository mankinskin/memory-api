# Cleanup migration: relocate misplaced context-engine-workspace tickets into the memory-api store

## Why
The root `context-engine/.ticket` (workspace `default`) store currently holds many tickets whose implementation domain is memory-api (session-api, ticket-api, rule-api, spec-api, feedback-api, cross-store/URN architecture). Because the ticket-mcp server only exposes the `default` store, and graph edges cannot cross stores, this split blocks hard-link / cross-store reference work: entities that must be linked live in two different stores.

This ticket performs the **cleanup migration** of those incorrectly-placed entities into the memory-api store using the cross-workspace move tooling, so the hard-link cross-store reference work can operate within a single store.

## Depends on
- [505b2cd4 Deliver safe cross-workspace ticket move for git-backed stores] — the move tooling (preflight, journaled execution, ref re-linking, board safety, CLI/HTTP/MCP, e2e validation) MUST be delivered and validated before any production move.

## Migration candidates (audit, then move)
Audit each candidate's correct owning store before moving; do not move blindly. Strong candidates currently in the `default` / root store:

- session-bootstrap epic + children: `effba966` (epic), `412964a3` (runtime), `6b2dc497` (cli/mcp), `b4a8dc5e` (rules), `d8f76965` (cascade), `afa00b5c` (design).
- cross-store / URN architecture: `671d4e47` (tracker), `82d6ada4` (URN resolver), `6bd67a7a` (multi-store discovery).
- feedback-api program: `b1e9e744` (tracker) + children (`9c95c1e4`, `3a1ec9f8`, `4f86d3d2`, `b7b84c10`, `c2d6a14a`).

Already correctly placed in memory-api (no move): `7f4aaa05` (update_ticket bug), `b03be2d5`, `f00291a3`, `29bf9628`, the move-tooling set.

## Requirements
- Use the move tooling's dry-run / preflight planner first; capture the plan.
- Automatic reference re-linking must rewrite `depends_on` edges and repo path references (per the move-tooling children 3a26572a / 13e9ce28).
- Board safety: no active board check-ins on tickets being moved (per 22cd3001).
- Preserve ticket history; the move must be journaled and reversible.
- After migration, re-run health checks in both stores; confirm no dangling edges.

## Acceptance criteria
1. A documented preflight/dry-run plan lists every entity to move and the edges/paths to rewrite.
2. All audited misplaced entities are moved into the memory-api store with history preserved and `depends_on` edges re-linked intra-store.
3. Repo path references to moved ticket folders are rewritten (no broken links in specs/docs).
4. Health checks pass in both stores post-migration (no dangling edges, no orphaned references).
5. The hard-link cross-entity-edge work (b03be2d5 / f00291a3) can draw intra-store edges between the previously-split entities.

## Blocks
- Hard-link cross-store reference work: `b03be2d5` (cross-entity edges spec↔ticket) and `f00291a3` (ticket↔spec integration) depend on this cleanup so their target entities are co-located.

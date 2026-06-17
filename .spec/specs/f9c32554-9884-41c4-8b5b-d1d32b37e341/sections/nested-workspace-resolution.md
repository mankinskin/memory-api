<!-- spec-api:file generated=true -->

<!-- spec-api:entry id=06c286d6-83f7-46ad-b1a5-a4d7336b8d6f slug=memory-api/recurring-principles/nested-workspace-resolution/nested-workspace-resolution/l1 -->
# Nested workspace resolution

A repository may contain multiple `memory-api` workspaces (for example the context-engine root, `memory-viewers/memory-api`, and `memory-viewers/viewer-api`, each with their own `.ticket/`, `.spec/`, `.rule/`). The workspace resolver normalises any caller-supplied path to a single owning root and never falls back silently to an ancestor checkout.

<!-- spec-api:entry id=9a6a7d88-194e-46f7-b608-9494a3f691c6 slug=memory-api/recurring-principles/nested-workspace-resolution/nested-workspace-resolution/resolution-rules/l5 -->
## Resolution rules

1. Find the nearest registered scan root that contains the path. If none exists, fall back to the nearest directory upward that contains a `.ticket/`, `.spec/`, or `.rule/` store.
2. If the path is inside a nested workspace, the nested workspace wins. Ancestor stores are not consulted for entities the nested workspace owns.
3. Ambiguous paths (matching more than one nested workspace, or matching no workspace at all) fail with `code: invalid_request` rather than picking arbitrarily.

<!-- spec-api:entry id=5e08d966-4985-4790-87c8-482ba22d83fb slug=memory-api/recurring-principles/nested-workspace-resolution/nested-workspace-resolution/parent-child-configuration/l11 -->
## Parent–child configuration

- Parent workspaces declare their child stores in `rule-targets.yaml` (`imports:`) and through registered scan roots in each store.
- A nested workspace's local `rule-targets.yaml` is consulted by `spec sync-generated` when the spec lives in that workspace; the parent's targets are not implicitly inherited.
- `<x>-cli ... --workspace-root <path>` forces the resolver to a specific root and is the supported way for an ancestor checkout to target a nested workspace explicitly.

### Cross-workspace moves (v1 contract)

- **v1 Support boundary**: Moves are supported only for tickets, only within git-backed workspaces, and must fail-closed. Both source and destination workspaces, as well as any tracked text files rewritten by the move, must reside in the same git worktree.
- **Destination-visibility rule**: A move is permitted only if **every** ticket-to-ticket reference involving the moved ticket (both inbound and outbound) remains visible from the destination store after the move. If any referenced or referencer ticket is not registered or visible from the target store, the preflight validation must reject the move.
- **Active board claims**: Any active or stale board claims/leases on the moved ticket in the source store block the move (fail-closed). Historical board audit rows are migrated along with the ticket.
- **Path-reference rewrites**: References utilizing relative paths (citing the old ticket folder path, such as in specs, tests, or documentation files) are parsed, validated, and automatically rewritten to point at the new target folder path; these rewritten entries are recorded in the move validation journal.
- **Journaled execution**: Since cross-store operations lack native transactions, execution uses a resume-or-rollback journal. Post-move index validation (by scanning source and target) confirms structural consistency before completing.

<!-- spec-api:entry id=c5295e96-8703-43dc-92fe-4633ecd4269b slug=memory-api/recurring-principles/nested-workspace-resolution/nested-workspace-resolution/why-one-owning-root/l17 -->
## Why one owning root

The store's append-only history and materialised index belong to exactly one workspace. Allowing a single entity to be visible from two stores would break the resolver, the edge index, and `spec sync-generated`'s output paths.

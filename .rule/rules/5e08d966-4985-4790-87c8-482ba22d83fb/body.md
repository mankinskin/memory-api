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
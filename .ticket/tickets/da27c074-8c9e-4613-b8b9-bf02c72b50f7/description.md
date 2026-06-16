# [ticket-api] Validate cross-workspace ticket move flows end to end

## Goal

Prove the move contract with focused automated coverage and one real consumer scenario before using the tool on `694d74b4`.

## Scope

Add integration coverage for at least:

- root -> nested workspace move that remains visible from the destination store (the `694d74b4` shape)
- nested -> parent move when the destination still owns every ticket reference
- sibling or unrelated-store move rejection when ticket references would strand
- dirty tracked git worktree rejection
- active/stale board blocker rejection
- journal resume / rollback after an injected mid-flight failure
- CLI / MCP / HTTP smoke coverage over the shared primitive

## Acceptance criteria

- [ ] Tests cover both supported and rejected topologies.
- [ ] Resume/rollback is exercised with an injected failure after file movement begins.
- [ ] The delivered evidence is sufficient to unblock reminder ticket `44abe1d4`.

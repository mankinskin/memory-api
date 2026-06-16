# [ticket-api] Deliver safe cross-workspace ticket move for git-backed stores

## Goal

Implement a safe, reviewable `ticket move` capability for git-backed `memory-api` workspaces that relocates a ticket into a different workspace store and preserves correctness of the owning store, references, and recovery path.

## Chosen v1 scheme

- **Scope**: ticket-only in v1. Do not generalize to arbitrary `memory-api` entities until the ticket move contract is proven.
- **Topology**: permit a move only when **every ticket-to-ticket reference involving the moved ticket remains visible from the destination store after the move**. If any inbound or outbound ticket reference would become invisible from the destination store, fail the move at preflight.
- **Git assumption**: source workspace, target workspace, and all tracked text files rewritten by the move must live in the **same git worktree**. The tool may use git status + git mv/restore semantics as part of its safety model.
- **Board policy**: fail closed on **active or stale** board claims/leases in v1; historical board rows are migrated.
- **Atomicity model**: no cross-store transaction exists, so execution must be **journaled and resumable** with rollback for tracked file edits and post-step reindex validation.
- **References**: UUID-keyed ticket edges remain by ID; path-based references in specs/tests/docs must be rewritten when they cite the old ticket folder path.

## Relationship to planning

This is the execution tracker that follows planning ticket `13e9ce28`. The child tickets under this tracker implement the chosen scheme in storage, surfaces, and validation. Consumer reminder ticket `44abe1d4` stays blocked on this tracker.

## Acceptance criteria

- [ ] The child tickets implementing spec, planning/preflight, storage move execution, board handling, path-reference rewrite, CLI, MCP, HTTP, and validation are linked under this tracker.
- [ ] The delivered tool follows the v1 support boundary above and refuses unsupported cross-store topologies instead of partially moving tickets.
- [ ] Reminder ticket `44abe1d4` is unblocked only after the move tool and validation are complete.

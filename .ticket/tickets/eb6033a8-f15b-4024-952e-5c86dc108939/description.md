# [ticket-api] Add move preflight planner and destination-visibility validation

## Goal

Build the read-only planning layer for `ticket move` that decides whether a move is supported and enumerates every object the execution phase would touch.

## Scope

Implement a move planner that:

- resolves source ticket, source store, and target store from concrete workspace roots
- verifies source and target are in the same git worktree
- computes inbound and outbound ticket references for the moved UUID
- evaluates whether every ticket reference involving the moved ticket remains visible from the **destination** store after the move
- detects path-based references to the old ticket folder path in repo-local specs/tests/docs
- detects active or stale board entries/leases that should block the move in v1
- verifies tracked files to be edited are clean before execution
- returns a dry-run plan/report usable by CLI, MCP, and HTTP surfaces

## Acceptance criteria

- [ ] Unsupported sibling/cross-store moves that would strand ticket references are rejected at preflight with a concrete reason.
- [ ] The planner reports touched ticket IDs, board entries, and path-reference files before any mutation occurs.
- [ ] The planner exposes a reusable API for both local CLI and remote transport wrappers.

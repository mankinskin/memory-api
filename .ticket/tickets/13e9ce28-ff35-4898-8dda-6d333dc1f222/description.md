# [ticket-api] Cross-workspace move + automatic reference re-linking for store entries

## Goal

Provide a first-class, safe operation to move a ticket from one `memory-api` workspace store to another and automatically preserve the references that can be preserved safely, while refusing unsupported topologies before any partial move occurs.

This ticket owns the **design and hand-off only**. Execution now lives under tracker `505b2cd4`.

## Final design decision

### v1 support boundary

- **Entity scope**: ticket-only in v1. Generalizing to arbitrary `memory-api` entities is deferred until the ticket flow is proven.
- **Git assumption**: source workspace, target workspace, and any tracked text files rewritten by the move must live in the **same git worktree**.
- **Topology rule**: allow a move only when **every ticket-to-ticket reference involving the moved ticket remains visible from the destination store after the move**. If any inbound or outbound ticket reference would become invisible from the destination store, fail the move at preflight.
- **Board rule**: fail closed on **active or stale** board claims/leases in v1; historical board rows migrate with the ticket.
- **Reference rule**: UUID-keyed edges remain keyed by stable ticket ID; path-based references to the old ticket folder path in repo-local tracked text files are rewritten automatically and recorded in the move journal.
- **Atomicity model**: there is no cross-store transaction, so the move must be **journaled, resumable, and rollbackable**, with source/target reindex validation after apply and rollback.

### Why this boundary

Research showed that sibling stores cannot resolve each other's UUIDs unless scan-root visibility is deliberately shared, and the resolver spec already says one owning root wins. A "best effort" cross-store move would therefore strand references in unsupported topologies. The safe default is to reject those moves rather than partially relocating a ticket.

## Gaps resolved by this planning pass

- Planning and execution are now separated. This ticket remains planning-only.
- Execution is tracked by **[505b2cd4 Deliver safe cross-workspace ticket move for git-backed stores]**.
- Focused child tickets now cover spec updates, preflight/visibility planning, storage execution, board handling, path-reference rewrites, CLI, MCP, HTTP, and validation.
- Consumer reminder **44abe1d4** now blocks on the execution tracker rather than only the plan.

## Follow-on execution tickets

Execution tracker: `505b2cd4`

- `dc70628a` — update workspace-ownership specs with the move/relink contract
- `eb6033a8` — add move preflight planner and destination-visibility validation
- `bc691249` — add journaled storage-layer move execution
- `22cd3001` — enforce board safety and migrate historical board rows
- `3a26572a` — rewrite repo path references that cite the moved ticket folder
- `53176121` — add the `ticket move` CLI with dry-run and recovery guidance
- `84d19fab` — expose move planning/execution over MCP
- `373a3317` — add the HTTP move endpoint
- `da27c074` — validate supported/rejected topologies and recovery end to end

## Acceptance criteria (planning)

- [x] The support boundary for v1 is explicit and fail-closed.
- [x] Planning and execution are separated into a planning ticket plus an execution tracker.
- [x] Focused implementation tickets are created with ordered dependencies.
- [x] The reminder ticket to move `694d74b4` is linked as a downstream consumer.
- [x] A spec-update ticket exists instead of assuming a new duplicate spec.

## Non-goals

- Implementing the move operation in this ticket.
- Allowing unsupported sibling-store moves that would strand ticket references.
- Batch moving multiple tickets in v1.
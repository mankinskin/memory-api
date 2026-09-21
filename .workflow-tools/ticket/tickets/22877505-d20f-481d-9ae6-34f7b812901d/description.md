# Blocker Objective
Explain why memory-api orphan findings persisted at 38 after explicit depends_on links in chunk-2, and determine whether the issue is graph data, audit interpretation, or index staleness.

# Reproduction Snapshot (2026-07-06)
1. Confirmed chunk-2 tracker `ed8eb348` has 14 persisted depends_on edges in `memory-api/.ticket`.
2. Verified graph visibility in ticket-api:
   - `ticket subgraph --workspace memory-api ed8eb348 --depth 1 --json` returns 14 edges.
   - `ticket topgraph --workspace . b458cba7 --depth 1 --json` shows incoming edge from `ed8eb348`.
3. Initial root audit run still reported stale memory-api slice:
   - ticket_graph 44 = orphan 38 + convergence 6.
4. After root-scope ticket graph/health operations and rerun, audit shifted and stabilized:
   - ticket_graph 32 = orphan 23 + convergence 9 (stable across repeated runs).

# Key Finding
The blocker is not missing graph links in chunk-2 tickets. The evidence points to cross-store index visibility/reconciliation drift between root audit execution and ticket graph commands. The same dependency edges are visible in ticket-api graph queries but were not reflected in the first audit run output.

# Delta Evidence
- Resolved orphan IDs after reconciliation: 15 total.
- Included all 14 chunk-2 targets plus one additional ID (`609099ac-c5b5-4fe2-8072-a7b19ff8d75c`).
- New convergence findings introduced: 3, all with dependent `ed8eb348` still ahead of some prerequisites by workflow state.

# Current Residual
- memory-api ticket_graph residual after reconciliation: 23 orphan + 9 convergence.
- Remaining work can continue in batch-2; this blocker is narrowed to audit/index reconciliation semantics and no longer blocks orphan reduction execution.

# Follow-up Actions
1. Normalize audit pre-step to force deterministic ticket index reconciliation for root-scope runs before reading ticket_graph deltas.
2. Resolve new convergence edges for `ed8eb348` (or move tracker state appropriately) in batch-2 cleanup.
3. If audit still diverges after deterministic pre-step, escalate as ticket-api/audit-api integration bug with minimal repro.

# Resolution Update (2026-07-06)
- Deterministic pre-step is now used before each audit delta capture in batch-2.
- `ed8eb348` convergence side-effects were resolved by state alignment (`in-implementation` -> `new`).
- Remaining orphan and convergence residuals in memory-api batch-2 were cleared to zero.
- Current assessment: blocker scope is resolved as operational workflow guidance (reconciliation order), not a persistent graph-link data bug.
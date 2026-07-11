## Problem
Store-scoped health checks report `dangling_edge` errors for `depends_on` edges that point to tickets in a DIFFERENT store (e.g. memory-api tickets depending on root-store architecture tickets 82d6ada4 / 6bd67a7a / f8b447b7). These are valid cross-store URN references, not stale edges — the check simply resolves only within the active store.

## Evidence
Health scoped to `memory-api` reports 5 dangling edges; the same check against the aggregated `default` workspace reports 0. Targets exist in the root store (82d6ada4 "URN cross-store reference model and resolver" — done; 6bd67a7a — done; f8b447b7 — cancelled).

## Desired behavior
- The dangling-edge check must resolve `depends_on` targets across stores using the URN cross-store reference resolver (delivered by 82d6ada4) before declaring an edge dangling.
- An edge is only `dangling` if the target exists in NO discoverable store.
- Applies to CLI/MCP/HTTP health uniformly (depends on the health_check parity ticket).

## Scope
- memory-api/crates/ticket-api — dangling-edge detection + cross-store resolution.
- Add a test: a cross-store depends_on edge to an existing descendant/ancestor ticket is NOT reported dangling.

## Acceptance criteria
- Cross-store depends_on edges to existing tickets are not flagged dangling under store-scoped health.
- Only genuinely unresolvable targets remain `dangling_edge`.
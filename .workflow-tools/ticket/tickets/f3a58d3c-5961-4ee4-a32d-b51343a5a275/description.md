## Problem
Store-scoped health checks report `dangling_edge` ERRORS for `depends_on` edges that point to entities in a DIFFERENT store/workspace (e.g. memory-api tickets depending on root-store architecture tickets 82d6ada4 / 6bd67a7a / f8b447b7). These are valid cross-store URN references, not stale edges — the check resolves only within the active store's index.

## Foundation (already delivered — NOT blocked)
- `82d6ada4` URN cross-store reference model + resolver (`ce://<workspace>/<store>/<entity>`, `Urn`, `UrnResolver`) — done. Code: memory-api/crates/memory-api/src/model/urn.rs.
- `6bd67a7a` Dynamic multi-store discovery — done. Code: memory-api/crates/memory-api/src/discovery.rs (`discover_stores` — recursive, loop-safe, bounded).
- `7e318b2a` Late store onboarding reconciliation — done.
- Workspace policy (`include_descendants` / `include_ancestors` / `deny_external_paths`) — memory-api/crates/memory-api/src/workspace_policy.rs.

## Desired behavior — three-way classification
This is a SHARED base-memory-api capability, implemented ONCE and adopted by every entity store's health check (ticket, spec, test, rule, audit) with complete parity. Given a `depends_on` edge whose target is not in the active store, resolve it across discoverable stores via the URN resolver + `discover_stores`, honoring `deny_external_paths`, then classify:

1. OK — target resolves in the active store OR a policy-INCLUDED store (e.g. an indexed descendant). No finding.
2. WARNING (`cross_workspace_edge`, severity=warning) — target resolves ONLY in a store the active workspace policy does NOT index (typically a parent/ancestor workspace when `include_ancestors = false`). Edges into non-indexed parent workspaces are DISCOURAGED: they cannot be traversed by the store's own graph queries and couple a child store to its parent. Remediate by ONE of:
   - remove/retarget the edge;
   - move the entity into a store where the edge stays intra-policy (move tooling `505b2cd4`);
   - change the workspace policy to index that ancestor (`include_ancestors = true`).
3. ERROR (`dangling_edge`, severity=error) — target resolves in NO discoverable store. Genuinely stale; remove or retarget.

## Scope
- Base layer: memory-api/crates/memory-api — shared cross-store edge resolver + classifier returning {Ok, CrossWorkspaceWarning, Dangling} plus remediation instructions. Reuse `Urn`/`UrnResolver`, `discover_stores`, and `WorkspacePolicy`. No per-store duplication.
- Per-store adoption: each store health (`ticket_api::health::collect_findings` and the spec/test/rule/audit equivalents) delegates edge classification to the shared base classifier so finding keys, severities, and messages are identical.
- Applies to CLI/MCP/HTTP uniformly (ticket-api parity via `c680b137`; mirror for other stores).

## Acceptance criteria
- Cross-store `depends_on` edges to existing entities in policy-INCLUDED stores are NOT flagged.
- Edges resolving only into a NON-indexed parent/ancestor workspace produce a `cross_workspace_edge` WARNING carrying the three remediation options — not an error, not silence.
- Only genuinely unresolvable targets remain `dangling_edge` (error).
- The classifier lives in base memory-api and is consumed by ticket + at least one other store (spec) with identical finding output (parity test).
- `deny_external_paths` is honored as the hard security boundary.
- memory-api-scoped ticket health drops from 5 danglers to 0 errors (the 5 root-store refs reclassify: existing→OK/warning, cancelled/absent→as appropriate); a synthetic truly-missing edge still flags `dangling_edge`.
- Tests: (a) edge to indexed descendant → OK; (b) edge to non-indexed ancestor → `cross_workspace_edge` warning; (c) unresolvable → `dangling_edge` error; (d) parity across ticket + spec health.

## Related
- `b03be2d5` (memory-api) cross-entity edges (spec↔ticket) — shares the base edge-resolution surface; align on one resolver, avoid a second implementation.
- `7599ed31` (memory-api) placement principle — this warning operationalizes "entities live in the lowest-owning store".
- `671d4e47` multi-store architecture tracker — umbrella.

## Blocker status
NOT blocked. Foundation (`82d6ada4` / `6bd67a7a` / `7e318b2a`) is done and the base primitives (`urn.rs`, `discovery.rs`, `workspace_policy.rs`) exist. Move tooling `505b2cd4` only enables one of three remediation paths; the warning itself does not require it.
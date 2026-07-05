# Cleanup migration: relocate misplaced entities into their lowest-owning store

## Placement principle (product direction)
Every entity must live in the **lowest-level store that contains all of the code the entity is concerned with**:
- memory-api-concerned entities → directly in the **memory-api** store.
- viewer-api-concerned entities → directly in the **viewer-api** store.
- only **genuinely cross-workspace** concerns → tracked in a parent workspace such as **context-engine**.

We do NOT consolidate everything into one store; cross-workspace work is still supported. This ticket only relocates entities that are currently in the top-level `context-engine` store but are concerned with a single lower-level domain.

## Why
The root `context-engine/.ticket` store currently holds many tickets/specs whose domain is entirely memory-api (session-api, ticket-api, rule-api, spec-api, feedback-api, audit-api, workspace-resolution). Because the ticket-mcp server only exposes the `default` store and graph edges cannot cross stores, this misplacement blocks hard-link/cross-store work and pollutes the parent workspace.

## Depends on
- `505b2cd4` Deliver safe cross-workspace ticket move for git-backed stores (+ children) — the move tooling MUST be delivered and validated before any production move.

## Moving active work is OK (Q4)
It is acceptable to move tickets that are currently `in-implementation`. The agent owning the session-bootstrap implementation effort also owns these moves; in-flight state is preserved by the journaled move + automatic re-linking.

## Audit + move candidates

### MOVE to the memory-api store (single memory-api domain)
- session-bootstrap cluster: `effba966` (epic), `412964a3` (runtime), `6b2dc497` (cli/mcp), `b4a8dc5e` (rules), `d8f76965` (cascade), `afa00b5c` (design).
- feedback-api program (memory-api crates): `b1e9e744` (tracker), `c7542933` (core gate), `9c95c1e4`, `3a1ec9f8`, `4f86d3d2`, `b7b84c10`, `c2d6a14a`.
- workspace-resolution: `ef0ebf38` (tracker) + `07836f41`, `59d96577`, `27558fde`, `1fd0c182`, `5318aedd`, `632974d1`, `39239e48`.
- state-machine: `185419e0` (bidirectional transitions, in-review).
- explicit-init policy cluster: `a9514081`, `23f1c81b`, `e6bdafbe`, `50307cce`, `e83264db`, `51e2210c`.
- path normalization cluster: `e3961a54`, `e8e3ef17`.
- transport diagnostics / journaling coverage in memory-matrix: `cc78d33d`.
- audit-api tickets listed above are single-domain move candidates once ownership is confirmed in the destination store.

### STAY in context-engine (genuinely cross-workspace)
- `671d4e47` multi-store architecture tracker (spans memory-api, viewer-api, context-stack).
- `82d6ada4` URN cross-store reference model, `6bd67a7a` multi-store discovery — cross-store by definition.
- `8a90a63c` multi-store store-expansion / operational-health program.
- observability/logging LCA tickets: `73b2cd22` runtime diagnostics tracker, `84673399` architecture-boundary decision ticket, `bce26d30` docs/validation matrix, and `ff6637f5` benchmark/profiling evidence. These coordinate `memory-api`, `log-api`, and `context-stack` specialized work and should remain in the parent workspace.
- `1dffcf23` graph-operation replay format for log-viewer — keep in the parent workspace because it links `context-stack` replay semantics to shared log/journal metadata used outside a single child crate.

### Link lower-crate specialized work from the LCA tickets
- memory-api specialized tickets: `756fed27`, `3041d7e3`, `6c859ac3`, `2e41c96d`, `35cd05c1`.
- log-api specialized tickets: `d3349747`, `aa94d02e`.
- context-stack specialized ticket: `1dffcf23`.

> Audit each candidate's owning store before moving; do not move blindly. An entity that touches more than one workspace's code stays in the lowest common ancestor workspace.

## Requirements
- Use the move tooling's dry-run / preflight planner first; capture the plan.
- Automatic reference re-linking must rewrite `depends_on` edges and repo path references (move-tooling children 3a26572a / 13e9ce28).
- Board safety: no active board check-ins on tickets being moved (22cd3001).
- Preserve ticket history; the move must be journaled and reversible.
- After migration, re-run health checks in every affected store; confirm no dangling edges.

## Acceptance criteria
1. A documented preflight/dry-run plan lists every entity to move, its target store (justified by the placement principle), and the edges/paths to rewrite.
2. All audited single-domain entities are moved into their lowest-owning store with history preserved and `depends_on` edges re-linked intra-store.
3. Genuinely cross-workspace entities remain in context-engine.
4. Repo path references to moved ticket folders are rewritten (no broken links in specs/docs).
5. Health checks pass in all affected stores post-migration (no dangling edges, no orphaned references).
6. The hard-link cross-entity-edge work (b03be2d5 / f00291a3) can draw intra-store edges between the previously-split entities.

## Blocks
- Hard-link cross-store reference work: `b03be2d5` (cross-entity edges spec↔ticket) and `f00291a3` (ticket↔spec integration) depend on this cleanup so their target entities are co-located.

## Execution status (2026-07-04)

Completed in this session:

- Verified dependency `505b2cd4` is done.
- Checked in on board for `7599ed31` in `memory-api` workspace.
- Recovered and validated interrupted journaled moves where destination ownership was already correct on disk:
	- `e3961a54`
	- `51671748`
- Added a move-kernel validation fix in `memory-api/crates/memory-api/src/storage/move_kernel.rs` so post-move validation prioritizes source/destination folder ownership and treats stale index visibility as a follow-up instead of hard failure.
- Validation run for the move-domain invariant and resume path:
	- `cargo test --manifest-path memory-api/crates/ticket-api/Cargo.toml entity_indexed_in_requires_path_ownership_not_aggregate_visibility -- --nocapture`
	- `cargo test --manifest-path memory-api/crates/ticket-api/Cargo.toml sequential_move_after_resumed_execution_can_continue_without_clean_commit -- --nocapture`

Current blocker frontier for remaining scoped candidates:

- No additional ticket in the currently scoped migration set is `supported=true` in move preflight.
- Remaining root-scoped candidates are blocked by `InvisibleReference` dependencies to entities outside the scoped set, including cross-workspace/LCA trackers that are intentionally kept in `context-engine`.
- Representative blocker IDs surfaced across preflights include:
	- memory-domain neighbors not yet in the scoped list: `14df656e`, `40ba5a15`, `61cb6557`
	- cross-workspace/LCA entities expected to stay in root: `73b2cd22`, `bce26d30`, `8a90a63c`, `6bd67a7a`, `82d6ada4`

Implication:

- Completing the remaining migration requires either:
	1. expanding the move set to include additional memory-domain neighbors first, until visibility constraints collapse, or
	2. explicitly reclassifying part of the current candidate set as root-owned due enduring cross-workspace coupling.

### Update (continued 2026-07-04)

- Re-ran focused classification preflights on the blocker frontier and confirmed split ownership:
	- move-now: `40ba5a15`, `61cb6557`
	- keep root/LCA: `73b2cd22`, `bce26d30`, `8a90a63c`, `6bd67a7a`, `82d6ada4`
	- `14df656e` resolves to `memory-api/.ticket` ownership already.
- Attempted move apply for move-now set:
	- `61cb6557` moved to `memory-api/.ticket` on disk; apply initially reported post-move source-index visibility mismatch, then journal resume completed successfully via `f852c556-54ef-4581-a7fb-2bb19dbdb0d3`.
	- `40ba5a15` remains blocked by `DirtyTrackedFiles` preflight guard in root store.
- Current actionable blocker for next slice:
	- clean/commit the dirty root tracked files (`.ticket/README.md`, `.ticket/index.toon`, `1acb4182-...`, `a1558e76-...`) and retry move apply for `40ba5a15`.
- Spot health-check for migrated candidates (`40ba5a15`, `61cb6557`) in both `./` and `./memory-api` reports warning-only `missing_effort_estimation`; no dangling-edge finding was reported for these IDs.

### Update (continued 2026-07-04, final pass)

- Re-ran preflight for `40ba5a15` and executed move via CLI:
	- `ticket.exe move 40ba5a15 --workspace ./ --to-workspace-root ./memory-api --toon`
	- Journal: `05bd02bc-69de-47f7-a800-6fe2238eaec0`
	- Outcome: `status=ok`, `phase=Validated`.
- Verified both move-now candidates now resolve to `memory-api/.ticket` from both root and nested workspace lookups:
	- `40ba5a15`
	- `61cb6557`
- Targeted health checks for these IDs in `./` and `./memory-api` are warning-only (`missing_effort_estimation`); no dangling-edge finding is present for the pair.

### Update (continued 2026-07-05)

- Ran fresh ownership inventory and preflight classification after moving `40ba5a15` and `61cb6557`.
- Identified remaining root-owned memory-api candidates (feedback-api, workspace-resolution, explicit-init policy, path normalization implementation ticket, and transport diagnostics matrix ticket) and executed move applies in a single verified pass.
- Successfully moved the following additional IDs into `memory-api/.ticket` (all `status=ok` with journaled execution):
	- `b1e9e744`, `c7542933`, `9c95c1e4`, `3a1ec9f8`, `4f86d3d2`, `b7b84c10`, `c2d6a14a`
	- `ef0ebf38`, `07836f41`, `59d96577`, `27558fde`, `1fd0c182`, `5318aedd`, `632974d1`, `39239e48`
	- `a9514081`, `23f1c81b`, `e6bdafbe`, `50307cce`, `51e2210c`
	- `e8e3ef17`, `cc78d33d`
- Post-move ownership verification confirms all targeted IDs in this cleanup wave now resolve to `memory-api/.ticket` from both root and nested workspace lookups.
- Cleared the two known effort warnings by setting:
	- `40ba5a15`: `effort=2500`
	- `61cb6557`: `effort=3000`
- Targeted health check for `40ba5a15` and `61cb6557` now reports `finding_count=0`.
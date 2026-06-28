# [memory-api] Generalize cross-workspace move into a domain-neutral kernel with per-domain trait specialization

## Goal

Promote the proven ticket-only cross-workspace move (delivered by `505b2cd4`) into a **domain-neutral generic move kernel** in `memory-api` that every domain store reuses through a trait, with **no duplicated move logic (DRY)**.

## Implementation (delivered)

Generic kernel extracted to `memory-api/crates/memory-api/src/storage/move_kernel.rs` (re-exported from `memory_api::storage`):
- Neutral types: `MovePlan`, `MoveJournal`, `MoveOutcome`, `MoveBlocker`, `MoveReferenceVisibility`, `MoveReferenceDirection`, `GitWorktreeTopology`, `MoveExecutionPhase`, `MovePathRewrite`, `MoveManualFollowup`, `MoveLeaseBlock`, `MoveReferences`, `MoveBoardState`, `MoveError`. No ticket-specific types in any kernel signature; identities are bare `Uuid`s and board rows use the shared `BoardEntry`.
- Generic functions `plan_move` / `execute_move` / `resume_move` / `rollback_move` own all git-topology classification, tracked-path-reference scan + rewrite, dirty-file detection, lock-set management, and journal persistence.
- `MoveDomain` trait injection points: `entity_subdir` + `store_index_dir` (path resolution), `source_entity_path`, `related_entities` (edge enumeration), `target_store_present` + `entity_indexed_in` (destination visibility), `board_state` + `active_leases` + `migrate_board_history` + `restore_board_history` (board/lease detection + historical migration), `scan_store`. Blocker classification is owned by the kernel over the shared `MoveBlocker` enum.

First adopter (`ticket-api`): `move_planner.rs` / `move_execution.rs` reduced to a `TicketMoveDomain` adapter + thin delegating methods that re-export the neutral kernel types under the established public paths. CLI/MCP/HTTP JSON keys preserved (only Rust field accessors updated to `source_entity_path` / `destination_entity_path` / `journal.entity_id`).

Second adopter (`spec-api`): `move_domain.rs` adds `SpecMoveDomain` + `SpecStore::{plan_move_preflight, execute/resume/rollback_move_with_journal}`, demonstrating reuse with empty board/lease hooks. Surface wiring (spec CLI/MCP/HTTP) left for a follow-up per scope.

## Acceptance criteria

- [x] Domain-neutral move planner + journaled executor in shared `memory-api`, no ticket-specific types in kernel signatures.
- [x] Trait specialization points defined + documented for entity/path resolution, reference enumeration + visibility, board/lease checks + historical migration, path-reference rewrite, and blocker mapping.
- [x] `ticket-api` move reimplemented on the kernel via the trait impl; existing move tests pass (planner preflight, execute/resume/rollback, active-board fail-closed, historical board migration, path-reference rewrite + rollback, cross-worktree e2e).
- [x] Second domain (`spec-api`) implements the trait and moves an entity between stores via the shared kernel.
- [x] No move logic duplicated across domain crates — domain crates contain only trait impls + domain helpers.

## Validation

- `cargo test -p ticket-api` move suites: `move_planner` (3) + `move_execution` (7) + `e2e_fixture_move` (1) pass.
- `cargo test -p spec-api --lib move_domain` (1) passes.
- `cargo test -p ticket-api -p spec-api -p memory-api` full suites pass; `ticket-cli`/`ticket-mcp`/`ticket-http` compile.
- Evidence: test-api `vt-move-kernel` / `exec-vt-move-kernel-20260628` (passed).

## Relationship / traceability

- Follows `505b2cd4` (proven ticket-only contract) and `21e6c015` (cross-git-worktree topology, exposed through the kernel rather than reimplemented).
- Cross-store context (default `.ticket` store, textual refs): `2b1279bd` neutral storage kernel; `671d4e47` multi-store architecture tracker.

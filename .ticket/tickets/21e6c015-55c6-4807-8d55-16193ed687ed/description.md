# [memory-api] Support cross-git-worktree (submodule) entity moves

## Goal

Extend the cross-workspace move contract so an entity can be moved between two stores that live in **different git worktrees** — most importantly across **git submodule boundaries**. This repository heavily uses submodules (`memory-api`, `viewer-api`, nested `memory-api`/`viewer-api` under `memory-viewers`), so moves between the root repo's store and a submodule store must be a supported, journaled, recoverable operation. The capability must apply to both the ticket move and the generic entity move scheme.

## Problem / current state

The v1 move (`505b2cd4`) fails closed on a `DifferentGitWorktree` blocker whenever source and destination stores are not in the same git worktree:

- `memory-api/crates/ticket-api/src/storage/move_planner.rs` — `git_toplevel` is computed for source and target; `MovePreflightBlocker::DifferentGitWorktree { source_worktree_root, target_worktree_root }` is raised when they differ.
- `memory-api/crates/ticket-api/src/storage/move_execution.rs` — the executor relies on single-worktree `git mv` / `git restore` semantics and journaled path-reference rewrites within one repo.

Concrete failing case observed: moving trackers `2b1279bd` / `671d4e47` from the root `.ticket` store into `memory-api/.ticket` is blocked solely by `DifferentGitWorktree` (all references stay visible, no board entries, no leases). `memory-api` is a submodule with its own worktree, so the v1 contract refuses the move.

## Scope

- Detect and classify the worktree relationship: same worktree, parent↔submodule (nested worktree), or unrelated worktrees.
- Allow moves across worktrees when the relationship is a recognized/safe topology (e.g. parent repo ↔ submodule checked out under it), instead of blanket-failing on any worktree difference.
- Per-worktree git operations: stage the removal in the source worktree's git and the addition in the destination worktree's git (two `git mv`-equivalent half-operations across repos), preserving journaled rollback for each side.
- Path-reference rewrite across worktrees: references may live in either repo; rewrites and their rollback must be journaled per worktree.
- Submodule-pointer awareness: moving content into/out of a submodule changes both repos' indexes; the move must leave each repo in a committable state and surface the required follow-up commits (source repo, submodule repo, and any parent submodule-pointer bump) as manual followups.
- Keep the fail-closed posture for genuinely unsafe topologies (unrelated worktrees, dirty index on either side, detached/missing submodule).

## Non-goals

- A single cross-repo atomic git transaction (does not exist) — execution stays journaled, resumable, and rollbackable per worktree.
- Auto-committing the resulting changes or auto-bumping submodule pointers; the move surfaces these as manual followups.
- Relaxing the board/lease fail-closed policy.

## Acceptance criteria

- [ ] Move preflight distinguishes "different but safe" worktree topologies (parent ↔ submodule) from genuinely unsafe ones, and only the unsafe cases raise a worktree blocker.
- [ ] A ticket can be moved from the root `.ticket` store into a submodule store (e.g. `memory-api/.ticket`) and back, with journaled execute / resume / rollback working across both worktrees.
- [ ] Path references in either worktree are rewritten and correctly restored on rollback.
- [ ] The move reports the required per-repo manual followups (source commit, submodule commit, parent submodule-pointer bump) without performing them automatically.
- [ ] The concrete case (`2b1279bd` / `671d4e47` root → `memory-api`) is moved successfully using the tool once this lands, or covered by an equivalent test fixture.
- [ ] Tests cover: parent→submodule move, submodule→parent move, rollback on each, and fail-closed on an unrelated/dirty worktree.

## Implementation notes — fixes landed

- **Windows verbatim-path (`\\?\`) bug fixed.** `std::fs::canonicalize` in the CLI move handler ([lifecycle.rs](memory-api/tools/cli/ticket-cli/src/cli/commands/lifecycle.rs)) and MCP (`normalize_workspace_root` in [mutations.rs](memory-api/tools/mcp/ticket-mcp/src/server/mutations.rs)) returned a Windows extended-length `\\?\` path. That prefix leaked into the move journal's `target_store_root`, into rewritten path references (corrupting tracked files with `//?/C:/...`), and into post-move validation (false `destination missing`). Fix: added `workspace::canonicalize_workspace_root` + `strip_verbatim_prefix` in [workspace.rs](memory-api/crates/memory-api/src/workspace.rs); both CLI and MCP now route through it. Defense-in-depth: `normalize_slashes` in [move_execution.rs](memory-api/crates/ticket-api/src/storage/move_execution.rs) also strips the prefix. Regression tests added in the workspace module.
- **Validation error message made informative.** The post-move validation no longer returns the vague "source still owns ticket or destination missing"; it now reports which side failed, the resolved store roots, the ticket id, the expected folder path, and a hint about the verbatim-prefix cause.

## Known gap — pending

- **Sequential moves blocked by own pending rewrites.** Moving one ticket rewrites tracked path references (e.g. a shared spec body) and leaves them uncommitted; a subsequent move of a second ticket then fail-closes on `DirtyTrackedFiles` for that same file. Observed when moving `2b1279bd` then `671d4e47` (both reference shared spec bodies). Decide whether the move should tolerate its own uncommitted rewrites, batch-move a set, or document that each cross-worktree move must be committed before the next. This needs a contract decision and a test.

## Relationship / traceability

- Extends `505b2cd4` ([ticket-api] Deliver safe cross-workspace ticket move for git-backed stores) — adds the cross-worktree topology the v1 contract deliberately excluded.
- Feeds the generic entity move scheme `0a510279` ([memory-api] Generalize cross-workspace move into a domain-neutral kernel with per-domain trait specialization): the generic kernel must expose cross-worktree support through the same trait specialization points, so this capability is part of the generic featureset rather than a ticket-only extension.
- E2E + benchmark coverage tracked by `026b2eb6` ([memory-api] E2E test workspace fixture repository — multi-store, multi-submodule) — the multi-submodule fixture would have caught the `\\?\` regression.

## Session handoff findings — 2026-06-30

- A focused retest attempt for the explicit workspace-root child-ticket path is currently blocked before the target test runs.
- Command attempted from the `memory-api` workspace root: `cargo test -p ticket-cli dispatch_list_reads_child_ticket_from_explicit_workspace_root`.
- Cargo compiled `ticket-http` through the `ticket-cli` dependency graph and failed in `memory-api/tools/http/ticket-http/src/middleware.rs` with `error[E0463]: can't find crate for axum`.
- That means the next session should first restore the `ticket-http` dependency resolution or otherwise isolate the `ticket-cli` slice before using this regression as evidence.
- After that compile blocker is cleared, re-run the focused child-ticket/root-targeting regression and then decide whether the remaining sequential-move dirty-reference gap belongs in this ticket or a follow-on contract ticket.
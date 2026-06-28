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

## Relationship / traceability

- Extends `505b2cd4` ([ticket-api] Deliver safe cross-workspace ticket move for git-backed stores) — adds the cross-worktree topology the v1 contract deliberately excluded.
- Feeds the generic entity move scheme `0a510279` ([memory-api] Generalize cross-workspace move into a domain-neutral kernel with per-domain trait specialization): the generic kernel must expose cross-worktree support through the same trait specialization points, so this capability is part of the generic featureset rather than a ticket-only extension.

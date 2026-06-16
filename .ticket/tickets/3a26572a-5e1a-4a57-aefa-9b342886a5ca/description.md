# [ticket-api] Rewrite repo path references that cite the moved ticket folder

## Goal

Automatically update repo-local text references that cite the moved ticket's old folder path so specs, tests, and docs do not point at a stale store location after the move.

## Scope

Implement a path-reference rewrite pass that:

- scans tracked repo-local text files for the moved ticket's old folder path
- rewrites references to the new target-store path
- records every rewritten file in the move journal
- reports untracked, binary, or unsupported files for manual follow-up rather than silently skipping them

This ticket is about **path-based** references. UUID-keyed ticket edges are handled by preflight visibility rules and the storage move itself.

## Acceptance criteria

- [ ] Specs/tests/docs that cite the old folder path are updated automatically when they are tracked text files in the same git worktree.
- [ ] The move report distinguishes rewritten references from manual follow-up references.
- [ ] A rollback restores the previous file contents for all rewritten tracked files.

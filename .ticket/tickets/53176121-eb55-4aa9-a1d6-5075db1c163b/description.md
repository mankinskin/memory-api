# [ticket-cli] Add `ticket move` CLI with dry-run and recovery guidance

## Goal

Expose the move planner and executor through `ticket-cli` so operators can preview, apply, resume, and roll back a move from the command line.

## Scope

Add a CLI command shaped around concrete workspace roots, for example:

- `ticket move <id> --to-workspace-root <PATH> --dry-run`
- `ticket move <id> --to-workspace-root <PATH>`
- `ticket move --resume <MOVE_ID>` / `--rollback <MOVE_ID>` (or equivalent)

The CLI should print a compact move plan, touched references, git cleanliness blockers, unsupported topology reasons, and journal recovery instructions.

## Acceptance criteria

- [ ] The CLI can preview a supported move without mutating storage.
- [ ] The CLI can execute, resume, and roll back a move using the shared storage-layer primitive.
- [ ] The CLI uses the normalized nested-workspace option contract rather than inventing a one-off path flag behavior.

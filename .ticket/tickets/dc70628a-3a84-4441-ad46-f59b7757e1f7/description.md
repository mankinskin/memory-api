# [ticket-api][spec] Extend workspace-ownership specs with move/relink contract

## Goal

Update the existing `memory-api` recurring-principles specs for `nested-workspace-resolution` and `workspace-identifiers` so the move operation has an explicit contract before implementation lands.

## Scope

Add or update acceptance criteria covering:

- the v1 support boundary: ticket-only, git-backed, fail-closed moves
- destination-visibility rule: a move is rejected if any ticket reference involving the moved ticket would become invisible from the destination store after the move
- source/target workspace-root normalization requirements
- active/stale board claims block a move in v1; historical board rows migrate
- journaled execution and recovery expectations when a move fails mid-flight
- path-reference rewrite expectations for specs/tests/docs that cite the old ticket folder path

Use the existing spec surfaces in `memory-api/.spec/` rather than creating an unrelated duplicate spec.

## Acceptance criteria

- [ ] Existing workspace-resolution spec sections are updated to describe move ownership and rejection rules.
- [ ] The spec names the failure modes the tool must report for unsupported topologies, dirty git worktrees, and active board claims.
- [ ] Related tickets include the exact spec links or identifiers once updated.

# Phase 6: Cleanup and Deduplication

## Objective

Clean up the ticket store by cancelling stale tickets whose agent files were deleted, merging duplicates, and ensuring all tickets with descriptions have `doc_category` set.

## Tasks

### 6a. Cancel Deleted-File Tickets

~21 tickets reference agent files that were deleted in commit `3b5b8c06` and no longer have useful content. Transition these to `cancelled` state.

### 6b. Merge Duplicate Pairs

Some tickets cover the same topic from different angles. Merge by:
1. Keeping the more complete ticket
2. Appending unique info from the other as an asset
3. Linking with `linked` edge, then closing the duplicate

Known candidate pairs to investigate (from cross-reference analysis):
- Multiple "parallel walk" tickets
- Multiple "cache invalidation" tickets
- Multiple "search optimization" tickets

### 6c. Set `doc_category` on All Described Tickets

Ensure every ticket with a `description.md` has `doc_category` set to one of:
`plan`, `interview`, `bug-report`, `design`, `research`, `analysis`, `bootstrap`

### 6d. Final Audit

- Run `ticket list` and verify:
  - No tickets in limbo (open with no description and no planned work)
  - All `doc_category` values are from the allowed set
  - Dependency graph is acyclic

## Risk

**Medium** — cancelling/merging is semi-destructive. Review each candidate before acting.

## Verification

- `ticket list --state cancelled` shows only intentionally cancelled tickets
- No orphan tickets without descriptions or planned work
- `doc_category` field set on all tickets with descriptions

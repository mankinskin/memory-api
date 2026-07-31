# Append-only history with materialized index

Every `memory-api` store keeps an append-only `history.ndjson` per entity as the source of truth, and rebuilds a materialised SQLite/Tantivy index from it. Reads serve from the index; writes append to history and update the index in the same transaction.

## Write path

1. The store validates the requested mutation against the canonical model.
2. A new history record is appended to `<entity>/history.ndjson` (or the rule equivalent) capturing the actor, timestamp, before/after fields, and the request id.
3. The materialised index is updated in the same logical transaction (SQLite write + Tantivy commit) so reads see the new state without replaying history.

## Recovery and rebuild

- The materialised index can be deleted at any time. `<x> scan` (or `<x> scan --force`) rebuilds it by replaying every history file.
- History files are never rewritten in place. Migrations append compensating records rather than editing prior ones, so the audit trail stays complete.
- Concurrent writers coordinate through a per-entity lock; the index update is atomic with the history append.

## Implications for callers

- Time travel and audit queries are served by replaying history, not by querying the index.
- `update --undo` requires at least one prior history record; tickets created directly in a non-initial state cannot be undone.
- Any field that needs to be queryable must be projected into the materialised index — adding a field to the model is a two-step change (history first, index projection second).

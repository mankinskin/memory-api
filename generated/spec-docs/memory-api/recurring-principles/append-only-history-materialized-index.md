<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=4c14f55b-4255-4b91-ada3-d060ea835cfd slug=memory-api/recurring-principles/append-only-history-materialized-index/append-only-history-with-materialized-index/l1 -->
# Append-only history with materialized index

Every `memory-api` store keeps an append-only `history.ndjson` per entity as the source of truth, and rebuilds a materialised SQLite/Tantivy index from it. Reads serve from the index; writes append to history and update the index in the same transaction.

<!-- rule-api:entry id=e3204d91-244e-4a84-bd9d-1949a158b66a slug=memory-api/recurring-principles/append-only-history-materialized-index/append-only-history-with-materialized-index/write-path/l5 -->
## Write path

1. The store validates the requested mutation against the canonical model.
2. A new history record is appended to `<entity>/history.ndjson` (or the rule equivalent) capturing the actor, timestamp, before/after fields, and the request id.
3. The materialised index is updated in the same logical transaction (SQLite write + Tantivy commit) so reads see the new state without replaying history.

<!-- rule-api:entry id=e9aaea46-54ef-4814-970d-ee23894ca6dc slug=memory-api/recurring-principles/append-only-history-materialized-index/append-only-history-with-materialized-index/recovery-and-rebuild/l11 -->
## Recovery and rebuild

- The materialised index can be deleted at any time. `<x> scan` (or `<x> scan --force`) rebuilds it by replaying every history file.
- History files are never rewritten in place. Migrations append compensating records rather than editing prior ones, so the audit trail stays complete.
- Concurrent writers coordinate through a per-entity lock; the index update is atomic with the history append.

<!-- rule-api:entry id=8b8a609a-638b-4c6b-8d42-3c4740720476 slug=memory-api/recurring-principles/append-only-history-materialized-index/append-only-history-with-materialized-index/implications-for-callers/l17 -->
## Implications for callers

- Time travel and audit queries are served by replaying history, not by querying the index.
- `update --undo` requires at least one prior history record; tickets created directly in a non-initial state cannot be undone.
- Any field that needs to be queryable must be projected into the materialised index — adding a field to the model is a two-step change (history first, index projection second).

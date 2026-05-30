## Write path

1. The store validates the requested mutation against the canonical model.
2. A new history record is appended to `<entity>/history.ndjson` (or the rule equivalent) capturing the actor, timestamp, before/after fields, and the request id.
3. The materialised index is updated in the same logical transaction (SQLite write + Tantivy commit) so reads see the new state without replaying history.
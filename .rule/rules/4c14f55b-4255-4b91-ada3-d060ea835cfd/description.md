# Append-only history with materialized index

Every `memory-api` store keeps an append-only `history.ndjson` per entity as the source of truth, and rebuilds a materialised SQLite/Tantivy index from it. Reads serve from the index; writes append to history and update the index in the same transaction.
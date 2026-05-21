# Problem

`ticket link` and `ticket unlink` currently mutate dependency edges only in the ignored `.ticket/tickets.db` SQLite index.
The tracked ticket files (`ticket.toml`, `history.ndjson`, `description.md`) do not record those edge mutations, and `ticket scan --force` does not rebuild edges from the tracked filesystem state.

That creates a mismatch with the documented storage model in `ticket-api`, which describes the filesystem as the source of truth and the SQLite store as metadata/index state.

# Evidence

- `TicketStore::add_edge` and `TicketStore::remove_edge` write through `self.index.insert_edge(...)` and `self.index.delete_edge(...)` only.
- The nested store ignores `.ticket/tickets.db` and `.ticket/search_index/` in `.ticket/.gitignore`.
- In a copied `.ticket` store with `tickets.db` and `search_index/` removed, `ticket init` plus `ticket scan --force` re-integrated 144 tickets but reconstructed 0 edges.
- The live local store still had 153 rows in the SQLite `edges` table, so the graph state was present only in ignored index artifacts.

# Impact

- Dependency graph cleanup can appear to succeed locally while producing no git-visible ticket-file changes.
- Rebuilding the index from tracked files loses the dependency graph.
- Audit results and ticket graph health depend on local ignored DB state rather than the tracked ticket store contents.

# Desired Outcome

Make dependency edges survive a clean rebuild from tracked ticket files, or explicitly redefine and document the storage model if DB-only graph state is intentional.

# Acceptance Criteria

- `ticket link` and `ticket unlink` update a file-backed representation of dependency edges in the tracked ticket store.
- `ticket scan --force` reconstructs the edge table from the tracked ticket files after deleting `tickets.db` and `search_index/`.
- Regression tests cover link persistence plus scan round-trip from tracked files into a fresh index.
- Storage documentation matches the implemented source-of-truth model for tickets and edges.

## What lives in `<x>-api`

- Entity types, field definitions, and the `required_states` schema.
- Store traits and their default implementations (filesystem, SQLite index, Tantivy search).
- Append-only history writers and the materialised-index rebuild path.
- Validation, edge semantics (including `depends_on`), and health checks.
- The single id/prefix resolver shared across all surfaces.
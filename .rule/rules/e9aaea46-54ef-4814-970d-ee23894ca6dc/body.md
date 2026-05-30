## Recovery and rebuild

- The materialised index can be deleted at any time. `<x> scan` (or `<x> scan --force`) rebuilds it by replaying every history file.
- History files are never rewritten in place. Migrations append compensating records rather than editing prior ones, so the audit trail stays complete.
- Concurrent writers coordinate through a per-entity lock; the index update is atomic with the history append.
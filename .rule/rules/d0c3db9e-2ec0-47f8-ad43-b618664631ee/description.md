# `depends_on` is the only blocking edge

In `ticket-api` and `spec-api`, a ticket or spec is considered actionable only if every `depends_on` target is in a resolved state. No other edge type blocks state transitions. Non-blocking relationships use the typed edge index, not Tantivy full-text search.
## Edge index, not Tantivy

Edge lookups (forward, reverse, transitive closure) are served by the typed edge index in the materialised SQLite store. Tantivy is reserved for full-text search over body content. Reusing Tantivy for edge traversal pollutes search ranking and forces edge queries through an inverted-index query plan that is far slower than a primary-key join.
# Problem

The current `rule-targets.toml` model builds each output file from one flat filter. That keeps the implementation small, but it makes file composition hard to reason about, encourages repeating rule entries across multiple outputs, and provides no explicit per-document outline or stable per-section ordering.

# Decision Record

- Keep rule entries reusable across repos and files.
- Move target construction from a single file-wide filter to a hierarchical document tree.
- Each output document should declare an explicit ordered outline of nodes or sections.
- Each node may apply a smaller local filter to collect entries for that portion of the document.
- Generation must be explainable so tooling can show which entries matched each node and why.
- Deterministic ordering must be explicit at both the outline-node level and the entry level.
- Existing flat targets need a migration path.

# Scope

Design and implement a hierarchical target-construction model for `rule-api`.

This includes:

- target schema changes for ordered outline nodes
- node-local filter evaluation and entry collection
- deterministic rendering from the target tree
- preview/explain tooling for document composition
- migration of existing flat targets to the new structure

# Acceptance Criteria

- A target config can define an ordered outline for a document instead of one flat file-level filter.
- Each outline node can bind local filter criteria and collect entries for only that part of the document.
- The rendered file order is deterministic and directly readable from the target config.
- CLI and MCP tooling can preview or explain which entries will appear in each node and why.
- Existing `rule-targets.toml` configurations can be migrated without losing current generated outputs.

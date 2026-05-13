# Problem

Even with a better schema, the current generation path still assumes one file-wide filter and one flat ordered list of rule entries. The evaluator needs to understand a hierarchical target tree and render each node deterministically.

# Scope

Implement hierarchical target evaluation and rendering in `rule-api`, `rule-cli`, and `rule-mcp`.

The implementation should cover:

- deterministic traversal of outline nodes
- node-local entry collection
- stable ordering within and across nodes
- duplicate-placement rules within a single rendered document
- provenance that remains understandable after hierarchical assembly

# Acceptance Criteria

- Target generation evaluates outline nodes in deterministic config order.
- Entry ordering within each node is deterministic and test-covered.
- Duplicate entry placement within one file is either rejected or controlled explicitly by config.
- Generated markdown remains deterministic across runs.
- CLI and MCP generation surfaces use the hierarchical evaluator instead of the flat file-wide filter.

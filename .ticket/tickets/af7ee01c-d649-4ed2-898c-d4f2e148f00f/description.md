# Problem

The current generation flow does not explain why a file contains the entries it does. That makes target construction difficult to review, debug, and evolve.

# Scope

Add preview and explain tooling for hierarchical target construction.

This should include:

- file-level outline preview
- node-by-node matched entry listing
- enough metadata to explain why an entry matched a node
- dry-run workflows for CLI and MCP consumers

# Acceptance Criteria

- A user can inspect the outline for a target before writing output.
- A user can see which entries matched each outline node.
- The tooling provides enough information to explain inclusion or exclusion decisions.
- The explain output is usable during both local CLI workflows and agent-driven MCP workflows.

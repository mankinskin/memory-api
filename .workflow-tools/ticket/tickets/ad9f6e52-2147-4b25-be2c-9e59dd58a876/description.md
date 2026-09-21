# Summary

Create `doc-cli` as the CLI interface for `doc-api`.

# Why

The desired workflow architecture relies on existing memory-system tools being the default surfaces for workflow metadata updates. `doc-api` already exists, but there is no dedicated `doc-cli` surface that can own documentation validation and inspection flows in the same style as `ticket-cli` and `spec-cli`.

# Scope

- add a `memory-api/tools/cli/doc-cli` crate
- expose the core `doc-api` operations needed for documentation inspection and workflow-driven validation
- keep the CLI thin over `doc-api`
- support the workflow metadata behaviors needed by the rewritten documentation validation spec
- document how the CLI fits into the default ticket/spec/doc workflow

# Acceptance criteria

- A `doc-cli` crate exists as the CLI interface for `doc-api`.
- The CLI exposes the first documentation inspection and validation commands needed by the workflow.
- Workflow metadata updates happen through the shared library behavior and the normal `doc-cli` surface, not through a separate workflow wrapper CLI.
- The CLI documentation explains how it participates in the default repository workflow.

# Summary

Add a first-class `log-api` for workflow validation log capture, indexing, and retrieval in the memory system.

# Why

Validation logs are part of review and debugging evidence. In the corrected architecture they should be addressable through native memory-system identities and shared-library integration, not only through external files or wrapper-owned artifacts.

# Scope

- add a `log-api` crate to the memory-system tool stack
- define the first log entities and retrieval model for validation and workflow-related logs
- support links from logs to tickets, specs, docs, and `test-api` executions
- support configuration-driven capture and lookup from the default tool surfaces
- define the minimal CLI/MCP/HTTP follow-up surfaces needed after the API exists

# Acceptance criteria

- A `log-api` crate exists for workflow and validation log capture/retrieval.
- The API provides native identifiers and queryable metadata for logs tied to workflow activity.
- The API supports first-class links to tickets, specs, docs, and `test-api` executions.
- The design integrates with the shared memory-system behavior instead of relying on a dedicated workflow wrapper CLI.

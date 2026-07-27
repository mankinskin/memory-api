## Problem

Follow-up from the review of `9faa3f5f` (closed done). That ticket unified `workspace` parameter semantics across five MCP servers using the canonical `InvalidWorkspaceSelector` error in [memory-api/src/workspace.rs](memory-api/src/workspace.rs), and updated schema docs on all five — but only added tests for two:

| Server | Docs updated | Test coverage |
|---|---|---|
| spec-mcp | yes | `spec_workspace_validation_error` (1 passing) |
| test-mcp | yes | `workspace_validation` (2 passing) |
| session-mcp | yes | none |
| rule-mcp | yes | none |
| feedback-mcp | yes | none |

Workspace-validation behavior in session-mcp, rule-mcp, and feedback-mcp is therefore unverified by test.

## Acceptance criteria

1. Each of session-mcp, rule-mcp, and feedback-mcp has a test asserting that an invalid workspace selector (omitted, empty, `default`, `.`, `..`) is rejected with the canonical `InvalidWorkspaceSelector` error and its recovery hint.
2. The rejected-selector set asserted is identical across all five servers.
3. `cargo test` passes for each of the three crates.

## Non-goals

- Changing workspace-resolution behavior.
- Touching spec-mcp or test-mcp, which already have coverage.

## Goal
Add a single-endpoint Rust tool for agents to audit repository quality and track findings in a synchronized local database.

## Initial scope
- File length findings with split-module guidance
- Compiler warning collection
- Unit test coverage collection
- Test success rate collection
- Static code quality metrics
- Local index synchronization with reliable stale-entry pruning
- Raw metrics plus human-readable remediation instructions

## Acceptance criteria
- A single MCP tool can analyze a repository root and return structured metrics and findings.
- The tool stores file metadata and analysis runs in a local SQLite index.
- Removed or renamed files are pruned from the index on a full sync.
- The audit report includes raw metric values and remediation text for each finding.
- Focused tests cover at least one trial and stale-index pruning behavior.
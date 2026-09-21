# Session Objective
Split remaining **ticket surfaces (HTTP/CLI/MCP)** file_length offenders while preserving external behavior.

# Scope
- memory-api/tools/http/ticket-http/tests/integration_parity.rs (898)
- memory-api/tools/http/ticket-http/src/serve/registry.rs (859)
- memory-api/tools/http/ticket-http/src/serve/routes.rs (778)
- memory-api/tools/cli/ticket-cli/src/cli/dispatch.rs (854)
- memory-api/tools/mcp/ticket-mcp/src/server/mutations.rs (836)

# Acceptance Criteria
- Route/handler/dispatch behavior remains unchanged.
- Extraction boundaries are by helper clusters/tests.
- Focused ticket-http, ticket-cli, and ticket-mcp checks pass.

Standardize error tracing and user-facing diagnostics across store CLIs, MCP servers, and HTTP handlers using one extended envelope contract.

Decision locked:
- target envelope includes `code`, `message`, `request_id`, `details`, `cause_chain`, `hint`, and `remediation_id`

Implementation plan:
1. Define envelope schema and mapping guidelines for memory-api and domain APIs.
2. Implement mapper helpers for CLI, MCP, and HTTP surfaces.
3. Add contextual metadata for scan/index/serialization failures (store, operation, path/id/table when available).
4. Align human output wording with actionable next steps while preserving machine-stable fields.

Rollout scope:
- rule-cli/spec-cli/ticket-cli error output
- rule-mcp/spec-mcp/ticket-mcp error translation
- ticket-http/spec-http/peer HTTP error adapters

Validation evidence:
- tests asserting stable machine fields across channels
- tests for serialization corruption messages with contextual hints
- sample logs proving request_id and cause chain visibility for multi-store failures

Acceptance criteria:
- corruption and reconciliation failures include contextual metadata and actionable hints
- machine outputs expose stable envelope fields for automation
- logs/traces preserve enough context for cross-store debugging in multi-workspace setups

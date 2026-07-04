# Problem

Reviewing a ticket currently requires chaining multiple read surfaces.

In this session, the MCP read path (`next_tickets`, `list_tickets`, `board_show`, `workflow fetch_ticket_context`) still did not provide one review-ready payload, and the CLI fallback required separate calls to `ticket get`, `ticket describe`, `ticket links`, and `ticket assets`.

That fragmentation makes ticket review and operator tooling slower than it should be, and it keeps pushing agents into ad hoc CLI orchestration instead of one stable detail contract.

# Session Evidence

- The session used MCP ticket discovery calls first, then still had to fall back to CLI detail calls to inspect acceptance text, links, and evidence.
- The operator explicitly noted that the deeper ticket-read calls were not available through the loaded MCP surface.
- One ticket review required at least four CLI reads after the initial MCP discovery pass.

# Scope

1. Add one consolidated ticket detail/context read surface in the CLI.
2. Return, in one machine-readable payload:
   - manifest fields
   - description body
   - dependency / linked edges
   - attached assets summary
   - validation fields / status
   - authoritative ticket path and root/workspace metadata
3. Add MCP parity for the same detail/context payload.
4. Keep the narrower subcommands (`get`, `describe`, `links`, `assets`) for specialized use, but document the consolidated surface as the default review workflow.
5. Reuse the root/workspace metadata contract from `07836f41` instead of inventing a parallel shape.

# Regression Validation Requirements

- **Specification / docs:** define the consolidated ticket-detail contract and how it relates to the existing narrow read commands.
- **CLI:** add integration coverage showing one command is sufficient to retrieve description, links, assets summary, validation fields, and path metadata for a seeded ticket.
- **MCP:** add parity coverage so the corresponding MCP tool returns the same fields and field names.
- **Frontends:** ticket-viewer / ticket-vscode should be able to consume the consolidated payload without recomputing path or link summaries client-side.
- **Manual validation:** repeat a review flow like the May 20 queue triage and confirm one read command/tool is enough to judge a ticket without extra shell spelunking.

# Acceptance Criteria

- One CLI command returns a review-ready ticket context payload.
- One MCP tool returns the same review-ready payload.
- The payload includes the ticket description, links, assets summary, validation fields, and authoritative path/root metadata.
- Existing narrow read commands remain available for specialized use.
- CLI and MCP contract tests prevent field drift between the two surfaces.

# Likely Surfaces

- `tools/ticket-cli/`
- `tools/ticket-mcp/`
- `crates/ticket-api/`
- `memory-viewers/ticket-viewer/`
- `memory-api/tools/ticket-vscode/`
- `memory-api/.spec/`

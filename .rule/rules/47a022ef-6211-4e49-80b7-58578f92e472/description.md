# Typed error envelopes

Every failure from a `memory-api` CLI, MCP tool, or HTTP handler is rendered as a typed JSON envelope. Agents and tooling can branch on `code` without parsing free-form text, and `request_id` lets operators correlate the failure with logs.
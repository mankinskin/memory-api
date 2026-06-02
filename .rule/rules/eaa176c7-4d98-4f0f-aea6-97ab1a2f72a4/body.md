# memory-api

memory-api is the repository that exposes the core crates and operator surfaces behind rules, specs, tickets, and audits.

Direct child READMEs:

- [tools/cli/rule-cli/README.md](tools/cli/rule-cli/README.md)
- [tools/cli/spec-cli/README.md](tools/cli/spec-cli/README.md)
- [tools/cli/ticket-cli/README.md](tools/cli/ticket-cli/README.md)
- [tools/cli/audit-cli/README.md](tools/cli/audit-cli/README.md)
- [tools/http/spec-http/README.md](tools/http/spec-http/README.md)
- [tools/http/ticket-http/README.md](tools/http/ticket-http/README.md)
- [tools/mcp/rule-mcp/README.md](tools/mcp/rule-mcp/README.md)
- [tools/mcp/spec-mcp/README.md](tools/mcp/spec-mcp/README.md)
- [tools/mcp/ticket-mcp/README.md](tools/mcp/ticket-mcp/README.md)
- [tools/mcp/audit-mcp/README.md](tools/mcp/audit-mcp/README.md)

Installable and executable surfaces in this repository include the `rule`, `spec`, `ticket`, and `audit` CLIs, the `rule-mcp`, `spec-mcp`, `ticket-mcp`, and `audit-mcp` MCP servers, and the `spec-http` and `ticket-http` HTTP binaries.

## Tool Surface

| Crate or surface | What it is used for | Typical entry points |
| --- | --- | --- |
| `memory-api` | Generic filesystem-backed entity storage, schema validation, SQLite indexing, search, edge management, and board coordination. | Reused by the other crates in this repo. |
| `rule-api` | Canonical rule manifests, markdown import, target composition, and nested workspace discovery. | `rule`, `rule-mcp` |
| `spec-api` | Specification manifests, sections, slugs, code references, and validation rules. | `spec`, `spec-mcp`, `spec-http` |
| `ticket-api` | Ticket domain logic, workspace state, board snapshots, execution contracts, watchers, and reconciliation. | `ticket`, `ticket-mcp`, `ticket-http`, `ticket-vscode` |
| `audit-api` | Repository quality audits, indexes, summaries, and review-oriented validation flows. | `audit`, `audit-mcp` |

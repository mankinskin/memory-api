# memory-api

memory-api is the repository that exposes the core crates and operator surfaces behind rules, specs, tickets, and audits.

## Tool Surface

| Crate or surface | What it is used for | Typical entry points |
| --- | --- | --- |
| `memory-api` | Generic filesystem-backed entity storage, schema validation, SQLite indexing, search, edge management, and board coordination. | Reused by the other crates in this repo. |
| `rule-api` | Canonical rule manifests, markdown import, target composition, and nested workspace discovery. | `rule`, `rule-mcp` |
| `spec-api` | Specification manifests, sections, slugs, code references, and validation rules. | `spec`, `spec-mcp`, `spec-http` |
| `ticket-api` | Ticket domain logic, workspace state, board snapshots, execution contracts, watchers, and reconciliation. | `ticket`, `ticket-mcp`, `ticket-http`, `ticket-vscode` |
| `audit-api` | Repository quality audits, indexes, summaries, and review-oriented validation flows. | `audit`, `audit-mcp` |

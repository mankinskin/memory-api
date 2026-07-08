<!-- aligned-structure:v1 -->

# Summary

`audit` is the repository quality audit tool for this workspace. Its code is split across three layers:

## Behavior Story

`audit` is the repository quality audit tool for this workspace. Its code is split across three layers:

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# audit

`audit` is the repository quality audit tool for this workspace. Its code is split across three layers:

- `audit-api` for audit logic, models, config loading, indexing, and quality trials
- `audit-cli` for the `audit` command and output rendering
- `audit-mcp` for the thin MCP transport exposing `audit` and `audit_summary`

Each run canonicalizes the repository root, loads `.audit.toml`, synchronizes source files into `.audit/audit.sqlite3`, prunes stale index rows, collects repository quality metrics, stores an audit run record, and returns actionable findings plus aggregated repair instructions.

The current quality trials cover:

- file length
- compiler warnings
- unit test success
- line coverage
- Rust static complexity metrics
- ticket dependency topology when a local `.ticket` store exists

Output is designed for both agents and humans:

- JSON from `audit` returns the full `AuditReport` contract for downstream automation.
- JSON from `audit summary --by ...` and `audit_summary` returns an `AuditSummaryReport` grouped by one requested key.
- Text output renders either the full audit report or the grouped summary as a compact terminal view.

The summary grouping keys are:

- `crate`
- `package` (alias of `crate`)
- `category`
- `severity`
- `metric`
- `path`

Paths in both surfaces are normalized to Unix format, including on Windows.

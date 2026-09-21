# Problem

`ticket next --filter` behaved inconsistently enough to break targeted discovery.

Observed behavior:

- `ticket next --filter "[doc-viewer]" --limit 20 --json` returned zero results.
- `ticket next --filter "doc-viewer" --limit 20 --json` also returned zero results.
- But `ticket next --limit 200 --json` contained a doc-viewer-related ticket at rank 83:
  `[LOG-1b] doc-viewer + spec-viewer: wire init_tracing_full with file logging`

This makes the filter impossible to trust for prefix- or substring-based ticket lookup. The observed behavior is consistent with an undocumented prefix-only implementation, which is a terrible contract for a user asking for the best next tickets for a codebase.

# Scope

1. Define and document the semantics of `next --filter`.
2. Make filtering behave consistently for bracketed prefixes, plain substrings, and mixed-prefix titles.
3. Add tests covering the kinds of titles used in this repository, including `[doc-viewer]`, `[viewer-api][ticket-viewer]`, and titles where the codebase name appears later in the string.
4. Ensure the filter works the same way across human and JSON output paths.
5. Ensure MCP and any frontend next-work filters follow the same matching rules.

# Regression Validation Requirements

- **Specification / docs:** document the filter contract, including whether matching is prefix, substring, case-insensitive, literal-bracket aware, or regex-free plain text.
- **CLI:** add integration tests for bracketed prefixes, plain substrings, mixed-prefix titles, and exact failures reproduced from the doc-viewer search.
- **MCP:** add parity tests so `next_tickets(filter=...)` returns the same candidates as the CLI.
- **Frontends:** if ticket-viewer or ticket-vscode expose next-work filtering, they must reuse the backend contract and test fixtures instead of inventing separate matching behavior.
- **Manual validation:** include a small matrix of real queries such as `doc-viewer`, `[doc-viewer]`, and `[viewer-api]` against the same seeded dataset.

# Acceptance Criteria

- If unfiltered `ticket next` contains a ticket whose title includes `doc-viewer`, then `ticket next --filter "doc-viewer"` returns it.
- If a ticket title contains a literal bracketed prefix like `[doc-viewer]`, then `ticket next --filter "[doc-viewer]"` returns it according to the documented semantics.
- `ticket next --help` documents the exact filter behavior.
- CLI and MCP regression tests cover literal brackets, case-insensitive substring matches, and multi-prefix titles.
- Manual validation checklist includes the doc-viewer filter cases that originally failed.

# Likely Surfaces

- `tools/ticket-cli/`
- `crates/ticket-api/`
- `tools/ticket-mcp/`
- `memory-api/.spec/`
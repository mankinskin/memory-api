# Problem

We now have several issue slices for board / next discovery, but no ticket owns the full contract for finding the best next ticket to implement.

Today the workflow is fragmented:

- the operator docs define `ticket next` at a high level, but not the full scope / filter / why-not / classification contract
- CLI coverage is narrow and mostly centered on board JSON behavior
- MCP has only partial cross-interface validation for the next-work path
- ticket-viewer and ticket-vscode do not yet expose the workflow as a first-class user surface
- manual validation is ad hoc rather than a reusable regression routine

Without a coordinated contract and validation matrix, each child fix can pass locally while the end-to-end workflow still misleads users about the best next work.

# Scope

1. Define the canonical next-workflow contract across specification, CLI, MCP, and frontends.
2. Coordinate the child tickets for scope, filter semantics, omission reasons, cross-root metadata, deferred / meta classification, and frontend surfaces.
3. Add a regression matrix spanning CLI integration tests, MCP cross-interface tests, ticket-viewer Playwright coverage, and ticket-vscode test coverage.
4. Add a manual validation checklist using the multi-root doc-viewer scenario that originally surfaced the failures.

# Child Tickets

- `68a08b34` — Scope-aware board and next for multi-root workspaces
- `68e3c713` — Fix `next --filter` matching for prefix and substring queries
- `61cbc31f` — Explain why tickets are absent from `next`
- `07836f41` — Make `get/search/list` workspace-aware across nested roots
- `86cde60c` — Distinguish deferred and meta work from actionable tickets
- frontend follow-up from this epic — surface the next-work workflow in ticket-viewer and ticket-vscode

# Acceptance Criteria

- Canonical spec / docs explain how a user finds the best next ticket, including scope selection, ranking, filter semantics, omission reasons, and deferred / meta handling.
- CLI, MCP, and frontend surfaces use the same contract and field names for the next-workflow.
- Automated regression coverage exists across CLI integration tests, MCP cross-interface tests, ticket-viewer release Playwright, and ticket-vscode test coverage.
- Manual validation checklist exists and is attached to this workflow or to a linked spec artifact.
- This parent ticket does not close until the child tickets above are complete and the regression routine passes.

# Likely Surfaces

- `memory-api/.spec/`
- `memory-api/tools/cli/ticket-cli/`
- `memory-api/tools/mcp/ticket-mcp/`
- `memory-viewers/ticket-viewer/`
- `memory-api/tools/ticket-vscode/`
- `.agents/instructions/ticket-system.instructions.md`
- `memory-api/README.md`
# Summary

Best-next-ticket discovery must remain consistent anywhere the repository surfaces candidate work.

The ordering contract applies to:

- `ticket next`
- `ticket board show` recommendations (`recommended_next` / `Next Up`)
- `ticket-mcp` `next_tickets`

No dedicated ticket-http `next` or board recommendation endpoint exists today, so HTTP is out of scope unless a future surface exposes the same workflow.

## Required behavior

### Ranking order

- Candidate tickets are ordered first by workflow progress using the schema state index, with tickets closest to terminal states ranked first.
- Ties on workflow progress are ordered by priority.
- Ties on workflow progress and priority are ordered chronologically by `created_at`, with newer tickets first.
- The last user-visible tiebreaker is alphabetical by title.
- Implementations may add one final deterministic fallback after title comparison to avoid unstable ordering for identical titles.

### Cross-interface consistency

- CLI and MCP must apply the same ordering contract for equivalent candidate sets.
- `ticket board show` recommendations must reuse the same candidate ordering as `ticket next` rather than drifting into a separate ranking scheme.
- `ticket board show` must expose at least 10 `Next Up` recommendations when at least 10 candidates exist.
- `ticket next` and `ticket-mcp` `next_tickets` must not embed a second board snapshot; `board show` remains the single board surface, while `next` surfaces only candidate data plus board-aware exclusions or warnings when relevant.
- Tool descriptions and user-facing contract text must describe the actual ordering keys.

### Compatibility

- Existing board-aware exclusion behavior stays unchanged.
- Board-aware warnings and `excluded_by_board` metadata may remain on `next` surfaces, but full board load/state belongs to `board show`.
- Existing state and priority ranking stay ahead of chronology and title ordering.

## Acceptance criteria

- `ticket next` returns newer tickets ahead of older tickets when state and priority are equal.
- `ticket board show` recommends newer tickets ahead of older tickets when state and priority are equal.
- `ticket board show` returns 10 `recommended_next` entries when at least 10 candidates are available.
- `ticket-mcp` `next_tickets` returns newer tickets ahead of older tickets when state and priority are equal.
- `ticket next` and `ticket-mcp` `next_tickets` do not return a top-level `board` snapshot field.
- When state, priority, and creation timestamp are equal, the ordering falls back to title order.

## Traceability

- Tracking ticket: `2df2a9e7-5755-43e3-b143-3b4d19c8a5e7`
- Updated interface contract text: `tools/mcp/ticket-mcp/src/server.rs`
- Updated MCP guidance: `.agents/instructions/mcp-tools.instructions.md`

## Validation

- `cargo test -p ticket-cli sort_candidates_ -- --nocapture`
- Focused integration tests in `tools/cli/ticket-cli/tests/integration_board_cli.rs` and `tools/mcp/ticket-mcp/tests/integration_board_mcp/cross_interface.rs` passed.
- `cargo test -p ticket-mcp sort_candidates_ -- --nocapture`

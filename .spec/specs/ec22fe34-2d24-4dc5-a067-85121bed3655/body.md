<!-- aligned-structure:v1 -->

# Summary

Best-next-ticket discovery must remain consistent anywhere the repository surfaces candidate work.

## Behavior Story

Best-next-ticket discovery must remain consistent anywhere the repository surfaces candidate work.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

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
- Ties on workflow progress and priority are ordered by `dependee_count`, a derived count of incoming `depends_on` edges whose target is the candidate ticket. Higher `dependee_count` values rank first.
- Ties on workflow progress, priority, and `dependee_count` are ordered chronologically by `created_at`, with newer tickets first.
- The last user-visible tiebreaker is alphabetical by title.
- Implementations may add one final deterministic fallback after title comparison to avoid unstable ordering for identical titles.

### Cross-interface consistency

- CLI and MCP must apply the same ordering contract for equivalent candidate sets.
- `ticket board show` recommendations must reuse the same candidate ordering as `ticket next` rather than drifting into a separate ranking scheme.
- `ticket board show` must expose at least 10 `Next Up` recommendations when at least 10 candidates exist.
- `ticket next` and `ticket-mcp` `next_tickets` must not embed a second board snapshot; `board show` remains the single board surface, while `next` surfaces only candidate data plus board-aware exclusions or warnings when relevant.
- `ticket next`, `ticket board show` recommendation JSON, and `ticket-mcp` `next_tickets` items must surface the derived numeric `dependee_count` field.
- The CLI `ticket board show` human `Next Up` section must render each recommendation as a compact labeled card that keeps all surfaced recommendation keys while prioritizing rank, short ticket id, and title.
- The CLI `ticket board show` `Immediate Actions` section must describe the suggested ticket as `Start <state> <short-id> "<title>" next.`, preserving the ticket state and escaping any inner title quotes.
- Default non-JSON `ticket next` output must render its candidate `items` using the same compact labeled card format as `ticket board show` `Next Up`, while preserving next-specific metadata like `count`, `warnings`, and `excluded_by_board`.
- Tool descriptions and user-facing contract text must describe the actual ordering keys.

### Compatibility

- Existing board-aware exclusion behavior stays unchanged.
- Board-aware warnings and `excluded_by_board` metadata may remain on `next` surfaces, but full board load/state belongs to `board show`.
- Human-readable `ticket board show` output must stop after the board-specific dashboard instead of appending a second raw structured dump; `--json` remains the full machine-readable payload surface.
- Existing state and priority ranking stay ahead of `dependee_count`, chronology, and title ordering.

## Acceptance criteria

- `ticket next` returns higher-`dependee_count` tickets ahead of lower-`dependee_count` tickets when state and priority are equal, even when the lower-`dependee_count` ticket is newer.
- `ticket board show` recommends higher-`dependee_count` tickets ahead of lower-`dependee_count` tickets when state and priority are equal, even when the lower-`dependee_count` ticket is newer.
- `ticket board show` returns 10 `recommended_next` entries when at least 10 candidates are available.
- `ticket-mcp` `next_tickets` returns higher-`dependee_count` tickets ahead of lower-`dependee_count` tickets when state and priority are equal, even when the lower-`dependee_count` ticket is newer.
- `ticket next`, `ticket board show`, and `ticket-mcp` `next_tickets` surface the numeric `dependee_count` field for each recommended item.
- `ticket board show` recommendation JSON preserves `created_at`, and the CLI `Next Up` cards print all recommendation keys while formatting `created_at` as a compact human timestamp that includes the year.
- `ticket board show` human `Immediate Actions` text includes the candidate's current state, short ticket id, and a quoted title with inner quotes escaped.
- Default non-JSON `ticket board show` output does not append the generic structured `[recommended_next]` dump after the board-specific human renderer.
- Default non-JSON `ticket next` output does not fall back to the generic `[items]` object dump; it uses the same compact recommendation cards as `ticket board show`.
- `ticket next` and `ticket-mcp` `next_tickets` do not return a top-level `board` snapshot field.
- When state, priority, `dependee_count`, and creation timestamp are equal, the ordering falls back to title order.

## Traceability

- Tracking ticket: [f4f5da07 Improve board immediate action wording](../../../../../.ticket/tickets/f4f5da07-8889-42ed-b32d-8638e811be76/ticket.toml)
- Tracking ticket: `.ticket/tickets/2d85467b-23a3-4a70-a376-70ef5370d9f8`
- Tracking ticket: `.ticket/tickets/77629631-8076-4fca-9640-316583ff290c`
- Tracking ticket: `.ticket/tickets/11450369-0d45-4922-988f-49bc88fd4079`
- Updated interface contract text: `tools/mcp/ticket-mcp/src/server.rs`
- Updated CLI contract text: `tools/cli/ticket-cli/src/cli.rs`
- Updated CLI human-output serializer: `tools/cli/ticket-cli/src/cli/human_output.rs`
- Updated CLI next/board recommendation bridge: `tools/cli/ticket-cli/src/cli/commands/board.rs`
- Updated CLI board renderer: `tools/cli/ticket-cli/src/cli/commands/board/render.rs`

## Validation

- `cargo test -p ticket-cli board_show_immediate_actions_include_state_and_escaped_title -- --nocapture`
- `cargo test -p ticket-cli dependees -- --nocapture`
- `cargo test -p ticket-mcp dependees -- --nocapture`
- `cargo test -p ticket-cli next_and_board_prefer_more_dependees_before_newer_tickets -- --nocapture`
- `cargo test -p ticket-cli board_show_lists_ten_recommendations_when_available -- --nocapture`
- `cargo test -p ticket-cli board_show_text_output_stops_after_dashboard -- --nocapture`
- `cargo test -p ticket-cli next_text_output_uses_pretty_card_format -- --nocapture`
- `cargo run --quiet --manifest-path tools/cli/ticket-cli/Cargo.toml -- next --limit 3`
- `spec refs ec22fe34 validate --json --workspace-root .`

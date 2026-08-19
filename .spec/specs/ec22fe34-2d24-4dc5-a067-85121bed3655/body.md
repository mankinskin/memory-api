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
- `ticket-mcp` `next_tickets`

`ticket board show` (CLI) and `ticket-mcp` `board_show` (MCP) are board-state-only surfaces: they report active/stale/conflict entries, file ownership, and warnings, and must never embed ticket recommendations (`recommended_next` / `Next Up`) or duplicate the `next` ordering. `next` / `next_tickets` are the sole recommendation surfaces on both interfaces, keeping CLI and MCP `board show` behavior identical.

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
- `ticket board show` (CLI) and `ticket-mcp` `board_show` (MCP) must not surface ticket recommendations at all; recommendation discovery lives exclusively in `ticket next` / `ticket-mcp` `next_tickets`, so both interfaces' `board show` behave identically for the same board state.
- `ticket next` and `ticket-mcp` `next_tickets` must not embed a second board snapshot; `board show` remains the single board surface, while `next` surfaces only candidate data plus board-aware exclusions or warnings when relevant.
- `ticket next` and `ticket-mcp` `next_tickets` items must surface the derived numeric `dependee_count` field.
- Default non-JSON `ticket next` output must render its candidate `items` using a compact labeled card format that prioritizes rank, short ticket id, and title, while preserving next-specific metadata like `count`, `warnings`, and `excluded_by_board`.
- The CLI `ticket board show` `Immediate Actions` section must not describe or name a specific ticket to start; when the board is clear it directs the caller to run `ticket next` to see unblocked tickets instead.
- Tool descriptions and user-facing contract text must describe the actual ordering keys.

### Compatibility

- Existing board-aware exclusion behavior stays unchanged.
- Board-aware warnings and `excluded_by_board` metadata may remain on `next` surfaces, but full board load/state belongs to `board show`.
- `ticket board show` output (JSON and human) never includes `recommended_next`, `Next Up`, or any per-ticket recommendation payload.
- Existing state and priority ranking stay ahead of `dependee_count`, chronology, and title ordering.

## Acceptance criteria

- `ticket next` returns higher-`dependee_count` tickets ahead of lower-`dependee_count` tickets when state and priority are equal, even when the lower-`dependee_count` ticket is newer.
- `ticket-mcp` `next_tickets` returns higher-`dependee_count` tickets ahead of lower-`dependee_count` tickets when state and priority are equal, even when the lower-`dependee_count` ticket is newer.
- `ticket next` and `ticket-mcp` `next_tickets` surface the numeric `dependee_count` field for each recommended item.
- Default non-JSON `ticket next` output does not fall back to the generic `[items]` object dump; it uses compact recommendation cards.
- `ticket next` and `ticket-mcp` `next_tickets` do not return a top-level `board` snapshot field.
- `ticket board show` (CLI, JSON and human) and `ticket-mcp` `board_show` never include `recommended_next`, `Next Up`, or any ticket-recommendation payload; both report only board/WIP state (active/stale/conflict counts, entries, file ownership, warnings).
- When the board is clear, the CLI `ticket board show` `Immediate Actions` text directs the caller to run `ticket next` rather than naming a specific recommended ticket.
- When state, priority, `dependee_count`, and creation timestamp are equal, the ordering falls back to title order.

## Traceability

- Tracking ticket: [f4f5da07 Improve board immediate action wording](../../../../../.ticket/tickets/f4f5da07-8889-42ed-b32d-8638e811be76/ticket.toml)
- Tracking ticket: `.ticket/tickets/2d85467b-23a3-4a70-a376-70ef5370d9f8`
- Tracking ticket: `.ticket/tickets/77629631-8076-4fca-9640-316583ff290c`
- Tracking ticket: `.ticket/tickets/11450369-0d45-4922-988f-49bc88fd4079`
- Tracking ticket: [b83d2e14 Remove recommended_next/Next Up from ticket board show to match ticket-mcp board_show parity](../../../../../.ticket/tickets/b83d2e14-98cb-464a-84c4-389c14e61080/ticket.toml)
- Updated interface contract text: `tools/mcp/ticket-mcp/src/server.rs`
- Updated CLI contract text: `tools/cli/ticket-cli/src/cli.rs`
- Updated CLI human-output serializer: `workflow-tools/ticket/src/cli/human_output.rs`
- Updated CLI next/board recommendation bridge: `workflow-tools/ticket/src/cli/commands/board.rs`
- Updated CLI board renderer: `workflow-tools/ticket/src/cli/commands/board/render.rs`

## Validation

- `cargo test -p ticket --features cli --test integration_board_cli`
- `cargo test -p ticket --features mcp --test integration_board_mcp`
- `cargo test -p ticket-mcp dependees -- --nocapture`
- `cargo test -p ticket-cli next_and_board_prefer_more_dependees_before_newer_tickets -- --nocapture`
- `cargo test -p ticket-cli board_show_lists_ten_recommendations_when_available -- --nocapture`
- `cargo test -p ticket-cli board_show_text_output_stops_after_dashboard -- --nocapture`
- `cargo test -p ticket-cli next_text_output_uses_pretty_card_format -- --nocapture`
- `cargo run --quiet --manifest-path tools/cli/ticket-cli/Cargo.toml -- next --limit 3`
- `spec refs ec22fe34 validate --json --workspace-root .`

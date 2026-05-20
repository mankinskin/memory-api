Render recommendation lists with the same compact pretty-card layout anywhere the CLI shows human-readable next-work candidates.

Acceptance criteria:
- default `ticket board show` text output stops after the board-specific human renderer
- `ticket board show` `Next Up` renders compact pretty cards while preserving all recommendation keys
- default `ticket next` text output renders its candidate list using that same pretty-card format
- human `created_at` uses a compact pretty timestamp
- `ticket next` keeps its next-specific metadata and JSON output remains unchanged

Implementation summary:
- exposed the existing `board show` recommendation card writer for reuse outside the board dashboard path
- taught `human_output.rs` to special-case `command == "next"` and render `items` with the shared `Next Up` cards instead of the generic `[items]` object dump
- added a focused CLI regression for default non-JSON `ticket next` output and initialized the text-output sandboxes explicitly with `ticket init`

Validation:
- `cargo test -p ticket-cli board_show_text_output_stops_after_dashboard -- --nocapture`
- `cargo test -p ticket-cli next_text_output_uses_pretty_card_format -- --nocapture`
- `cargo run --quiet --manifest-path tools/cli/ticket-cli/Cargo.toml -- next --limit 3`
- updated `.spec/specs/ec22fe34-2d24-4dc5-a067-85121bed3655/body.md` for the `ticket next` human-output contract and validation evidence
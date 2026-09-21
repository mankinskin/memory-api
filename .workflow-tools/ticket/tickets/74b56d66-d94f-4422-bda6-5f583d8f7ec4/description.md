## Problem

Ticket 44119807 (T2, output-size capture) claims AC1 satisfied via the e2e test `e2e_hook_binary_captures_output_chars_from_hook_stdin_tool_response`, but no genuinely live-captured session has ever shown a non-empty `output_char_sizes`. Verified during review by reading `.session/sessions/918886cc-3f7d-4ced-b2a2-10e72e68abb4/tool-metrics.json` (captured 2026-07-30T11:53:46Z, this very review session) — every tool shows `output_char_sizes: []` — and by grepping all 201 session `tool-metrics.json` files under `.session/sessions/**`, none of which contain a non-empty `output_char_sizes` array.

## Acceptance criteria

- AC1 — At least one real (non-replayed, non-e2e-fixture) captured session under `.session/sessions/<id>/tool-metrics.json` shows a tool with non-empty `output_char_sizes` and `output_source: "hook_payload"` (once AC2's persistence gap in 44119807 is fixed), verified by reading the file directly. Cite the session id and value.
- AC2 — If, after a reasonable capture window, no live session shows this, investigate why the PostToolUse hook-payload layer is not activating in production (hook registration, stdin field names, or binary wiring) and record the root cause.

## Notes

Depends on the parent ticket's AC2 rework (output_source discriminant must reach the persisted `ToolCallSummary` before this can even be checked meaningfully).
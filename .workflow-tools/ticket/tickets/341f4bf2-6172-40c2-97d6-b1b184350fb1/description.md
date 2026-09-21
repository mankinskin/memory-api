## Problem

For months, `.session/sessions/<id>/tool-metrics.json` was `{"tools":{}}` in **every** session while ten sibling tickets shipped `done`. Root cause (diagnosed 2026-07-29, fixed in memory-api commit `7df14ea`): `compute_session_summary` read only transcript turns with `role == SessionRole::Tool`, a shape the Copilot transcript producer never emits. All tool telemetry lives in captured events (`tool.execution_start` / `tool.execution_complete`).

That fix restores call counts, outcome classification, durations and input sizes across 203 sessions. **It does not restore output sizes.** The raw `tool.execution_complete` payload carries only `{success, toolCallId}`, so `output_char_sizes` is still empty everywhere — and `est-output-tokens` is precisely what the graded cost scale in spec 29ae5f6e is anchored on. The empirical dataset the whole cost-gate design consumes has therefore never existed.

## Why this epic exists rather than more layer tickets

Post-mortem (review + roast + session forensics, 2026-07-29) found the track was decomposed by **architectural layer** (core / surfaces / rollup writer / CLI parity / MCP surface / graded cost / grants / benchmark harness) rather than by observable outcome. Every layer was independently `done` while the composed system produced zero data. Zero tests ran against a real or producer-shaped transcript before `7df14ea`. This epic is decomposed by **outcome**, and every child ticket carries an acceptance criterion of the form "on real data, X observably happens" — never "X is implemented".

## Objective

One real tool call produces one real, non-zero, attributable cost number that the gate budgets on.

## Acceptance criteria

- AC1 — A real captured session yields `tool-metrics.json` with non-empty `output_char_sizes` for at least one tool, verified by **reading the file**, not by citing code or test counts.
- AC2 — The rollup reports `measured_call_fraction`; that fraction is > 0.8 over the active window on real data.
- AC3 — `graded_cost` calibration (`tokens_at_max`) is derived from the real rollup, replacing the provisional `8000.0` anchor.
- AC4 — A validation spec asserting AC1 exists and is recorded in `.test`, and fails if the pipeline regresses.

## Non-goals

- Re-tuning model budgets beyond replacing the provisional anchor.
- Argument-based dynamic estimation (9c9e2edc) — that remains downstream of real calibration data.

## Decisions (user, 2026-07-29)

- Output size is captured from layered sources, probe first (see child T1/T2).
- Fail-open is retained; a coverage metric is added so unmeasured is no longer indistinguishable from healthy.
- 84c7757d is reopened; stale duplicates are wired as dependants rather than re-ticketed.
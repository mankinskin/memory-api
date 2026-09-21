## Status: returned from 3rd review (FAIL) — user decisions require further rework

Review verdict (3rd pass, 2026-07-30): AC1 conditional pass (independently confirmed real evidence, but coverage measured at only ~24%, 7/29 real calls), AC2 pass, AC3 pass, AC4 pass — but overall FAIL because the user's decisions on the open judgement calls require rework before this can close.

### User decisions (confirmed via interview 2026-07-30, do not re-litigate)
1. **Retry-loop latency**: the current ~2.4s worst-case synchronous retry (in `SessionStoreConfig::capture_copilot_transcript_with_tool_response`) is **not acceptable**. Required: move the retry off the hook's synchronous critical path entirely — async/non-blocking, not just a smaller bound.
2. **AC1 real coverage (~24%, 7/29)**: **not acceptable** as final state. Requires further investigation into why most qualifying real calls in the post-fix window still didn't resolve within the retry budget, and/or a different mitigation with materially higher catch rate.
3. **Cross-session rollup gap**: `output_source` is captured per-session (`ToolCallSummary`) but never threaded through the cross-session `ToolAggregation` rollup (`session tool-metrics` CLI path in `tool_metrics.rs`). User decided: **fold this into 44119807 now**, do not defer to sibling ticket T3 (eb984596).
4. **Follow-up ticket 74b56d66-d94f-4422-bda6-5f583d8f7ec4**: user decided to **close it** (satisfied by the review's independent AC1 verification) — attempted but blocked: ticket-store enforces `depends_on` progression, so 74b56d66 cannot move to `done` while 44119807 is still `in-review`/`in-implementation`. Close it as the very first action once 44119807 reaches `done`.

### Objective for next implementation pass
Redesign the real-output-size capture path so it does not add synchronous latency to the hook's critical path, while achieving materially higher than ~24% real-session coverage. Two established root causes remain from prior passes and are NOT to be re-investigated (already confirmed via live diagnostic evidence, see history.ndjson for full detail):
- VS Code fires PostToolUse before flushing the triggering call's own transcript completion entry (empirically: 750ms insufficient, 5s usually sufficient, but not always — hence the low catch rate even at 2.4s).
- `merge_events`'s `captured_event_key` dedup + `record_event_tool_call`'s per-`tool_call_id` dedup mean an override must land on the FIRST persist of an event or it is permanently lost — there is no "catch up later" path today.

Design options to evaluate for the async rework (not prescriptive, next implementer's call, but flag ambiguity back to interview if genuinely undecidable):
- Defer the output-size backfill to a later event (e.g. `SessionEnd`/`Stop` hook, which fires well after the transcript has caught up) instead of trying to resolve it synchronously inside the triggering `PostToolUse` invocation.
- Or: persist a lightweight "pending override" record and reconcile it opportunistically on a LATER hook invocation (fixing the dedup-drops-second-copy bug in `record_event_tool_call`/`merge_events` so a late-arriving override can still land).

### Target files (from this pass, still uncommitted on disk, do not revert)
- memory-api/crates/session-api/src/bin/copilot-capture-hook.rs (`build_tool_response_override`, `stat_spill_output_chars` — bare/suffix id split and spill-path convention are correct, keep)
- memory-api/crates/session-api/src/hook/transcript.rs (`ToolResponseOverride.output_source` field — correct, keep)
- memory-api/crates/session-api/src/store/config/capture_query.rs (the synchronous retry loop — THIS is what must be replaced with an async/deferred design)
- memory-api/crates/session-api/src/tool_metrics.rs (`output_source` on `ToolCallSummary` — correct, keep; still needs threading into `ToolAggregation`/rollup per decision 3)
- memory-api/crates/session-api/tests/copilot_capture_hook_e2e.rs (8 tests, all green — keep, extend for the new async/rollup behavior)

### Validation required before next review
- `cargo build -p session-api`, `cargo test -p session-api` (184 lib + 8/8 e2e, currently green — must remain green).
- New test(s) covering `output_source` reaching the `ToolAggregation`/`session tool-metrics` rollup output.
- Fresh live-session evidence (read back from a real `.session/sessions/<id>/events.json`) showing the new mechanism's real catch rate is materially above ~24%, with the actual measured rate cited.
- Confirm the hook's own synchronous latency is no longer bounded by the transcript-catch-up wait (i.e., PostToolUse returns promptly regardless of override outcome).

### Non-goals (unchanged)
- MCP proxy telemetry capture layer (deferred per original T2 design).

Progress update:

- DONE: first-wins `tool_call_id` dedup in memory-api/crates/session-api/src/tool_metrics.rs replaced with rank-aware upgrade (hook_payload > spill_file > transcript_turn > unspecified), so a late-arriving richer tool-output copy now lands instead of being dropped.
- DONE: blocking retry loop (MAX_ATTEMPTS=12 x 200ms) removed from `capture_copilot_transcript_with_tool_response` in memory-api/crates/session-api/src/store/config/capture_query.rs; sync path is single-parse/persist, late override reconciles on the next hook invocation.
- DONE: validation green — `cargo test -p session-api` 228 passed / 1 ignored; capture+stop e2e 12 passed. New tests: `late_arriving_richer_output_copy_upgrades_the_dropped_record`, `poorer_or_duplicate_late_copy_never_downgrades_or_double_counts`, `capture_copilot_transcript_with_tool_response_never_blocks_on_missing_override_match`.
- REMAINING: thread `output_source` through `ToolAggregation` into the cross-session rollup (`ToolTokenStats` / `ToolMetricsReport`) — in progress.
- REMAINING: fresh live-session `output_char_sizes` coverage evidence, materially above the ~24% (7/29) measured in the failed 3rd-pass review. Spec 97f25cf8-f869-48d8-a48d-49d02badd188 proposes a >=90% threshold, PENDING USER CONFIRMATION.

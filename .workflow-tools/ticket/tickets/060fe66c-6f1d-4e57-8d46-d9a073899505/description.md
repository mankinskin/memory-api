## Problem
There is currently no way to replace the running child process in place. A future watcher (T6) needs a supervisor that can kill/respawn the child, fail in-flight requests safely, and never let a bad respawn kill the whole proxy. Part of epic 25780944; depends on T3 (shadow-copy exec, since respawn re-copies from P to a fresh shadow path).

## Approach
Introduce a supervisor component owning the child process handle and in-flight request table (request id -> pending response channel). Expose `swap_child()`: quiesce new requests, wait for in-flight to either complete or be synthesized as errors after a bounded grace period, kill the current child, re-copy P to a new shadow path, respawn, resume routing. On respawn failure, do NOT propagate a process exit — retry with exponential backoff, and fall back to serving from the last-known-good shadow copy while retries continue.

## Acceptance criteria
- [ ] Supervisor tracks in-flight JSON-RPC requests by id
- [ ] `swap_child()` kills the current child and respawns from a freshly re-copied shadow binary
- [ ] Requests in flight when the child dies receive synthesized JSON-RPC error responses (proper `error` object, correlated `id`) instead of hanging
- [ ] On respawn failure, the proxy process does not exit; it retries with backoff (bounded max interval) and continues serving errors for new requests during the outage
- [ ] Last-known-good shadow copy is retained and used as fallback if respawn of the new binary fails validation (e.g. process exits immediately)
- [ ] Unit/integration test simulating a fake child binary crash mid-request confirms synthesized error, no proxy exit, and successful recovery on next swap
- [ ] No public MCP tool exposed for triggering swap (that remains internal, per rejected-design constraint; trigger comes from T6's watcher)

## Files touched
- memory-api/tools/mcp/mcp-toolmon/src/supervisor.rs (new)
- memory-api/tools/mcp/mcp-toolmon/src/main.rs
- memory-api/tools/mcp/mcp-toolmon/src/shadow.rs
- memory-api/tools/mcp/mcp-toolmon/tests/ (supervisor swap/failure tests)


## Validation acceptance criteria (addendum)
- [ ] Unit test `swap_child_replaces_running_child`: using the fake-mcp-v1/v2 fixture (see spec Validation Strategy), directly `.await`s `swap_child()`, then asserts a `generation` tool call returns `"v2"` — no reliance on the real poller/wall-clock
- [ ] Unit test `inflight_request_synthesized_error_on_kill`: a request is issued to a fake child rigged to hang; `swap_child()` is invoked mid-flight; the test asserts the client receives a JSON-RPC response with an `error` object and the same `id` as the original request, within the drain window
- [ ] Unit test `respawn_backoff_no_process_exit`: swap is pointed at a corrupt/non-executable binary; test asserts the proxy process/task does not exit or panic, retries are observed (e.g. via a counter or log hook), and the proxy continues answering `tools/call` using the last-known-good (v1) shadow copy
- [ ] Unit test confirms no new tool entry appears in a `tools/list` response before/after a swap (grep the response tool name list)
- [ ] `cargo test -p mcp-toolmon` includes all three named supervisor tests, all passing, with zero added wall-clock sleeps as the assertion mechanism
## Completion note (T4)

Implemented `Supervisor::swap_child()` / `swap_child_with_drain_ms()` in `memory-api/tools/mcp/mcp-toolmon/src/supervisor.rs`.

- Atomicity: replaced the three separate `child`/`stdin`/`stdout` mutexes with one `RwLock<Option<Arc<ChildHandles>>>` holding a single bundled `ChildHandles` struct per generation. Every `read_line`/`write_line` snapshots the `Arc` under one lock acquisition, so stdin/stdout/child can never be observed from mismatched generations.
- Pending-request tracking: `record_pending`/`resolve_pending`/`synthesize_and_clear`, wired into `main.rs`'s client and reader loops. Kill drains pending ids for up to `TOOLMON_DRAIN_MS` (default 2000, env-driven; tests use `swap_child_with_drain_ms` to avoid env races), then synthesizes `{"error":{"code":-32001,...}}` JSON-RPC responses for any left over.
- Never-exit fallback: retry with capped exponential backoff (25ms->500ms, 4 attempts), then falls back to a durably-snapshotted last-known-good shadow copy (fixed a real bug found during implementation: `shadow::make_shadow_copy` keys its dest path only by canonical-path hash, so a naive re-copy-and-retry would have overwritten the last-known-good file with corrupt bytes before it could be used -- added `snapshot_last_known_good` to guard against this).
- `main.rs`: child death/respawn failure no longer calls `process::exit`; only client stdin EOF still shuts the proxy down. Added best-effort shadow-directory cleanup on graceful shutdown.
- Tests (`tests/supervisor.rs`, 5 new): `swap_child_replaces_running_child`, `inflight_request_synthesized_error_on_kill`, `respawn_backoff_no_process_exit`, `no_swap_tool_appears_in_tools_list`, `concurrent_swap_produces_no_corrupted_responses` (proves absence of observable corruption under concurrent swap; the internal invariant itself is structural, not independently provable by an external test).

Validation: `cargo build -p mcp-toolmon` clean; `cargo test -p mcp-toolmon` 64/64 passed (59 prior + 5 new), zero regressions. Smoke: `initialize` piped through `mcp-toolmon -- peek-mcp` returned a real `serverInfo`/`capabilities` result payload.

Concern for T6: the reader pump's `read_line` only returns `None` (ending the pump) on supervisor shutdown or a live child's real stdout EOF; an unexpected crash outside of an explicit `swap_child()` call currently stops relaying until the next swap rather than auto-triggering one -- T6's watcher is expected to detect that and call `swap_child()`.


## Verification note (2026-07-31)
cargo test -p mcp-toolmon -p toolmon-policy-api -p toolmon-costgate -> 76/76 passed, run twice, 0 flakes. Property A (proxied target-server functionality not compromised) and Property B (child hot-restart) both PROVEN. Full evidence and known limitations recorded on epic 25780944.
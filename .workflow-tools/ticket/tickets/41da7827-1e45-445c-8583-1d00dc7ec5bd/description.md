## Problem
When a child is respawned (T4), it needs a fresh MCP `initialize` handshake to function, but the client already completed its own handshake and must not see a second one. Part of epic 25780944; depends on T4 (supervisor owns the swap point where replay is injected).

## Approach
Cache the client's original `initialize` request params verbatim, plus the `notifications/initialized` notification, the first time they are observed. On every respawn (triggered by supervisor `swap_child()`), replay both into the new child before resuming normal routing. Suppress forwarding the new child's `initialize` response to the client (client already has its own). Compare the new child's reported capabilities against the cached original; log any divergence (do not fail the swap on divergence alone).

## Acceptance criteria
- [ ] Original client `initialize` params and `notifications/initialized` are cached on first observation
- [ ] On every respawn, cached `initialize` is sent to the new child and `notifications/initialized` follows, before any queued/new tool calls are routed to it
- [ ] The new child's `initialize` response is never forwarded to the client
- [ ] Capability divergence (new child's declared capabilities differ from the original handshake) is logged with enough detail to diagnose, but does not abort the swap
- [ ] Unit/integration test: after a simulated swap, the client sees no second `initialize` response and the new child receives correct `initialize`/`initialized` before any tool call
- [ ] Unit test: capability mismatch produces a log entry (or captured warning) without failing the swap

## Files touched
- memory-api/tools/mcp/mcp-toolmon/src/supervisor.rs
- memory-api/tools/mcp/mcp-toolmon/src/handshake.rs (new)
- memory-api/tools/mcp/mcp-toolmon/src/proxy.rs
- memory-api/tools/mcp/mcp-toolmon/tests/ (handshake replay tests)


## Validation acceptance criteria (addendum)
- [ ] Unit test `handshake_replayed_before_tool_calls`: after a direct `swap_child()` call, the fake-mcp-v2 process's captured stdin transcript shows `initialize` then `notifications/initialized` before any `tools/call` frame
- [ ] Unit test `handshake_response_never_forwarded`: the client-facing transcript across a swap contains exactly one `initialize` response (the original), never a second one from the new child
- [ ] Unit test `capability_divergence_logged_not_fatal`: new child's `initialize` response declares different capabilities than the cached original; test asserts a log/warning record is produced AND the swap still completes successfully (post-swap tool call succeeds)
- [ ] `cargo test -p mcp-toolmon` includes all three named handshake tests, all passing
## T5 completion note

Implemented handshake replay cache in `Supervisor` (memory-api/tools/mcp/mcp-toolmon/src/supervisor.rs), no separate handshake.rs module needed — the cache is a small private `HandshakeCache` struct + methods on `Supervisor` itself, since `write_line`/`read_line` are the natural interception points already used by both main.rs and tests.

- Cache populated verbatim on first observation inside `Supervisor::write_line` (initialize request + id) and `notifications/initialized` (also via write_line).
- Baseline `initialize` response captured inside `Supervisor::read_line` by id-correlation.
- Replay (`replay_handshake`) runs on the new `ChildHandles` BEFORE it is installed into `self.current`, in both the normal respawn path and the last-known-good fallback path in `swap_child_with_drain_ms`. This structurally guarantees ordering (no other caller can reach the new child pre-handshake) and suppression (the reader pump only ever reads via `self.current`, so it cannot observe the replayed response).
- Capability divergence (protocolVersion/capabilities/serverInfo) logged to stderr and also recorded in a `divergence_log` field for test assertion; never aborts the swap.
- No-cache case (swap before any handshake observed) is a clean no-op.

Fixtures extended: fake-mcp-v1/v2 now reject `tools/call` received before `initialize` with a JSON-RPC error, which is what makes the ordering test honest. Still byte-different (only the ordering guard + comment header changed, GENERATION/serverInfo.name differ as before).

New tests: `tests/handshake.rs` (`handshake_replayed_before_tool_calls`, `handshake_response_never_forwarded`, `capability_divergence_logged_not_fatal`), all passing. 4 pre-existing T4 tests in `tests/supervisor.rs` needed a `perform_handshake()` helper call added before their first `tools/call` (they previously called `generation` with no prior `initialize`, which the new fixture guard now rejects) — behavior/assertions unchanged, only test setup added.

`cargo build -p mcp-toolmon`: clean. `cargo test -p mcp-toolmon`: 67 passed (64 baseline + 3 new), 0 failed. Smoke test: `initialize` piped through `mcp-toolmon -- peek-mcp` returned a real result payload (protocolVersion, capabilities, serverInfo, instructions).

Remaining risk: divergence logging is not tested against a plain-stderr capture (no `gag`-style crate in the dep tree); it is proven via an in-process `divergence_log` field mirroring the `eprintln!`, which is honest but does not prove the literal stderr text shape byte-for-byte.


## Verification note (2026-07-31)
cargo test -p mcp-toolmon -p toolmon-policy-api -p toolmon-costgate -> 76/76 passed, run twice, 0 flakes. Property A (proxied target-server functionality not compromised) and Property B (child hot-restart) both PROVEN. Full evidence and known limitations recorded on epic 25780944.
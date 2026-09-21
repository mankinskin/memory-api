## Problem Statement

A bug fix landed in `memory-api/crates/ticket-api/src/storage/store.rs` and all 207 ticket-api tests passed, but every ticket-mcp caller kept receiving the OLD behavior: the MCP server runs a prebuilt binary from `~/.cargo/bin/ticket-mcp.exe` built the previous day. The stale server returned a domain-shaped error that was indistinguishable from real policy, causing incorrect conclusions about system behavior. Reinstalling then FAILED with `Zugriff verweigert (os error 5)` because the running child held a Windows file lock on the binary it was executing. The failure mode is twofold: staleness is invisible, and the obvious remedy (reinstall) is blocked by the very process that is stale.

## Goals

- Detect when a proxied child binary on disk changes and transparently respawn the child running the new binary, with zero required client action.
- Make the crate a general-purpose MCP proxy (`mcp-toolmon`) with cost gating as one pluggable `Policy` implementation, not hardwired behavior.
- Make `cargo install --force` for a child binary always succeed, even while a proxy is currently running that child, by never holding a lock on the canonical installed path.
- Preserve full backward compatibility for all 12 servers currently wrapped by `mcp-cost-gate` in `.vscode/mcp.json` and `.github/mcp.json`.

## Resolved Decisions

### Decision 1: tokio async runtime for T2

The proxy core should be rewritten on tokio, using `tokio::process::Command` and async stdio pumps rather than raw `std::thread` pumps. This resolves the CONCERNS item that the reload subsystem is harder to make race-free on the current sync-threaded architecture. The reason is structural: the reload subsystem needs a drain timer, in-flight request tracking, backoff/retry on respawn failure, and a swappable child handle held across two concurrent pumps, and tokio gives cancellation, timeouts, and `select`-based supervision directly. The cost is accepted: `src/proxy.rs` is large and its pump/framing paths must be ported, but the binary only starts once per MCP server and the startup overhead is acceptable. This port belongs to T2, so T4 and T6 are built on the async core rather than retrofitted afterward, and T2 must remain behavior-neutral and keep existing tests passing.

### Decision 2: land the crate first, flip `mcp.json` last

The rollout sequence is normative: rename the crate source to `mcp-toolmon`, update the root `Cargo.toml` members and `install-tools.sh`, build/test/install the new `mcp-toolmon` binary, smoke-test that installed binary, then and only then flip the `command` entries in `.vscode/mcp.json` and `.github/mcp.json` from `mcp-cost-gate` to `mcp-toolmon`, reload VS Code, and finally delete the old `~/.cargo/bin/mcp-cost-gate.exe` once nothing is using it. This is zero-downtime because the new filename does not contend with the lock held by the running old binary; flipping config too early would strand all 12 MCP servers, including the ticket and spec servers this workflow depends on.

### Decision 3: proxy self-reload limitation and thin-core resolution

`mcp-toolmon` cannot hot-reload itself. Shadow-copy execution protects the child MCP server binary from the Windows file lock, but the proxy's own `.exe` is held by its own running process, so updating the proxy still requires a VS Code window reload. That limitation is accepted as deliberate behavior, not a defect.

The resolution path is to keep the proxy core thin and stable, with transport/interception logic isolated from policy so rebuild churn stays low. Policy and gate implementations, including the cost gate, belong behind the `Policy` trait boundary in separate libraries rather than inside the transport core. The current scope is only that clean separation of concerns; dynamic linking remains deferred until gate-code changes are frequent enough that reload cost becomes a real cost.

Deferred / Future Work: if policy churn proves high in practice, the separated policy libraries can later be loaded dynamically so gate updates no longer require a VS Code reload. That is explicitly out of scope for the current work.

## Non-Goals

- No new MCP tool is injected into `tools/list` to trigger restarts. Rejected explicitly: 12 duplicate tool schemas would cost context on every request across every wrapped server.
- No supervision of HTTP viewer servers (doc-viewer, log-viewer, ticket-viewer, spec-viewer, etc.) — that remains the responsibility of `viewer-ctl` (see `viewer-ctl/lifecycle/server`, spec 351e65fe-0629-4a0f-9c19-27dabb36b72f). This spec covers only stdio MCP child processes.
- No cross-server orchestration (e.g., coordinated reload ordering across the 12 servers, shared reload bus). Each proxy instance manages exactly one child.

## Rejected Alternatives (and why)

1. **Standalone `toolmon` wrapper process** (a third process alongside proxy + child): rejected — 3 processes per server, duplicated stdio framing/parsing logic, no benefit over folding reload into the existing single proxy process.
2. **Injected restart tool** in `tools/list`: rejected — the schema-injection cost is paid on every `tools/list` response, multiplied across 12 concurrently wrapped servers, for a capability that should be fully automatic.
3. **Restart-without-shadow-copy**: rejected — does not solve the underlying Windows file lock. The `cargo install --force` that must precede any restart still fails with the same `os error 5`, so this alternative does not fix the incident's second failure mode at all.

## Architecture: Three Subsystems (Single Process)

The renamed crate/binary lives at `memory-api/tools/mcp/mcp-toolmon` (was `memory-api/tools/mcp/mcp-cost-gate`). It remains one OS process per wrapped server, spawning one child.

1. **Transport/Proxy Core** — parses JSON-RPC over stdio, dispatches to child, forwards responses, applies whichever `Policy` is configured. Cost gating (current `gate.rs` logic) becomes a `Policy` trait implementation, not the only path.
2. **Shadow-Copy Execution** — resolves the child binary via PATH to its canonical path `P`; copies `P` to a private, per-instance shadow path `S` (under `TOOLMON_SHADOW_DIR`); spawns `S`, never `P`. Because the running child's open file handle is on `S`, `P` is never locked and `cargo install --force <path>` targeting `P` always succeeds while the proxy runs.
3. **Reload Subsystem** — polls `P`'s (mtime, size) on an interval; on a stable change, copies `P` to a new shadow path, diff-hashes against the current shadow, and if different, drives the reload state machine (below) to swap the running child.

## Normative Requirements

- R1 (Rename): The crate and its produced binary are named `mcp-toolmon`. `install-tools.sh` and both `.vscode/mcp.json` / `.github/mcp.json` are updated to invoke `mcp-toolmon -- <server> [extra-args]` in place of `mcp-cost-gate -- <server> [extra-args]`, for all 12 currently wrapped servers (context-mcp, ticket-mcp, spec-mcp, test-mcp, feedback-mcp, session-mcp, peek-mcp, rule-mcp, audit-mcp, compact-terminal-mcp, fs-mcp, and `log-viewer --mcp`).
- R2 (Policy trait): Cost gating is implemented as one `Policy` trait implementation selected at startup; the trait boundary must not require the transport core to know about cost-specific concepts (budgets, grants, scale).
- R3 (Shadow-copy): The proxy never spawns the canonical PATH-resolved binary directly. It always spawns a private shadow copy. The canonical path must remain writable/replaceable by `cargo install --force` at all times while the proxy is running.
- R4 (Auto-only reload, no tool): Reload is triggered exclusively by the binary watcher. No MCP tool for restart/reload is added to any `tools/list` response.
- R5 (Handshake replay): The proxy caches the client's original `initialize` request params verbatim, plus the `notifications/initialized` notification, and replays both into every newly spawned child in order. The new child's `initialize` response is captured for capability comparison and logged on divergence, but is NEVER forwarded to the client — the client already received its own original `initialize` response and must not observe a second one.
- R6 (No orphaned requests): Any request in flight when the child is torn down for a swap receives a synthesized JSON-RPC error response (not a dropped connection, not silence).
- R7 (Proxy resilience): The proxy process itself must never exit as a consequence of a failed respawn. On respawn failure it retries with backoff and falls back to serving from the last-known-good shadow copy while surfacing errors for affected requests. Dropping the client's stdio connection is not an acceptable failure mode under any reload-related condition.
- R8 (Backward-compatible config): All existing `COST_GATE_*` env vars (`COST_GATE_TABLE`, `COST_GATE_TOOL_METRICS`, `COST_GATE_GRANTS_DIR`, `COST_GATE_SCALE_MAX`, `COST_GATE_BUDGET_ZERO_PRICE`, `COST_GATE_TELEMETRY_LOG`) continue to work unchanged. New knobs: `TOOLMON_RELOAD` (default: on), `TOOLMON_POLL_MS`, `TOOLMON_SHADOW_DIR`, `TOOLMON_DRAIN_MS` (default: 2000).
- R9 (Change notification): After a successful reload swap, the proxy emits `notifications/tools/list_changed` to the client.
- R10 (Drain grace): On a detected change, in-flight requests are given a bounded grace period of `TOOLMON_DRAIN_MS` (default 2000 ms) to complete before the old child is killed. Anything still pending after the grace period is failed per R6, not awaited indefinitely.
- R11 (Write-race safety): A change is acted on only after the canonical path's (mtime, size) is observed STABLE across two consecutive polls, AND the subsequent shadow copy succeeds, AND the copy's content hash differs from the currently running shadow's hash. A copy failure (e.g., file mid-write) is treated as "not yet changed" and retried on the next poll — never surfaced as an error.
- R12 (Shadow cleanup, no TTL): On startup, the proxy sweeps its shadow directory and deletes any shadow artifact whose owning process is no longer alive. After a successful swap, the superseded shadow artifact is deleted. No time-based expiry is used for cleanup; liveness/supersession are the only deletion triggers.

## Reload State Machine

```
steady --(watcher detects mtime/size stable across 2 polls)--> change-detected
change-detected --(shadow copy succeeds AND hash differs)--> draining
change-detected --(copy fails OR hash same)--> steady   [retry next poll, not an error]
draining --(TOOLMON_DRAIN_MS elapses OR all in-flight complete)--> swapping
swapping --(old child killed, new child spawned from new shadow)--> replaying
replaying --(cached initialize + notifications/initialized replayed; new child's initialize response compared, not forwarded)--> steady
steady --(after successful swap)--> emit notifications/tools/list_changed
```

Failure branch (may be entered from `change-detected`, `swapping`, or `replaying`):
```
any-state --(respawn fails: spawn error, handshake failure, or repeated crash)--> retry-with-backoff
retry-with-backoff --(retries exhausted for this attempt)--> serve-from-last-known-good
serve-from-last-known-good --(surface synthesized errors for requests needing the new binary; proxy stays alive; next poll cycle may retry)--> change-detected
```
The proxy never transitions to a terminal/exited state as a result of any failure branch (R7).

## Failure Modes and Required Behavior

| Failure | Required behavior |
|---|---|
| Copy of canonical binary fails mid-write | Treat as not-yet-changed; retry next poll (R11). Not logged as an error. |
| New child fails to spawn | Retry with backoff (R7); do not kill the old child until a replacement is confirmed healthy. |
| New child spawns but fails handshake replay | Treat as respawn failure; retry with backoff; keep serving from last-known-good shadow. |
| Requests pending past drain grace | Synthesize JSON-RPC error responses for each (R6, R10); do not block the swap indefinitely. |
| Repeated respawn failures exhaust retry budget | Fall back to last-known-good shadow; continue serving policy-gated/proxied traffic; keep polling for a future good change. |
| Shadow directory contains orphaned artifacts from a crashed prior instance | Startup sweep deletes any shadow whose owning process is not alive (R12). |

## Configuration Reference

Existing (unchanged): `COST_GATE_TABLE`, `COST_GATE_TOOL_METRICS`, `COST_GATE_GRANTS_DIR`, `COST_GATE_SCALE_MAX`, `COST_GATE_BUDGET_ZERO_PRICE`, `COST_GATE_TELEMETRY_LOG`.

New: `TOOLMON_RELOAD` (bool, default on), `TOOLMON_POLL_MS` (poll interval for the binary watcher), `TOOLMON_SHADOW_DIR` (base directory for shadow copies), `TOOLMON_DRAIN_MS` (drain grace period, default 2000).

## Migration / Rollout Notes

- `memory-api/tools/mcp/mcp-cost-gate` is renamed to `memory-api/tools/mcp/mcp-toolmon`; update the root `Cargo.toml` `[workspace] members` explicit list accordingly (Edition 2024, per repo convention).
- `install-tools.sh#L59-L84`'s tool-name → crate-dir map is updated to point `mcp-toolmon` (and any alias needed for the old `mcp-cost-gate` name during transition) at the new crate path, keeping the `cargo install --path <path> --bin <bin> --quiet [--force]` pattern.
- Both `.vscode/mcp.json` and `.github/mcp.json` update all 12 server entries from `mcp-cost-gate -- <server>` to `mcp-toolmon -- <server>`, including the `log-viewer --mcp` entry which passes an extra arg to its child.
- The rollout order is fixed: land T1 first, build/test/install and smoke-test `mcp-toolmon`, then flip both `mcp.json` files, then reload VS Code, and only after the transition delete the now-unused `mcp-cost-gate.exe` once no process holds it.
- T1 is behavior-neutral: `COST_GATE_*` env var names, the `costGateWarning` response field, the `gate` module, and the `verdict` subcommand remain unchanged so the existing `mcp.json` `env` blocks stay valid.

## CONCERNS

Resolved by user decision: `src/main.rs` today spawns the child via `std::process::Command` with piped stdin/stdout, inherited stderr, and synchronous threads — there is no tokio runtime. The reload subsystem as specified (concurrent polling, drain-timer, in-flight request tracking, backoff retries, all while stdio forwarding continues) is substantially easier to implement correctly on an async runtime with cancellation-safe timers than on raw OS threads with condvars/mutexes. This is not internally inconsistent or unimplementable on sync threads — it can be done with a poller thread, a shared in-flight registry guarded by a mutex, and explicit thread joins for drain — but the sync-threaded design raises real risk of race conditions in R10/R11/R6 interaction (e.g., a request accepted by the transport thread just as the drain timer fires) that would be structurally easier to rule out with an async select-based state machine. T2 (Policy trait extraction) now explicitly owns the tokio async-runtime port for this rework rather than retrofitting reload onto the current sync design.

## Related Specs

- `viewer-ctl/lifecycle/server` (351e65fe-0629-4a0f-9c19-27dabb36b72f) — the Windows binary-lock problem for HTTP viewer servers is prior art for the shadow-copy fix here; that spec's domain (HTTP viewers) is explicitly out of scope for this spec.

## Validation Strategy

### Test fixture (C2)

A new workspace member crate `memory-api/tools/mcp/mcp-toolmon/tests/fixtures/fake-mcp` provides two bin targets, `fake-mcp-v1` and `fake-mcp-v2`, each its own `src/bin/*.rs` source file (not templated from one source with an env flag) so the compiled binaries are byte-different by construction. Each is a minimal stdio JSON-RPC server that answers `initialize` and exposes one tool (`generation`) whose response embeds a literal generation string (`"v1"` / `"v2"`) baked into that binary's source. The fixture crate is added as a `[dev-dependencies]` path dependency of `mcp-toolmon` so `cargo test` builds it and integration tests can locate the executables via `env!("CARGO_BIN_EXE_fake-mcp-v1")` / `..._v2`. Tests copy `fake-mcp-v1`'s exe to a temp "canonical" path, start mcp-toolmon against it, overwrite that same path with the `fake-mcp-v2` bytes, drive a swap, and assert the `generation` tool call now returns `"v2"`.

### Determinism (C1 / C3)

T4 introduces a `ReloadTrigger` seam: `Supervisor::swap_child()` is a directly callable async method, and the watcher (T6) is only one caller of it. Unit and most integration tests call `swap_child()` (or push into an injected trigger channel consumed by the supervisor) directly and `.await` the result, so the swap is synchronous from the test's point of view — no sleeping on the real poll interval. Exactly one integration test (`integration_watcher_real_poll.rs`) exercises the real mtime/size poller end-to-end, using a short `TOOLMON_POLL_MS` and a `wait_until(condition, timeout, message)` bounded-wait helper in `tests/common/mod.rs`. Bare `sleep` as a synchronization primitive is disallowed in the test suite; `sleep` may only appear inside the fixture server or the debounce timer under test itself, never as the assertion mechanism.

### Test matrix

| Test | Level | Proves |
|---|---|---|
| `policy_trait_dispatch` | unit | `CostGatePolicy` reached only via `Policy` trait, identical output to pre-T2 |
| `gate.rs` / `proxy.rs` existing 54 tests | unit | no regression (C6) |
| `shadow_copy_spawns_from_shadow_path` | unit | spawned exe path != canonical path P (T3) |
| `shadow_dir_env_override` | unit | `TOOLMON_SHADOW_DIR` honored (T3) |
| `startup_sweep_removes_dead_shadow` | unit | R12 liveness-based cleanup |
| `swap_child_replaces_running_child` | unit | supervisor swap via injected trigger, fake-mcp-v1→v2 (T4) |
| `inflight_request_synthesized_error_on_kill` | unit | R6: pending request gets JSON-RPC error, not hang (T4) |
| `respawn_backoff_no_process_exit` | unit | R7: proxy survives repeated respawn failure (T4) |
| `handshake_replayed_before_tool_calls` | unit | R5 ordering: init + initialized replayed before routed calls (T5) |
| `handshake_response_never_forwarded` | unit | client sees no second `initialize` response (T5) |
| `capability_divergence_logged_not_fatal` | unit | mismatch logged, swap still succeeds (T5) |
| `watcher_debounces_partial_write` | unit | injected clock/trigger, stable-across-2-polls logic, exactly one swap (T6, R11) |
| `watcher_disabled_by_env` | unit | `TOOLMON_RELOAD=0` runs no poller (T6) |
| `integration_reload_end_to_end` | integration | fake-mcp-v1→v2 via direct `swap_child()` call: no dropped connection, in-flight request resolved, post-swap call served by v2, `tools/list_changed` emitted (T7, R5/R6/R9) |
| `integration_watcher_real_poll` | integration | the one real-timing test: file overwrite detected by actual poller within bounded wait (T6/T7) |
| `windows_lock_freedom` | integration, `#[cfg(windows)]` | canonical path P renamable/overwritable while child runs from shadow copy (T3/C5) |

### Negative / failure-path matrix (C4)

- Corrupt/non-executable replacement binary: respawn fails, proxy does not exit, continues serving from last-known-good shadow, new requests during outage receive synthesized errors, no client stdio drop.
- Kill child mid-request: in-flight request receives a JSON-RPC `error` object with the correlated `id`, never silence, never a hang.
- Post-swap: client stdio never observes a second `initialize` response.
- Respawn retries exhausted repeatedly: proxy still answers `tools/list`/`tools/call` for the old (last-known-good) binary; no process exit under any condition (R7).

### Windows lock test (C5)

`windows_lock.rs` is compiled only under `#[cfg(windows)]` (not `#[ignore]`) — on other platforms the test does not exist, since the scenario (Windows-specific `os error 5` mandatory-locking semantics) has no non-Windows analogue to gate against. It starts mcp-toolmon against a copied canonical path, asserts the child is running from a shadow copy, then renames/overwrites the canonical path from the test process itself and asserts success (no OS lock error) while mcp-toolmon keeps serving.

### Regression guarantee (C6)

All 54 existing tests in `gate.rs`, `proxy.rs`, and `tests/integration_gate.rs` must pass unchanged after every ticket T2–T7. `COST_GATE_*` env var names, the `costGateWarning` field shape, and the `verdict` subcommand output are asserted byte-identical pre/post by keeping their existing assertions untouched (rename-only diffs permitted).

### Reviewer validation commands

```bash
cd memory-api/tools/mcp/mcp-toolmon
cargo test                                   # unit + all integration tests, all platforms
cargo test --test windows_lock               # Windows only; no-op/absent elsewhere
cargo test --test integration_reload_end_to_end
cargo test --test integration_watcher_real_poll
cargo install --path . --bin mcp-toolmon --force   # must succeed while a prior instance runs a wrapped server
```

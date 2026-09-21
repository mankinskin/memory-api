## Problem
MCP servers run from prebuilt binaries in `~/.cargo/bin/`. After a source fix + rebuild, the running server keeps serving stale code, with no reload path short of a full VS Code MCP restart. Worse, on Windows `cargo install` fails with `Zugriff verweigert (os error 5)` because the running child process holds a file lock on `~/.cargo/bin/<server>.exe`. This already caused a landed ticket-api bug fix to be invisible to every MCP caller, producing domain errors indistinguishable from real policy.

## Approved design (decided by user, do not re-litigate)
1. **Rename + restructure, not a new process.** Rename crate/binary `mcp-cost-gate` -> `mcp-toolmon` (dir `memory-api/tools/mcp/mcp-toolmon`) to reflect general-purpose MCP proxy behavior. Within the SAME process, separate concerns: a transport/proxy core, a pluggable policy layer (cost gating becomes one implementation behind a trait, not hardwired), and a new reload subsystem. A standalone third wrapper process was explicitly REJECTED.
2. **Shadow-copy execution.** On startup the proxy resolves the child binary on PATH to a canonical path P, copies it to a private shadow path S (e.g. under a temp dir keyed by name+pid+hash), and spawns S. Consequence: P is never locked, so `cargo install --force` always succeeds on Windows.
3. **Auto-watch reload only — NO new MCP tool.** A watcher polls P's (mtime,size)/hash, debounced so a half-written file is never grabbed. On change: quiesce, kill child, re-copy, respawn, resume. Explicitly rejected: injecting a restart tool into tools/list (12 duplicate tool schemas cost context on every request).
4. **Handshake replay is mandatory.** The proxy caches the client's original `initialize` request params verbatim plus `notifications/initialized`, and replays them into every respawned child. The new child's `initialize` RESPONSE must NOT be forwarded to the client (client already completed handshake); capabilities should be compared and divergence logged.
5. **No orphaned requests.** Requests in flight when the child dies must receive synthesized JSON-RPC error responses so the client never hangs.
6. **Never drop the client connection.** If respawn fails (e.g. broken new binary), the proxy must NOT exit. It retries with backoff and falls back to the last-known-good shadow copy, serving errors meanwhile. This is the whole point of the feature.
7. **Backward compatibility.** Existing `COST_GATE_*` env vars must keep working (set in both mcp.json files). New knobs: `TOOLMON_RELOAD` (default on), `TOOLMON_POLL_MS`, `TOOLMON_SHADOW_DIR`.
8. **Rollout: all 12 servers.** The rename forces both mcp.json files to change anyway. Servers: context-mcp, ticket-mcp, spec-mcp, test-mcp, feedback-mcp, session-mcp, peek-mcp, rule-mcp, audit-mcp, compact-terminal-mcp, fs-mcp, log-viewer-mcp (this one is `mcp-toolmon -- log-viewer --mcp`, extra args must be preserved).
9. After reload, emit `notifications/tools/list_changed` to the client.

## Verified code facts (baseline before this epic)
- `memory-api/tools/mcp/mcp-cost-gate/src/main.rs` (306 LOC): spawns child via `std::process::Command` L132-L138, piped stdin/stdout, inherited stderr, sync threads (no tokio). Reader thread L232-L253; client->server loop L256-L299. On client stdin EOF: drop server stdin, join reader, wait child, exit with child's code (L301-L305).
- `src/proxy.rs` (823 LOC): parses JSON-RPC; intercepts `tools/list` and `tools/call`; injects `caller_model` into tool schemas; modifies responses (inserts `costGateWarning` L344-L347).
- `src/gate.rs` (829 LOC): cost policy — price table, per-model budget, per-tool empirical cost, grants, Allow/Delegate/Reject verdicts.
- `src/lib.rs` (7 LOC).
- Tests: unit tests in `gate.rs`/`proxy.rs`; integration in `memory-api/tools/mcp/mcp-cost-gate/tests/integration_gate.rs`.
- Env vars read: COST_GATE_TABLE, COST_GATE_TOOL_METRICS, COST_GATE_GRANTS_DIR, COST_GATE_SCALE_MAX, COST_GATE_BUDGET_ZERO_PRICE, COST_GATE_TELEMETRY_LOG.
- No restart/reload/watch/respawn logic exists today.
- 12 MCP servers launched as `mcp-cost-gate -- <server>` in both `.vscode/mcp.json` and `.github/mcp.json` (files agree).
- `install-tools.sh` maps tool name -> crate dir in `tool_path()` L59-L84; installs via `cargo install --path "<path>" --bin "<bin>" --quiet [--force]`. `mcp-cost-gate -> memory-api/tools/mcp/mcp-cost-gate` is in that table.
- Root `Cargo.toml` has an explicit (non-glob) `[workspace] members` list. Edition 2024.
- Prior art: spec `351e65fe-0629-4a0f-9c19-27dabb36b72f` ("server lifecycle", viewer-api store); ticket `d30e13e1-3304-4128-9653-be7c47679f9f` ("[install-tools] Install all viewer binaries", done) recorded a real case where `cargo install` required killing a stale PID to release a Windows `.exe` lock. `viewer-api/viewer-ctl` is the existing lifecycle manager but manages HTTP viewers, not stdio MCP children.

## Child slices (ordered, each independently implementable)
T1 rename, T2 policy-trait extraction, T3 shadow-copy exec, T4 lifecycle supervisor, T5 handshake replay, T6 binary watcher, T7 validation. See each child ticket for scope, acceptance criteria, and touched files.

## Done condition
All child tickets closed; all 12 servers run via `mcp-toolmon`; a live rebuild-while-running is transparent to MCP clients with no dropped connection, no orphaned requests, and calls after reload hit the new binary.


## Verification summary (2026-07-31)

Test suite: `cargo test -p mcp-toolmon -p toolmon-policy-api -p toolmon-costgate` -> 76 passed / 0 failed / 0 ignored, run twice with identical per-target counts, no flakes. Breakdown: mcp-toolmon lib 20, crash_inflight_subprocess 1, handshake 3, integration_gate 16, integration_reload_end_to_end 1, shadow 4, supervisor 5, watcher 4, toolmon-costgate 22.

**Property A — proxied target-server functionality is not compromised: PROVEN.**
- test_stdio_cheap_model_allowed (tests/integration_gate.rs#L471): full stdio round-trip through the real mcp-toolmon binary; allowed call forwards to child, response returned, caller_model stripped from forwarded arguments (#L550-553).
- test_stdio_expensive_model_refused (tests/integration_gate.rs#L375): expensive model + expensive tool refused before reaching the child.
- tools_list_response_gets_schema_injected (mcp-toolmon lib proxy tests): caller_model injected into each tool schema in a tools/list response.
- handshake_replayed_before_tool_calls (tests/handshake.rs#L75-89): after a swap the next tools/call succeeds because the replayed initialize already completed against the new child.

**Property B — child hot-restart works as expected: PROVEN.**
- transparent_reload_end_to_end_subprocess (tests/integration_reload_end_to_end.rs#L100): external-process test; real handshake, pre-swap call served "v1" (#L133), canonical binary overwritten on disk, real watcher detects the change, notifications/tools/list_changed observed (#L155-160), racing in-flight request id=3 answered (#L167-169), post-swap call returns "v2" (#L195), exactly one initialize response for the whole session (#L200-205), ids 1/2/3 all answered (#L208-210).
- integration_watcher_real_poll (tests/watcher.rs): real on-disk overwrite detected by the actual mtime/size poller; list_changed observed; subsequent call returns "v2".
- swap_child_replaces_running_child (tests/supervisor.rs#L97-119): pre-swap "v1", post-swap "v2", no synthesized errors.
- inflight_request_synthesized_error_on_kill (tests/supervisor.rs#L122-158): a request pending at swap time receives exactly one synthesized JSON-RPC error carrying the original request id (code -32001); supervisor healthy afterward.
- crash_auto_recovery_respawns_and_serves_again (tests/watcher.rs): spontaneous child crash detected, supervisor auto-respawns and resumes serving.

**Known limitations (not proven, recorded honestly):**
1. In-flight preservation across a swap is a no-loss guarantee, not a real-answer guarantee: a pending request gets either its real result or a clean synthesized JSON-RPC error (-32001), never a drop or hang.
2. crash_auto_recovery_respawns_and_serves_again documents a race: a write issued between an unexpected crash and its detection can vanish into the dead child's pipe with no synthesized error.\n3. No test drives a real VS Code client; tests use a hand-built stdio harness speaking raw JSON-RPC.\n4. No coverage of TOOLMON_POLL_MS/TOOLMON_DRAIN_MS boundary values or repeated v1->v2->v1 swap cycles.\n5. mcp-toolmon cannot hot-reload itself — updating the proxy binary still requires a VS Code reload; accepted design limitation, mitigated by keeping the proxy thin with policy/gate logic in toolmon-policy-api and toolmon-costgate.\n\nSee follow-up ticket f7244064 (shutdown-latency root cause) for a related but separate, not-yet-fixed issue on the whole-process shutdown path (not the hot-swap path).
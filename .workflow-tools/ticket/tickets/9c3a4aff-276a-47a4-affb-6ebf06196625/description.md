## Problem
`gate.rs` cost logic is hardwired into the proxy path in `proxy.rs`. To make mcp-toolmon a general-purpose proxy (reload/lifecycle features are policy-agnostic), the cost gate must become one pluggable implementation, not the only possible behavior. Part of epic 25780944; depends on T1 (post-rename paths). This ticket also owns the tokio async-runtime port for the proxy core so the reload subsystem can be built on async cancellation, timers, and select-based supervision instead of raw thread pumps.

## Approach
Define a `Policy` trait (e.g. `fn on_tools_list`, `fn on_tool_call_request`, `fn on_tool_call_response`) in the proxy core. Move existing `gate.rs` cost logic behind a `CostGatePolicy` implementation of that trait. Port the transport/proxy core onto tokio (`tokio::process::Command`, async stdio pumps, async timers, and cancellation-safe supervision) while keeping behavior neutral. `proxy.rs` calls into `Box<dyn Policy>` / generic policy instead of calling gate functions directly. Behavior-neutral: identical JSON-RPC behavior, identical `costGateWarning` injection, identical env var handling (`COST_GATE_*` unchanged).

## Acceptance criteria
- [ ] `Policy` trait defined with hook points covering current gate interception points (tools/list schema injection, tools/call request/response handling)
- [ ] `CostGatePolicy` struct implements `Policy`, wrapping existing `gate.rs` logic with no behavior change
- [ ] `proxy.rs` invokes policy through the trait, not direct gate function calls, and the transport/proxy core runs on tokio async primitives instead of raw thread pumps
- [ ] All existing unit tests in `gate.rs` and `proxy.rs` pass unchanged
- [ ] `tests/integration_gate.rs` passes unchanged (rename allowed, assertions untouched)
- [ ] No new env vars or CLI flags introduced by this ticket (that's T3/T6)
- [ ] `costGateWarning` injection point and JSON shape unchanged

## Files touched
- memory-api/tools/mcp/mcp-toolmon/src/proxy.rs
- memory-api/tools/mcp/mcp-toolmon/src/gate.rs
- memory-api/tools/mcp/mcp-toolmon/src/lib.rs
- memory-api/tools/mcp/mcp-toolmon/src/policy.rs (new)
- memory-api/tools/mcp/mcp-toolmon/tests/integration_gate.rs


## Validation acceptance criteria (addendum)
- [ ] `mcp_cost_gate` lib name renamed to `mcp_toolmon` in `Cargo.toml` `[lib] name` and all internal `use`/path references; `cargo build` and `cargo test` succeed with the new lib name
- [ ] A `Policy` trait is defined in `src/policy.rs`; unit test `policy_trait_dispatch` asserts `CostGatePolicy::on_tools_list`/`on_tool_call_request`/`on_tool_call_response` are reached only through `Box<dyn Policy>` (or generic bound), not direct `gate::` calls from `proxy.rs`
- [ ] Transport core ported to tokio (`tokio::process::Command`, async stdio pumps); an injectable `ReloadTrigger` seam (e.g. a directly callable async `swap_child()`-shaped hook, or an mpsc channel the supervisor consumes) is introduced at this layer specifically so T4's supervisor and T6's watcher can each drive a swap without depending on wall-clock polling in tests
- [ ] All 54 pre-existing tests in `gate.rs`, `proxy.rs`, and `tests/integration_gate.rs` pass unchanged: `cargo test -p mcp-toolmon` (or equivalent path-based invocation) shows 54/54 passing, zero renamed/removed assertions beyond mechanical `mcp_cost_gate`→`mcp_toolmon` path updates
- [ ] `COST_GATE_TABLE`, `COST_GATE_TOOL_METRICS`, `COST_GATE_GRANTS_DIR`, `COST_GATE_SCALE_MAX`, `COST_GATE_BUDGET_ZERO_PRICE`, `COST_GATE_TELEMETRY_LOG` env vars and the `costGateWarning` JSON field remain byte-compatible (asserted by the unmodified existing tests continuing to pass)
## T2 completion note

- `Policy` trait added in `src/policy.rs` (`on_tools_list`, `resolves`, `evaluate`); `CostGatePolicy` wraps `gate::Gate` unchanged. `proxy.rs` now dispatches through `Option<&dyn Policy>` in `handle_client_message`/`handle_server_message`, no direct `gate::` calls remain there.
- Lib renamed `mcp_cost_gate` -> `mcp_toolmon` (`Cargo.toml` `[lib]` + all `use` sites in `src/main.rs` and `tests/integration_gate.rs`).
- Transport core ported to tokio: `src/supervisor.rs` (`Supervisor`) owns the child's stdin/stdout behind `tokio::sync::Mutex`, giving pumps a stable seam a future T4 supervisor can swap without rewriting them. `main.rs` runs on a multi-thread tokio runtime with async stdio pumps (`tokio::process::Command`, `tokio::io`).
- Tests: 55 passed (54 pre-existing unchanged + 1 new `policy_trait_dispatch`), 0 failed. `cargo build -p mcp-toolmon` clean; `cargo clippy` shows only pre-existing warnings in unmodified `gate.rs` and one pre-existing warning at `proxy.rs:148` (`needless_as_bytes`, predates this ticket). Smoke test: piped `initialize` through `target/debug/mcp-toolmon.exe -- peek-mcp`, observed valid `{"id":1,"jsonrpc":"2.0","result":{...}}`.
- No behavior deviations identified; all `COST_GATE_*` env vars, `costGateWarning` injection, `verdict` subcommand, and extra-arg forwarding are unchanged.


## Verification note (2026-07-31)
cargo test -p mcp-toolmon -p toolmon-policy-api -p toolmon-costgate -> 76/76 passed, run twice, 0 flakes. Property A (proxied target-server functionality not compromised) and Property B (child hot-restart) both PROVEN. Full evidence and known limitations recorded on epic 25780944.
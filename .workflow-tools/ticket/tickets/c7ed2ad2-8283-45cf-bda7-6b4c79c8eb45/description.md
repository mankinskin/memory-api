## Problem

An audit found the mcp-toolmon `Policy` trait boundary is leaky:
- src/proxy.rs imports gate::Decision directly and matches on it.
- src/policy.rs's trait signature returns gate::Decision and depends on Gate, so the boundary is named in terms of the thing it should abstract.
- src/policy.rs depends on src/proxy.rs for schema injection (inject_caller_model_schema), creating a dependency cycle if gate were extracted alone.
- src/main.rs reads all COST_GATE_* env vars, constructs Gate, writes COST_GATE_TELEMETRY_LOG telemetry, and implements the verdict subcommand directly, hardwiring CostGatePolicy.

## Approach

Split mcp-toolmon into three crates:
1. toolmon-policy-api (new lib): owns the Policy trait, Decision type, CALLER_MODEL_ARG, and inject_caller_model_schema. Zero knowledge of the cost gate; no dependency on mcp-toolmon. Breaks the cycle.
2. toolmon-costgate (new lib): the gate engine (gate.rs) plus gate-owned concerns stranded in main.rs — COST_GATE_* env loading, Gate construction, the telemetry writer, and the verdict subcommand logic. Depends on toolmon-policy-api; implements Policy.
3. mcp-toolmon (existing bin): transport only (main.rs, proxy.rs, supervisor.rs, shadow.rs, watcher.rs). Depends on both new crates; proxy.rs references only the API crate, never the gate.

main.rs keeps a thin CLI shell that delegates verdict into toolmon-costgate and wires Arc<dyn Policy> from the gate crate's constructor.

This is a target-architecture step toward a thin transport/proxy core with policy/gate logic behind a library boundary, so transport rarely needs rebuilding and a future improvement could dynamically link policy plugins so gate changes need no VS Code reload. Dynamic linking is explicitly deferred — this ticket is only the clean separation of concerns.

Depends on T7 (15c66e77-379b-4a1f-b2a2-4cf1cb808af9). Linked to epic 25780944-a784-4373-8991-88c2902b1556.

## Acceptance Criteria
- [ ] toolmon-policy-api crate created with Policy trait, Decision enum, CALLER_MODEL_ARG, inject_caller_model_schema; zero dependency on mcp-toolmon or the cost gate.
- [ ] toolmon-costgate crate created with the gate engine plus COST_GATE_* env loading, policy construction, telemetry writer, and verdict subcommand logic; depends only on toolmon-policy-api.
- [ ] mcp-toolmon reduced to transport only; proxy.rs references only toolmon-policy-api, never the gate directly (outside test fixtures).
- [ ] main.rs delegates the verdict subcommand into toolmon-costgate and wires policy via toolmon-costgate's constructor instead of reading COST_GATE_* itself.
- [ ] Both new crates registered in the root Cargo.toml workspace members list.
- [ ] All 76 pre-split tests still pass (moved tests count toward their new crate; no test coverage lost).
- [ ] COST_GATE_TABLE, COST_GATE_TOOL_METRICS, COST_GATE_GRANTS_DIR, COST_GATE_SCALE_MAX, COST_GATE_BUDGET_ZERO_PRICE, COST_GATE_TELEMETRY_LOG keep identical semantics.
- [ ] costGateWarning response field, caller_model rejection message text, and verdict subcommand CLI surface/output unchanged.
- [ ] Binary still named mcp-toolmon; fake-mcp-v1/fake-mcp-v2 remain bin targets of mcp-toolmon.
- [ ] cargo build and cargo clippy clean (no newly introduced issues) across all three crates.
- [ ] Behavioral smoke check (JSON-RPC initialize through the built binary) and verdict subcommand check both pass.
- [ ] toolmon-policy-api's lib.rs documents the deferred dynamic-plugin-boundary intent and known FFI blockers.

## Completion note

Implemented as designed: three crates now exist — toolmon-policy-api (Policy trait, Decision, CALLER_MODEL_ARG, inject_caller_model_schema, zero gate knowledge), toolmon-costgate (Gate engine, COST_GATE_* env loading, telemetry writer generic over Serialize, verdict subcommand), and mcp-toolmon (transport only: main.rs/proxy.rs/supervisor.rs/shadow.rs/watcher.rs).

All 76 pre-split tests pass post-split (20 mcp-toolmon lib + 54 mcp-toolmon integration/tests total across its test binaries + 22 toolmon-costgate lib = 76; gate.rs's 21 unit tests + policy.rs's 1 test moved to toolmon-costgate as-is). cargo build clean, no new clippy warnings introduced (8 pre-existing warnings, all outside touched-behavior code or copied verbatim from the original gate.rs test module). Behavioral smoke check (JSON-RPC initialize through the built binary fronting peek-mcp) and the verdict subcommand both produce identical output to before the split.

One disclosed deviation: proxy.rs's `#[cfg(test)]` module still constructs a real `toolmon_costgate::{Gate, CostGatePolicy}` fixture to exercise the proxy dispatch path end-to-end (test-only; non-test proxy.rs code references only toolmon-policy-api). `Gate::new` was narrowed from `pub` to private (only ever called from gate.rs's own tests; fixes a pre-existing visibility-leak clippy warning as a side effect, not requested but harmless).

Ticket store: repo-root .ticket/, workspace default. All three crates registered in the root Cargo.toml workspace members list (no separate memory-api/Cargo.toml exists).


## Verification note (2026-07-31)
cargo test -p mcp-toolmon -p toolmon-policy-api -p toolmon-costgate -> 76/76 passed, run twice, 0 flakes. Property A (proxied target-server functionality not compromised) and Property B (child hot-restart) both PROVEN. Full evidence and known limitations recorded on epic 25780944.
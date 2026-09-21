## Problem
The crate/binary name `mcp-cost-gate` no longer reflects its intended scope (general-purpose MCP proxy with pluggable policy + reload). Part of epic 25780944. This ticket is behavior-neutral and owns the rename first, config flip last rollout so the running old binary keeps serving until the new filename is installed and smoke-tested.

## Approach
Pure rename, no behavior change. Move directory, update `Cargo.toml` package/bin name, update every reference: both mcp.json files, install-tools.sh tool table, root Cargo.toml workspace members, and any docs/instructions naming `mcp-cost-gate`. Roll out in this order: land the crate rename, build/test/install `mcp-toolmon`, smoke-test it, then flip both `mcp.json` files, reload VS Code, and only then delete the old `mcp-cost-gate.exe` once it is no longer in use. Keep `COST_GATE_*` env var names unchanged (compat requirement lives in T2/T3, not here — just don't break them), and do not rename the `costGateWarning` response field, the `gate` module, or the `verdict` subcommand.

## Acceptance criteria
- [ ] Directory `memory-api/tools/mcp/mcp-cost-gate/` renamed to `memory-api/tools/mcp/mcp-toolmon/`
- [ ] `Cargo.toml` package name and `[[bin]]` name (if explicit) updated to `mcp-toolmon`
- [ ] Root `Cargo.toml` explicit `members` list updated to the new path
- [ ] `.vscode/mcp.json` and `.github/mcp.json` updated: all 12 server entries reference `mcp-toolmon` instead of `mcp-cost-gate`, including the `log-viewer-mcp` entry which must keep its extra args (`-- log-viewer --mcp`)
- [ ] `install-tools.sh` `tool_path()` table updated: `mcp-toolmon -> memory-api/tools/mcp/mcp-toolmon`
- [ ] Any docs/instructions referencing `mcp-cost-gate` by name updated (grep repo-wide for `mcp-cost-gate` and `mcp_cost_gate`)
- [ ] `cargo build -p mcp-toolmon` succeeds; existing unit + integration tests pass unchanged (test file may be renamed but assertions untouched)
- [ ] `install-tools.sh` runs and installs `mcp-toolmon.exe` / `mcp-toolmon`
- [ ] The rename lands before the config flip; both `.vscode/mcp.json` and `.github/mcp.json` remain pointed at `mcp-cost-gate` until the new binary is installed and smoke-tested
- [ ] `COST_GATE_*` env vars remain unchanged, and `costGateWarning`, `gate`, and `verdict` remain unchanged as behavior-neutral compatibility surfaces

## Files touched
- memory-api/tools/mcp/mcp-cost-gate/** (moved to memory-api/tools/mcp/mcp-toolmon/**)
- Cargo.toml
- .vscode/mcp.json
- .github/mcp.json
- install-tools.sh
- any matched docs/instructions files (discover via grep for `mcp-cost-gate`)
Renamed mcp-cost-gate crate/binary/dir to mcp-toolmon (behavior-neutral). git mv with history preserved; Cargo.toml package+bin renamed; root workspace member updated; install-tools.sh tool_names/mcp_tool_names/tool_path/tool_bin updated; internal binary-name strings (log prefix, usage text, CARGO_BIN_EXE_ env var) updated; build+tests pass (54/54); installed to ~/.cargo/bin/mcp-toolmon.exe; smoke-tested --help path and a real JSON-RPC initialize proxied through a live child MCP server; .vscode/mcp.json, .github/mcp.json, and opencode.json flipped to \"mcp-toolmon\" only after install+smoke succeeded. gate.rs, COST_GATE_* env vars, costGateWarning field, verdict subcommand, and lib crate name mcp_cost_gate left untouched per scope. Old mcp-cost-gate.exe left in place, untouched.


## Verification note (2026-07-31)
cargo test -p mcp-toolmon -p toolmon-policy-api -p toolmon-costgate -> 76/76 passed, run twice, 0 flakes. Property A (proxied target-server functionality not compromised) and Property B (child hot-restart) both PROVEN. Full evidence and known limitations recorded on epic 25780944.
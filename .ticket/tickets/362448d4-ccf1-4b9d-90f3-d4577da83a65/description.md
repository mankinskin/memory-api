# [ticket-vscode] Add dual-host packaging, bundling, and extension test harnesses

Package the ported extension so it activates in both the desktop/remote Node host and the web extension host, and add the harnesses that validate both.

Scope:
- `package.json` must expose both `main` and `browser` entrypoints (currently only `main`).
- bundle the `browser` entry into a single WebWorker-compatible file including the WASM asset + JS glue.
- desktop-only modules (`browserBridge.ts`, `browserBridgeCdp.ts`, server-spawn helpers) must be excluded from the web bundle.

## Dependency links

- Tracker: [6d07d610 Rust/WASM port track](../6d07d610-75c1-448a-afd5-6ae15098ca21/ticket.toml)
- Depends on: [bfafde19 Replace Node-bound behaviors with host capability adapters](../bfafde19-ddf7-47ef-966e-a1135be4efd6/ticket.toml)
- Unblocks: [6de424b0 Validate Rust/WASM parity across desktop, web, and remote hosts](../6de424b0-68ec-43c7-9d70-eb8d17305ab3/ticket.toml)

Acceptance criteria:
- [ ] `package.json` ships both `main` and `browser` entries and the web bundle is a single WebWorker-compatible file.
- [ ] The WASM asset and generated glue are included for both desktop and web packaging paths.
- [ ] Desktop-only code is not loaded in the web bundle.
- [ ] TypeScript typecheck and bundler builds pass for both entrypoints, plus a `@vscode/test-web` smoke harness.

## Frozen architecture boundary

The Rust/WASM architecture is frozen in spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`):
- "Target Architecture → Runtime model": ship both `main` (desktop/remote) and `browser` (web worker) entries calling the same core through one adapter boundary.
- "Module Portability Matrix": desktop-only/deferred modules (`browserBridge.ts`, `browserBridgeCdp.ts`, server-spawn/binary-discovery in `extensionSupport.ts`) must be excluded from the web bundle.
- "Validation Strategy → Extension packaging": typecheck/bundle both entries, activation smoke test, and `@vscode/test-web` browser-hosted test.
- Loader/bundling constraints recorded by the dual-host activation spike (14047b99) feed this ticket.

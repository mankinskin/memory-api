# [ticket-vscode] Add dual-host packaging, bundling, and extension test harnesses

Package the ported extension so it activates in both the desktop/remote Node host and the web extension host, and add the harnesses that validate both.

Scope:
- `package.json` must expose both `main` and `browser` entrypoints (currently only `main`).
- bundle the `browser` entry into a single WebWorker-compatible file including the WASM asset + JS glue.
- desktop-only modules (`browserBridge.ts`, `browserBridgeCdp.ts`, server-spawn helpers) must be excluded from the web bundle.

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
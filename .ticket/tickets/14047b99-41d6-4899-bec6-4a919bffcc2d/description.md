# [ticket-vscode] Prove dual-host WASM activation

Build a narrow architecture spike that proves a Rust/WASM module can be loaded by both VS Code extension hosts used by this port:

- desktop/local or remote Node/Electron host via `main`
- browser/web worker host via `browser`

The spike should validate loader and bundling choices before any meaningful behavior is ported.

Acceptance criteria:
- [ ] A minimal Rust crate compiles to `wasm32-unknown-unknown` and exports a smoke-tested function callable from the extension host.
- [ ] The desktop `main` entry and the web `browser` entry both activate successfully while loading the same WASM module or the same generated bindings.
- [ ] The chosen bundler/artifact flow is documented, including how the WASM asset and generated JS glue are included in the VSIX/web bundle.
- [ ] The spike records any host-specific loader constraints that affect later tickets.

Notes:
- This ticket is about runtime feasibility and bundling shape only, not feature parity.

## Frozen architecture boundary

The Rust/WASM architecture is frozen in spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`). Use it as authoritative instead of re-deciding locally:
- "Host Capability Contract" rule 1-2: the core depends only on required capabilities and never imports `vscode`/Node; the spike must keep the same narrow adapter seam for both `main` and `browser` entries.
- "Per-Host Behavior Differences" → host detection summary: `env.uiKind` / `extension.extensionKind` / `env.remoteName` / `isVirtualWorkspace` are the only allowed host-mode signals.
- "Module Portability Matrix": `extension.ts` splits into a shared activation core plus `main`/`browser` entrypoints — the spike proves that shape.
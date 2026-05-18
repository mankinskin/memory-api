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

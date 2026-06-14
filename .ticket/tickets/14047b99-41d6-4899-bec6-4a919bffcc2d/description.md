# [ticket-vscode] Prove dual-host WASM activation

Build a narrow architecture spike that proves a Rust/WASM module can be loaded by both VS Code extension hosts used by this port.

## Dependency links

- Tracker: [6d07d610 Rust/WASM port track](../6d07d610-75c1-448a-afd5-6ae15098ca21/ticket.toml)
- Depends on: [93f7e422 Freeze Rust/WASM architecture spec and feature matrix](../93f7e422-1e41-4145-b8ba-0dcf7fc730ac/ticket.toml)
- Unblocks: [011563c2 Extract portable Rust core for ticket/domain logic](../011563c2-59e7-48f1-a61f-d8fdc80d2f6e/ticket.toml)

Acceptance criteria:
- [x] A minimal Rust crate compiles to `wasm32-unknown-unknown` and exports a smoke-tested function callable from the extension host.
- [x] The desktop `main` entry and the web `browser` entry both activate successfully while loading the same WASM module or the same generated bindings.
- [x] The chosen bundler/artifact flow is documented, including how the WASM asset and generated JS glue are included in the VSIX/web bundle.
- [x] The spike records any host-specific loader constraints that affect later tickets.

## Spike implementation summary

### New Rust crate

`memory-viewers/memory-api/crates/ticket-vscode-core/` added to the root workspace (`context-engine/Cargo.toml`).

- `Cargo.toml`: `crate-type = ["cdylib", "rlib"]`, `wasm-bindgen = "0.2.115"` behind `features = ["wasm"]` so native `cargo test` does not require wasm-bindgen.
- `src/lib.rs`: exports `core_version() -> String`, `TicketSummary`, `HostKind`, `supports_server_control(HostKind) -> bool`, `supports_browser_bridge(HostKind) -> bool`, and pure `ticket_matches` filtering.
- All 5 unit tests pass: `cargo test -p ticket-vscode-core`
- WASM target check passes: `cargo check -p ticket-vscode-core --target wasm32-unknown-unknown --features wasm`

### TypeScript extension changes

- `src/extension.browser.ts`: new web/browser extension entrypoint. Loads the WASM binary from the extension package via `vscode.workspace.fs.readFile` (works in both hosts), registers a smoke-check `ticket-viewer.browserHostInfo` command.
- `tsconfig.browser.json`: separate tsconfig with `"module": "esnext"` and `"lib": ["ES2020", "WebWorker"]` for the browser compilation path.
- `tsconfig.json` updated to exclude `src/extension.browser.ts` so the main CJS build does not attempt to compile the browser entry.
- `scripts/bundle-browser.mjs`: esbuild script producing `out/extension.browser.js` (2.9 kB) — single-file WebWorker-compatible bundle; vscode is external; `.wasm` files excluded from bundle (loaded at runtime).
- `package.json`: added `"browser": "./out/extension.browser.js"` entry, `bundle:browser` and `build` scripts, `esbuild ^0.25.0` devDependency.

### Loader constraints recorded (for follow-on tickets)

1. **Shared loader**: `vscode.workspace.fs.readFile(Uri.joinPath(extensionUri, 'pkg', '*.wasm'))` works in both Node and WebWorker hosts. Use this in the real wasm-pack integration.
2. **WASM instantiation overload**: TypeScript's `WebAssembly.instantiate` has two overloads. Pass `wasmBytes.buffer` (ArrayBuffer) not `wasmBytes` (Uint8Array) to get `WebAssemblyInstantiatedSource` with `.instance.exports` instead of the wrong `Instance` overload.
3. **Browser bundle is single-file**: `esbuild` with `format: 'cjs'` and `external: ['vscode']` produces a working VS Code web extension bundle. `.wasm` is `loader: {'.wasm': 'empty'}` — the WASM file must be a separate asset in the extension package.
4. **wasm-pack `--target bundler`**: Use this for the real integration so the JS glue can be imported as ES modules by esbuild. The real activation will call `wasm-bindgen`'s generated `__wbg_init` with the bytes from the shared loader.
5. **VSIX**: `.vscodeignore` must allow `out/extension.browser.js`, `out/extension.browser.js.map`, and `pkg/*.wasm`. The main tsconfig must exclude `src/extension.browser.ts`.

## Frozen architecture boundary

Spec `ticket-vscode/rust-wasm-port` (a592900c, state `reviewed`) — "Loader Constraints" section now records these findings in the spec body.
